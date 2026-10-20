using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using uniffi.tomoshibi_core;
using Windows.Graphics.Imaging;
using Windows.Media.Capture;
using Windows.Media.Capture.Frames;
using Windows.Media.MediaProperties;

namespace Tomoshibi.Media;

/// <summary>
/// The real camera, through Windows.Media.Capture: opened at the format
/// closest to what the web sends from a computer (CameraFormats), read as
/// NV12 at the size sent, which the frame reader scales to, and pushed into
/// the core as it comes (`push_planes`, its interleaved-chroma form), no
/// faster than the web's rate (FramePacer). Only with
/// TOMOSHIBI_SOURCE=devices or in a person's own launch; the tests never
/// open it.
/// </summary>
internal sealed partial class CameraFeed : IDisposable
{
    /// <summary>What a computer sends (share.rs <c>camera_capture</c>, CameraDevice::Computer).</summary>
    public const uint WantWidth = 1280, WantHeight = 720, WantRate = 30;

    private readonly MediaCapture _capture;
    private readonly MediaFrameReader _reader;
    private readonly FramePacer _pacer = new(WantRate);
    /// <summary>Where the frames go; set when the camera is published, null while it only shows a picture of yourself.</summary>
    public volatile VideoInput? Input;
    public long FramesPushed;
    private long _arrived;

    /// <summary>The camera it runs on (a device interface path).</summary>
    public readonly string Device;
    /// <summary>The size it sends at.</summary>
    public readonly uint Width, Height;

    /// <summary>A picture of yourself, as BGRA rows, about ten times a second, on the reader's thread.</summary>
    public event Action<byte[], int, int>? Previewed;

    /// <summary>The camera stopped under it (unplugged, or taken by something else); on a capture thread.</summary>
    public event Action? Failed;

    private CameraFeed(MediaCapture capture, MediaFrameReader reader, string device, uint width, uint height, VideoInput? input, BackgroundBlur? blur)
    {
        _capture = capture;
        _reader = reader;
        Device = device;
        Width = width;
        Height = height;
        Input = input;
        Blur = blur;
        _reader.FrameArrived += Arrived;
        _capture.Failed += (_, args) =>
        {
            Log.Line($"camera failed: 0x{args.Code:x8} {args.Message}");
            Failed?.Invoke();
        };
    }

    /// <summary>
    /// Opens <paramref name="device"/> and starts reading it at the format
    /// CameraFormats chooses. Throws where it will not open: in use
    /// elsewhere, refused by the privacy settings, gone.
    /// </summary>
    public static async Task<CameraFeed> Start(string device, VideoInput? input, BackgroundBlur? blur)
    {
        var capture = new MediaCapture();
        try
        {
            await capture.InitializeAsync(new MediaCaptureInitializationSettings
            {
                VideoDeviceId = device,
                SharingMode = MediaCaptureSharingMode.ExclusiveControl,
                MemoryPreference = MediaCaptureMemoryPreference.Cpu,
                StreamingCaptureMode = StreamingCaptureMode.Video,
            });
            // The colour stream: a camera with a face-unlock sensor lists its
            // infrared stream beside it.
            var source = capture.FrameSources.Values
                .Where(s => s.Info.SourceKind == MediaFrameSourceKind.Color)
                .OrderBy(s => s.Info.MediaStreamType == Windows.Media.Capture.MediaStreamType.VideoRecord ? 0 : 1)
                .FirstOrDefault() ?? throw new InvalidOperationException("the camera has no colour stream");
            var formats = source.SupportedFormats.ToList();
            var offers = formats.Select(f => new CameraFormats.Offer(
                f.VideoFormat.Width,
                f.VideoFormat.Height,
                f.FrameRate.Denominator == 0 ? 0 : (double)f.FrameRate.Numerator / f.FrameRate.Denominator,
                f.Subtype)).ToList();
            var chosen = CameraFormats.Choose(offers, WantWidth, WantHeight, WantRate);
            if (chosen >= 0) await source.SetFormatAsync(formats[chosen]);
            var current = source.CurrentFormat.VideoFormat;
            var (width, height) = CameraFormats.Sent(current.Width, current.Height, WantWidth, WantHeight);
            // The reader converts to NV12 and scales to the size sent, so a
            // 1080p camera's frames reach the core at 720p without a copy here.
            var reader = await capture.CreateFrameReaderAsync(source, MediaEncodingSubtypes.Nv12, new BitmapSize { Width = width, Height = height });
            reader.AcquisitionMode = MediaFrameReaderAcquisitionMode.Realtime;
            var camera = new CameraFeed(capture, reader, device, width, height, input, blur);
            var started = await reader.StartAsync();
            if (started != MediaFrameReaderStartStatus.Success)
            {
                camera.Dispose();
                throw new InvalidOperationException($"the camera's reader did not start: {started}");
            }
            Log.Line($"camera open: {current.Width}x{current.Height} {(chosen >= 0 ? offers[chosen].Rate : 0):F1} fps {(chosen >= 0 ? offers[chosen].Subtype : "?")}, sending {width}x{height}");
            return camera;
        }
        catch (Exception)
        {
            capture.Dispose();
            throw;
        }
    }

    private unsafe void Arrived(MediaFrameReader reader, MediaFrameArrivedEventArgs args)
    {
        using var frame = reader.TryAcquireLatestFrame();
        var bitmap = frame?.VideoMediaFrame?.SoftwareBitmap;
        if (bitmap == null) return;
        var micros = (long)(frame!.SystemRelativeTime ?? TimeSpan.Zero).TotalMicroseconds;
        // A camera at sixty sends thirty, as the web's does.
        if (!_pacer.Take(micros))
        {
            bitmap.Dispose();
            return;
        }
        // Blurred behind the person before it is sent or shown (CameraBlur.cs),
        // after the pacing: a camera at sixty would otherwise be segmented and
        // blurred twice as often as it is sent, at about 11 ms a frame.
        if (Blur is { } blur)
        {
            Blurred(bitmap, blur, micros);
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
/// The real microphone, through WASAPI (WasapiCapture): 48 kHz mono, with
/// the system's echo cancellation, noise suppression and gain control for a
/// call's voice and none of it for original sound (VoiceCapture), pushed
/// into the core as 16-bit PCM (`push_pcm`) and measured for the meters and
/// the song desk's duck. Only with TOMOSHIBI_SOURCE=devices or in a person's
/// own launch; the tests never open it.
/// </summary>
internal sealed class MicrophoneFeed : IDisposable
{
    private readonly WasapiCapture _capture;
    private byte[] _pcm = new byte[960];

    /// <summary>Where the voice goes; null while it is only measured (the settings' check).</summary>
    public volatile AudioInput? Input;
    public long ChunksPushed;
    /// <summary>How loud the microphone has been over the last tenth of a second.</summary>
    public readonly SoundMeter Meter = new();

    /// <summary>The microphone it runs on: a device interface path, or null for the system's.</summary>
    public readonly string? Device;
    public readonly bool Original;

    /// <summary>The device went away under it; told once, on the capture's thread.</summary>
    public event Action? Lost;

    private MicrophoneFeed(string? device, bool original, AudioInput? input, Func<MicrophoneFeed, WasapiCapture> open)
    {
        Device = device;
        Original = original;
        Input = input;
        _capture = open(this);
        _capture.Lost += () => Lost?.Invoke();
    }

    /// <summary>Opens <paramref name="device"/> (null: the system's); throws where it will not open.</summary>
    public static MicrophoneFeed Start(string? device, bool original, AudioInput? input)
    {
        // Raw is asked for and the capture falls back by itself where the
        // microphone has none (WasapiCapture.Start).
        var mode = VoiceCapture.For(original, rawOffered: true);
        return new MicrophoneFeed(device, original, input, feed => WasapiCapture.Start(device, mode, feed.Heard));
    }

    /// <summary>The speakers the call plays to, for the echo canceller (null: the system's).</summary>
    public void EchoReference(string? speakers) => _capture.EchoReference(DeviceRules.EndpointId(speakers));

    private void Heard(float[] floats, int frames)
    {
        if (_pcm.Length != frames * 2) _pcm = new byte[frames * 2];
        VoiceCapture.ToPcm16(floats.AsSpan(0, frames), _pcm);
        Meter.Add(_pcm);
        if (Input is not { } input) return;
        input.PushPcm(_pcm);
        Interlocked.Increment(ref ChunksPushed);
    }

    public void Dispose() => _capture.Dispose();
}
