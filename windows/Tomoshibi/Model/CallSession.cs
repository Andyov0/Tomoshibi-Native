using Microsoft.UI.Dispatching;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model;

/// <summary>
/// One call: the core's Call, what this participant publishes into it, and
/// the remote sounds it hears. Everything the core says arrives on its own
/// threads and is handed to the UI thread as <see cref="Event"/>.
/// </summary>
internal sealed class CallSession : IDisposable
{
    public readonly Call Call;
    public readonly string Room;
    private readonly Launch _launch;
    private readonly DispatcherQueue _ui;
    private readonly Playback? _playback;
    private readonly Dictionary<string, RemoteSound> _sounds = new();
    private readonly Dictionary<string, ParticipantInfo> _people = new();
    private readonly Synthetic? _synthetic;
    private AudioInput? _microphone;
    private VideoInput? _camera;
    private CameraFeed? _cameraCapture;
    private MicrophoneFeed? _microphoneCapture;

    /// <summary>What the core said, on the UI thread.</summary>
    public event Action<CallEvent>? Event;

    public IReadOnlyDictionary<string, ParticipantInfo> People => _people;
    public bool MicrophoneOn { get; private set; }
    public bool CameraOn { get; private set; }

    private sealed class Observer(Action<CallEvent> heard) : CallObserver
    {
        public void OnEvent(CallEvent @event) => heard(@event);
    }

    private CallSession(Call call, string room, Launch launch, DispatcherQueue ui, Playback? playback)
    {
        Call = call;
        Room = room;
        _launch = launch;
        _ui = ui;
        _playback = playback;
        if (!launch.RealDevices) _synthetic = new Synthetic();
    }

    /// <summary>
    /// Joins <paramref name="room"/> the way the web client does: signed in,
    /// the account's session is the proof and no passphrase is sent
    /// (api.ts); otherwise the passphrase typed.
    /// </summary>
    public static async Task<CallSession> Join(Deployment deployment, bool signedIn, Launch launch, string room, string name, string passphrase, DispatcherQueue ui)
    {
        var grant = await deployment.Join(room, name, signedIn ? "" : passphrase, null, "", null);
        Log.Line($"granted room {grant.Room} as {grant.Identity}");
        var forward = grant.Forward is { } f ? new IceServer(f.Url, f.Username, f.Credential) : null;
        var options = new CallOptions(grant.Url, grant.Token, grant.Room, string.IsNullOrEmpty(launch.SealingWord) ? null : launch.SealingWord, forward);
        // Nothing is ever played in a muted launch: the graph is not opened.
        var playback = launch.Mute ? null : await Playback.Open();

        CallSession? session = null;
        var early = new List<CallEvent>();
        var observer = new Observer(e =>
        {
            // The participants already there are told before join_call
            // returns; they wait for the session to exist.
            lock (early)
            {
                if (session == null)
                {
                    early.Add(e);
                    return;
                }
            }
            session.Heard(e);
        });
        var call = await TomoshibiCoreMethods.JoinCall(options, observer);
        lock (early)
        {
            session = new CallSession(call, grant.Room, launch, ui, playback);
            foreach (var e in early) session.Heard(e);
        }
        return session;
    }

    /// <summary>On a core thread: sinks are attached here, at once, and the UI hears afterwards.</summary>
    private void Heard(CallEvent e)
    {
        switch (e)
        {
            case CallEvent.TrackSubscribed { Track.Kind: MediaKind.Audio } subscribed:
            {
                var sound = new RemoteSound(subscribed.Track.Sid, _playback);
                lock (_sounds) _sounds[sound.Sid] = sound;
                Call.AttachPcm(sound.Sid, sound);
                Log.Line($"hearing {subscribed.Track.Source} {subscribed.Track.Sid} from {subscribed.Identity}");
                break;
            }
            case CallEvent.TrackUnsubscribed unsubscribed:
                Forget(unsubscribed.Sid);
                break;
            case CallEvent.TrackUnpublished unpublished:
                Forget(unpublished.Sid);
                break;
            case CallEvent.Connection:
                // Every two seconds; the UI has no use for it yet.
                return;
        }
        _ui.TryEnqueue(() =>
        {
            switch (e)
            {
                case CallEvent.ParticipantJoined joined:
                    _people[joined.Participant.Identity] = joined.Participant;
                    break;
                case CallEvent.ParticipantChanged changed:
                    _people[changed.Participant.Identity] = changed.Participant;
                    break;
                case CallEvent.ParticipantLeft left:
                    _people.Remove(left.Identity);
                    break;
            }
            Event?.Invoke(e);
        });
    }

    private void Forget(string sid)
    {
        RemoteSound? sound;
        lock (_sounds)
        {
            if (!_sounds.Remove(sid, out sound)) return;
        }
        Call.Detach(sid);
        sound.Dispose();
    }

    public string NameOf(string identity) => _people.TryGetValue(identity, out var person) ? person.Name : identity;

    public async Task SetMicrophone(bool on)
    {
        if (on && _microphone == null)
        {
            _microphone = await Call.PublishAudio(new SoundKind.Voice(false));
            Log.Line($"published microphone {_microphone.Sid()} ({(_launch.RealDevices ? "device" : "synthetic")})");
            if (_launch.RealDevices) _microphoneCapture = await MicrophoneFeed.Start(_microphone);
            else _synthetic!.Audio = _microphone;
        }
        else
        {
            _microphone?.SetMuted(!on);
        }
        MicrophoneOn = on;
    }

    public async Task SetCamera(bool on)
    {
        if (on && _camera == null)
        {
            if (_launch.RealDevices)
            {
                var opened = await CameraFeed.Open(1280, 720);
                if (opened is not { } device)
                {
                    Log.Line("no camera");
                    return;
                }
                _camera = await Call.PublishVideo(new VideoKind.Camera(), device.Width, device.Height);
                _cameraCapture = await CameraFeed.Start(device.Capture, device.Source, _camera);
            }
            else
            {
                _camera = await Call.PublishVideo(new VideoKind.Camera(), Synthetic.Width, Synthetic.Height);
                _synthetic!.Video = _camera;
            }
            Log.Line($"published camera {_camera.Sid()} ({(_launch.RealDevices ? "device" : "synthetic")})");
        }
        else
        {
            _camera?.SetMuted(!on);
        }
        CameraOn = on;
    }

    /// <summary>A line for the log: what has gone out, and every sound heard since the last reading.</summary>
    public string Reading()
    {
        // Counters a capture thread bumps; a 64-bit read is whole.
        var frames = _synthetic?.FramesPushed ?? _cameraCapture?.FramesPushed ?? 0;
        var chunks = _synthetic?.ChunksPushed ?? _microphoneCapture?.ChunksPushed ?? 0;
        var sent = $"sent video {frames} frames, audio {chunks} chunks";
        List<string> heard;
        lock (_sounds)
        {
            heard = _sounds.Values.Select(s =>
            {
                var (total, since, dbfs) = s.Reading();
                return $"audio {s.Sid} chunks={total} (+{since}) level={dbfs:F1} dBFS";
            }).ToList();
        }
        return string.Join("; ", heard.Prepend(sent));
    }

    public async Task Leave()
    {
        StopMedia();
        await Call.Leave();
    }

    private void StopMedia()
    {
        if (_synthetic != null)
        {
            _synthetic.Video = null;
            _synthetic.Audio = null;
            _synthetic.Dispose();
        }
        _cameraCapture?.Dispose();
        _cameraCapture = null;
        _microphoneCapture?.Dispose();
        _microphoneCapture = null;
        lock (_sounds)
        {
            foreach (var sound in _sounds.Values) sound.Dispose();
            _sounds.Clear();
        }
    }

    public void Dispose()
    {
        StopMedia();
        _playback?.Dispose();
        _camera?.Dispose();
        _microphone?.Dispose();
        Call.Dispose();
    }
}
