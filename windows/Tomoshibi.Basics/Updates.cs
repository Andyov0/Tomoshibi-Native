using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Basics;

/// <summary>A newer build, as the app speaks of it.</summary>
public sealed class Offer
{
    public string Version { get; }
    /// <summary>Bytes.</summary>
    public long Size { get; }
    private readonly Func<string, string?> _notes;
    /// <summary>What the source needs to download it again; nobody else reads it.</summary>
    public object? Handle { get; }

    public Offer(string version, long size, Func<string, string?>? notes = null, object? handle = null)
    {
        Version = version;
        Size = size;
        _notes = notes ?? (_ => null);
        Handle = handle;
    }

    /// <summary>What changed, for somebody reading <paramref name="language"/> ("zh-Hans"); null where there is nothing to say in a language they read.</summary>
    public string? Notes(string language) => _notes(language);

    public override bool Equals(object? other) => other is Offer o && o.Version == Version && o.Size == Size;
    public override int GetHashCode() => HashCode.Combine(Version, Size);
    public override string ToString() => $"Offer({Version}, {Size})";
}

/// <summary>Where builds come from: the core, or a test's stand-in.</summary>
public interface IUpdateSource
{
    /// <summary>The newer build there is, null where this one is the newest; throws where the deployment could not be asked.</summary>
    Task<Offer?> Latest(string running);

    /// <summary>A download of <paramref name="offer"/>, which starts when it is run.</summary>
    IUpdateTransfer Download(Offer offer);

    /// <summary>Removes every build and part of one that was downloaded.</summary>
    void Clear();
}

/// <summary>One download, run once.</summary>
public interface IUpdateTransfer
{
    /// <summary>Where the build is once <see cref="Run"/> returns.</summary>
    string File { get; }

    /// <summary>Downloads the build, throwing an <see cref="UpdateFailure"/> where it does not arrive whole.</summary>
    Task Run(Action<long, long> progress);

    void Cancel();
}

/// <summary>Why a download did not finish (the core's UpdateError, as the app tells them apart).</summary>
public abstract class UpdateFailure(string message) : Exception(message)
{
    public sealed class Cancelled() : UpdateFailure("cancelled");
    public sealed class Broken(string detail) : UpdateFailure(detail);
    public sealed class Damaged(string detail) : UpdateFailure(detail);
    public sealed class Superseded() : UpdateFailure("superseded");
    public sealed class Storage(string detail) : UpdateFailure(detail);
    public sealed class Refused(string code) : UpdateFailure(code);
    public sealed class Other(string detail) : UpdateFailure(detail);
}

/// <summary>What became of an install.</summary>
public abstract record Installed
{
    public sealed record Done : Installed;

    /// <summary>Given up before it was done: the person left the lobby.</summary>
    public sealed record Stopped : Installed;

    public sealed record Failed(string Sentence) : Installed;
}

/// <summary>Where a build is installed: the system's package manager, or a test's stand-in.</summary>
public interface IUpdateInstaller
{
    /// <summary>
    /// Hands the build, <paramref name="version"/> as offered, to the system;
    /// returns once it is done, given up or failed (when it is done the app
    /// has usually been ended). <paramref name="started"/> is called, on the
    /// caller's thread, once the install has gone past the point where it
    /// can be given up: a setup program that has been started runs to its
    /// end whatever the app does.
    /// </summary>
    Task<Installed> Install(string file, string version, Action started);

    /// <summary>Gives up an install under way, which then returns <see cref="Installed.Stopped"/>; false where it has already started and cannot be.</summary>
    bool Stop();
}

/// <summary>Time, for the checks' six hours: the real one, or a test's that passes six hours in an instant.</summary>
public interface IUpdateClock
{
    Task Wait(TimeSpan time);
}

/// <summary>
/// The app's own updates: looking for a newer build, saying so, and with the
/// person's agreement downloading and installing it (Android's
/// model/Updates.kt; Updates.swift on the Mac).
///
/// The asking and the fetching are the core's (core/protocol/src/api/updates.rs),
/// which compares the versions and refuses a file that is not the build; what
/// is here is when to ask, what to say, and the one rule the owner set above
/// the rest: nothing about an update interrupts a call. An offer is shown
/// only in the lobby; one found during a call waits for the lobby; a download
/// is held when the person leaves the lobby and carries on, where it
/// stopped, when they are back; an install under way when they leave is
/// given up and the build kept, since installing ends the app; and while one
/// is under way no call is begun (<see cref="HoldsCalls"/>), since a setup
/// once started cannot be called back.
///
/// Asked once at start and every six hours after. A check that fails is kept
/// quiet and asked again in ten minutes; a notice for each would teach
/// somebody to ignore the one that matters. "Later" puts that version away
/// until the app is next started; a newer one is offered all the same. A
/// check somebody asks for in the settings always answers, "You have the
/// latest version." included, and brings back a version put away.
///
/// Windows asks nobody to confirm an install and has no permission to ask
/// for first, where Android has both. One thread, the caller's.
///
/// The download's bar is kept here (<see cref="Bar"/>), drawn at an even
/// pace behind what has arrived, and the install waits for it to be drawn
/// full: a build that came in faster than the bar went across was installed
/// with the bar a third of the way, and the setup ends the app, so the last
/// thing somebody saw of the download was a leap to an install. Not for
/// longer than <see cref="FillAtMost"/>, since a window with no frames (one
/// minimised) draws nothing, and an install is not held for a bar nobody sees.
///
/// The deployment can say how often to look (<see cref="CheckEvery"/>, its
/// update_check_hours), and switch the whole of it off (<see cref="Allow"/>,
/// Remote's app-updates): then nothing is looked for or offered and the
/// settings say nothing about updates, but a download or an install already
/// under way is seen through, since one stopped half way is a download
/// somebody agreed to and lost (Android's Updates.kt the same).
/// </summary>
public sealed class Updates
{
    public static readonly TimeSpan Every = TimeSpan.FromHours(6);
    public static readonly TimeSpan Retry = TimeSpan.FromMinutes(10);

    /// <summary>
    /// The longest an install waits after the last byte for the bar to be
    /// drawn full: the whole bar takes <see cref="Pace.Whole"/> at twenty
    /// frames a second or more, and this is more than twice that.
    /// </summary>
    public static readonly TimeSpan FillAtMost = TimeSpan.FromSeconds(3);

    /// <summary>Where an update has got to.</summary>
    public abstract record Step
    {
        public sealed record None : Step;

        /// <summary>A newer build, and what went wrong with the last try at it, if anything did.</summary>
        public sealed record Offered(Offer Offer, string? Problem = null) : Step;

        public sealed record Downloading(Offer Offer, long Received, long Total) : Step;

        /// <summary>A download the person left the lobby during: stopped, kept, and carried on when they are back.</summary>
        public sealed record Held(Offer Offer, long Received, long Total) : Step;

        /// <summary>
        /// Being installed; <paramref name="Started"/> once the install can no
        /// longer be given up (<see cref="IUpdateInstaller.Install"/>), when
        /// there is no Cancel to offer.
        /// </summary>
        public sealed record Installing(Offer Offer, bool Started = false) : Step;
    }

    /// <summary>What a check somebody asked for found, for the settings to say.</summary>
    public enum Said { Checking, Current, Failed }

    private readonly IUpdateSource _source;
    private readonly IUpdateInstaller _installer;
    private readonly IUpdateClock _clock;

    /// <summary>This build's version, dotted as the core and apps.json write it.</summary>
    public string Running { get; }

    /// <summary>Whether updates are looked for and offered at all (<see cref="Allow"/>).</summary>
    public bool Allowed { get; private set; } = true;

    /// <summary>How long between looks; the deployment can say (<see cref="CheckEvery"/>).</summary>
    public TimeSpan Interval { get; private set; } = Every;

    public Step Now { get; private set; } = new Step.None();

    /// <summary>
    /// The download's bar as it is drawn, aimed at how much has arrived and
    /// moved on by the frames of whatever draws it (the lobby's banner, the
    /// settings' row); one for the model, so that a banner drawn again partway
    /// (the window made narrow) carries on from where the last one was.
    /// </summary>
    public Pace Bar { get; } = new();

    /// <summary>The answer to the last check asked for in the settings; null where none was asked or it found a build.</summary>
    public Said? Answer { get; private set; }

    public event Action? Changed;

    private readonly HashSet<string> _later = new();
    private bool _free;
    private bool _started;
    /// <summary>Moved on by every new schedule of looks, so that a wait in the one before ends in nothing.</summary>
    private int _schedule;
    private IUpdateTransfer? _transfer;
    private Task _transferRun = Task.CompletedTask;
    private bool _holding;
    private bool _walkedAway;
    private (string Version, string File)? _ready;
    /// <summary>A download that has arrived whole and waits for its bar to be drawn full before it is installed.</summary>
    private IUpdateTransfer? _filling;

    public Updates(IUpdateSource source, IUpdateInstaller installer, string running, IUpdateClock clock)
    {
        _source = source;
        _installer = installer;
        Running = running;
        _clock = clock;
    }

    private void Set(Step step)
    {
        Now = step;
        if (step is Step.Downloading downloading) Bar.Aim(downloading.Total > 0 ? (double)downloading.Received / downloading.Total : 0);
        Changed?.Invoke();
    }

    /// <summary>
    /// Whether the banner is up: something to say, in the lobby, and not put
    /// away; with updates switched off, only a download or an install that
    /// was already under way.
    /// </summary>
    public bool Shown => _free && Now switch
    {
        Step.None => false,
        Step.Offered offered => Allowed && !_later.Contains(offered.Offer.Version),
        Step.Held => false,
        _ => true,
    };

    /// <summary>Whether the settings say anything about updates: unless they are switched off, and then still while one is under way, for its progress and its Cancel.</summary>
    public bool Speaks => Allowed || Now is Step.Downloading or Step.Held or Step.Installing;

    /// <summary>
    /// Whether a call may not be begun now: while a build is being installed,
    /// which ends the app. The install is started off the window's thread,
    /// so the lobby goes on answering while the system looks the setup over
    /// (eight to eleven seconds for the 145 MB setup in the VM, Defender
    /// reading all of it); a call begun in those seconds would be ended by the
    /// install as it began, and once the setup has started nothing can call it
    /// back. The
    /// app refuses to begin one and says why; where the install can still be
    /// given up (a package's), its Cancel is there for somebody who would
    /// rather be in the call.
    /// </summary>
    public bool HoldsCalls => Now is Step.Installing;

    /// <summary>The build being offered, downloaded or installed, whatever the step.</summary>
    public Offer? Offer => Now switch
    {
        Step.Offered s => s.Offer,
        Step.Downloading s => s.Offer,
        Step.Held s => s.Offer,
        Step.Installing s => s.Offer,
        _ => null,
    };

    /// <summary>Starts asking: now, and every six hours after. Once for the life of the model.</summary>
    public void Start()
    {
        if (_started) return;
        _started = true;
        // Whatever an earlier run downloaded was installed, failed, or was
        // left behind; none of it is trusted to be the build offered now.
        try
        {
            _source.Clear();
        }
        catch (Exception)
        {
        }
        if (Allowed) Arrange(lookingFirst: true);
    }

    /// <summary>
    /// The looking, every <see cref="Interval"/>, from now or from one wait
    /// from now. Any schedule before is given up, a wait in it included: it
    /// wakes, sees it is not the schedule any more, and looks no more.
    /// </summary>
    private void Arrange(bool lookingFirst)
    {
        var mine = ++_schedule;
        _ = Asking();

        async Task Asking()
        {
            if (!lookingFirst)
            {
                await _clock.Wait(Interval);
                if (mine != _schedule) return;
            }
            while (true)
            {
                var answered = await Check(asked: false);
                if (mine != _schedule) return;
                await _clock.Wait(answered ? Interval : Retry);
                if (mine != _schedule) return;
            }
        }
    }

    /// <summary>
    /// Whether to look for and offer updates (the deployment's app-updates).
    /// Switched off, the looking stops and an offer is taken off the screen;
    /// a download or an install under way goes on to its end. Switched on
    /// again, the app looks at once, since the offer it missed may be days
    /// old.
    /// </summary>
    public void Allow(bool on)
    {
        if (on == Allowed) return;
        Allowed = on;
        if (_started)
        {
            if (on) Arrange(lookingFirst: true);
            else _schedule++;
        }
        Changed?.Invoke();
    }

    /// <summary>
    /// How long between looks (the deployment's update_check_hours, the
    /// app's six hours where it says none). A change counts from now: the
    /// next look is one new wait away, not at once, since it is said at every
    /// start, moments after the first look.
    /// </summary>
    public void CheckEvery(TimeSpan interval)
    {
        if (interval == Interval) return;
        Interval = interval;
        if (_started && Allowed) Arrange(lookingFirst: false);
    }

    /// <summary>Told whenever the screen changes: the lobby is the only place an update is shown or worked on.</summary>
    public void Lobby(bool inLobby)
    {
        if (inLobby == _onLobby) return;
        _onLobby = inLobby;
        Reconsider();
    }

    /// <summary>
    /// Told when a join is out and again when it is over, landed or not.
    ///
    /// A join pressed in the lobby leaves it only once it lands, seconds
    /// later: the start and join sheet asks whether the meeting is live,
    /// then the deployment for a grant, then the room to let it in, all with
    /// the lobby's screen behind the sheet. An install that came to its swap
    /// in those seconds saw a lobby, replaced the app and Windows ended it,
    /// and the call being entered ended as it began. So a join out counts as
    /// the lobby left, from the press; one that is refused or called off is
    /// back in it, and a download held for it carries on.
    /// </summary>
    public void Joining(bool outOnAJoin)
    {
        if (outOnAJoin == _settingOff) return;
        _settingOff = outOnAJoin;
        Reconsider();
    }

    private bool _onLobby;
    private bool _settingOff;

    private void Reconsider()
    {
        var inLobby = _onLobby && !_settingOff;
        if (inLobby == _free) return;
        _free = inLobby;
        switch (Now)
        {
            case Step.Downloading downloading when !inLobby && _filling != null:
                // Whole, its bar not yet drawn full: kept, and installed only
                // once the person is back and says so, as a build that
                // arrives in the moment they leave is.
                Keep(downloading.Offer);
                return;
            case Step.Downloading downloading when !inLobby:
                _holding = true;
                Set(new Step.Held(downloading.Offer, downloading.Received, downloading.Total));
                _transfer?.Cancel();
                return;
            case Step.Installing when !inLobby:
                // Given up rather than let end the app in whatever the
                // person went on to: a call is ended by the install. The build
                // is kept for "Update" back in the lobby. One that has
                // started goes on (HoldsCalls keeps a call from being begun
                // meanwhile), and the app goes with it.
                _walkedAway = true;
                if (!_installer.Stop()) _walkedAway = false;
                Changed?.Invoke();
                return;
            case Step.Held held when inLobby:
                Download(held.Offer);
                return;
        }
        Changed?.Invoke();
    }

    /// <summary>The settings' "Check for updates".</summary>
    public void CheckNow()
    {
        if (!Allowed || Answer == Said.Checking) return;
        Answer = Said.Checking;
        Changed?.Invoke();
        _ = Check(asked: true);
    }

    /// <summary>"Update": downloads the build offered, or installs it if it is already here.</summary>
    public void Accept()
    {
        if (Now is not Step.Offered offered || !_free || !Allowed) return;
        if (_ready is { } ready && ready.Version == offered.Offer.Version && System.IO.File.Exists(ready.File)) Install(offered.Offer, ready.File);
        else Download(offered.Offer);
    }

    /// <summary>"Later": puts this version away until the app is next started, and lets go of anything it left.</summary>
    public void Later()
    {
        if (Offer is not { } offer) return;
        if (Now is Step.Downloading or Step.Held or Step.Installing) return;
        if (_ready?.Version == offer.Version) Forget();
        _later.Add(offer.Version);
        Set(new Step.Offered(offer));
    }

    /// <summary>"Cancel", on a download. What arrived is kept, so "Update" again carries on from there.</summary>
    public void Cancel()
    {
        if (Now is not Step.Downloading downloading) return;
        // Pressed in the second the bar takes to finish, with every byte
        // already here: nothing is left to stop, and the build is kept.
        if (_filling != null)
        {
            Keep(downloading.Offer);
            return;
        }
        _holding = false;
        _transfer?.Cancel();
    }

    /// <summary>Lets the install of a build whose bar is still finishing go, the build kept for "Update" again.</summary>
    private void Keep(Offer offer)
    {
        _ready = (offer.Version, _filling!.File);
        _filling = null;
        Set(new Step.Offered(offer));
    }

    /// <summary>
    /// Puts the model at <paramref name="step"/> and asks nothing of the
    /// deployment: for pictures of each step (a debug launch's
    /// TOMOSHIBI_SHOW=update:…), as Android's screenshot tests set theirs.
    /// </summary>
    public void Pose(Step step, Said? answer = null)
    {
        _started = true;
        Answer = answer;
        Set(step);
    }

    /// <summary>"Cancel", on an install: given up, and the build kept for "Update" again; nothing once the install has started, when Cancel is no longer offered.</summary>
    public void StopInstalling()
    {
        if (Now is not Step.Installing { Started: false }) return;
        _walkedAway = false;
        // Refused by an install that started in the moment before the press,
        // before it had said so: it goes on, and says so now.
        _installer.Stop();
    }

    private async Task<bool> Check(bool asked)
    {
        Offer? found;
        try
        {
            found = await _source.Latest(Running);
        }
        catch (Exception)
        {
            if (asked)
            {
                Answer = Said.Failed;
                Changed?.Invoke();
            }
            return false;
        }
        if (found == null)
        {
            if (asked) Answer = Said.Current;
            // A build offered and since taken back is not offered any more.
            if (Now is Step.Offered) Set(new Step.None());
            else Changed?.Invoke();
            return true;
        }
        if (asked)
        {
            Answer = null;
            _later.Remove(found.Version);
        }
        switch (Now)
        {
            case Step.None:
                Set(new Step.Offered(found));
                break;
            case Step.Offered offered when offered.Offer.Version != found.Version:
                Set(new Step.Offered(found));
                break;
            default:
                // Whatever is under way goes on; a newer build is offered once it is over.
                Changed?.Invoke();
                break;
        }
        return true;
    }

    private void Download(Offer offer)
    {
        // The one before is waited for rather than abandoned: a transfer is
        // stopped by its own cancel, and the core would otherwise go on
        // appending to the part the new one appends to.
        var previous = _transferRun;
        _transfer?.Cancel();
        var transfer = _source.Download(offer);
        _transfer = transfer;
        _holding = false;
        // Carried on from where it was held, it keeps what it had and its
        // bar, since the core goes on from there; any other starts empty.
        Set(Now is Step.Held held && held.Offer.Version == offer.Version
            ? new Step.Downloading(offer, held.Received, held.Total)
            : new Step.Downloading(offer, 0, offer.Size));
        _transferRun = Run();

        async Task Run()
        {
            try
            {
                await previous;
            }
            catch (Exception)
            {
            }
            Exception? outcome = null;
            try
            {
                await transfer.Run((received, total) =>
                {
                    if (Now is Step.Downloading now && now.Offer.Version == offer.Version && ReferenceEquals(_transfer, transfer)) Set(now with { Received = received, Total = total });
                });
            }
            catch (Exception fault)
            {
                outcome = fault;
            }
            if (!ReferenceEquals(_transfer, transfer)) return;
            _transfer = null;
            switch (outcome)
            {
                case null when _free:
                    // The bar drawn full first, then the install, which
                    // ends the app (the class's summary). Cancel or leaving
                    // the lobby meanwhile lets go of the wait and keeps the
                    // build (Keep).
                    _filling = transfer;
                    // All of it, checked: said whole, which wakes whatever
                    // draws the bar should it have stood still at the last report.
                    if (Now is Step.Downloading arrived) Set(arrived with { Received = arrived.Total });
                    Bar.Aim(1);
                    if (!Bar.Full) await Task.WhenAny(Bar.WhenFull(), _clock.Wait(FillAtMost));
                    if (!ReferenceEquals(_filling, transfer)) return;
                    _filling = null;
                    Install(offer, transfer.File);
                    break;
                case null:
                    // Arrived in the moment the person left the lobby: kept,
                    // and installed only once they are back and say so.
                    _ready = (offer.Version, transfer.File);
                    Set(new Step.Offered(offer));
                    break;
                case UpdateFailure.Cancelled:
                    if (_holding && Now is Step.Held) return;
                    Set(new Step.Offered(offer));
                    break;
                case UpdateFailure.Superseded:
                    // The deployment has a newer build now; ask what it is
                    // rather than offer one nobody is handing out.
                    Set(new Step.None());
                    _ = Check(asked: false);
                    break;
                default:
                    Set(new Step.Offered(offer, Explain(outcome)));
                    break;
            }
        }
    }

    private void Install(Offer offer, string file)
    {
        if (!_free)
        {
            _ready = (offer.Version, file);
            Set(new Step.Offered(offer));
            return;
        }
        Set(new Step.Installing(offer));
        _walkedAway = false;
        _ = Installing();

        async Task Installing()
        {
            Installed installed;
            try
            {
                installed = await _installer.Install(file, offer.Version, () =>
                {
                    if (Now is Step.Installing { Started: false } now && now.Offer.Version == offer.Version) Set(now with { Started = true });
                });
            }
            catch (Exception)
            {
                installed = new Installed.Failed(T("The update could not be installed."));
            }
            switch (installed)
            {
                // The system replaces the app and ends this process as it
                // does; were this ever reached, nothing is left to say.
                case Installed.Done:
                    Forget();
                    Set(new Step.None());
                    break;
                // Given up because the person left the lobby: the build is
                // kept, so that "Update" again installs it without
                // downloading it, and nothing is said about it.
                case Installed.Stopped:
                    _ready = (offer.Version, file);
                    Set(_walkedAway ? new Step.Offered(offer) : new Step.Offered(offer, T("The update was not installed.")));
                    break;
                case Installed.Failed failed:
                    Forget();
                    Set(new Step.Offered(offer, failed.Sentence));
                    break;
            }
        }
    }

    /// <summary>Lets go of a build that arrived: once it is installed, has failed, or has been put away.</summary>
    private void Forget()
    {
        _ready = null;
        try
        {
            _source.Clear();
        }
        catch (Exception)
        {
        }
    }

    private static string Explain(Exception fault) => fault switch
    {
        UpdateFailure.Broken => T("The download stopped. Check your connection and try again."),
        UpdateFailure.Damaged => T("The download was damaged and has been deleted. Try again."),
        UpdateFailure.Storage => T("The update could not be saved on this device."),
        _ => T("The update could not be downloaded. Try again later."),
    };
}
