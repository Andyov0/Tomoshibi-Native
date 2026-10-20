using System.Diagnostics;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;
using Windows.Graphics.Imaging;
using Windows.Storage;

namespace Tomoshibi.Media;

/// <summary>
/// A camera and a microphone that are not there: a moving picture and a
/// steady tone, pushed into the core exactly as captured ones would be
/// (`push_planes` and `push_pcm`). What the tests publish, so that nothing
/// in a test ever captures a real room, and what a machine without devices
/// can still send.
///
/// The tone is sent, never played: the browser that receives it runs muted.
/// </summary>
internal sealed class Synthetic : IDisposable
{
    public const uint Width = 1280;
    public const uint Height = 720;
    private const int FramesPerSecond = 30;
    private const int Rate = 48_000;
    /// <summary>Ten milliseconds, the core's audio frame.</summary>
    private const int Chunk = Rate / 100;
    /// <summary>A frequency a spectrum finds at once, and nobody's voice.</summary>
    private const double Tone = 440.0;

    private readonly Thread _thread;
    private volatile bool _stopped;

    /// <summary>Where the picture goes, or nowhere; set and cleared as the camera is published and unpublished.</summary>
    public volatile VideoInput? Video;
    /// <summary>Where the tone goes, or nowhere.</summary>
    public volatile AudioInput? Audio;

    public long FramesPushed;
    public long ChunksPushed;
    /// <summary>How loud the generated tone has been over the last tenth of a second, as the microphone's would be.</summary>
    public readonly SoundMeter Meter = new();

    /// <summary>The background blur the picture goes through before it is sent and shown, or none.</summary>
    public volatile BackgroundBlur? Blur;

    /// <summary>A picture of a person laid over the bands (Launch.CameraPicture), read on the generator's thread.</summary>
    private readonly string? _picture;
    private (byte[] Y, byte[] U, byte[] V, int Width)? _person;

    public Synthetic(string? picture = null)
    {
        _picture = picture;
        _thread = new Thread(Run) { IsBackground = true, Name = "synthetic media", Priority = ThreadPriority.AboveNormal };
        _thread.Start();
    }

    /// <summary>
    /// The picture, the frame's height and as wide as it keeps its shape, in
    /// planes, once: laid over each frame by copying rows, which costs
    /// nothing beside the bands.
    /// </summary>
    private void LoadPicture()
    {
        if (_picture == null) return;
        try
        {
            var file = StorageFile.GetFileFromPathAsync(_picture).AsTask().GetAwaiter().GetResult();
            using var stream = file.OpenReadAsync().AsTask().GetAwaiter().GetResult();
            var decoder = BitmapDecoder.CreateAsync(stream).AsTask().GetAwaiter().GetResult();
            var width = Math.Min(Width, (uint)(decoder.PixelWidth * Height / decoder.PixelHeight) / 2 * 2);
            var transform = new BitmapTransform { ScaledWidth = width, ScaledHeight = Height, InterpolationMode = BitmapInterpolationMode.Fant };
            var pixels = decoder.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Ignore, transform,
                ExifOrientationMode.RespectExifOrientation, ColorManagementMode.DoNotColorManage).AsTask().GetAwaiter().GetResult();
            var (y, u, v) = Planes.FromBgra(pixels.DetachPixelData(), (int)width, (int)Height);
            _person = (y, u, v, (int)width);
            Log.Line($"synthetic camera: a picture {width}x{Height}");
        }
        catch (Exception fault)
        {
            Log.Line($"synthetic camera: the picture could not be read: {fault.GetType().Name}: {fault.Message}");
        }
    }

    /// <summary>The picture over the bands, drifting from side to side so that a mask has something to follow.</summary>
    private void LayPicture(byte[] y, byte[] u, byte[] v, long frame)
    {
        if (_person is not { } person) return;
        var room = (int)Width - person.Width;
        var drift = (int)(room / 2 + room / 3 * Math.Sin(frame * 2 * Math.PI / (FramesPerSecond * 8))) / 2 * 2;
        for (var row = 0; row < Height; row++)
        {
            Array.Copy(person.Y, row * person.Width, y, row * (int)Width + drift, person.Width);
        }
        var cw = person.Width / 2;
        for (var row = 0; row < Height / 2; row++)
        {
            Array.Copy(person.U, row * cw, u, row * (int)(Width / 2) + drift / 2, cw);
            Array.Copy(person.V, row * cw, v, row * (int)(Width / 2) + drift / 2, cw);
        }
    }

    private void Run()
    {
        var clock = Stopwatch.StartNew();
        long frames = 0, chunks = 0;
        var pcm = new byte[Chunk * 2];
        var (y, u, v) = (new byte[Width * Height], new byte[Width * Height / 4], new byte[Width * Height / 4]);
        var preview = new byte[PreviewWidth * PreviewHeight * 4];
        double phase = 0;
        LoadPicture();
        while (!_stopped)
        {
            try
            {
                var now = clock.Elapsed.TotalMilliseconds;
                // Everything that is due, so a late wake-up catches up rather
                // than sending less than real time.
                for (; chunks * 10 <= now; chunks++)
                {
                    if (Audio is not { } audio) continue;
                    for (var i = 0; i < Chunk; i++)
                    {
                        // A tenth of full scale: -20 dBFS.
                        var sample = (short)(Math.Sin(phase) * 3277);
                        phase += 2 * Math.PI * Tone / Rate;
                        pcm[2 * i] = (byte)sample;
                        pcm[2 * i + 1] = (byte)(sample >> 8);
                    }
                    phase %= 2 * Math.PI;
                    audio.PushPcm(pcm);
                    Meter.Add(pcm);
                    Interlocked.Increment(ref ChunksPushed);
                }
                if (frames * 1000.0 / FramesPerSecond <= now)
                {
                    // Half the rate is enough for a picture of yourself, and
                    // it is the same picture the call is sent, so that what a
                    // person sees of themselves is what the room sees: drawn
                    // from the frame itself where the frame is more than the
                    // bands (a person laid over them, a blur), which a
                    // picture painted on its own would not show.
                    var previewing = frames % 2 == 0 && Previewing && Previewed != null;
                    var blur = Blur;
                    var fromFrame = blur != null || _person != null;
                    if (Video != null || (previewing && fromFrame))
                    {
                        Paint(y, u, v, frames);
                        LayPicture(y, u, v, frames);
                        blur?.Process(y, u, v, (int)Width, (int)Height);
                        if (Video is { } video)
                        {
                            video.PushPlanes(y, u, v, Width, Width / 2, 1, Width, Height, (long)(now * 1000), 0);
                            Interlocked.Increment(ref FramesPushed);
                        }
                    }
                    if (previewing && Previewed is { } previewed)
                    {
                        if (fromFrame) Planes.ToBgra(y, u, v, (int)Width, (int)Height, (int)(Width / PreviewWidth), preview);
                        else PaintPreview(preview, frames);
                        previewed(preview, PreviewWidth, PreviewHeight);
                    }
                    frames++;
                }
            }
            catch (Exception fault)
            {
                // An input unpublished between the check and the push.
                Log.Line($"synthetic media: {fault.GetType().Name}: {fault.Message}");
            }
            Thread.Sleep(2);
        }
    }

    /// <summary>
    /// Bands of light that slide down the picture and a bright bar that
    /// crosses it every four seconds, on a ground whose hue turns: anything
    /// that freezes or tears shows at once.
    /// </summary>
    private static void Paint(byte[] y, byte[] u, byte[] v, long frame)
    {
        var bar = (int)(frame * Width / (FramesPerSecond * 4) % Width);
        for (var row = 0; row < Height; row++)
        {
            var line = y.AsSpan(row * (int)Width, (int)Width);
            line.Fill((byte)(48 + (row + frame * 2) % 160));
            line.Slice(bar, Math.Min(96, (int)Width - bar)).Fill(235);
        }
        var turn = frame * 2 * Math.PI / (FramesPerSecond * 10);
        u.AsSpan().Fill((byte)(128 + 60 * Math.Cos(turn)));
        v.AsSpan().Fill((byte)(128 + 60 * Math.Sin(turn)));
    }

    /// <summary>The picture of yourself is drawn this size, a quarter of the one sent in each direction.</summary>
    public const int PreviewWidth = 320;
    public const int PreviewHeight = 180;

    /// <summary>
    /// Hears a picture of what is sent, as BGRA rows of PreviewWidth by
    /// PreviewHeight, on the generator's thread; the buffer is reused, so a
    /// listener copies what it keeps.
    /// </summary>
    public event Action<byte[], int, int>? Previewed;

    /// <summary>Whether a picture of yourself is showing anywhere, so that one is painted.</summary>
    public volatile bool Previewing;

    /// <summary>
    /// The same picture as <see cref="Paint"/>, at a quarter of the size and
    /// in colour: the bands are the luma, the ground's hue the chroma, which
    /// is uniform, so each row is one colour and the bar a brighter one.
    /// </summary>
    private static void PaintPreview(byte[] bgra, long frame)
    {
        var turn = frame * 2 * Math.PI / (FramesPerSecond * 10);
        var cb = 60 * Math.Cos(turn);
        var cr = 60 * Math.Sin(turn);
        var bar = (int)(frame * Width / (FramesPerSecond * 4) % Width) / 4;
        for (var row = 0; row < PreviewHeight; row++)
        {
            var luma = 48 + (row * 4 + frame * 2) % 160;
            var (r, g, b) = Rgb(luma, cb, cr);
            var (br, bg, bb) = Rgb(235, cb, cr);
            var line = row * PreviewWidth * 4;
            for (var x = 0; x < PreviewWidth; x++)
            {
                var lit = x >= bar && x < bar + 24;
                var at = line + x * 4;
                bgra[at] = lit ? bb : b;
                bgra[at + 1] = lit ? bg : g;
                bgra[at + 2] = lit ? br : r;
                bgra[at + 3] = 255;
            }
        }
    }

    /// <summary>BT.601 from a luma and two chroma offsets about 128.</summary>
    private static (byte R, byte G, byte B) Rgb(double y, double cb, double cr)
    {
        static byte Clamp(double value) => (byte)Math.Clamp((int)Math.Round(value), 0, 255);
        return (Clamp(y + 1.402 * cr), Clamp(y - 0.344 * cb - 0.714 * cr), Clamp(y + 1.772 * cb));
    }

    public void Dispose()
    {
        _stopped = true;
        _thread.Join(500);
    }
}
