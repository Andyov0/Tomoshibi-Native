using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using uniffi.tomoshibi_core;
using Windows.Graphics.Imaging;
using Windows.Media;
using Windows.Media.Audio;
using Windows.Media.Capture;
using Windows.Media.Capture.Frames;
using Windows.Media.MediaProperties;
using Windows.Media.Render;

namespace Tomoshibi.Media;

/// <summary>
/// The real camera, through Windows.Media.Capture: frames read as NV12 in
/// memory and pushed into the core as they come (`push_planes`, its
/// interleaved-chroma form). Only with TOMOSHIBI_SOURCE=devices in this
/// phase; the tests never open it.
/// </summary>
internal sealed partial class CameraFeed : IDisposable
{
    private readonly MediaCapture _capture;
    private readonly MediaFrameReader _reader;
    /// <summary>Where the frames go; set when the camera is published, null while it only shows a picture of yourself.</summary>
    public volatile VideoInput? Input;
    public long FramesPushed;
    private long _arrived;

    /// <summary>A picture of yourself, as BGRA rows, about ten times a second, on the reader's thread.</summary>
    public event Action<byte[], int, int>? Previewed;

    private CameraFeed(MediaCapture capture, MediaFrameReader reader, VideoInput? input)
    {
        _capture = capture;
        _reader = reader;
        Input = input;
        _reader.FrameArrived += Arrived;
    }

    /// <summary>
    /// The camera last picked where it is still there, otherwise the first
    /// colour camera, at the size closest to the one asked for; null when
    /// there is none.
    /// </summary>
    public static async Task<(MediaCapture Capture, MediaFrameSource Source, uint Width, uint Height)?> Open(uint width, uint height, string? preferred = null)
    {
        var groups = (await MediaFrameSourceGroup.FindAllAsync())
            .OrderByDescending(g => preferred != null && g.SourceInfos.Any(s => s.DeviceInformation?.Id == preferred))
            .ToList();
        foreach (var group in groups)
        {
            var info = group.SourceInfos.FirstOrDefault(s => s.SourceKind == MediaFrameSourceKind.Color);
            if (info == null) continue;
            var capture = new MediaCapture();
            await capture.InitializeAsync(new MediaCaptureInitializationSettings
            {
                SourceGroup = group,
                SharingMode = MediaCaptureSharingMode.ExclusiveControl,
                MemoryPreference = MediaCaptureMemoryPreference.Cpu,
                StreamingCaptureMode = StreamingCaptureMode.Video,
            });
            var source = capture.FrameSources[info.Id];
            var format = source.SupportedFormats
                .Where(f => f.VideoFormat.Width > 0)
                .OrderBy(f => Math.Abs((long)f.VideoFormat.Width * f.VideoFormat.Height - (long)width * height))
                .ThenByDescending(f => f.FrameRate.Denominator == 0 ? 0 : (double)f.FrameRate.Numerator / f.FrameRate.Denominator)
                .FirstOrDefault();
            if (format != null) await source.SetFormatAsync(format);
            return (capture, source, source.CurrentFormat.VideoFormat.Width, source.CurrentFormat.VideoFormat.Height);
        }
        return null;
    }

    public static async Task<CameraFeed> Start(MediaCapture capture, MediaFrameSource source, VideoInput? input)
    {
        var reader = await capture.CreateFrameReaderAsync(source, MediaEncodingSubtypes.Nv12);
        reader.AcquisitionMode = MediaFrameReaderAcquisitionMode.Realtime;
        var camera = new CameraFeed(capture, reader, input);
        var started = await reader.StartAsync();
        Log.Line($"camera reader: {started}");
        return camera;
    }

    private unsafe void Arrived(MediaFrameReader reader, MediaFrameArrivedEventArgs args)
    {
        using var frame = reader.TryAcquireLatestFrame();
        var bitmap = frame?.VideoMediaFrame?.SoftwareBitmap;
        if (bitmap == null) return;
        // Blurred behind the person before it is sent or shown (CameraBlur.cs).
        if (Blur is { } blur)
        {
            Blurred(bitmap, blur, frame!.SystemRelativeTime);
            bitmap.Dispose();
            return;
        }
        if (Previewed is { } previewed && Interlocked.Increment(ref _arrived) % 3 == 0)
        {
            using var bgra = SoftwareBitmap.Convert(bitmap, BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied);
            var pixels = new byte[bgra.PixelWidth * bgra.PixelHeight * 4];
            bgra.CopyToBuffer(System.Runtime.InteropServices.WindowsRuntime.WindowsRuntimeBufferExtensions.AsBuffer(pixels));
            previewed(pixels, bgra.PixelWidth, bgra.PixelHeight);
        }
        if (Input is not { } input)
        {
            bitmap.Dispose();
            return;
        }
        using var nv12 = bitmap.BitmapPixelFormat == BitmapPixelFormat.Nv12 ? null : SoftwareBitmap.Convert(bitmap, BitmapPixelFormat.Nv12);
        var picture = nv12 ?? bitmap;
        using var buffer = picture.LockBuffer(BitmapBufferAccessMode.Read);
        var luma = buffer.GetPlaneDescription(0);
        var chroma = buffer.GetPlaneDescription(1);
        using var reference = buffer.CreateReference();
        var (data, _) = ComCalls.Bytes(reference);
        var y = new ReadOnlySpan<byte>((byte*)data + luma.StartIndex, luma.Stride * luma.Height).ToArray();
        var uv = new ReadOnlySpan<byte>((byte*)data + chroma.StartIndex, chroma.Stride * chroma.Height).ToArray();
        // NV12's chroma is one plane of U and V side by side: U starts at
        // its first byte and V at its second, each a step of two from the next.
        var v = uv.AsSpan(1).ToArray();
        var micros = (long)(frame!.SystemRelativeTime ?? TimeSpan.Zero).TotalMicroseconds;
        input.PushPlanes(y, uv, v, (uint)luma.Stride, (uint)chroma.Stride, 2, (uint)picture.PixelWidth, (uint)picture.PixelHeight, micros, 0);
        Interlocked.Increment(ref FramesPushed);
        bitmap.Dispose();
    }

    public void Dispose()
    {
        _reader.FrameArrived -= Arrived;
        _ = _reader.StopAsync();
        _reader.Dispose();
        _capture.Dispose();
    }
}

/// <summary>
/// The real microphone, through an audio graph: each quantum's samples read
/// out of a frame output node as 48 kHz mono floats and pushed into the core
/// as 16-bit PCM (`push_pcm`). Only with TOMOSHIBI_SOURCE=devices in this
/// phase; the tests never open it.
/// </summary>
internal sealed class MicrophoneFeed : IDisposable
{
    private readonly AudioGraph _graph;
    private readonly AudioFrameOutputNode _output;
    private readonly AudioInput _input;
    public long ChunksPushed;
    /// <summary>How loud the microphone has been over the last tenth of a second, for the song desk's duck.</summary>
    public readonly SoundMeter Meter = new();

    private MicrophoneFeed(AudioGraph graph, AudioFrameOutputNode output, AudioInput input)
    {
        _graph = graph;
        _output = output;
        _input = input;
        _graph.QuantumStarted += Quantum;
    }

    public static async Task<MicrophoneFeed?> Start(AudioInput input, string? deviceId = null)
    {
        var settings = new AudioGraphSettings(AudioRenderCategory.Communications)
        {
            QuantumSizeSelectionMode = QuantumSizeSelectionMode.LowestLatency,
        };
        var format = AudioEncodingProperties.CreatePcm(48_000, 1, 32);
        format.Subtype = MediaEncodingSubtypes.Float;
        settings.EncodingProperties = format;
        var made = await AudioGraph.CreateAsync(settings);
        if (made.Status != AudioGraphCreationStatus.Success)
        {
            Log.Line($"no audio graph for the microphone: {made.Status}");
            return null;
        }
        // The microphone last picked where it is still there; the system's otherwise.
        Windows.Devices.Enumeration.DeviceInformation? chosen = null;
        if (deviceId != null)
        {
            try
            {
                chosen = await Windows.Devices.Enumeration.DeviceInformation.CreateFromIdAsync(deviceId);
            }
            catch (Exception)
            {
                chosen = null;
            }
        }
        var device = chosen != null
            ? await made.Graph.CreateDeviceInputNodeAsync(MediaCategory.Communications, format, chosen)
            : await made.Graph.CreateDeviceInputNodeAsync(MediaCategory.Communications, format);
        if (device.Status != AudioDeviceNodeCreationStatus.Success)
        {
            Log.Line($"no microphone: {device.Status}");
            made.Graph.Dispose();
            return null;
        }
        var output = made.Graph.CreateFrameOutputNode(format);
        device.DeviceInputNode.AddOutgoingConnection(output);
        var microphone = new MicrophoneFeed(made.Graph, output, input);
        made.Graph.Start();
        return microphone;
    }

    private unsafe void Quantum(AudioGraph graph, object args)
    {
        using var frame = _output.GetFrame();
        using var buffer = frame.LockBuffer(AudioBufferAccessMode.Read);
        using var reference = buffer.CreateReference();
        var (data, _) = ComCalls.Bytes(reference);
        var samples = (int)(buffer.Length / sizeof(float));
        if (samples == 0) return;
        var floats = (float*)data;
        var pcm = new byte[samples * 2];
        for (var i = 0; i < samples; i++)
        {
            var sample = (short)Math.Clamp(floats[i] * 32767f, short.MinValue, short.MaxValue);
            pcm[2 * i] = (byte)sample;
            pcm[2 * i + 1] = (byte)(sample >> 8);
        }
        _input.PushPcm(pcm);
        Meter.Add(pcm);
        Interlocked.Increment(ref ChunksPushed);
    }

    public void Dispose()
    {
        _graph.QuantumStarted -= Quantum;
        _graph.Stop();
        _graph.Dispose();
    }
}
