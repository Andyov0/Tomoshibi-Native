using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Tomoshibi.Media;

namespace Tomoshibi.UI;

/// <summary>
/// The picture of yourself (Android's ui/lobby/SelfView.kt): what the camera
/// sends, mirrored as a mirror shows a face, cut to fill its place. Fed from
/// the app's one camera (CallMedia), which runs while any of these is on the
/// screen; a test's camera is the generated picture, so a test shows the
/// very picture the room receives.
/// </summary>
internal sealed class SelfView : Grid
{
    private readonly CallMedia _media;
    private readonly Image _image = new() { Stretch = Stretch.UniformToFill };
    private readonly DispatcherQueue _ui;
    private WriteableBitmap? _bitmap;
    private byte[]? _pending;
    private int _pendingWidth, _pendingHeight;
    private bool _queued;
    private bool _first = true;
    private readonly object _gate = new();
    private bool _running;

    public SelfView(CallMedia media)
    {
        _media = media;
        _ui = DispatcherQueue.GetForCurrentThread();
        IsHitTestVisible = false;
        // Mirrored about its middle.
        _image.RenderTransformOrigin = new Windows.Foundation.Point(0.5, 0.5);
        _image.RenderTransform = new ScaleTransform { ScaleX = -1 };
        Motion.Hide(_image);
        Children.Add(_image);
        Loaded += (_, _) => Start();
        Unloaded += (_, _) => Stop();
    }

    private void Start()
    {
        if (_running) return;
        _running = true;
        _media.Previewed += Arrived;
        _media.StartPreview();
    }

    private void Stop()
    {
        if (!_running) return;
        _running = false;
        _media.Previewed -= Arrived;
        _media.StopPreview();
    }

    /// <summary>On a capture thread: the picture is copied and drawn on the UI thread, the newest only.</summary>
    private void Arrived(byte[] pixels, int width, int height)
    {
        lock (_gate)
        {
            if (_pending == null || _pending.Length != pixels.Length) _pending = new byte[pixels.Length];
            Buffer.BlockCopy(pixels, 0, _pending, 0, pixels.Length);
            _pendingWidth = width;
            _pendingHeight = height;
            if (_queued) return;
            _queued = true;
        }
        _ui.TryEnqueue(Draw);
    }

    private void Draw()
    {
        byte[] pixels;
        int width, height;
        lock (_gate)
        {
            _queued = false;
            if (_pending == null || !_running) return;
            pixels = _pending;
            width = _pendingWidth;
            height = _pendingHeight;
        }
        if (_bitmap == null || _bitmap.PixelWidth != width || _bitmap.PixelHeight != height)
        {
            _bitmap = new WriteableBitmap(width, height);
            _image.Source = _bitmap;
        }
        using (var stream = _bitmap.PixelBuffer.AsStream())
        {
            lock (_gate) stream.Write(pixels, 0, Math.Min(pixels.Length, width * height * 4));
        }
        _bitmap.Invalidate();
        if (_first)
        {
            // Faded in once there is something to show, as the camera's
            // picture arrives on the other apps.
            _first = false;
            Motion.FadeTo(_image, 1);
        }
    }
}
