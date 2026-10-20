using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Media;

/// <summary>
/// What a call is made of on this machine, for the app's life rather than
/// one call's (Android's media/CallMedia.kt): the camera, which the join
/// screens show a picture of before there is any call, the microphone, the
/// lists of devices, and the playback a call opens.
///
/// One camera for the whole app, as on Android: the picture of yourself in
/// the lobby, on the join screen and in the call are all fed from it, and it
/// runs only while one of them is showing or a call is sending it. One
/// microphone likewise, open while a call sends it or the settings measure it.
///
/// Where they come from is one of three: the real devices (a person's own
/// launch, or TOMOSHIBI_SOURCE=devices); the generated picture and tone of
/// every test; or a test's made-up devices (TOMOSHIBI_DEVICES), listed,
/// picked, switched and unplugged like real ones, each made-up camera a
/// picture of its own hue and each microphone a tone of its own, so that a
/// browser in the room can tell which is being sent and nothing real is
/// opened.
///
/// A device unplugged under a call is followed as the web's SDK follows it
/// (DeviceRules.Follow): the microphone moves to the system's, the camera to
/// the first still there, on the same track; when nothing of the kind is
/// left, the call is told (CameraLost, MicrophoneLost) and says so.
/// </summary>
internal sealed class CallMedia : IDisposable
{
    public readonly Launch Launch;
    private Synthetic? _synthetic;
    private CameraFeed? _cameraFeed;
    /// <summary>The made-up camera the generated picture stands for, while it runs.</summary>
    private string? _scriptedCamera;
    private Task? _cameraOpening;
    private int _previewers;
    private VideoInput? _cameraInput;
    private readonly object _gate = new();
    private readonly DispatcherQueue? _ui;
    private readonly ScriptedDevices? _scripted;

    /// <summary>A picture of yourself, as BGRA rows, on a capture thread; the buffer may be reused, so a listener copies what it keeps.</summary>
    public event Action<byte[], int, int>? Previewed;

    /// <summary>The camera a call sends went away and no other would open; on the UI thread.</summary>
    public event Action? CameraLost;

    /// <summary>The microphone a call sends went away and no other would open; on the UI thread.</summary>
    public event Action? MicrophoneLost;

    /// <summary>A device came or went, after the camera and the microphone have followed; on the UI thread.</summary>
    public event Action? DevicesChanged;

    public CallMedia(Launch launch)
    {
        Launch = launch;
        _ui = DispatcherQueue.GetForCurrentThread();
        if (launch.DevicesFile is { } file)
        {
            _scripted = new ScriptedDevices(file);
            Devices = _scripted;
        }
        else
        {
            Devices = SystemDevices.Shared;
        }
        DeviceList.Directory = Devices;
        Devices.Changed += () => OnUi(async () =>
        {
            await FollowCamera();
            await FollowMicrophone();
            DevicesChanged?.Invoke();
        });
        // The share's picture and sound are generated wherever the camera's
        // are: a test never sends a real desktop or a real room.
        Sharing = new Sharing(!launch.RealDevices, WindowHandle)
        {
            SkipPicker = launch.SharePicked,
        };
    }

    private static IntPtr WindowHandle() => MainWindow.Current is { } window ? WinRT.Interop.WindowNative.GetWindowHandle(window) : IntPtr.Zero;

    /// <summary>Runs on the UI thread, where every change to the devices is made, one at a time with the presses.</summary>
    private void OnUi(Func<Task> work)
    {
        async void Run()
        {
            try
            {
                await work();
            }
            catch (Exception fault)
            {
                Log.Line($"devices: {fault.GetType().Name}: {fault.Message}");
            }
        }
        if (_ui == null || !_ui.TryEnqueue(Run)) Run();
    }

    /// <summary>The share from this device, one for the app's life as the menu's choices are; each call stops it as it goes.</summary>
    public readonly Sharing Sharing;

    /// <summary>Where the lists of devices come from: Windows', or a test's made-up ones.</summary>
    public IDeviceDirectory Devices { get; }

    /// <summary>The devices are a test's made-up ones (TOMOSHIBI_DEVICES).</summary>
    public bool Scripted => _scripted != null;

    /// <summary>The camera and the microphone last picked (DeviceMemory), or null for the system's.</summary>
    public string? CameraId { get; set; }
    public string? MicrophoneId { get; set; }

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

    public async Task<Playback?> OpenPlayback(string? speakers) => Mute ? null : await Playback.Open(speakers, Launch.SilentSpeakers);

    private async Task<List<string>> Present(DeviceKind kind) => (await Devices.List(kind)).Select(d => d.Id).ToList();

    /// <summary>
    /// The device of this kind that is in use, or would be if it started
    /// now, as a list ticks it: the system's own named as the device it is.
    /// <paramref name="speakers"/> is the speakers the call plays to, for
    /// that kind (null: the system's).
    /// </summary>
    public async Task<string?> Current(DeviceKind kind, string? speakers = null)
    {
        var present = await Present(kind);
        string? id = kind switch
        {
            DeviceKind.Camera => CameraInUse ?? DeviceRules.Opened(CameraId, present, systemHasOne: false),
            DeviceKind.Microphone => _microphoneOpen ? _microphoneOn : DeviceRules.Opened(MicrophoneId, present, systemHasOne: true),
            _ => speakers != null && present.Contains(speakers) ? speakers : null,
        };
        return id ?? SystemsOwn(kind, present);
    }

    /// <summary>The device Windows itself would use for a call, by its path; for made-up devices, the first listed.</summary>
    private string? SystemsOwn(DeviceKind kind, List<string> present)
    {
        if (Scripted || kind == DeviceKind.Camera) return present.FirstOrDefault();
        try
        {
            var id = kind == DeviceKind.Microphone
                ? Windows.Media.Devices.MediaDevice.GetDefaultAudioCaptureId(Windows.Media.Devices.AudioDeviceRole.Communications)
                : Windows.Media.Devices.MediaDevice.GetDefaultAudioRenderId(Windows.Media.Devices.AudioDeviceRole.Communications);
            return string.IsNullOrEmpty(id) ? present.FirstOrDefault() : present.FirstOrDefault(p => string.Equals(p, id, StringComparison.OrdinalIgnoreCase)) ?? id;
        }
        catch (Exception)
        {
            return present.FirstOrDefault();
        }
    }

    // The camera

    /// <summary>The camera running now (a device path, or a made-up camera's id); null when none is, or for the generated picture of a test that names no devices.</summary>
    public string? CameraInUse => _cameraFeed?.Device ?? _scriptedCamera;

    private bool CameraRunning => RealDevices ? _cameraFeed != null : Scripted ? _scriptedCamera != null : true;

    /// <summary>
    /// Another camera, at once where one is running: the feed is opened on
    /// it and goes on sending to the same track, so the room sees the
    /// picture change and not a track go and come back (CallModel.kt
    /// <c>useCamera</c>). False where it would not open; the camera that was
    /// running is then running again.
    /// </summary>
    public async Task<bool> UseCamera(string? id)
    {
        var was = CameraId;
        CameraId = id;
        if (!(RealDevices || Scripted) || CameraInUse == null) return true;
        if (_cameraOpening != null) await _cameraOpening;
        var running = CameraInUse;
        CloseCamera();
        if (await EnsureCamera()) return true;
        CameraId = was;
        Log.Line("the camera picked would not open; back to the one before");
        if (running != null) await EnsureCamera();
        return false;
    }

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

    /// <summary>
    /// Opens the camera for a call to publish, and says the size it sends
    /// at, which the track is published with (share.rs: the web's encodings
    /// follow the size the capture came back at); null where there is none.
    /// </summary>
    public async Task<(uint Width, uint Height)?> PrepareCamera()
    {
        if (!await EnsureCamera(prepared: true)) return null;
        return _cameraFeed is { } feed ? (feed.Width, feed.Height) : (Synthetic.Width, Synthetic.Height);
    }

    /// <summary>The camera sends to this call's track as well as to the pictures of yourself.</summary>
    public async Task<bool> Publish(VideoInput input)
    {
        _cameraInput = input;
        if (!await EnsureCamera())
        {
            _cameraInput = null;
            return false;
        }
        return true;
    }

    public async Task Unpublish()
    {
        _cameraInput = null;
        if (_synthetic != null) _synthetic.Video = null;
        if (_cameraFeed != null) _cameraFeed.Input = null;
        await SettleCamera();
    }

    public long FramesSent => _cameraFeed?.FramesPushed ?? _synthetic?.FramesPushed ?? 0;

    /// <summary>Opens the camera if it is wanted and not open; true once it runs, fed to whatever is waiting for it.</summary>
    private async Task<bool> EnsureCamera(bool prepared = false, string? avoid = null)
    {
        if (!RealDevices && !Scripted)
        {
            SyntheticSource.Previewing = _previewers > 0;
            SyntheticSource.Video = _cameraInput;
            return true;
        }
        if (_cameraOpening != null) await _cameraOpening;
        if (!CameraRunning && (prepared || _previewers > 0 || _cameraInput != null))
        {
            _cameraOpening = OpenCamera(avoid);
            try
            {
                await _cameraOpening;
            }
            finally
            {
                _cameraOpening = null;
            }
        }
        if (!CameraRunning) return false;
        if (_cameraFeed != null) _cameraFeed.Input = _cameraInput;
        if (_scriptedCamera != null)
        {
            SyntheticSource.Video = _cameraInput;
            SyntheticSource.Previewing = _previewers > 0;
        }
        return true;
    }

    /// <summary>Opens the camera picked, or the first there; never <paramref name="avoid"/>, a camera that just stopped under the call.</summary>
    private async Task OpenCamera(string? avoid = null)
    {
        var present = await Present(DeviceKind.Camera);
        // Windows lists an unplugged camera for a moment after it stops, and
        // opening it again then fails: a call whose webcam was pulled out
        // turned its camera off though the laptop's own was there.
        if (avoid != null) present.Remove(avoid);
        var id = DeviceRules.Opened(CameraId, present, systemHasOne: false);
        if (id == null)
        {
            Log.Line("no camera");
            return;
        }
        if (_scripted != null)
        {
            _scriptedCamera = id;
            SyntheticSource.Hue = (float)(_scripted.Number(DeviceKind.Camera, id) ?? double.NaN);
            Log.Line($"camera open: {id}");
            return;
        }
        try
        {
            var feed = await CameraFeed.Start(id, _cameraInput);
            feed.Previewed += (pixels, width, height) => Previewed?.Invoke(pixels, width, height);
            feed.Failed += () => OnUi(() => CameraFailed(feed));
            _cameraFeed = feed;
        }
        catch (Exception fault)
        {
            Log.Line($"camera failed: {fault.GetType().Name}: {fault.Message}");
        }
    }

    private void CloseCamera()
    {
        if (_cameraFeed != null)
        {
            _cameraFeed.Dispose();
            _cameraFeed = null;
            Log.Line("camera closed");
        }
        if (_scriptedCamera != null)
        {
            _scriptedCamera = null;
            if (_synthetic != null)
            {
                _synthetic.Video = null;
                _synthetic.Previewing = false;
            }
            Log.Line("camera closed");
        }
    }

    /// <summary>Lets the camera go once nothing shows it and no call sends it.</summary>
    private async Task SettleCamera()
    {
        if (!RealDevices && !Scripted)
        {
            if (_synthetic != null) _synthetic.Previewing = _previewers > 0;
            return;
        }
        if (_cameraOpening != null) await _cameraOpening;
        if (_scriptedCamera != null && _synthetic != null) _synthetic.Previewing = _previewers > 0;
        if (_previewers > 0 || _cameraInput != null || !CameraRunning) return;
        CloseCamera();
    }

    /// <summary>
    /// The devices changed: a camera running on one that went away carries
    /// on, on the same track, on the first still there, as the browser's
    /// does when its camera's track ends (livekit-client
    /// <c>handleTrackEnded</c>, <c>restartTrack</c>); with none left, the
    /// call is told.
    /// </summary>
    private async Task FollowCamera()
    {
        if (!RealDevices && !Scripted) return;
        if (_cameraOpening != null) await _cameraOpening;
        if (CameraInUse is not { } using_) return;
        var (move, next) = DeviceRules.Follow(using_, await Present(DeviceKind.Camera), systemHasOne: false);
        if (move == DeviceRules.Move.Stay) return;
        Log.Line(move == DeviceRules.Move.Switch ? $"the camera in use went away; on to {next}" : "the camera in use went away, and no other is there");
        CloseCamera();
        if (move == DeviceRules.Move.Stop || !await EnsureCamera())
        {
            if (_cameraInput != null) CameraLost?.Invoke();
        }
    }

    /// <summary>
    /// The camera stopped by itself: unplugged (the list says so a moment
    /// later), or taken by something else. Opened again as it would be now,
    /// once the list has caught up; the call is told if that does not work.
    /// </summary>
    private async Task CameraFailed(CameraFeed feed)
    {
        if (_cameraFeed != feed) return;
        CloseCamera();
        await Task.Delay(400);
        if (_cameraInput == null && _previewers == 0) return;
        // Another camera where there is one; the same one again where it is
        // the only one (taken by something else for a moment).
        if (await EnsureCamera(avoid: feed.Device) || await EnsureCamera()) return;
        if (_cameraInput != null) CameraLost?.Invoke();
    }

    // The microphone

    private MicrophoneFeed? _microphoneFeed;
    private AudioInput? _microphoneInput;
    private bool _original;
    private bool _microphoneOpen;
    /// <summary>The microphone it was opened on: a device, or null for the system's.</summary>
    private string? _microphoneOn;
    /// <summary>Whether the settings measure the microphone (StartCheck).</summary>
    private bool _checking;

    /// <summary>The speakers the call plays to (null: the system's), which the echo canceller listens to.</summary>
    public string? SpeakersDevice
    {
        get => _speakers;
        set
        {
            _speakers = value;
            _microphoneFeed?.EchoReference(value);
        }
    }
    private string? _speakers;

    /// <summary>How loud the microphone is, 0 to 1 root-mean-square, while it is open.</summary>
    public double MicrophoneRms => !_microphoneOpen ? 0 : _microphoneFeed?.Meter.Rms ?? _synthetic?.Meter.Rms ?? 0;

    public long ChunksSent => (_synthetic?.ChunksPushed ?? 0) + (_microphoneFeed?.ChunksPushed ?? 0);

    /// <summary>
    /// The microphone sends to this call's track, with the voice processed
    /// or, for original sound, as it is. False where it would not open.
    /// </summary>
    public async Task<bool> StartMicrophone(AudioInput input, bool original)
    {
        _microphoneInput = input;
        if (_microphoneOpen && original == _original)
        {
            Feed();
            return true;
        }
        _original = original;
        var present = await Present(DeviceKind.Microphone);
        return await OpenMicrophone(DeviceRulesOpened(present), present);
    }

    private string? DeviceRulesOpened(List<string> present) => DeviceRules.Opened(MicrophoneId, present, systemHasOne: true);

    /// <summary>The call's microphone track is gone: the microphone closes unless the settings are measuring it.</summary>
    public void StopMicrophone()
    {
        _microphoneInput = null;
        Feed();
        if (!_checking) CloseMicrophone();
    }

    /// <summary>
    /// Another microphone, at once where one is open: opened beside the one
    /// running, which goes only once the new one runs, so that a microphone
    /// that will not open leaves the call with the one it had (DeviceMenu.tsx
    /// <c>select</c>). False where it would not open.
    /// </summary>
    public async Task<bool> UseMicrophone(string? id)
    {
        var was = MicrophoneId;
        MicrophoneId = id;
        if (!_microphoneOpen) return true;
        var present = await Present(DeviceKind.Microphone);
        if (await OpenMicrophone(DeviceRulesOpened(present), present)) return true;
        MicrophoneId = was;
        return false;
    }

    /// <summary>Whatever is open sends to whatever wants it: the call's track, the settings' meter.</summary>
    private void Feed()
    {
        if (_microphoneFeed != null) _microphoneFeed.Input = _microphoneInput;
        if (RealDevices) return;
        if (_microphoneOpen || !Scripted)
        {
            SyntheticSource.Audio = _microphoneInput;
            SyntheticSource.Metering = _checking;
        }
    }

    /// <summary>Opens the microphone on <paramref name="device"/> (null: the system's), the one open before closed only once this one runs.</summary>
    private async Task<bool> OpenMicrophone(string? device, List<string> present)
    {
        if (!RealDevices && !Scripted)
        {
            _microphoneOpen = true;
            _microphoneOn = null;
            Feed();
            return true;
        }
        if (present.Count == 0)
        {
            Log.Line("no microphone");
            return false;
        }
        if (_scripted != null)
        {
            var sounding = device ?? present[0];
            SyntheticSource.ToneHz = (float)(_scripted.Number(DeviceKind.Microphone, sounding) ?? Synthetic.Tone);
            _microphoneOpen = true;
            _microphoneOn = device;
            Feed();
            Log.Line($"microphone open: {sounding} ({SyntheticSource.ToneHz} Hz)");
            return true;
        }
        MicrophoneFeed fresh;
        try
        {
            // Activation waits on the audio service; not on the UI thread.
            var original = _original;
            var input = _microphoneInput;
            fresh = await Task.Run(() => MicrophoneFeed.Start(device, original, input));
        }
        catch (Exception fault)
        {
            Log.Line($"microphone failed: {fault.GetType().Name}: {fault.Message}");
            return false;
        }
        var old = _microphoneFeed;
        _microphoneFeed = fresh;
        _microphoneOpen = true;
        _microphoneOn = device;
        fresh.Lost += () => OnUi(() => MicrophoneFailed(fresh));
        fresh.EchoReference(_speakers);
        Feed();
        old?.Dispose();
        return true;
    }

    private void CloseMicrophone()
    {
        _microphoneOpen = false;
        _microphoneOn = null;
        _microphoneFeed?.Dispose();
        _microphoneFeed = null;
        if (_synthetic != null)
        {
            _synthetic.Audio = null;
            _synthetic.Metering = false;
            _synthetic.Meter.Clear();
        }
    }

    /// <summary>
    /// The devices changed: a microphone open on one that went away moves to
    /// the system's on the same track, as the browser's does when its
    /// microphone's track ends (livekit-client <c>handleTrackEnded</c>,
    /// <c>restartTrack({ deviceId: "default" })</c>); with none left, the
    /// call is told. The system's own needs nothing: Windows moves it.
    /// </summary>
    private async Task FollowMicrophone()
    {
        if (!_microphoneOpen || (!RealDevices && !Scripted)) return;
        var present = await Present(DeviceKind.Microphone);
        var (move, _) = DeviceRules.Follow(_microphoneOn, present, systemHasOne: true);
        if (move == DeviceRules.Move.Stay)
        {
            // A made-up system's microphone is the first listed, which may
            // be another one now.
            if (_scripted != null && _microphoneOn == null) await OpenMicrophone(null, present);
            return;
        }
        Log.Line(move == DeviceRules.Move.Switch ? "the microphone in use went away; on to the system's" : "the microphone in use went away, and no other is there");
        if (move == DeviceRules.Move.Stop || !await OpenMicrophone(null, present)) Gone();
    }

    /// <summary>The capture stopped by itself (AUDCLNT_E_DEVICE_INVALIDATED): opened again on the system's, once the list has caught up.</summary>
    private async Task MicrophoneFailed(MicrophoneFeed feed)
    {
        if (_microphoneFeed != feed) return;
        await Task.Delay(400);
        if (_microphoneFeed != feed) return;
        if (!await OpenMicrophone(null, await Present(DeviceKind.Microphone))) Gone();
    }

    private void Gone()
    {
        var sending = _microphoneInput != null;
        CloseMicrophone();
        if (sending) MicrophoneLost?.Invoke();
    }

    // The settings' check

    /// <summary>
    /// The camera and the microphone open while the settings' audio and
    /// video page shows (Android's DeviceCheck): the picture of yourself and
    /// how loudly you are heard, before any call. A light left on after the
    /// page has gone would be a light that lies, so StopCheck closes both.
    /// </summary>
    public async Task StartCheck()
    {
        if (_checking) return;
        _checking = true;
        StartPreview();
        if (!_microphoneOpen)
        {
            var present = await Present(DeviceKind.Microphone);
            if (!_checking) return;
            await OpenMicrophone(DeviceRulesOpened(present), present);
        }
        Feed();
    }

    public void StopCheck()
    {
        if (!_checking) return;
        _checking = false;
        StopPreview();
        Feed();
        if (_microphoneInput == null) CloseMicrophone();
    }

    /// <summary>A device picked on the settings page, shown at once by a check that is open.</summary>
    public async Task CheckMicrophone(string? id)
    {
        MicrophoneId = id;
        if (!_checking || _microphoneInput != null) return;
        var present = await Present(DeviceKind.Microphone);
        await OpenMicrophone(DeviceRulesOpened(present), present);
    }

    public void Dispose()
    {
        _synthetic?.Dispose();
        _cameraFeed?.Dispose();
        _microphoneFeed?.Dispose();
    }
}
