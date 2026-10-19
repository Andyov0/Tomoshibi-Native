using System.Diagnostics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using Windows.Graphics;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;

namespace Tomoshibi.Media;

/// <summary>
/// What a screen share sends its picture from: the system's capture of a
/// screen or a window, or, in a test, a generated screen. Either is fed into
/// the share's video input at the share's rate, a still screen sent again
/// twice a second (ScreenCapture.kt: a key frame is only ever made from a
/// frame that comes, and a person who arrives while nothing moves would get
/// a black tile).
/// </summary>
internal abstract class ScreenSource : IDisposable
{
    /// <summary>A still screen is sent again this often (ScreenCapture.kt <c>STILL_NANOS</c>).</summary>
    protected static readonly TimeSpan Still = TimeSpan.FromMilliseconds(500);

    private readonly Thread _pump;
    private volatile bool _stopped;
    private volatile VideoInput? _video;
    private int _fps = 30;
    private (int Width, int Height) _out;
    private readonly object _gate = new();
    public long FramesPushed;

    /// <summary>The captured thing went away (its window closed, the screen came off): the share stops.</summary>
    public event Action? Ended;

    protected ScreenSource(string name)
    {
        _pump = new Thread(Pump) { IsBackground = true, Name = name, Priority = ThreadPriority.AboveNormal };
    }

    /// <summary>The picture's size as captured, which the share's size is fitted to.</summary>
    public abstract (int Width, int Height) Content { get; }

    protected void Begin() => _pump.Start();

    /// <summary>Sends to this input from now on, at this rate and size: the share's start, or a change of its size and rate.</summary>
    public void Feed(VideoInput? video, int fps, (int Width, int Height) size)
    {
        lock (_gate)
        {
            _video = video;
            _fps = Math.Max(1, fps);
            _out = size;
        }
    }

    protected void End() => Ended?.Invoke();

    /// <summary>Whether something new has been captured since the last send.</summary>
    protected abstract bool Fresh { get; }

    /// <summary>Sends the newest picture, or the last one again, into the input at the size given.</summary>
    protected abstract bool Send(VideoInput video, (int Width, int Height) size, long micros, bool again);

    private void Pump()
    {
        var clock = Stopwatch.StartNew();
        var due = 0.0;
        var lastSent = -Still.TotalMilliseconds;
        while (!_stopped)
        {
            VideoInput? video;
            int fps;
            (int, int) size;
            lock (_gate)
            {
                video = _video;
                fps = _fps;
                size = _out;
            }
            var interval = 1000.0 / fps;
            var now = clock.Elapsed.TotalMilliseconds;
            if (video != null && now >= due)
            {
                // Taken no faster than the share's rate, on a deadline that
                // moves on by the interval; one woken late does not try to
                // catch up with frames that are gone.
                due = now - due > interval ? now + interval : due + interval;
                var fresh = Fresh;
                if (fresh || now - lastSent >= Still.TotalMilliseconds)
                {
                    try
                    {
                        if (Send(video, size, (long)(now * 1000), again: !fresh))
                        {
                            lastSent = now;
                            if (Interlocked.Increment(ref FramesPushed) % 300 == 0) Log.Line($"screen sent {FramesPushed} frames at {size}");
                        }
                    }
                    catch (Exception fault)
                    {
                        // An input unpublished between the look and the push.
                        Log.Line($"screen frame not sent: {fault.GetType().Name}: {fault.Message}");
                    }
                }
            }
            Thread.Sleep(Math.Clamp((int)(due - clock.Elapsed.TotalMilliseconds), 1, 20));
        }
    }

    public virtual void Dispose()
    {
        _stopped = true;
        _video = null;
        if (_pump.IsAlive) _pump.Join(500);
    }

    /// <summary>
    /// The size a share sends for content of this size (ScreenCapture.kt
    /// <c>fit</c>): the content's own shape, its shorter side brought down to
    /// what the setting asks for (the protocol crate's capture request), on
    /// even numbers, as an encoder wants them.
    /// </summary>
    public static (int Width, int Height) Fit((int Width, int Height) content, ShareQuality quality, int fps)
    {
        var ask = TomoshibiProtocolMethods.ShareCaptureRequest((uint)fps, quality);
        double limit = Math.Min(ask.Width, ask.Height);
        double width = content.Width, height = content.Height;
        var shorter = Math.Min(width, height);
        if (shorter > limit)
        {
            width = width * limit / shorter;
            height = height * limit / shorter;
        }
        return (Math.Max(2, (int)width & ~1), Math.Max(2, (int)height & ~1));
    }
}

/// <summary>
/// A screen or a window, through Windows.Graphics.Capture: the system's
/// picker asks the person which, as a browser's does, and every frame the
/// system hands over is copied on the GPU into a texture kept for reading
/// back; the share's pump reads the newest one at the share's rate and hands
/// its rows to the core (<c>push_bgra</c>), which converts and scales them
/// with libyuv.
///
/// The yellow border the system draws round what is captured stays: it says
/// to the person sharing, and to whoever is looking over their shoulder,
/// what everybody else can see.
/// </summary>
internal sealed class WindowsScreen : ScreenSource
{
    private readonly GraphicsCaptureItem _item;
    private readonly Direct3D _gpu;
    private readonly Direct3D11CaptureFramePool _pool;
    private readonly GraphicsCaptureSession _session;
    private SizeInt32 _size;
    private byte[] _rows = Array.Empty<byte>();
    private (int Width, int Height) _read;
    private volatile bool _fresh;
    private bool _gone;

    private WindowsScreen(GraphicsCaptureItem item, Direct3D gpu) : base("screen capture")
    {
        _item = item;
        _gpu = gpu;
        _size = item.Size;
        _pool = Direct3D11CaptureFramePool.CreateFreeThreaded(gpu.Device, DirectXPixelFormat.B8G8R8A8UIntNormalized, 2, _size);
        _pool.FrameArrived += Arrived;
        _session = _pool.CreateCaptureSession(item);
        _item.Closed += (_, _) =>
        {
            Log.Line("the captured screen or window went away");
            End();
        };
        _session.StartCapture();
        Begin();
    }

    public override (int Width, int Height) Content => (_size.Width, _size.Height);

    /// <summary>Whether the system can capture at all here (Windows 10 1903 and later, and not every session).</summary>
    public static bool Supported => GraphicsCaptureSession.IsSupported();

    /// <summary>The system's picker of a screen or a window, over the app's window; nothing when the person cancels it.</summary>
    public static async Task<GraphicsCaptureItem?> Pick(IntPtr window)
    {
        var picker = new GraphicsCapturePicker();
        WinRT.Interop.InitializeWithWindow.Initialize(picker, window);
        return await picker.PickSingleItemAsync();
    }

    /// <summary>The whole of the first screen, without asking: a debug build's test of the capture with nobody to answer the picker.</summary>
    public static GraphicsCaptureItem? FirstScreen()
    {
        var area = Microsoft.UI.Windowing.DisplayArea.Primary;
        return GraphicsCaptureItem.TryCreateFromDisplayId(new DisplayId(area.DisplayId.Value));
    }

    public static WindowsScreen? Start(GraphicsCaptureItem item)
    {
        var gpu = Direct3D.Make();
        if (gpu == null)
        {
            Log.Line("no Direct3D device for the screen capture");
            return null;
        }
        try
        {
            return new WindowsScreen(item, gpu);
        }
        catch (Exception)
        {
            gpu.Dispose();
            throw;
        }
    }

    private void Arrived(Direct3D11CaptureFramePool pool, object _)
    {
        using var frame = pool.TryGetNextFrame();
        if (frame == null || _gone) return;
        // A window made larger or smaller: the pool is made again at its new
        // size, and the share is sent at the shape it now is.
        if (frame.ContentSize.Width != _size.Width || frame.ContentSize.Height != _size.Height)
        {
            _size = frame.ContentSize;
            pool.Recreate(_gpu.Device, DirectXPixelFormat.B8G8R8A8UIntNormalized, 2, _size);
            return;
        }
        var texture = Direct3D.TextureOf(frame.Surface);
        try
        {
            if (_gpu.Stage(texture)) _fresh = true;
        }
        finally
        {
            System.Runtime.InteropServices.Marshal.Release(texture);
        }
    }

    protected override bool Fresh => _fresh;

    protected override bool Send(VideoInput video, (int Width, int Height) size, long micros, bool again)
    {
        if (!again || _read.Width == 0)
        {
            _fresh = false;
            if (_gpu.Read(ref _rows) is not { } read) return false;
            _read = read;
        }
        var (width, height) = _read;
        var (outWidth, outHeight) = Fitted(size, _read);
        video.PushBgra(_rows, (uint)(width * 4), (uint)width, (uint)height, (uint)outWidth, (uint)outHeight, micros);
        return true;
    }

    /// <summary>
    /// The size asked for, kept to the shape of what is captured now: a
    /// window dragged to another shape is sent at its new shape, never
    /// stretched to the old one, within the same shorter side.
    /// </summary>
    private static (int Width, int Height) Fitted((int Width, int Height) asked, (int Width, int Height) content)
    {
        if (asked.Width <= 0 || asked.Height <= 0) return content;
        var limit = Math.Min(asked.Width, asked.Height);
        var shorter = Math.Min(content.Width, content.Height);
        if (shorter <= limit) return (content.Width & ~1, content.Height & ~1);
        return ((int)((long)content.Width * limit / shorter) & ~1, (int)((long)content.Height * limit / shorter) & ~1);
    }

    public override void Dispose()
    {
        _gone = true;
        base.Dispose();
        _pool.FrameArrived -= Arrived;
        _session.Dispose();
        _pool.Dispose();
        _gpu.Dispose();
    }
}

/// <summary>
/// A screen that is not there: a page of lines being written, a pointer that
/// wanders over it and a clock in the corner, as I420 at the share's size.
/// What a test shares (TOMOSHIBI_SOURCE=synthetic), so that no test ever
/// sends a picture of whoever's desktop the virtual machine shows; and
/// unlike a camera's, mostly still, as a shared document is, with a change
/// a few times a second.
/// </summary>
internal sealed class SyntheticScreen : ScreenSource
{
    public const int Width = 1920;
    public const int Height = 1080;
    private long _drawn;
    private readonly Stopwatch _clock = Stopwatch.StartNew();
    private byte[] _y = Array.Empty<byte>(), _u = Array.Empty<byte>(), _v = Array.Empty<byte>();
    private (int Width, int Height) _painted;

    public SyntheticScreen() : base("synthetic screen") => Begin();

    public override (int Width, int Height) Content => (Width, Height);

    /// <summary>Something changes eight times a second; between, the screen is still.</summary>
    protected override bool Fresh => _clock.ElapsedMilliseconds / 125 != _drawn;

    protected override bool Send(VideoInput video, (int Width, int Height) size, long micros, bool again)
    {
        var (width, height) = size.Width > 0 ? size : (Width, Height);
        var step = _clock.ElapsedMilliseconds / 125;
        if (!again || _painted != (width, height))
        {
            Paint(width, height, step);
            _drawn = step;
            _painted = (width, height);
        }
        video.PushPlanes(_y, _u, _v, (uint)width, (uint)(width / 2), 1, (uint)width, (uint)height, micros, 0);
        return true;
    }

    private void Paint(int width, int height, long step)
    {
        if (_y.Length != width * height)
        {
            _y = new byte[width * height];
            _u = new byte[width * height / 4];
            _v = new byte[width * height / 4];
            _u.AsSpan().Fill(128);
            _v.AsSpan().Fill(128);
        }
        // A light page, and lines of "text" written one more word at a time.
        _y.AsSpan().Fill(225);
        var line = Math.Max(4, height / 40);
        var written = (int)(step % 400);
        for (var row = 0; row < 30; row++)
        {
            var top = height / 12 + row * line * 3 / 2;
            if (top + line >= height) break;
            var words = Math.Min(12, Math.Max(0, written - row * 12));
            for (var word = 0; word < words; word++)
            {
                var left = width / 12 + word * width / 15;
                var length = width / 22 + (row * 7 + word * 13) % (width / 40 + 1);
                for (var y = top; y < top + line / 2; y++) _y.AsSpan(y * width + left, Math.Min(length, width - left)).Fill(40);
            }
        }
        // The pointer, wandering.
        var px = (int)(width / 2 + width / 3 * Math.Sin(step / 9.0));
        var py = (int)(height / 2 + height / 3 * Math.Cos(step / 13.0));
        for (var y = Math.Max(0, py); y < Math.Min(height, py + height / 30); y++)
        {
            var span = Math.Min(width - px, (y - py) / 2 + 2);
            if (px >= 0 && span > 0) _y.AsSpan(y * width + px, span).Fill(16);
        }
    }
}
