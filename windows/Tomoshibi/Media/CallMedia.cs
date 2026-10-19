using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Media;

/// <summary>
/// What a call is made of on this machine, for the app's life rather than
/// one call's (Android's media/CallMedia.kt): the camera, which the join
/// screens show a picture of before there is any call, the microphone, and
/// the playback a call opens.
///
/// One camera for the whole app, as on Android: the picture of yourself in
/// the lobby, on the join screen and in the call are all fed from it, and it
/// runs only while one of them is showing or a call is sending it.
/// </summary>
internal sealed class CallMedia : IDisposable
{
    public readonly Launch Launch;
    private Synthetic? _synthetic;
    private CameraFeed? _cameraFeed;
    private Task? _cameraOpening;
    private int _previewers;
    private VideoInput? _cameraInput;
    private readonly object _gate = new();

    /// <summary>A picture of yourself, as BGRA rows, on a capture thread; the buffer may be reused, so a listener copies what it keeps.</summary>
    public event Action<byte[], int, int>? Previewed;

    public CallMedia(Launch launch) => Launch = launch;

    /// <summary>The camera and the microphone last picked (DeviceMemory), or the system's.</summary>
    public string? CameraId { get; set; }
    public string? MicrophoneId { get; set; }

    /// <summary>
    /// Another camera, at once where one is running: the feed is opened on
    /// it and goes on sending to the same track, so the room sees the
    /// picture change and not a track go and come back.
    /// </summary>
    public async Task UseCamera(string? id)
    {
        CameraId = id;
        if (!RealDevices || _cameraFeed == null) return;
        if (_cameraOpening != null) await _cameraOpening;
        _cameraFeed?.Dispose();
        _cameraFeed = null;
        await EnsureCamera();
    }

    /// <summary>Plays nothing (TOMOSHIBI_MUTE): the audio graph is never opened.</summary>
    public bool Mute => Launch.Mute;

    /// <summary>The real camera and microphone, or the generated ones every test uses.</summary>
    public bool RealDevices => Launch.RealDevices;

    public Synthetic SyntheticSource
    {
        get
        {
            lock (_gate)
            {
                if (_synthetic == null)
                {
                    _synthetic = new Synthetic();
                    _synthetic.Previewed += (pixels, width, height) => Previewed?.Invoke(pixels, width, height);
                }
                return _synthetic;
            }
        }
    }

    public async Task<Playback?> OpenPlayback() => Mute ? null : await Playback.Open();

    /// <summary>A picture of yourself is shown somewhere; the camera runs while any is.</summary>
    public void StartPreview()
    {
        _previewers++;
        Log.Line($"camera preview on ({_previewers})");
        _ = EnsureCamera();
    }

    public void StopPreview()
    {
        _previewers = Math.Max(0, _previewers - 1);
        if (_previewers == 0) Log.Line("camera preview off");
        _ = SettleCamera();
    }

    /// <summary>The camera sends to this call's track as well as to the pictures of yourself.</summary>
    public async Task<bool> Publish(VideoInput input)
    {
        _cameraInput = input;
        if (!RealDevices)
        {
            SyntheticSource.Video = input;
            return true;
        }
        await EnsureCamera();
        if (_cameraFeed == null) return false;
        _cameraFeed.Input = input;
        return true;
    }

    public async Task Unpublish()
    {
        _cameraInput = null;
        if (_synthetic != null) _synthetic.Video = null;
        if (_cameraFeed != null) _cameraFeed.Input = null;
        await SettleCamera();
    }

    public (uint Width, uint Height) CameraSize => (Synthetic.Width, Synthetic.Height);

    public long FramesSent => _synthetic?.FramesPushed ?? _cameraFeed?.FramesPushed ?? 0;

    private async Task EnsureCamera()
    {
        if (!RealDevices)
        {
            SyntheticSource.Previewing = _previewers > 0;
            return;
        }
        if (_cameraFeed != null) return;
        if (_cameraOpening != null)
        {
            await _cameraOpening;
            return;
        }
        _cameraOpening = OpenCamera();
        try
        {
            await _cameraOpening;
        }
        finally
        {
            _cameraOpening = null;
        }
    }

    private async Task OpenCamera()
    {
        try
        {
            var opened = await CameraFeed.Open(1280, 720, CameraId);
            if (opened is not { } device)
            {
                Log.Line("no camera");
                return;
            }
            _cameraFeed = await CameraFeed.Start(device.Capture, device.Source, _cameraInput);
            _cameraFeed.Previewed += (pixels, width, height) => Previewed?.Invoke(pixels, width, height);
        }
        catch (Exception fault)
        {
            Log.Line($"camera failed: {fault.GetType().Name}: {fault.Message}");
        }
    }

    /// <summary>Lets the camera go once nothing shows it and no call sends it.</summary>
    private async Task SettleCamera()
    {
        if (!RealDevices)
        {
            if (_synthetic != null) _synthetic.Previewing = _previewers > 0;
            return;
        }
        if (_cameraOpening != null) await _cameraOpening;
        if (_previewers > 0 || _cameraInput != null || _cameraFeed == null) return;
        _cameraFeed.Dispose();
        _cameraFeed = null;
        Log.Line("camera closed");
    }

    public void Dispose()
    {
        _synthetic?.Dispose();
        _cameraFeed?.Dispose();
    }
}
