using System.Diagnostics;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using Windows.AI.MachineLearning;

namespace Tomoshibi.Media;

/// <summary>
/// The background behind somebody blurred, frame by frame, before the frame
/// is sent and before it is shown to them (live/blur.ts; Android's
/// BackgroundBlur.kt, whose design this is).
///
/// The person is found by the web's own segmenter: MediaPipe's selfie model,
/// the very file the web serves (web/public/segment), converted to ONNX
/// (windows/Blur/convert.py, which also checks the conversion against
/// MediaPipe itself) and run by Windows' own machine learning, which every
/// Windows 10 and 11 has, so the app carries no inference runtime of its
/// own. What is kept and what is blurred is therefore the same as in a
/// browser. The blur and the blend are BlurMath's, shared with Android.
///
/// Finding the person takes longer than a frame has on a slow machine, so it
/// runs on a thread of its own and never holds a frame up: each frame is
/// blended through the newest mask there is, and the segmenter is handed the
/// next frame whenever it is free. Until the first mask, the whole frame is
/// blurred rather than none of it.
///
/// Made when blur is turned on and closed when it is turned off, so that a
/// camera without blur pays nothing for it, not even the model in memory.
/// <see cref="Process"/> is for the capture thread; the rest for any.
/// </summary>
internal sealed class BackgroundBlur : IDisposable
{
    /// <summary>The model's square side; the frame is stretched to it, as MediaPipe does for this model.</summary>
    public const int ModelSide = 256;

    /// <summary>The converted model, beside the app.</summary>
    public static string ModelPath => Path.Combine(AppContext.BaseDirectory, "segment", "selfie_segmenter.onnx");

    private readonly Thread _worker;
    private readonly AutoResetEvent _wake = new(false);
    private int _busy;
    private volatile bool _closed;
    private volatile bool _failed;

    /// <summary>The newest mask, on the grid of the frame it was found in.</summary>
    private sealed record Person(int Width, int Height, int Rotation, float[] Grid);
    private volatile Person? _person;

    /// <summary>The frame handed over, read by the worker while _busy is set.</summary>
    private readonly float[] _small = new float[ModelSide * ModelSide * 3];
    private (int Width, int Height, int Rotation, int GridWidth, int GridHeight) _job;

    // Capture thread only.
    private BlurMath? _math;
    private int _frames;
    private long _spent;

    /// <summary>Told, on the worker's thread, when the segmenter would not start: the frame cannot be kept.</summary>
    public event Action<Exception>? Failed;

    public BackgroundBlur()
    {
        _worker = new Thread(Work) { IsBackground = true, Name = "tomoshibi segment" };
        _worker.Start();
    }

    /// <summary>
    /// Blurs what is behind the person in the packed planes, in place:
    /// <paramref name="y"/> of <paramref name="width"/> by
    /// <paramref name="height"/>, <paramref name="u"/> and <paramref name="v"/>
    /// at half that, captured <paramref name="rotation"/> clockwise degrees
    /// from upright.
    /// </summary>
    public void Process(byte[] y, byte[] u, byte[] v, int width, int height, int rotation = 0)
    {
        var began = Stopwatch.GetTimestamp();
        var math = _math is { } kept && kept.Width == width && kept.Height == height ? kept : (_math = new BlurMath(width, height));
        if (!_failed && !_closed && Interlocked.CompareExchange(ref _busy, 1, 0) == 0)
        {
            BlurMath.ModelInput(y, u, v, width, height, rotation, ModelSide, _small);
            _job = (width, height, rotation, math.GridWidth, math.GridHeight);
            _wake.Set();
        }
        var found = _person is { } person && person.Width == width && person.Height == height && person.Rotation == rotation ? person : null;
        math.Blur(y, u, v, found?.Grid);
        _spent += Stopwatch.GetTimestamp() - began;
        if (++_frames % 150 == 0)
        {
            Log.Line($"blur: {Stopwatch.GetElapsedTime(0, _spent / 150).TotalMilliseconds:F1} ms a frame on the capture thread, {(found != null ? "a person found" : "nobody found yet")}");
            _spent = 0;
        }
    }

    private void Work()
    {
        Segmenter? segmenter = null;
        try
        {
            while (!_closed)
            {
                _wake.WaitOne();
                if (_closed) break;
                if (Volatile.Read(ref _busy) == 0) continue;
                try
                {
                    if (segmenter == null)
                    {
                        try
                        {
                            segmenter = Segmenter.Load(ModelPath);
                            Log.Line("blur: the segmenter is ready");
                        }
                        catch (Exception fault)
                        {
                            _failed = true;
                            Log.Line($"blur: the segmenter would not start: {fault.GetType().Name}: {fault.Message}");
                            Failed?.Invoke(fault);
                            return;
                        }
                    }
                    var job = _job;
                    var mask = segmenter.Run(_small);
                    if (mask != null && !_closed)
                    {
                        _person = new Person(job.Width, job.Height, job.Rotation,
                            BlurMath.PersonOnGrid(mask, ModelSide, ModelSide, job.Width, job.Height, job.Rotation, job.GridWidth, job.GridHeight));
                    }
                }
                catch (Exception fault)
                {
                    Log.Line($"blur: a frame was not segmented: {fault.GetType().Name}: {fault.Message}");
                }
                finally
                {
                    Volatile.Write(ref _busy, 0);
                }
            }
        }
        finally
        {
            segmenter?.Dispose();
        }
    }

    /// <summary>Lets the segmenter and its model go, once it has finished with any frame it has.</summary>
    public void Dispose()
    {
        if (_closed) return;
        _closed = true;
        _person = null;
        _wake.Set();
    }
}

/// <summary>
/// The selfie model in Windows' machine learning (Windows.AI.MachineLearning),
/// on the processor: the model is small, a frame of it a few milliseconds,
/// and the graphics card is the one thing a call is already busy with.
/// One thread at a time.
/// </summary>
internal sealed class Segmenter : IDisposable
{
    private readonly LearningModel _model;
    private readonly LearningModelSession _session;
    private readonly string _input;
    private readonly string _output;
    private static readonly long[] Shape = [1, BackgroundBlur.ModelSide, BackgroundBlur.ModelSide, 3];

    private Segmenter(LearningModel model, LearningModelSession session)
    {
        _model = model;
        _session = session;
        _input = model.InputFeatures[0].Name;
        _output = model.OutputFeatures[0].Name;
    }

    public static Segmenter Load(string path)
    {
        var model = LearningModel.LoadFromFilePath(path);
        var session = new LearningModelSession(model, new LearningModelDevice(LearningModelDeviceKind.Cpu));
        return new Segmenter(model, session);
    }

    /// <summary>The person's share of each pixel of the model's square, from its RGB input; null where nothing came back.</summary>
    public unsafe float[]? Run(float[] input)
    {
        var binding = new LearningModelBinding(_session);
        binding.Bind(_input, TensorFloat.CreateFromArray(Shape, input));
        var result = _session.Evaluate(binding, "");
        if (result.Outputs[_output] is not TensorFloat output) return null;
        using var buffer = output.CreateReference();
        var (data, capacity) = ComCalls.Bytes(buffer);
        var count = BackgroundBlur.ModelSide * BackgroundBlur.ModelSide;
        if (capacity < count * sizeof(float)) return null;
        return new ReadOnlySpan<float>((void*)data, count).ToArray();
    }

    public void Dispose()
    {
        _session.Dispose();
        _model.Dispose();
    }

    /// <summary>
    /// The model run here on MediaPipe's test portrait, against MediaPipe's
    /// own mask for it (TOMOSHIBI_DEBUG_SEGMENT, a folder holding
    /// portrait-input.bin and portrait-mediapipe.bin from convert.py): what
    /// shows that Windows' runtime gives the browser's answer, which a test
    /// of the conversion alone would not.
    /// </summary>
    public static string Check(string folder)
    {
        var count = BackgroundBlur.ModelSide * BackgroundBlur.ModelSide;
        var input = MemoryMarshalFloats(File.ReadAllBytes(Path.Combine(folder, "portrait-input.bin")));
        var expected = MemoryMarshalFloats(File.ReadAllBytes(Path.Combine(folder, "portrait-mediapipe.bin")));
        var clock = Stopwatch.StartNew();
        using var segmenter = Load(BackgroundBlur.ModelPath);
        var loaded = clock.Elapsed.TotalMilliseconds;
        float[]? mask = null;
        var runs = new List<double>();
        for (var i = 0; i < 10; i++)
        {
            clock.Restart();
            mask = segmenter.Run(input);
            runs.Add(clock.Elapsed.TotalMilliseconds);
        }
        if (mask == null || mask.Length != count || expected.Length != count) return "segment check: no mask";
        double largest = 0, sum = 0;
        int both = 0, either = 0;
        for (var i = 0; i < count; i++)
        {
            var difference = Math.Abs(mask[i] - expected[i]);
            largest = Math.Max(largest, difference);
            sum += difference;
            if (mask[i] > 0.5f && expected[i] > 0.5f) both++;
            if (mask[i] > 0.5f || expected[i] > 0.5f) either++;
        }
        runs.Sort();
        return $"segment check: largest difference {largest:F6}, mean {sum / count:F8}, IoU {(double)both / Math.Max(1, either):F4}; loaded in {loaded:F0} ms, a frame in {runs[runs.Count / 2]:F1} ms (median of ten, slowest {runs.Max():F1})";

        static float[] MemoryMarshalFloats(byte[] bytes) => System.Runtime.InteropServices.MemoryMarshal.Cast<byte, float>(bytes).ToArray();
    }
}
