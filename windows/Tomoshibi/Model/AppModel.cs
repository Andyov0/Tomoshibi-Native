using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model;

/// <summary>The core's refusals, as the shared sentences take them.</summary>
internal static class Faults
{
    public static Failure Of(Exception fault) => fault switch
    {
        ProtocolException.Network => new Failure.Network(),
        ProtocolException.Refused refused => new Failure.Refused(refused.code),
        _ => new Failure.Other(),
    };

    /// <summary>The code of a refusal, or null for anything else.</summary>
    public static string? Code(Exception fault) => (fault as ProtocolException.Refused)?.code;
}

/// <summary>
/// Where the app is, and everything the screens before a call need: the
/// Apple apps' AppModel and Android's model/AppModel.kt, decision for
/// decision.
///
/// It follows the web's App.tsx and doorway.ts, with one step the web never
/// needs: a browser is already at its deployment, while an app has to be
/// told which one. After that the order is the web's: who is signed in and
/// what the deployment allows decide between the lobby and the join form,
/// an invitation in a link wins over everything, and a call that the server
/// moves is joined again on its own.
///
/// Every method is for the UI thread; the awaits come back to it.
/// </summary>
internal sealed class AppModel : Observable
{
    public enum Screen
    {
        /// <summary>The deployment the app opens on is being asked about; nothing to fill in yet.</summary>
        Starting,
        Unreachable,
        Lobby,
        Prejoin,
        Knocking,
        Waiting,
        Call,
        Ended,
    }

    public const string NameKey = "meet-live.name";
    public const string RelayKey = "meet-live.relay";

    private readonly Launch _launch;
    private readonly CallMedia _media;
    private readonly IDefaults _defaults;
    private readonly ISecrets _secrets;

    /// <summary>A test is driving the app, and nothing it does is kept (Launch.Rehearsal).</summary>
    public bool Rehearsal => _launch.Rehearsal;

    /// <summary>
    /// A test with nowhere to keep things: a rehearsal not given a store of
    /// its own (TOMOSHIBI_KEEP). One that was keeps the rooms it went to, as
    /// a person's app does, so that a lobby with a history can be looked at.
    /// </summary>
    public bool Forgets => Rehearsal && _launch.KeepIn == null;

    public Launch Launch => _launch;
    public CallMedia Media => _media;
    public IDefaults Defaults => _defaults;

    public AppModel(Launch launch, CallMedia media, IDefaults defaults, ISecrets secrets)
    {
        _launch = launch;
        _media = media;
        // A rehearsal keeps nothing, unless it was given a store of its own.
        _defaults = Forgets ? new MemoryDefaults() : defaults;
        _secrets = Forgets ? new MemorySecrets() : secrets;
        // The deployment this app is for, from the core, which holds a
        // release build to that one address; a test names its own. No
        // screen ever shows it: the app is for this deployment and not for
        // anybody's own.
        _serverAddress = launch.Base ?? TomoshibiProtocolMethods.HomeDeployment();
        _name = _defaults.String(NameKey) ?? "";
        // A relay the person picked on their own deployment means nothing on
        // a test's.
        _relayChoice = Rehearsal ? null : _defaults.String(RelayKey);
        // A rehearsal's own store (TOMOSHIBI_KEEP) keeps the devices picked,
        // so that a test can see the next launch start on them; one without
        // keeps them only in memory, as everything else.
        DeviceMemory = new DeviceMemory(Rehearsal && _launch.KeepIn == null ? null : _defaults);
        var joining = DeviceMemory.JoiningChoice;
        _startWithMicrophone = joining.Microphone;
        _startWithCamera = joining.Camera;
        Lobby = new LobbyModel(this, _defaults);
        media.CameraId = DeviceMemory.Choice(DeviceMemory.Kind.Camera);
        media.MicrophoneId = DeviceMemory.Choice(DeviceMemory.Kind.Microphone);
        // Blurred from the first frame where blur is chosen, the join
        // screen's picture of yourself included (PreJoin.tsx asks the same of
        // the web's preview). Blur that cannot be done here is turned off and
        // said, rather than the whole picture staying blurred with nobody in
        // it (Android's CallMedia).
        media.Blurring = launch.Blur || DeviceMemory.Blur;
        // The deployment's ceiling over a share, read as each share starts
        // and each change of one is made (Sharing.Ceiling).
        media.Sharing.Ceiling = () => _remote.Ceiling;
        var ui = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        _ui = ui;
        media.BlurFailed += () => ui?.TryEnqueue(() =>
        {
            SetBlur(false);
            Call?.PressFailed();
        });
    }

    /// <summary>Whether the background behind the camera is blurred (CallMedia.Blurring).</summary>
    public bool Blurring => _media.Blurring;

    /// <summary>Blurs the background or stops, from the next frame, and remembers it as the web does (<c>meet-live.blur</c>).</summary>
    public void SetBlur(bool on)
    {
        _media.Blurring = on;
        DeviceMemory.Blur = on;
        Notify();
    }

    private Screen _screen = Screen.Starting;
    public Screen Showing
    {
        get => _screen;
        private set
        {
            if (value != _screen) Log.Line($"screen {value.ToString().ToLowerInvariant()}");
            Set(ref _screen, value);
            Updates?.Lobby(value == Screen.Lobby);
            _remote.Lobby(value == Screen.Lobby);
        }
    }

    private Remote _remote = new(null, null, new RealUpdateClock());

    /// <summary>
    /// What the deployment says to the apps between their releases
    /// (Basics.Remote): phrases laid over, features switched off, ceilings,
    /// and the lobby's notice. Told of every change of screen, the lobby being
    /// where the notice shows and coming back to it a time to ask again.
    /// Without one given, it asks nothing and offers everything the app was
    /// built with: a debug build started by hand.
    /// </summary>
    public Remote Remote
    {
        get => _remote;
        set
        {
            _remote.Changed -= FollowRemote;
            _remote = value;
            value.Changed += FollowRemote;
            value.Lobby(_screen == Screen.Lobby);
            FollowRemote();
        }
    }

    /// <summary>What follows what the deployment says, outside the screens, which read it themselves: the updates and the blur.</summary>
    private void FollowRemote()
    {
        FollowUpdates();
        FollowBlur();
        Notify();
    }

    private void FollowUpdates()
    {
        if (_updates is not { } updates) return;
        updates.Allow(_remote.Allows(RemoteFeature.AppUpdates));
        updates.CheckEvery(_remote.Answer.UpdateInterval ?? Basics.Updates.Every);
    }

    /// <summary>
    /// The blur from here on, by the deployment's background-blur
    /// (RemoteFeatures.Blur): left as it is in a call, and out of one what
    /// was chosen where blur is allowed. Not on a move, which leaves the
    /// meeting and joins it again by itself, and is the same meeting.
    /// </summary>
    private void FollowBlur()
    {
        if (MovingTo != null) return;
        _media.Blurring = RemoteFeatures.Blur(_launch.Blur || DeviceMemory.Blur, _remote.Allows(RemoteFeature.BackgroundBlur), inCall: _call != null, _media.Blurring);
    }

    /// <summary>Whether the blur's switch is offered (the deployment's background-blur).</summary>
    public bool BlurOffered => _remote.Allows(RemoteFeature.BackgroundBlur);

    private Updates? _updates;

    /// <summary>
    /// The app's own updates (Basics.Updates), told of every change of
    /// screen, since the lobby is the only place one is shown or worked on.
    /// None in a debug build started by hand, which has no test deployment to
    /// ask and would ask the home one.
    /// </summary>
    public Updates? Updates
    {
        get => _updates;
        set
        {
            _updates = value;
            value?.Lobby(_screen == Screen.Lobby);
            value?.Joining(Joining);
            FollowUpdates();
        }
    }

    public Deployment? Deployment { get; private set; }
    public DeploymentInfo? Info { get; private set; }

    private Account? _account;
    public Account? Account
    {
        get => _account;
        private set => Set(ref _account, value);
    }

    private CallModel? _call;
    public CallModel? Call
    {
        get => _call;
        private set
        {
            Set(ref _call, value);
            // Out of the call: a blur the deployment switched off during it
            // counts as off from here, for the next meeting.
            if (value == null) FollowBlur();
            FollowAwake();
        }
    }

    /// <summary>The window's thread, which keeps the display awake (KeepAwake) and lives as long as the app.</summary>
    private readonly Microsoft.UI.Dispatching.DispatcherQueue? _ui;

    private readonly KeepAwake _awake = new(Interop.Power.Ask);

    /// <summary>
    /// The display and the machine kept awake while there is a call, or one
    /// being moved to another relay, which has none for the moment between
    /// leaving the old room and joining the new, and let go once there is
    /// neither. Asked on the window's thread whichever thread the change
    /// came on, since the state is the asking thread's.
    /// </summary>
    private void FollowAwake()
    {
        void Ask()
        {
            var wanted = _call != null || _movingTo != null;
            if (wanted == _awake.Held) return;
            var took = _awake.Hold(wanted);
            Log.Line(took ? (wanted ? "keeping the display awake" : "letting the display sleep") : "the display could not be kept awake");
        }
        if (_ui == null || _ui.HasThreadAccess) Ask();
        else _ui.TryEnqueue(Ask);
    }

    private RelayList? _relays;
    public RelayList? Relays
    {
        get => _relays;
        private set
        {
            Set(ref _relays, value);
            DropRelayNotOffered();
        }
    }

    private int _asking;
    /// <summary>
    /// Something is being asked of the deployment on the person's behalf, and
    /// the buttons that would ask it again wait. Counted, because the asking
    /// overlaps: a join that was called off is still out until it comes back.
    /// </summary>
    public bool Busy => _asking > 0;

    private void Ask(int by)
    {
        _asking += by;
        Notify();
    }

    public Ending Ending { get; private set; } = Ending.Left;

    private string? _problem;
    private bool _telling;
    /// <summary>
    /// Something to tell the person before a call, at the foot of the
    /// screen. Most of them fade (notices.ts <c>actionFailed</c>);
    /// <see cref="ProblemStays"/> marks the ones about why they are not in a
    /// meeting, which stay until dismissed (notices.ts <c>joinFailed</c>).
    /// </summary>
    public string? Problem
    {
        get => _problem;
        set
        {
            _problem = value;
            if (!_telling) ProblemStays = false;
            Notify();
        }
    }

    public bool ProblemStays { get; private set; }

    /// <summary>Says something that stays until the person closes it.</summary>
    public void Tell(string sentence)
    {
        _telling = true;
        ProblemStays = true;
        Problem = sentence;
        _telling = false;
    }

    /// <summary>
    /// Refuses to begin a call while a build is being installed
    /// (Updates.HoldsCalls), and says why: the install ends the app, and the
    /// lobby is no longer frozen while it starts, so a press that used to
    /// fall on a frozen window would now reach a call that the update ended
    /// as it began. Every way into a call comes through here: the start and
    /// join sheet, a link (pasted, handed over by the system, or a meeting's
    /// row), the join screen.
    /// </summary>
    public bool RefusedForUpdate()
    {
        if (Updates?.HoldsCalls != true) return false;
        Log.Line("refused a call: an update is being installed");
        Problem = T("Tomoshibi is installing an update and will open again in a moment. Start or join the meeting then.");
        return true;
    }

    private readonly string _serverAddress;

    private string _room = "";
    public string Room
    {
        get => _room;
        set => Set(ref _room, value);
    }

    private string _name;
    /// <summary>
    /// The web's own storage name, so the habit carries over. Only ever what
    /// the person typed: an invitation's name is <see cref="InvitedName"/>
    /// and is never written here (PreJoin.tsx: "Not a name somebody else
    /// chose for them").
    /// </summary>
    public string Name
    {
        get => _name;
        set
        {
            if (Set(ref _name, value)) _defaults.Set(NameKey, value);
        }
    }

    private string _passphrase = "";
    public string Passphrase
    {
        get => _passphrase;
        set
        {
            if (Set(ref _passphrase, value)) _secrets.Write(PassphraseKey, value);
        }
    }

    private string _sealingWord = "";
    /// <summary>Never stored, as the web never stores it (secrecy.ts).</summary>
    public string SealingWord
    {
        get => _sealingWord;
        set => Set(ref _sealingWord, value);
    }

    private string? _relayChoice;
    /// <summary>
    /// Null picks whichever relay answers fastest. Kept under the web's own
    /// name (api.ts <c>rememberRelay</c>), and dropped again whenever the
    /// deployment stops offering it.
    /// </summary>
    public string? RelayChoice
    {
        get => _relayChoice;
        set
        {
            if (Set(ref _relayChoice, value)) _defaults.Set(RelayKey, value);
        }
    }

    /// <summary>Where the device choices are kept: nowhere in a rehearsal.</summary>
    public DeviceMemory DeviceMemory { get; }

    private bool _startWithMicrophone;
    private bool _startWithCamera;
    /// <summary>
    /// What starts on when somebody joins (preferences.ts <c>meet-live.devices</c>):
    /// the microphone on and the camera off unless somebody chose otherwise.
    /// Written as it is switched, so the settings and the join screen are one
    /// choice and not two that disagree.
    /// </summary>
    public bool StartWithMicrophone
    {
        get => _startWithMicrophone;
        set
        {
            if (Set(ref _startWithMicrophone, value)) RememberDevices();
        }
    }

    public bool StartWithCamera
    {
        get => _startWithCamera;
        set
        {
            if (Set(ref _startWithCamera, value)) RememberDevices();
        }
    }

    private void RememberDevices() => DeviceMemory.Remember(new DeviceMemory.Joining(StartWithCamera, StartWithMicrophone));

    /// <summary>
    /// A device picked in the settings (MediaSettings.kt <c>DeviceChoice</c>):
    /// remembered at once, as the page's choice is a preference rather than
    /// a switch, and taken at once by whatever is using that kind of device:
    /// the picture and the meter on the page, and a call that is on, which
    /// moves to it (or back to the system's, for null) as it would from its
    /// own devices panel.
    /// </summary>
    public async Task ChooseDevice(DeviceMemory.Kind kind, string? id)
    {
        DeviceMemory.Remember(kind, id);
        try
        {
            switch (kind)
            {
                case DeviceMemory.Kind.Camera:
                    if (Call is { } withCamera) await withCamera.UseCamera(id);
                    else await _media.UseCamera(id);
                    break;
                case DeviceMemory.Kind.Microphone:
                    if (Call is { } withMicrophone) await withMicrophone.UseMicrophone(id);
                    else await _media.CheckMicrophone(id);
                    break;
                default:
                    if (Call is { } withSpeakers) await withSpeakers.UseSpeakers(id);
                    break;
            }
        }
        catch (Exception fault)
        {
            // The call has said so; the choice stays for the next one.
            Log.Line($"{kind} not taken now: {fault.Message}");
        }
    }

    /// <summary>The devices picked are forgotten (the settings' Forget): from now on, the system's.</summary>
    public async Task ForgetDevices()
    {
        foreach (var kind in Enum.GetValues<DeviceMemory.Kind>()) await ChooseDevice(kind, null);
        DeviceMemory.ForgetChoices();
    }

    public string? Invite { get; private set; }
    public InviteInfo? Invitation { get; private set; }

    /// <summary>The arranged meeting a <c>?meeting=</c> link points at, while it is waited for (meeting.ts, Waiting.tsx).</summary>
    public Arrangement? Arrangement { get; private set; }
    public WaitingState Waiting { get; private set; } = new();
    private string? _meetingToken;
    private CancellationTokenSource? _waitTask;
    private CancellationTokenSource? _knockTask;

    /// <summary>Moved on by every new picture, so the old one is not shown from a cache under the same address.</summary>
    public int PictureVersion { get; private set; }

    /// <summary>The arranged meetings, recent rooms and measured relays the screens before a call show.</summary>
    public LobbyModel Lobby { get; }

    /// <summary>A rehearsal's identity and its proof, for its own joins only, as a fresh browser tab has one of its own.</summary>
    private KeptIdentity _rehearsalIdentity = new();

    private string BaseUrl => Deployment?.BaseUrl() ?? _serverAddress;
    private string PassphraseKey => "passphrase:" + BaseUrl;
    private string CookieKey => "account:" + BaseUrl;

    /// <summary>The relays' light, asked again every minute while no call is showing.</summary>
    private Microsoft.UI.Dispatching.DispatcherQueueTimer? _relayTimer;

    // Reaching a deployment

    private bool _started;

    /// <summary>
    /// Picks up where the last launch left off, or does what a test's launch
    /// asks (Android's AppModel.start). Once for the life of the model.
    /// </summary>
    public async Task Start()
    {
        if (_started) return;
        _started = true;
        // Asked first, so that an answer already says whether to look for
        // updates; the kept one (Restore) said so before the window opened.
        _remote.Start();
        Updates?.Start();
        _relayTimer = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread().CreateTimer();
        _relayTimer.Interval = TimeSpan.FromSeconds(60);
        _relayTimer.Tick += async (_, _) =>
        {
            if (Showing != Screen.Call) await RefreshRelays();
        };
        _relayTimer.Start();

        if (Rehearsal)
        {
            if (_launch.Name != null) Name = _launch.Name;
            StartWithMicrophone = _launch.Microphone;
            StartWithCamera = _launch.Camera;
            await Connect();
            if (_launch.SignInName != null && _launch.SignInPassphraseFile != null)
            {
                // Read into a variable and never written anywhere: the file is
                // the test's, and the log is read over a shared folder.
                var passphrase = (await File.ReadAllTextAsync(_launch.SignInPassphraseFile)).Trim();
                await SignIn(_launch.SignInName, passphrase);
            }
            if (_launch.Passphrase != null) Passphrase = _launch.Passphrase;
            if (_launch.SealingWord != null) SealingWord = _launch.SealingWord;
            if (_launch.Autojoin != null)
            {
                Prepare(_launch.Autojoin);
                await Join();
            }
            else if (_launch.Prepare != null)
            {
                Prepare(_launch.Prepare);
            }
            else if ((_launch.Open ?? OpenedWith) is { } link)
            {
                await Open(link);
            }
            else if (_launch.Ended != null)
            {
                Ending = _launch.Ended switch
                {
                    "closed" => Ending.Closed,
                    "removed" => Ending.Removed,
                    "lost" => Ending.Lost,
                    _ => Ending.Left,
                };
                if (_launch.Prepare == null && Room.Length == 0) Room = "friday-standup";
                Showing = Screen.Ended;
            }
            return;
        }
        await Connect();
        if (OpenedWith is { } opened) await Open(opened);
    }

    /// <summary>The link the app was launched with (Program.OpenedWith): opened once the deployment has answered.</summary>
    public string? OpenedWith { get; set; }

    public async Task Connect()
    {
        // Nothing here may take the screen off a call that is live; the only
        // way from a call to another deployment is to leave it first.
        if (Call != null || MovingTo != null) return;
        Ask(+1);
        try
        {
            var address = _serverAddress.Trim();
            if (!address.Contains("://")) address = "https://" + address;
            var deployment = new Deployment(address);
            if (_secrets.Read("account:" + deployment.BaseUrl()) is { } cookie)
            {
                try
                {
                    deployment.RestoreAccountCookie(cookie);
                }
                catch (Exception)
                {
                    Log.Line("the kept session could not be restored");
                }
            }
            var info = await deployment.DeploymentInfo();
            Deployment = deployment;
            Info = info;
            _passphrase = _secrets.Read(PassphraseKey) ?? "";
            try
            {
                Account = await deployment.Me();
            }
            catch (Exception)
            {
                Account = null;
            }
            Log.Line(Account != null ? "reached the deployment, signed in" : "reached the deployment");
            Problem = null;
            Showing = Room.Length == 0 ? Screen.Lobby : Screen.Prejoin;
            _ = RefreshRelays();
            if (Account != null) _ = Lobby.RefreshArrangements();
        }
        catch (Exception fault)
        {
            Log.Line($"the deployment did not answer: {fault.GetType().Name}");
            Tell(Explain(fault));
            // Opening on a deployment that will not answer leaves somebody
            // where they can try again, rather than on the mark.
            if (Showing == Screen.Starting) Showing = Screen.Unreachable;
        }
        finally
        {
            Ask(-1);
        }
    }

    /// <summary>Asks again how much of the fleet is answering, for the light in the corner (Lobby.tsx <c>Fleet</c>).</summary>
    public async Task RefreshRelays()
    {
        if (Deployment is not { } deployment) return;
        try
        {
            Relays = await deployment.Relays();
        }
        catch (Exception)
        {
        }
    }

    /// <summary>Whether there is a machine to choose at all (PreJoin.tsx <c>offerChoice</c>): more than one relay, on a deployment that acts on the choice.</summary>
    public bool OffersRelayChoice => Relays is { } relays && relays.Measure && relays.Relays.Length > 1;

    /// <summary>
    /// Whether the join screen and the meeting's waiting screen offer the
    /// server picker: where there is a choice, to somebody who did not come
    /// to them through the lobby. Somebody signed in who starts or joins from
    /// the lobby has its picker in the sheet they press Join in; somebody who
    /// opened an invitation or a meeting's link, signed in or not, or who is
    /// not signed in at all, never sees the lobby's picker, and could not
    /// choose (the owner, 2026-10-19). The same rule as the iPhone and Android
    /// (<c>offersRelayChoiceAtTheDoor</c>), which is the web's.
    /// </summary>
    public bool OffersRelayChoiceAtTheDoor => OffersRelayChoice && (Account == null || ArrivedByLink);

    /// <summary>
    /// The screen before a call was reached by a link (an invitation, a
    /// meeting, a room's address), handed over by the system or pasted, and
    /// not by the lobby's own join. Kept until the person goes back to the
    /// lobby, so a join that is refused, or a move that fails, comes back to
    /// the same screen with the same picker on it.
    /// </summary>
    public bool ArrivedByLink { get; private set; }

    /// <summary>A relay remembered from before that this deployment does not offer, or no longer takes calls on, would be sent with every join and refused.</summary>
    private void DropRelayNotOffered()
    {
        if (RelayChoice is not { } choice || Relays is not { } relays) return;
        if (!relays.Relays.Any(r => r.Name == choice && !r.Maintenance)) RelayChoice = null;
    }

    // Links

    /// <summary>
    /// An invitation, an arranged meeting or a room, from a link
    /// (<c>https://&lt;deployment&gt;/?invite=T#/room</c> and its kin). Not
    /// over a call: the call stays, and says how to get to the link.
    ///
    /// The packaged app is handed <c>tomoshibi://</c> links by the system (its
    /// manifest registers the scheme), at launch or while it runs
    /// (Program); a link pasted into the sign-in screen or the join bar comes
    /// here too, as does TOMOSHIBI_OPEN.
    /// </summary>
    public async Task Open(string link)
    {
        if (Call is { } call)
        {
            call.Announce(new CallToast(T("Leave this call to open that link."), Lasts: NoticeTimes.AtMost));
            return;
        }
        if (MovingTo != null) return;
        if (RefusedForUpdate()) return;
        CancelJoin();
        if (OpenedLink.Parse(link) is not { } parts) return;
        // A link opened at launch can arrive before the deployment has
        // answered; it is this deployment's or nobody's.
        if (Deployment == null) await Connect();
        if (Deployment is not { } deployment) return;
        // A link to another server is refused rather than followed: the app
        // is for this deployment alone.
        if (!parts.IsFor(deployment.BaseUrl()))
        {
            Tell(T("That link is for another server."));
            return;
        }
        if (parts.Meeting != null)
        {
            if (parts.Room.Length > 0) Room = parts.Room;
            ArrivedByLink = true;
            Wait(parts.Meeting);
            return;
        }
        if (parts.Invite != null)
        {
            InviteInfo? said;
            try
            {
                said = await deployment.InviteInfo(parts.Invite);
            }
            catch (Exception)
            {
                said = null;
            }
            // An invitation that is no good is still somebody sent here on
            // purpose, and they are told which of three things it is, because
            // they are three different next moves (App.tsx).
            var error = said?.Error ?? (said == null ? "no_such_invite" : null);
            if (error != null)
            {
                Invite = null;
                Invitation = null;
                var from = Refusals.Instant(said?.From);
                if (error == "invite_not_yet" && from is { } opens) Tell(T("That invitation opens {when}. Come back then.", ("when", LobbyModel.Said(opens))));
                else if (error == "invite_expired") Tell(T("That meeting has ended."));
                else if (error == "rate_limited") Tell(T("Too many attempts. Try again in a moment."));
                else Tell(T("That invitation is no longer good. Ask for another."));
                ArrivedByLink = false;
                Showing = Screen.Lobby;
                return;
            }
            Invite = parts.Invite;
            Invitation = said;
            Room = said?.Room ?? parts.Room;
        }
        else if (parts.Room.Length > 0)
        {
            // A link with a room and nothing else is a room name, and only
            // somebody signed in is taken anywhere by one of those.
            if (Account == null)
            {
                Tell(T("{room} is by invitation. Ask the organiser for a link.", ("room", parts.Room)));
                ArrivedByLink = false;
                Showing = Screen.Lobby;
                return;
            }
            Room = parts.Room;
        }
        if (Room.Length > 0)
        {
            ArrivedByLink = true;
            Showing = Screen.Prejoin;
        }
        Notify();
    }

    /// <summary>The name an invitation was made for, which the guest will wear whatever is typed (PreJoin.tsx <c>locked</c>).</summary>
    public string? InvitedName => string.IsNullOrEmpty(Invitation?.Name) ? null : Invitation!.Name;

    /// <summary>Somebody who arrived on an invitation link, who has a name to choose and nothing else to answer.</summary>
    public bool ArrivedInvited => Invitation != null;

    /// <summary>The name this person will be called in the room: the account's, the invitation's, or what they typed.</summary>
    public string JoiningName => Account?.Name ?? InvitedName ?? Name.Trim();

    // Account

    public async Task SignIn(string name, string passphrase)
    {
        if (Deployment is not { } deployment) return;
        Ask(+1);
        try
        {
            Account = await deployment.SignIn(name, passphrase);
            _secrets.Write(CookieKey, deployment.AccountCookie());
            Log.Line("signed in");
            Problem = null;
            _ = Lobby.RefreshArrangements();
        }
        catch (Exception fault)
        {
            Log.Line($"sign-in refused: {fault.GetType().Name}");
            Problem = Explain(fault);
        }
        finally
        {
            Ask(-1);
        }
    }

    public async Task SignOut()
    {
        if (Deployment is not { } deployment) return;
        try
        {
            await deployment.SignOut();
        }
        catch (Exception)
        {
        }
        _secrets.Write(CookieKey, null);
        Account = null;
        Log.Line("signed out");
    }

    /// <summary>Account.tsx <c>Picture</c>: the picture as a data URL, already scaled; the sentence to say if it did not take.</summary>
    public async Task<string?> SetPicture(string dataUrl)
    {
        if (Deployment is not { } deployment) return null;
        try
        {
            Account = await deployment.SetAvatar(dataUrl);
            PictureVersion++;
            Notify();
            return null;
        }
        catch (Exception)
        {
            return T("That picture could not be used.");
        }
    }

    public async Task<string?> RemovePicture()
    {
        if (Deployment is not { } deployment) return null;
        try
        {
            Account = await deployment.RemoveAvatar();
            PictureVersion++;
            Notify();
            return null;
        }
        catch (Exception)
        {
            return T("That picture could not be used.");
        }
    }

    /// <summary>Account.tsx <c>Passphrase</c>; null when it took.</summary>
    public async Task<string?> ChangePassphrase(string current, string next)
    {
        if (Deployment is not { } deployment) return null;
        try
        {
            Account = await deployment.ChangePassphrase(current, next);
            return null;
        }
        catch (Exception fault)
        {
            return Refusals.ExplainPassphrase(Faults.Of(fault));
        }
    }

    /// <summary>Account.tsx <c>GuestLinks</c>: a standing link into a scope's room, or the sentence why not.</summary>
    public async Task<(MadeInvite? Made, string? Refusal)> MakeGuestLink(string room, string name, DateTimeOffset? from, DateTimeOffset? until)
    {
        if (Deployment is not { } deployment) return (null, T("That link could not be made. Try again."));
        try
        {
            return (await deployment.CreateStandingInvite(room, name.Trim(), from?.ToUnixTimeMilliseconds(), until?.ToUnixTimeMilliseconds()), null);
        }
        catch (Exception fault)
        {
            return (null, Refusals.ExplainLink(Faults.Of(fault)));
        }
    }

    /// <summary>Stops every link into a room, returning how many stopped.</summary>
    public async Task<(uint? Count, string? Refusal)> StopGuestLinks(string room)
    {
        if (Deployment is not { } deployment) return (null, T("That link could not be made. Try again."));
        try
        {
            return (await deployment.RevokeInvites(room, null), null);
        }
        catch (Exception fault)
        {
            return (null, Refusals.ExplainLink(Faults.Of(fault)));
        }
    }

    /// <summary>
    /// Whether the account holds a permission, by what the server said; a
    /// server older than permissions said nothing, which reads as the
    /// defaults (manage/grants.ts <c>DEFAULTS</c>: everything but Emby).
    /// </summary>
    public bool Holds(string permission)
    {
        if (Account is not { } account) return false;
        if (account.Can is { } can) return can.Contains(permission);
        return permission != "emby";
    }

    /// <summary>Where the account's picture is, or null: an absolute address as it is, a path on the deployment otherwise, with the version that keeps a cache honest.</summary>
    public string? AvatarAddress
    {
        get
        {
            if (string.IsNullOrEmpty(Account?.Avatar)) return null;
            var avatar = Account!.Avatar!;
            if (avatar.StartsWith("http://") || avatar.StartsWith("https://")) return avatar;
            var path = avatar.StartsWith('/') ? avatar : "/" + avatar;
            return BaseUrl.TrimEnd('/') + path + (PictureVersion > 0 ? "?" + PictureVersion : "");
        }
    }

    // Joining

    /// <summary>
    /// Only somebody signed in, or holding an invitation, is taken to a room
    /// by its name: the app is a way in for the people who belong to the
    /// deployment and for the people they send links to (the owner's rule,
    /// 2026-10-15).
    /// </summary>
    public void Prepare(string room)
    {
        if (Call != null || MovingTo != null) return;
        if (RefusedForUpdate()) return;
        if (Account == null && Invite == null && !Rehearsal)
        {
            Tell(T("Rooms here are by invitation. Open the link you were sent, or sign in."));
            return;
        }
        Room = room.Trim().ToLowerInvariant();
        Problem = null;
        Showing = Screen.Prejoin;
    }

    /// <summary>Straight into a room by name from the start and join sheet, which has already taken the choices the join screen would.</summary>
    public async Task Enter(string room)
    {
        if (Account == null && Invite == null && !Rehearsal)
        {
            Tell(T("Rooms here are by invitation. Open the link you were sent, or sign in."));
            return;
        }
        Room = room.Trim().ToLowerInvariant();
        Problem = null;
        await Join();
    }

    /// <summary>A join is out and has not yet landed in its call, or was called off and has not yet come back. One at a time.</summary>
    public bool Joining { get; private set; }

    /// <summary>Moved on by <see cref="CancelJoin"/>; a join compares the value it started with after every wait.</summary>
    private int _joinAttempt;

    /// <summary>Calls off a join that has not yet landed: Back on the join screen, Cancel on the start and join sheet, leaving the door or the wait.</summary>
    public void CancelJoin()
    {
        if (!Joining) return;
        _joinAttempt++;
    }

    public Task Join() => Join(relay: null);

    /// <summary>The relay to ask for on this join alone, in place of the one chosen; a move sends "" (App.tsx ~620).</summary>
    private async Task Join(string? relay)
    {
        if (Deployment is not { } deployment) return;
        if (Joining) return;
        if (RefusedForUpdate()) return;
        Joining = true;
        Updates?.Joining(true);
        Ask(+1);
        var attempt = _joinAttempt;
        var landed = false;
        try
        {
            // A join while a call is live replaces it; the call it replaces
            // is left first, rather than going on publishing out of sight.
            if (Call is { } old)
            {
                Call = null;
                await old.Leave();
            }
            RememberDevices();
            // Signed in, the web sends the account's name and no passphrase:
            // the session is the proof (api.ts). A guest of a room held under
            // a scope is minted a mark of their own and a passphrase is
            // ignored, so none is sent (PreJoin.tsx `scopedGuest`).
            var signed = Account != null;
            var scopedGuest = ArrivedInvited && TomoshibiProtocolMethods.ScopeOf(Room).Length > 0;
            Log.Line($"joining {Room}");
            var kept = Rehearsal ? _rehearsalIdentity : KeptIdentity.Read(_defaults, BaseUrl);
            var grant = await deployment.Join(
                Room, JoiningName,
                signed || scopedGuest ? "" : Passphrase,
                kept.Identity,
                kept.Proof,
                relay ?? RelayChoice ?? "",
                Invite);
            // Every join's identity is kept, with its proof or with none:
            // signing in or out makes the server issue a new one, a stale one
            // sent back is refused, and an issued one sent back without the
            // proof it came with is replaced by a fresh one.
            var answered = new KeptIdentity(grant.Identity, grant.Proof);
            if (Rehearsal) _rehearsalIdentity = answered;
            else KeptIdentity.Keep(answered, _defaults, BaseUrl);
            if (attempt != _joinAttempt) return;
            var forward = grant.Forward is { } f ? new IceServer(f.Url, f.Username, f.Credential) : null;
            var options = new CallOptions(grant.Url, grant.Token, grant.Room, SealingWord.Length == 0 ? null : SealingWord, forward);
            var model = await CallModel.Join(options, deployment, grant.Region, _media, DeviceMemory);
            // Called off while the room was being entered: the room was
            // entered all the same, and is left at once.
            if (attempt != _joinAttempt)
            {
                await model.Leave();
                model.Dispose();
                return;
            }
            model.Relay = grant.Relay;
            model.Holding = grant.Holding;
            model.Moved += to => _ = Rejoin(model, to);
            model.EndedBy += reason => _ = EndedFor(reason, model);
            Call = model;
            landed = true;
            Joining = false;
            Room = grant.Room;
            Lobby.Remember(grant.Room);
            Problem = null;
            Showing = Screen.Call;
            Log.Line($"in call {grant.Room} as {model.LocalIdentity} with {model.People.Count} others");
            // After joining rather than before, so somebody is in the room
            // the moment they join and their devices come up a beat later,
            // each on its own: a refused microphone is one person's problem
            // with their own device, not a room that would not open.
            if (StartWithMicrophone && Call == model) await model.SetMicrophone(true);
            if (StartWithCamera && Call == model) await model.SetCamera(true);
            if (_launch.Share is { } share && Rehearsal && Call == model)
            {
                if (share is "screen" or "both") await model.ShareScreen();
                if (share is "sound" or "both") await model.ShareSound();
            }
            if (_launch.Watch is { } link && Rehearsal && Call == model) _ = WatchFromLaunch(model, link);
            if (_launch.QuitAfterSeconds is { } seconds && Rehearsal) _ = QuitAfter(seconds);
        }
        catch (ProtocolException fault)
        {
            // Why they are not in the meeting, which stays until read; a join
            // that was called off has nothing to say.
            if (attempt != _joinAttempt) return;
            Log.Line($"the join was refused: {Faults.Code(fault) ?? fault.GetType().Name}");
            Tell(Explain(fault));
        }
        catch (Exception fault)
        {
            if (attempt != _joinAttempt) return;
            Log.Line($"the join failed: {fault.GetType().Name} {fault.Message}");
            Tell(T("Could not join {room}.", ("room", Room)));
        }
        finally
        {
            if (!landed) Joining = false;
            // A join that lands leaves the lobby by the screen changing,
            // which the update is told of; one that does not, lands back in
            // the lobby, and a download held for it carries on.
            Updates?.Joining(false);
            Ask(-1);
        }
    }

    /// <summary>
    /// The launch's video, added once the server has said this person may
    /// start a show (TOMOSHIBI_WATCH): asked of the server once the call is
    /// joined, so the answer is waited for, a tenth of a second at a time,
    /// for at most ten seconds.
    /// </summary>
    private static async Task WatchFromLaunch(CallModel model, string link)
    {
        var engine = Watch.WatchEngine.Of(model);
        for (var i = 0; i < 100 && !engine.Ready && !model.Over.IsCancellationRequested; i++) await Task.Delay(100);
        if (!model.Over.IsCancellationRequested) await engine.Add(link);
    }

    private async Task QuitAfter(int seconds)
    {
        await Task.Delay(TimeSpan.FromSeconds(seconds));
        Log.Line("leaving, as the launch asked");
        await Leave();
        Microsoft.UI.Xaml.Application.Current.Exit();
    }

    /// <summary>
    /// The server moved the call to another relay; the web rejoins at once
    /// with the same passphrase and word, and lets the server choose, for
    /// this join only (App.tsx ~620 and ~676). Only for the call showing.
    /// </summary>
    private async Task Rejoin(CallModel moved, string to)
    {
        if (Call != moved) return;
        MovingTo = to;
        try
        {
            Call = null;
            await moved.Leave();
            await Join(relay: "");
            // A join that failed left the screen on the call with no call in
            // it. Back to the join screen, with the reason and the Join button.
            if (Call == null && Showing == Screen.Call) Showing = Screen.Prejoin;
        }
        finally
        {
            MovingTo = null;
        }
    }

    private string? _movingTo;
    /// <summary>Where a move is taking the meeting while there is no call to show it over.</summary>
    public string? MovingTo
    {
        get => _movingTo;
        private set
        {
            Set(ref _movingTo, value);
            FollowAwake();
        }
    }

    public Task Leave() => End(Ending.Left);

    /// <summary>
    /// The call ended without anybody here pressing leave, for the media
    /// server's reason. Only while it is the call showing: one already
    /// replaced would otherwise end the one still working.
    /// </summary>
    private async Task EndedFor(string reason, CallModel model)
    {
        if (model != Call) return;
        if (model == _leaving) return;
        await End(Refusals.EndingFor(reason));
    }

    /// <summary>The call being left, until it has gone.</summary>
    private CallModel? _leaving;

    private async Task End(Ending how)
    {
        Ending = how;
        var model = Call;
        _leaving = model;
        try
        {
            if (model != null) await model.Leave();
        }
        finally
        {
            _leaving = null;
        }
        if (Call == model)
        {
            // The ended screen first, then no call: the other way round, the
            // call screen with no call in it gave way to the join screen for
            // an instant before the ended screen replaced it.
            Showing = Screen.Ended;
            Call = null;
        }
        // A relay that has just dropped a call is the one thing known about
        // the fleet; without forgetting the measurement it would go on being
        // named the fastest for minutes (App.tsx, the lost connection).
        TomoshibiProtocolMethods.ForgetRelayMeasurement();
        Notify();
    }

    /// <summary>Leaves the call this was asked of, if it is still the one showing (ControlBar.kt <c>leaveCall</c>).</summary>
    public async Task LeaveCall(CallModel call)
    {
        if (Call == call) await Leave();
        else await call.Leave();
    }

    public void BackToLobby()
    {
        if (Call != null) return;
        CancelJoin();
        Batch(() =>
        {
            ArrivedByLink = false;
            Showing = Screen.Lobby;
            Problem = null;
            Invite = null;
            Invitation = null;
            StopWaitingTasks();
            Arrangement = null;
        });
    }

    // An arranged meeting

    /// <summary>
    /// Waiting.tsx, which is a join screen whose button waits. Nothing is
    /// asked of the meeting but what it is until somebody presses: the web's
    /// first draft joined the moment the host arrived, and somebody still
    /// typing their name was joined with none.
    /// </summary>
    public sealed record WaitingState(bool Asking = false, WaitingState.GoneHow? Gone = null, bool Unreachable = false)
    {
        public enum GoneHow { Cancelled, Ended, NotComing }
    }

    private void Wait(string token)
    {
        StopWaitingTasks();
        _meetingToken = token;
        Arrangement = null;
        Waiting = new WaitingState();
        Problem = null;
        Showing = Screen.Waiting;
        // Read on arrival so the screen can say what it is for, and again,
        // slowly, until it can (Waiting.tsx `RETRY_EVERY`).
        var task = _waitTask = new CancellationTokenSource();
        _ = Run(async () =>
        {
            while (!task.IsCancellationRequested)
            {
                if (await Look() != null) return;
                if (Waiting.Gone != null) return;
                await Task.Delay(30_000, task.Token);
            }
        });
    }

    private static async Task Run(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (OperationCanceledException)
        {
        }
    }

    /// <summary>Reads the meeting once.</summary>
    private async Task<Arrangement?> Look()
    {
        if (Deployment is not { } deployment || _meetingToken is not { } token) return null;
        try
        {
            var said = await deployment.Arranged(token);
            Arrangement = said;
            Room = said.Room;
            Waiting = Waiting with { Unreachable = false, Gone = said.Ended ? WaitingState.GoneHow.Ended : Waiting.Gone };
            Notify();
            return said;
        }
        catch (ProtocolException.Refused fault)
        {
            Waiting = fault.code == "no_such_meeting" ? Waiting with { Gone = WaitingState.GoneHow.Cancelled } : Waiting with { Unreachable = true };
            Notify();
            return null;
        }
        catch (Exception)
        {
            Waiting = Waiting with { Unreachable = true };
            Notify();
            return null;
        }
    }

    /// <summary>Whether the meeting has begun and carries the way in, so the button is the ordinary Join.</summary>
    public bool ArrangedReady => Arrangement is { } a && a.Started && a.Invite != null;

    /// <summary>When the host may begin it: the server begins a meeting only from <c>from</c>.</summary>
    public DateTimeOffset? ArrangedOpens => Arrangement is { } a ? Refusals.Instant(a.From ?? a.At) : null;

    /// <summary>The one button: join where it has begun, begin it where it is the host's, otherwise wait for the host.</summary>
    public async Task GoArranged()
    {
        if (Arrangement is not { } arrangement) return;
        if (ArrangedReady && arrangement.Invite is { } invite)
        {
            Invite = invite;
            await Join();
            return;
        }
        // The host does not wait for the host.
        if (arrangement.Mine)
        {
            await Join();
            return;
        }
        AskForArranged();
    }

    /// <summary>Asks at the web's pace until the meeting begins (then joins with the invitation it carries), ends, or the host is two hours late.</summary>
    private void AskForArranged()
    {
        _waitTask?.Cancel();
        Waiting = Waiting with { Asking = true, Gone = null };
        Notify();
        var task = _waitTask = new CancellationTokenSource();
        _ = Run(async () =>
        {
            while (!task.IsCancellationRequested)
            {
                var said = await Look();
                if (task.IsCancellationRequested) return;
                if (said?.Ended == true || Waiting.Gone != null)
                {
                    Waiting = Waiting with { Asking = false };
                    Notify();
                    return;
                }
                if (said != null && said.Started && said.Invite is { } invite)
                {
                    Waiting = Waiting with { Asking = false };
                    Invite = invite;
                    Notify();
                    await Join();
                    return;
                }
                if (said != null && Refusals.Instant(said.At) is { } at &&
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() - at.ToUnixTimeMilliseconds() > (long)TomoshibiProtocolMethods.HostNotComingAfterMs())
                {
                    Waiting = Waiting with { Asking = false, Gone = WaitingState.GoneHow.NotComing };
                    Notify();
                    return;
                }
                // A read that failed is not a reason to ask faster.
                var pause = said != null ? (long)TomoshibiProtocolMethods.MeetingAskEveryMs(said.At, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()) : 30_000L;
                await Task.Delay(TimeSpan.FromMilliseconds(pause), task.Token);
            }
        });
    }

    /// <summary>"Ask again", after the host was said not to be coming.</summary>
    public void AskAgain() => AskForArranged();

    public void StopWaiting()
    {
        // The button that waits may already have turned into a join.
        CancelJoin();
        StopWaitingTasks();
        Arrangement = null;
        _meetingToken = null;
        Waiting = new WaitingState();
        Showing = Screen.Lobby;
        Notify();
    }

    private void StopWaitingTasks()
    {
        _waitTask?.Cancel();
        _waitTask = null;
        Waiting = Waiting with { Asking = false };
    }

    // The door

    /// <summary>
    /// Asks the room's host to let this person in, then waits, under the name
    /// the room will know them by: somebody signed in knocked as the guest
    /// name this device last typed, and was let in as somebody else.
    /// </summary>
    public async Task Knock()
    {
        if (Deployment is not { } deployment) return;
        var from = Showing;
        try
        {
            var ticket = await deployment.Knock(Room, JoiningName);
            // Somebody who went back while the knock was being sent is not
            // put at the door afterwards.
            if (Showing != from) return;
            Showing = Screen.Knocking;
            _knockTask?.Cancel();
            var task = _knockTask = new CancellationTokenSource();
            var giveUpAfter = (long)TomoshibiProtocolMethods.KnockGiveUpAfterMs();
            var started = Environment.TickCount64;
            _ = Run(async () =>
            {
                while (!task.IsCancellationRequested)
                {
                    await Task.Delay(TimeSpan.FromMilliseconds(TomoshibiProtocolMethods.KnockAskEveryMs()), task.Token);
                    // A round that did not answer is not the end of the
                    // waiting: the person inside has not gone anywhere.
                    DoorAnswer? answer;
                    try
                    {
                        answer = await deployment.AtTheDoor(Room, ticket.Token);
                    }
                    catch (Exception)
                    {
                        answer = null;
                    }
                    if (task.IsCancellationRequested) return;
                    if (answer != null)
                    {
                        if (answer.State == DoorState.Admitted)
                        {
                            Invite = answer.Invite;
                            await Join();
                            // Let in and then refused at the join: the reason
                            // is on the join screen, with its button.
                            if (Call == null && Showing == Screen.Knocking) Showing = Screen.Prejoin;
                            return;
                        }
                        if (answer.State == DoorState.Refused)
                        {
                            Tell(T("Nobody let you in. Ask for a link."));
                            Showing = Screen.Prejoin;
                            return;
                        }
                    }
                    // knock.ts `GIVE_UP_AFTER`, counted whatever the rounds
                    // said: a server that never answered at all left the
                    // person waiting for as long as the app was open.
                    if (Environment.TickCount64 - started > giveUpAfter)
                    {
                        Tell(T("Nobody let you in. Ask for a link."));
                        Showing = Screen.Prejoin;
                        return;
                    }
                }
            });
        }
        catch (Exception)
        {
            if (Showing != from) return;
            Problem = T("That could not be sent.");
        }
    }

    public void StopKnocking()
    {
        _knockTask?.Cancel();
        _knockTask = null;
        // Let in a moment ago, and already joining.
        CancelJoin();
        Showing = Screen.Prejoin;
    }

    // Words

    /// <summary>What the server refused, in the reader's language (api.ts <c>explain</c>).</summary>
    public string Explain(Exception fault) => Refusals.Explain(Faults.Of(fault), Room, SafeScope(Room));

    /// <summary>Whether the person could ask to be let in instead (PreJoin.tsx ~521).</summary>
    public bool CanKnock => Refusals.CanKnock(Problem, Room, SafeScope(Room));

    public static string SafeScope(string room)
    {
        try
        {
            return TomoshibiProtocolMethods.ScopeOf(room);
        }
        catch (Exception)
        {
            return "";
        }
    }
}
