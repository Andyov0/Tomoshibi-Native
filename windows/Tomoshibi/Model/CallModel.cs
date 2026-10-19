using System.Diagnostics;
using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model;

/// <summary>One person in the call, as the screens see them (Android's model/CallModel.kt <c>Person</c>).</summary>
internal sealed record Person(
    string Id,
    string Name,
    IReadOnlyDictionary<string, string> Attributes,
    IReadOnlyDictionary<string, TrackInfo> Tracks,
    LinkQuality Quality = LinkQuality.Good,
    bool Speaking = false)
{
    public bool HandUp => Attributes.TryGetValue("hand", out var hand) && hand == "up";

    public TrackInfo? Track(MediaSource source) => Tracks.Values.FirstOrDefault(t => t.Source == source);

    public string Shown => Refusals.Named(Name, Id);
}

/// <summary>The unit of layout: a picture, not a person (the web's <c>Surface</c>). Someone sharing their screen with their camera on is two of these.</summary>
internal sealed record Surface(string Id, Person Person, Surface.Kinds Kind, string? Sid)
{
    public enum Kinds { Camera, Screen }
}

/// <summary>
/// One thing said in the corner of a call: a notice (web/src/live/notices.ts).
/// Something that happened is said here and fades; something still true is a
/// line over the stage (<see cref="CallModel.Notice"/>, "Reconnecting"). A
/// notice about something over carries no close button; one somebody may
/// want to deal with now, or not at all, has one.
/// </summary>
internal sealed record CallToast(
    string Title,
    string? Detail = null,
    CallToast.Kinds Kind = CallToast.Kinds.Info,
    double? Lasts = NoticeTimes.Briefly,
    string? ActionTitle = null,
    string? Key = null,
    bool? Closable = null)
{
    public enum Kinds { Info, Signal, Warning, Failure }

    private static long _next;

    /// <summary>Kept when it is said again under the same key, so the card stays where it is and only its words change.</summary>
    public long Id { get; init; } = Interlocked.Increment(ref _next);

    /// <summary>Whether it carries a close button (notices.ts <c>closeButton</c>); unless told, only one that would otherwise never go.</summary>
    public bool CanClose => Closable ?? Lasts == null;
}

internal sealed record ChatLine(string Id, string Identity, string Name, string Text, DateTimeOffset At, bool Mine);

/// <summary>A reaction on screen: whose, which, and the timer's own id (hands.ts <c>useReactions</c>).</summary>
internal sealed record ShownReaction(string From, string What, long Id);

/// <summary>
/// A call and everything the screens draw from it: Android's
/// model/CallModel.kt, the parts of it this phase of the Windows app has
/// (people and their tracks, chat, the hand and reactions, notices, the
/// connection, the camera and microphone, leaving and ending for everybody).
///
/// The core speaks on its own threads; every event is carried to the UI
/// thread in the order it was said and applied there. Remote sound is
/// played the moment it is subscribed, because a participant nobody can
/// hear is a fault nobody can see.
/// </summary>
internal sealed class CallModel : Observable, IDisposable
{
    public const string SignalDetailKey = "meet-live.signal-detail";
    /// <summary>What others said stays in the corner this long (live/chat.ts SAID_FOR).</summary>
    public const int SaidForMs = 6_000;
    /// <summary>The web keeps the last 500 messages (chat.ts).</summary>
    public const int MessagesKept = 500;
    /// <summary>A room that closes within this long of a <c>moving</c> message is the move, not the end (App.tsx ~560-627).</summary>
    private static readonly TimeSpan MoveWindow = TimeSpan.FromSeconds(15);
    /// <summary>hands.ts <c>REACTION</c>.</summary>
    public const string ReactionTopic = "reaction";

    public readonly Call Core;
    public readonly Deployment Deployment;
    public readonly string? Region;
    public readonly bool Sealed;
    private readonly CallMedia _media;
    private readonly DeviceMemory _memory;
    private readonly DispatcherQueue _ui;
    private readonly Playback? _playback;
    private readonly Dictionary<string, RemoteSound> _sounds = new();
    private readonly CancellationTokenSource _over = new();

    public string Room { get; }
    public string LocalIdentity { get; }
    public string LocalName { get; }

    /// <summary>What the join answer called the machine this call was sent to, and the one the meeting is on where that is another.</summary>
    public string? Relay { get; set; }
    public string? Holding { get; set; }

    public CallState State { get; private set; } = CallState.Connected;
    public IReadOnlyList<Person> People { get; private set; } = Array.Empty<Person>();
    public IReadOnlyList<ChatLine> Messages { get; private set; } = Array.Empty<ChatLine>();
    /// <summary>What others said in the last six seconds, for the corner while the messages are closed.</summary>
    public IReadOnlyList<ChatLine> Arriving { get; private set; } = Array.Empty<ChatLine>();
    /// <summary>Something was said while the messages were closed (useChat.ts).</summary>
    public bool Unread { get; private set; }
    public IReadOnlyDictionary<string, TrackInfo> LocalTracks { get; private set; } = new Dictionary<string, TrackInfo>();

    private bool _chatOpen;
    /// <summary>Whether the messages panel is open; set by the room view.</summary>
    public bool ChatOpen
    {
        get => _chatOpen;
        set
        {
            _chatOpen = value;
            if (value)
            {
                Unread = false;
                Arriving = Array.Empty<ChatLine>();
            }
            Notify();
        }
    }

    /// <summary>Something still true, said as a line across the top of the call for as long as it is (the web's banners).</summary>
    public string? Notice { get; set; }

    /// <summary>What the call says in the corner, oldest first. At most three that go by themselves.</summary>
    public IReadOnlyList<CallToast> Toasts { get; private set; } = Array.Empty<CallToast>();
    public bool ToastsHeld { get; private set; }
    private readonly Dictionary<long, Action> _toastActions = new();
    private readonly Dictionary<long, NoticeClock> _toastClocks = new();
    private readonly Dictionary<long, CancellationTokenSource> _toastTimers = new();

    public LinkQuality LocalQuality { get; private set; } = LinkQuality.Good;
    /// <summary>How the call is going from here, every two seconds (connection.ts): null until the first sample.</summary>
    public ConnectionReading? Connection { get; private set; }

    private bool _connectionOpen;
    /// <summary>Whether the badge's figures are open, kept between calls under the web's name (Signal.tsx <c>DETAIL_KEY</c>).</summary>
    public bool ConnectionOpen
    {
        get => _connectionOpen;
        set
        {
            if (value == _connectionOpen) return;
            _connectionOpen = value;
            _memory.Defaults?.Set(SignalDetailKey, value);
            Notify();
        }
    }

    public bool MicrophoneOn { get; private set; }
    public bool CameraOn { get; private set; }
    public bool HandUp { get; private set; }

    /// <summary>Whether this participant runs the room, as the server says (host.ts <c>standing</c>): only they are offered ending it for everybody.</summary>
    public bool Hosting { get; private set; }

    /// <summary>Who spoke last, for bringing speakers forward in a room that runs to more than a page.</summary>
    public SpeakingRecord Speaking { get; private set; } = new();
    public bool MeSpeaking { get; private set; }

    public IReadOnlyList<ShownReaction> Reactions { get; private set; } = Array.Empty<ShownReaction>();

    /// <summary>The relay a move is taking this call to, between the server saying so and the room closing.</summary>
    private (string Relay, DateTime At)? _moving;

    /// <summary>The call ended because the server moved it; the app joins again.</summary>
    public event Action<string>? Moved;

    /// <summary>The call ended under this participant for any other reason: the app says which end it was.</summary>
    public event Action<string>? EndedBy;

    /// <summary>What the core said, after the model has applied it, for whoever draws from the event itself (a tile hearing its track go).</summary>
    public event Action<CallEvent>? Heard;

    private AudioInput? _microphone;
    private VideoInput? _camera;
    private MicrophoneFeed? _microphoneFeed;
    private bool _microphoneStarting;
    private bool _cameraStarting;
    /// <summary>Left, or ended under it: a start that woke after the call had gone would open a device with nobody to send to.</summary>
    private bool _over_;
    /// <summary>Whether the join has finished telling this model who was already here; until then nobody arriving is announced.</summary>
    private bool _settled;

    private CallModel(Call core, Deployment deployment, string? region, bool sealedCall, CallMedia media, DeviceMemory memory, Playback? playback)
    {
        Core = core;
        Deployment = deployment;
        Region = region;
        Sealed = sealedCall;
        _media = media;
        _memory = memory;
        _playback = playback;
        _ui = DispatcherQueue.GetForCurrentThread();
        Room = core.RoomName();
        LocalIdentity = core.LocalIdentity();
        LocalName = core.LocalName();
        _connectionOpen = memory.Defaults?.Bool(SignalDetailKey) ?? false;
    }

    private sealed class Bridge(Action<CallEvent> heard) : CallObserver
    {
        public void OnEvent(CallEvent @event) => heard(@event);
    }

    /// <summary>
    /// Joins the call that <paramref name="options"/> describes. The people
    /// already here are told during the join itself, and nobody is told they
    /// joined: the web listens only once its room is connected (notices.ts
    /// <c>watch</c>).
    /// </summary>
    public static async Task<CallModel> Join(CallOptions options, Deployment deployment, string? region, CallMedia media, DeviceMemory memory)
    {
        // Nothing is ever played in a muted launch: the graph is not opened.
        var playback = await media.OpenPlayback();
        CallModel? model = null;
        var early = new List<CallEvent>();
        var bridge = new Bridge(e =>
        {
            lock (early)
            {
                if (model == null)
                {
                    early.Add(e);
                    return;
                }
            }
            model.Arrived(e);
        });
        var core = await TomoshibiCoreMethods.JoinCall(options, bridge);
        lock (early)
        {
            model = new CallModel(core, deployment, region, options.SealingWord != null, media, memory, playback);
            foreach (var e in early) model.Arrived(e);
        }
        // Applied on the UI thread after the events queued above.
        model._ui.TryEnqueue(() =>
        {
            model._settled = true;
            _ = model.KeepStanding();
        });
        return model;
    }

    /// <summary>On a core thread: sinks are attached here, at once, and the model hears afterwards on the UI thread.</summary>
    private void Arrived(CallEvent e)
    {
        switch (e)
        {
            case CallEvent.TrackSubscribed { Track.Kind: MediaKind.Audio } subscribed:
            {
                var sound = new RemoteSound(subscribed.Track.Sid, _playback);
                lock (_sounds) _sounds[sound.Sid] = sound;
                Core.AttachPcm(sound.Sid, sound);
                Log.Line($"hearing {subscribed.Track.Source} {subscribed.Track.Sid} from {subscribed.Identity}");
                break;
            }
            case CallEvent.TrackUnsubscribed unsubscribed:
                Forget(unsubscribed.Sid);
                break;
            case CallEvent.TrackUnpublished unpublished:
                Forget(unpublished.Sid);
                break;
        }
        _ui.TryEnqueue(() => Apply(e));
    }

    private void Forget(string sid)
    {
        RemoteSound? sound;
        lock (_sounds)
        {
            if (!_sounds.Remove(sid, out sound)) return;
        }
        Core.Detach(sid);
        sound.Dispose();
    }

    // What the screens read

    public IReadOnlyList<Surface> Surfaces => People.SelectMany(person =>
    {
        // A picture is drawn once its track is subscribed, not when it is
        // first listed: a panel attached to a track the core had not yet
        // subscribed to was sometimes never fed, the participant's list
        // reaching the app before the subscription's track did, and the
        // tile stayed black for the whole call.
        var camera = person.Track(MediaSource.Camera);
        var list = new List<Surface> { new(person.Id + "/camera", person, Surface.Kinds.Camera, camera is { Muted: false, Subscribed: true } ? camera.Sid : null) };
        if (person.Track(MediaSource.ScreenShare) is { } screen) list.Add(new Surface(person.Id + "/screen", person, Surface.Kinds.Screen, screen.Subscribed ? screen.Sid : null));
        return list;
    }).ToList();

    public Person? PersonOf(string identity) => People.FirstOrDefault(p => p.Id == identity);

    public IReadOnlyList<string> Identities => People.Select(p => p.Id).ToList();

    // Notices

    /// <summary>Says a notice; one with an action calls it when its button is pressed.</summary>
    public void Announce(CallToast toast, Action? action = null)
    {
        var said = toast;
        var existing = toast.Key == null ? -1 : Toasts.ToList().FindIndex(t => t.Key == toast.Key);
        if (existing >= 0)
        {
            said = toast with { Id = Toasts[existing].Id };
            StopClock(said.Id);
            var list = Toasts.ToList();
            list[existing] = said;
            Toasts = list;
        }
        else
        {
            var next = Toasts.Append(said).ToList();
            // The fourth pushes out the oldest one that goes by itself; one
            // somebody has to act on is never what is lost.
            foreach (var index in NoticeTimes.Overflow(next.Select(t => t.Lasts == null).ToList()).Reverse())
            {
                ForgetToast(next[index].Id);
                next.RemoveAt(index);
            }
            Toasts = next;
        }
        if (action != null) _toastActions[said.Id] = action;
        else _toastActions.Remove(said.Id);
        if (said.Lasts is { } lasts)
        {
            _toastClocks[said.Id] = new NoticeClock(lasts, Now(), running: !ToastsHeld);
            RunClock(said.Id);
        }
        Log.Line($"notice: {said.Title}{(said.Detail != null ? $" ({said.Detail})" : "")}");
        Notify();
    }

    public void Dismiss(long id)
    {
        ForgetToast(id);
        Toasts = Toasts.Where(t => t.Id != id).ToList();
        Notify();
    }

    /// <summary>Takes down whatever was said under <paramref name="key"/>, if anything still is.</summary>
    public void Dismiss(string key)
    {
        if (Toasts.FirstOrDefault(t => t.Key == key) is { } toast) Dismiss(toast.Id);
    }

    /// <summary>The notice's button was pressed.</summary>
    public void Act(long id)
    {
        _toastActions.TryGetValue(id, out var action);
        Dismiss(id);
        action?.Invoke();
    }

    /// <summary>Holds every countdown while the corner is read, and lets them run on once it is not, each with what it had left.</summary>
    public void HoldToasts(bool held)
    {
        if (held == ToastsHeld) return;
        ToastsHeld = held;
        var now = Now();
        foreach (var id in _toastClocks.Keys.ToList())
        {
            var clock = _toastClocks[id];
            if (held)
            {
                _toastClocks[id] = clock.Hold(now);
                if (_toastTimers.Remove(id, out var timer)) timer.Cancel();
            }
            else
            {
                _toastClocks[id] = clock.Run(now);
                RunClock(id);
            }
        }
    }

    private void RunClock(long id)
    {
        if (_toastTimers.Remove(id, out var old)) old.Cancel();
        if (!_toastClocks.TryGetValue(id, out var clock) || clock.Due(Now()) is not { } due) return;
        var timer = _toastTimers[id] = CancellationTokenSource.CreateLinkedTokenSource(_over.Token);
        _ = Later(TimeSpan.FromSeconds(due), timer.Token, () => Dismiss(id));
    }

    private void StopClock(long id)
    {
        if (_toastTimers.Remove(id, out var timer)) timer.Cancel();
        _toastClocks.Remove(id);
    }

    private void ForgetToast(long id)
    {
        StopClock(id);
        _toastActions.Remove(id);
    }

    private static double Now() => Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;

    private static async Task Later(TimeSpan after, CancellationToken token, Action then)
    {
        try
        {
            await Task.Delay(after, token);
            then();
        }
        catch (OperationCanceledException)
        {
        }
    }

    /// <summary>Something somebody pressed did not take (notices.ts <c>actionFailed</c>): it fades, because pressing again is the whole of the remedy.</summary>
    public void ActionFailed(string reason) =>
        Announce(new CallToast(reason, Kind: CallToast.Kinds.Failure, Lasts: NoticeTimes.AtMost));

    /// <summary>Says that a camera or a microphone would not start (notices.ts <c>deviceFailed</c>), taken down by the next start that works.</summary>
    private void DeviceFailed(bool camera)
    {
        Announce(new CallToast(
            camera ? T("Can't use your camera") : T("Can't use your microphone"),
            Detail: T("Something else may be using it."),
            Kind: CallToast.Kinds.Failure,
            Lasts: NoticeTimes.AtMost,
            Key: camera ? "device.camera" : "device.microphone",
            Closable: true));
    }

    // What the person does

    /// <summary>The microphone on or off; one that will not start says so itself, wherever it was asked from.</summary>
    public async Task SetMicrophone(bool on)
    {
        if (on)
        {
            // Once, whoever asks: a press while the first was still
            // publishing published a second voice track.
            if (_microphoneStarting) return;
            _microphoneStarting = true;
            try
            {
                if (_over_) return;
                if (_microphone == null)
                {
                    var input = await Core.PublishAudio(new SoundKind.Voice(_memory.OriginalSound));
                    _microphone = input;
                    Log.Line($"published microphone {input.Sid()} ({(_media.RealDevices ? "device" : "synthetic")})");
                    if (_over_) return;
                    if (_media.RealDevices)
                    {
                        _microphoneFeed = await MicrophoneFeed.Start(input, _media.MicrophoneId);
                        if (_microphoneFeed == null) throw new InvalidOperationException("no microphone");
                    }
                    else
                    {
                        _media.SyntheticSource.Audio = input;
                    }
                }
                _microphone.SetMuted(false);
                Dismiss("device.microphone");
            }
            catch (Exception fault)
            {
                Log.Line($"microphone failed: {fault.GetType().Name}: {fault.Message}");
                // Muted at the track, so that nobody sees a microphone on that sends nothing.
                _microphone?.SetMuted(true);
                DeviceFailed(camera: false);
                return;
            }
            finally
            {
                _microphoneStarting = false;
            }
        }
        else
        {
            _microphone?.SetMuted(true);
        }
        MicrophoneOn = on;
        Notify();
    }

    public async Task SetCamera(bool on)
    {
        if (on)
        {
            // Once: a second press while the first was still asking would
            // publish a second camera.
            if (_camera != null || _cameraStarting) return;
            _cameraStarting = true;
            try
            {
                if (_over_) return;
                var (width, height) = _media.CameraSize;
                var input = await Core.PublishVideo(new VideoKind.Camera(), width, height);
                if (_over_)
                {
                    await Quietly(input.Unpublish);
                    return;
                }
                if (!await _media.Publish(input))
                {
                    // A camera published and never fed is a black tile for
                    // everybody else.
                    await Quietly(input.Unpublish);
                    throw new InvalidOperationException("no camera");
                }
                _camera = input;
                Log.Line($"published camera {input.Sid()} ({(_media.RealDevices ? "device" : "synthetic")})");
                Dismiss("device.camera");
            }
            catch (Exception fault)
            {
                Log.Line($"camera failed: {fault.GetType().Name}: {fault.Message}");
                DeviceFailed(camera: true);
                return;
            }
            finally
            {
                _cameraStarting = false;
            }
        }
        else if (_camera is { } input)
        {
            _camera = null;
            await _media.Unpublish();
            await Quietly(input.Unpublish);
            input.Dispose();
        }
        CameraOn = on;
        Notify();
    }

    private static async Task Quietly(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (Exception fault)
        {
            Log.Line($"ignored: {fault.Message}");
        }
    }

    /// <summary>Another microphone, at once where it is on: its feed is started on the new device for the same track. Remembered once it took.</summary>
    public async Task UseMicrophone(string? id)
    {
        _media.MicrophoneId = id;
        if (_media.RealDevices && _microphone is { } input && _microphoneFeed != null)
        {
            _microphoneFeed.Dispose();
            _microphoneFeed = await MicrophoneFeed.Start(input, id);
            if (_microphoneFeed == null)
            {
                DeviceFailed(camera: false);
                return;
            }
        }
        _memory.Remember(DeviceMemory.Kind.Microphone, id);
    }

    /// <summary>Another camera, the same track (CallModel.kt <c>useCamera</c>); with the camera off, the one it turns on with. Remembered.</summary>
    public async Task UseCamera(string? id)
    {
        await _media.UseCamera(id);
        _memory.Remember(DeviceMemory.Kind.Camera, id);
    }

    /// <summary>
    /// The web's "original sound" (sound.ts <c>applyOriginalSound</c>): the
    /// voice republished as it is, at the higher rate, since nothing is taken
    /// out of it any more. Remembered once it took.
    /// </summary>
    public async Task SetOriginalSound(bool original)
    {
        if (original == _memory.OriginalSound && _microphone == null) return;
        _memory.OriginalSound = original;
        if (_microphone is not { } old) return;
        var fresh = await Core.PublishAudio(new SoundKind.Voice(original));
        _microphone = fresh;
        if (_media.RealDevices)
        {
            _microphoneFeed?.Dispose();
            _microphoneFeed = await MicrophoneFeed.Start(fresh, _media.MicrophoneId);
        }
        else
        {
            _media.SyntheticSource.Audio = fresh;
        }
        fresh.SetMuted(!MicrophoneOn);
        await Quietly(old.Unpublish);
        Notify();
    }

    public bool OriginalSound => _memory.OriginalSound;

    /// <summary>A message is on its way (useChat.ts <c>sending</c>): the box sends nothing more until it has gone or failed.</summary>
    public bool Sending { get; private set; }

    private string _chatDraft = "";
    /// <summary>What is in the messages box, kept with the call rather than the panel, so that closing the panel half way through a sentence does not lose it.</summary>
    public string ChatDraft
    {
        get => _chatDraft;
        set
        {
            var changed = value != _chatDraft;
            _chatDraft = value;
            if (changed && value.Length > 0) ChatFailed = false;
        }
    }

    /// <summary>The last message did not leave. Its words are back in the box, and the box says so until the next is typed or sent.</summary>
    public bool ChatFailed { get; private set; }

    /// <summary>
    /// Sends what is in the box (ChatPanel.tsx <c>send</c>): cleared at once
    /// and put back if it did not leave, because losing what somebody typed
    /// is worse than showing it twice.
    /// </summary>
    public async Task SendDraft()
    {
        var body = ChatDraft.Trim();
        if (body.Length == 0 || Sending || State != CallState.Connected) return;
        _chatDraft = "";
        ChatFailed = false;
        Sending = true;
        Notify();
        try
        {
            await Core.SendChat(body);
            Messages = Messages.Append(new ChatLine(Guid.NewGuid().ToString(), LocalIdentity, LocalName, body, DateTimeOffset.Now, Mine: true)).TakeLast(MessagesKept).ToList();
            Log.Line($"said {body.Length} characters");
        }
        catch (Exception fault)
        {
            Log.Line($"chat failed: {fault.Message}");
            // Whatever was typed since goes after it, rather than being replaced by it.
            _chatDraft = _chatDraft.Length == 0 ? body : body + "\n" + _chatDraft;
            ChatFailed = true;
        }
        finally
        {
            Sending = false;
            Notify();
        }
    }

    /// <summary>The hand is an attribute, written only when it changes (hands.ts).</summary>
    public async Task SetHand(bool up)
    {
        if (up == HandUp) return;
        await Core.SetAttributes(new Dictionary<string, string> { ["hand"] = up ? "up" : "" });
        HandUp = up;
        Notify();
    }

    /// <summary>The six a browser shows, in its order (hands.ts <c>REACTIONS</c>).</summary>
    public static IReadOnlyList<string> ReactionChoices { get; } = TomoshibiProtocolMethods.ReactionChoices();

    /// <summary>
    /// Sends a reaction to everybody and shows it on this participant's own
    /// picture at once: data does not come back to whoever sent it (hands.ts
    /// <c>react</c>). The payload is the reaction's own text, which the
    /// protocol crate makes only for the six a browser would show.
    /// </summary>
    public void React(string what)
    {
        byte[] payload;
        try
        {
            payload = TomoshibiProtocolMethods.ReactionPayload(what);
        }
        catch (Exception)
        {
            return;
        }
        Show(what, LocalIdentity);
        _ = Quietly(() => Core.SendData(ReactionTopic, payload, Array.Empty<string>(), true));
    }

    private void Show(string what, string from)
    {
        var one = new ShownReaction(from, what, Interlocked.Increment(ref _reactionIds));
        Reactions = Reactions.Append(one).ToList();
        Notify();
        _ = Later(TimeSpan.FromMilliseconds(TomoshibiProtocolMethods.ReactionShownForMs()), _over.Token, () =>
        {
            Reactions = Reactions.Where(r => r.Id != one.Id).ToList();
            Notify();
        });
    }

    private static long _reactionIds;

    /// <summary>
    /// Asks the server whether this participant runs the room, at the join
    /// and every half minute after: a room can be handed over, and only its
    /// host is offered ending it for everybody (HostFeature.kt
    /// <c>refreshStanding</c>). TODO(windows 2b): the host panel (the door,
    /// muting and moving people, handing the room over) is not here yet.
    /// </summary>
    private async Task KeepStanding()
    {
        while (!_over.IsCancellationRequested)
        {
            try
            {
                var standing = await Deployment.HostStanding(Room, Core.Token());
                if (standing.Yours != Hosting)
                {
                    Hosting = standing.Yours;
                    Log.Line(Hosting ? "this room is ours" : "this room is not ours");
                    Notify();
                }
            }
            catch (Exception)
            {
                // A round that did not answer says nothing about who runs it.
            }
            try
            {
                await Task.Delay(TimeSpan.FromSeconds(30), _over.Token);
            }
            catch (OperationCanceledException)
            {
                return;
            }
        }
    }

    /// <summary>
    /// Ends the meeting for everybody (ControlBar.kt <c>endForEverybody</c>):
    /// the server closes the room with the call's current join token, which
    /// it refreshes while the call lasts, and after a moment, so that the
    /// server's disconnect normally arrives first, this participant leaves.
    /// </summary>
    public async Task EndForEverybody()
    {
        await Deployment.Dissolve(Room, Core.Token());
        await Task.Delay(1200);
    }

    // Leaving

    private bool _left;

    public async Task Leave()
    {
        if (_left) return;
        _left = true;
        _over_ = true;
        _over.Cancel();
        StopMedia();
        try
        {
            await Core.Leave();
        }
        catch (Exception fault)
        {
            Log.Line($"leaving: {fault.Message}");
        }
        Log.Line("left the call");
    }

    private void StopMedia()
    {
        if (!_media.RealDevices) _media.SyntheticSource.Audio = null;
        _ = _media.Unpublish();
        _microphoneFeed?.Dispose();
        _microphoneFeed = null;
        lock (_sounds)
        {
            foreach (var sound in _sounds.Values) sound.Dispose();
            _sounds.Clear();
        }
    }

    // Events

    private void Apply(CallEvent e)
    {
        switch (e)
        {
            case CallEvent.StateChanged changed:
                State = changed.State;
                Log.Line($"call {changed.State.ToString().ToLowerInvariant()}");
                break;
            case CallEvent.Ended ended:
                _over_ = true;
                StopMedia();
                Log.Line($"call ended: {ended.Reason}");
                // The server deleting the room is `RoomDeleted` in the Rust
                // SDK; waiting only for `RoomClosed` left the app sitting in a
                // call that had gone, never rejoining.
                if (_moving is { } moving && DateTime.UtcNow - moving.At < MoveWindow && Refusals.MovedAway(ended.Reason)) Moved?.Invoke(moving.Relay);
                else EndedBy?.Invoke(ended.Reason);
                break;
            case CallEvent.ParticipantJoined joined:
            {
                var arriving = PersonOf(joined.Participant.Identity) == null;
                Upsert(joined.Participant);
                if (arriving) Log.Line($"participant joined: {joined.Participant.Identity}");
                if (arriving && _settled) Announce(new CallToast(T("{name} joined", ("name", Refusals.Named(joined.Participant.Name, joined.Participant.Identity)))));
                break;
            }
            case CallEvent.ParticipantChanged changed:
                Upsert(changed.Participant);
                break;
            case CallEvent.ParticipantLeft left:
                if (PersonOf(left.Identity) is { } gone) Announce(new CallToast(T("{name} left", ("name", gone.Shown))));
                Log.Line($"participant left: {left.Identity}");
                People = People.Where(p => p.Id != left.Identity).ToList();
                break;
            case CallEvent.TrackPublished published:
            {
                var person = PersonOf(published.Identity);
                if (_settled && person != null && !person.Tracks.ContainsKey(published.Track.Sid)) AnnounceShare(published.Track, person);
                Update(published.Identity, p => p with { Tracks = With(p.Tracks, published.Track) });
                break;
            }
            case CallEvent.TrackSubscribed subscribed:
                Update(subscribed.Identity, p => p with { Tracks = With(p.Tracks, subscribed.Track) });
                Log.Line($"subscribed {subscribed.Track.Kind.ToString().ToLowerInvariant()} {subscribed.Track.Source.ToString().ToLowerInvariant()} {subscribed.Track.Sid} from {subscribed.Identity}");
                break;
            case CallEvent.TrackUnpublished unpublished:
                Update(unpublished.Identity, p => p with { Tracks = Without(p.Tracks, unpublished.Sid) });
                break;
            case CallEvent.TrackUnsubscribed unsubscribed:
                Update(unsubscribed.Identity, p => p.Tracks.TryGetValue(unsubscribed.Sid, out var track) ? p with { Tracks = With(p.Tracks, track with { Subscribed = false }) } : p);
                break;
            case CallEvent.TrackMuted muted:
                Update(muted.Identity, p => p.Tracks.TryGetValue(muted.Sid, out var track) ? p with { Tracks = With(p.Tracks, track with { Muted = muted.Muted }) } : p);
                break;
            case CallEvent.LocalTrackPublished local:
                LocalTracks = new Dictionary<string, TrackInfo>(LocalTracks) { [local.Track.Sid] = local.Track };
                break;
            case CallEvent.LocalTrackUnpublished local:
                LocalTracks = LocalTracks.Where(p => p.Key != local.Sid).ToDictionary(p => p.Key, p => p.Value);
                break;
            case CallEvent.Speaking speaking:
            {
                var now = speaking.Identities.ToHashSet();
                People = People.Select(p => p.Speaking == now.Contains(p.Id) ? p : p with { Speaking = now.Contains(p.Id) }).ToList();
                MeSpeaking = now.Contains(LocalIdentity);
                // Written down in the same pass as the flag arrives: the web's
                // first version wrote it after the render that sorted by it.
                Speaking = Speaking.Hear(speaking.Identities, Identities, Now());
                break;
            }
            case CallEvent.Quality quality:
                if (quality.Identity == LocalIdentity) LocalQuality = quality.QualityValue;
                Update(quality.Identity, p => p with { Quality = quality.QualityValue });
                break;
            case CallEvent.Chat chat:
            {
                var line = new ChatLine(chat.Id, chat.Identity, chat.Name, chat.Text, DateTimeOffset.FromUnixTimeMilliseconds(chat.Timestamp).ToLocalTime(), Mine: false);
                // The web keeps the last 500 (chat.ts).
                Messages = Messages.Append(line).TakeLast(MessagesKept).ToList();
                Log.Line($"heard a message of {chat.Text.Length} characters from {chat.Identity}");
                if (!ChatOpen)
                {
                    Unread = true;
                    Arriving = Arriving.Append(line).ToList();
                    _ = Later(TimeSpan.FromMilliseconds(SaidForMs), _over.Token, () =>
                    {
                        Arriving = Arriving.Where(l => l.Id != line.Id).ToList();
                        Notify();
                    });
                }
                break;
            }
            case CallEvent.Data data:
                // Server messages come with no sender, and only those are
                // believed (host.ts, App.tsx).
                if (data.Identity == null && data.Topic is "moving" or "placing")
                {
                    _moving = (System.Text.Encoding.UTF8.GetString(data.Payload), DateTime.UtcNow);
                }
                else if (TomoshibiProtocolMethods.ReceivedReaction(data.Topic, data.Identity, data.Payload) is { } reaction && data.Identity is { } from)
                {
                    Show(reaction, from);
                }
                break;
            case CallEvent.AttributesChanged attributes:
                if (attributes.Identity == LocalIdentity) HandUp = attributes.Attributes.TryGetValue("hand", out var hand) && hand == "up";
                else Update(attributes.Identity, p => p with { Attributes = attributes.Attributes });
                break;
            case CallEvent.Connection connection:
                Connection = connection.Reading;
                break;
            case CallEvent.LosslessGaveUp:
                Announce(new CallToast(
                    T("Lossless sound stopped"),
                    Detail: T("Your connection could not keep up. Everyone still hears the sound in real time."),
                    Lasts: NoticeTimes.AtMost, Closable: true));
                break;
        }
        Heard?.Invoke(e);
        Notify();
    }

    /// <summary>Somebody else taking the stage with a share (notices.ts <c>onPublished</c>); only a track that was not already there.</summary>
    private void AnnounceShare(TrackInfo track, Person person)
    {
        var soundOnly = track.Source == MediaSource.ScreenShareAudio && track.Name is "listening" or "listening-lossless";
        if (track.Source != MediaSource.ScreenShare && !soundOnly) return;
        Announce(new CallToast(
            soundOnly ? T("{name} is sharing sound", ("name", person.Shown)) : T("{name} started sharing", ("name", person.Shown)),
            Kind: CallToast.Kinds.Signal, Lasts: NoticeTimes.AMoment));
    }

    private static IReadOnlyDictionary<string, TrackInfo> With(IReadOnlyDictionary<string, TrackInfo> tracks, TrackInfo track) =>
        new Dictionary<string, TrackInfo>(tracks) { [track.Sid] = track };

    private static IReadOnlyDictionary<string, TrackInfo> Without(IReadOnlyDictionary<string, TrackInfo> tracks, string sid) =>
        tracks.Where(p => p.Key != sid).ToDictionary(p => p.Key, p => p.Value);

    private void Upsert(ParticipantInfo participant)
    {
        var tracks = participant.Tracks.ToDictionary(t => t.Sid);
        var index = People.ToList().FindIndex(p => p.Id == participant.Identity);
        if (index >= 0)
        {
            var list = People.ToList();
            var was = list[index];
            var merged = new Dictionary<string, TrackInfo>(was.Tracks);
            foreach (var (sid, track) in tracks) merged[sid] = track;
            list[index] = was with { Name = participant.Name, Attributes = participant.Attributes, Tracks = merged };
            People = list;
        }
        else
        {
            People = People.Append(new Person(participant.Identity, participant.Name, participant.Attributes, tracks)).ToList();
        }
    }

    private void Update(string identity, Func<Person, Person> change)
    {
        var list = People.ToList();
        var index = list.FindIndex(p => p.Id == identity);
        if (index < 0) return;
        var changed = change(list[index]);
        if (changed == list[index]) return;
        list[index] = changed;
        People = list;
    }

    /// <summary>A line for the log: what has gone out, and every sound heard since the last reading.</summary>
    public string Reading()
    {
        var sent = $"sent video {_media.FramesSent} frames, audio {_media.SyntheticSource.ChunksPushed + (_microphoneFeed?.ChunksPushed ?? 0)} chunks";
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

    public void Dispose()
    {
        StopMedia();
        _playback?.Dispose();
        _camera?.Dispose();
        _microphone?.Dispose();
        Core.Dispose();
    }
}
