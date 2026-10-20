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
    /// <summary>Hands the build, <paramref name="version"/> as offered, to the system; returns once it is done, given up or failed (when it is done the app has usually been ended).</summary>
    Task<Installed> Install(string file, string version);

    /// <summary>Gives up an install under way, which then returns <see cref="Installed.Stopped"/>.</summary>
    void Stop();
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
/// given up and the build kept, since installing ends the app.
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
/// </summary>
public sealed class Updates
{
    public static readonly TimeSpan Every = TimeSpan.FromHours(6);
    public static readonly TimeSpan Retry = TimeSpan.FromMinutes(10);

    /// <summary>Where an update has got to.</summary>
    public abstract record Step
    {
        public sealed record None : Step;

        /// <summary>A newer build, and what went wrong with the last try at it, if anything did.</summary>
        public sealed record Offered(Offer Offer, string? Problem = null) : Step;

        public sealed record Downloading(Offer Offer, long Received, long Total) : Step;

        /// <summary>A download the person left the lobby during: stopped, kept, and carried on when they are back.</summary>
        public sealed record Held(Offer Offer, long Received, long Total) : Step;

        public sealed record Installing(Offer Offer) : Step;
    }

    /// <summary>What a check somebody asked for found, for the settings to say.</summary>
    public enum Said { Checking, Current, Failed }

    private readonly IUpdateSource _source;
    private readonly IUpdateInstaller _installer;
    private readonly IUpdateClock _clock;

    /// <summary>This build's version, dotted as the core and apps.json write it.</summary>
    public string Running { get; }

    public Step Now { get; private set; } = new Step.None();

    /// <summary>The answer to the last check asked for in the settings; null where none was asked or it found a build.</summary>
    public Said? Answer { get; private set; }

    public event Action? Changed;

    private readonly HashSet<string> _later = new();
    private bool _free;
    private bool _started;
    private IUpdateTransfer? _transfer;
    private Task _transferRun = Task.CompletedTask;
    private bool _holding;
    private bool _walkedAway;
    private (string Version, string File)? _ready;

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
        Changed?.Invoke();
    }

    /// <summary>Whether the banner is up: something to say, in the lobby, and not put away.</summary>
    public bool Shown => _free && Now switch
    {
        Step.None => false,
        Step.Offered offered => !_later.Contains(offered.Offer.Version),
        Step.Held => false,
        _ => true,
    };

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
        _ = Asking();

        async Task Asking()
        {
            while (true)
            {
                var answered = await Check(asked: false);
                await _clock.Wait(answered ? Every : Retry);
            }
        }
    }

    /// <summary>Told whenever the screen changes: the lobby is the only place an update is shown or worked on.</summary>
    public void Lobby(bool inLobby)
    {
        if (inLobby == _free) return;
        _free = inLobby;
        switch (Now)
        {
            case Step.Downloading downloading when !inLobby:
                _holding = true;
                Set(new Step.Held(downloading.Offer, downloading.Received, downloading.Total));
                _transfer?.Cancel();
                return;
            case Step.Installing when !inLobby:
                // Given up rather than let end the app in whatever the
                // person went on to: a call is ended by the install. The build
                // is kept for "Update" back in the lobby.
                _walkedAway = true;
                _installer.Stop();
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
        if (Answer == Said.Checking) return;
        Answer = Said.Checking;
        Changed?.Invoke();
        _ = Check(asked: true);
    }

    /// <summary>"Update": downloads the build offered, or installs it if it is already here.</summary>
    public void Accept()
    {
        if (Now is not Step.Offered offered || !_free) return;
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
        if (Now is not Step.Downloading) return;
        _holding = false;
        _transfer?.Cancel();
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

    /// <summary>"Cancel", on an install: given up, and the build kept for "Update" again.</summary>
    public void StopInstalling()
    {
        if (Now is not Step.Installing) return;
        _walkedAway = false;
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
        Set(new Step.Downloading(offer, 0, offer.Size));
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
                installed = await _installer.Install(file, offer.Version);
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
