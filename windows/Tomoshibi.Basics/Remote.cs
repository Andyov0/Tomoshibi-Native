using System.Text.Json;

namespace Tomoshibi.Basics;

/// <summary>
/// What the deployment says to the apps between their releases (the core's
/// api/remote.rs, <c>GET /api/apps/remote</c>): wording laid over the app's
/// own phrases, features switched off, ceilings over what the app would
/// otherwise use, and a notice for the lobby. The owner, 2026-10-22: a small
/// change should not each time need a new build. Android's model/Remote.kt
/// and the Apple apps' Remote.swift, by the same rules and under the same
/// names.
///
/// The last answer is kept in the app's preferences, and read back before
/// the first screen is drawn (<see cref="Restore"/>), so that a feature
/// switched off stays off on a train and a phrase laid over does not flicker
/// back to the app's own at every start. Then the deployment is asked: at
/// start, every half hour after, and again whenever the window comes to the
/// front or the app back to the lobby (<see cref="Refresh"/>). An answer
/// replaces what applied and is kept; a question that is not answered
/// changes nothing and says nothing, since what it would have said is
/// whatever was said last time. With nothing kept and nothing heard, the app
/// is exactly what it was built as.
///
/// The asking, the reading and the working out by language are the core's
/// (IRemoteSource, the app's CoreRemoteSource); here are when to ask, what to
/// keep, and what follows, apart from the core so that the rules are tested
/// where the core cannot be loaded. One thread, the caller's.
/// </summary>
public sealed class Remote
{
    /// <summary>Every half hour (the owner's, in the spec of 2026-10-22).</summary>
    public static readonly TimeSpan Every = TimeSpan.FromMinutes(30);

    /// <summary>How soon a question asked for by coming back may follow the one before.</summary>
    public static readonly TimeSpan Soonest = TimeSpan.FromMinutes(1);

    /// <summary>Where the last answer and the notices closed are kept, under the Apple apps' and Android's names.</summary>
    public const string KeptKey = "tomoshibi.remote";
    public const string ClosedKey = "tomoshibi.remote.closed";

    /// <summary>
    /// How many closed notices are remembered: a notice's id is the
    /// deployment's and is never put up again once it is taken down, so the
    /// oldest only take up room.
    /// </summary>
    public const int ClosedHeld = 20;

    /// <summary>The longest one wait for a notice to run out lasts; a longer wait is made of several (Task.Delay takes no more than 24 days).</summary>
    private static readonly TimeSpan LongestWait = TimeSpan.FromDays(1);

    private readonly IRemoteSource? _source;
    private readonly IDefaults? _store;
    private readonly IUpdateClock _clock;
    private readonly Func<DateTimeOffset> _now;
    private readonly Localization _localization;
    private readonly List<string> _closed;
    private bool _inLobby;
    private bool _started;
    private bool _asking;
    private DateTimeOffset? _lastAsked;
    private int _expiry;

    /// <param name="source">Null asks nothing and reads nothing: a debug build started by hand, which has no test deployment to ask and would ask the real one.</param>
    /// <param name="store">Where the answer and the notices closed are kept; null for nowhere.</param>
    /// <param name="localization">Where the phrases laid over go: the app's own (Localization.Shared) unless a test gives one.</param>
    public Remote(IRemoteSource? source, IDefaults? store, IUpdateClock clock, Func<DateTimeOffset>? now = null, Localization? localization = null)
    {
        _source = source;
        _store = store;
        _clock = clock;
        _now = now ?? (() => DateTimeOffset.UtcNow);
        _localization = localization ?? Localization.Shared;
        _closed = ReadClosed(store);
    }

    /// <summary>What applies now: the last answer, or what was kept from the one before, or nothing.</summary>
    public RemoteAnswer Answer { get; private set; } = RemoteAnswer.Nothing;

    /// <summary>Told whenever what applies changes: an answer kept or heard, a notice closed or run out, the lobby come or gone.</summary>
    public event Action? Changed;

    /// <summary>Whether a feature is offered (the answer's <c>off</c>).</summary>
    public bool Allows(RemoteFeature feature) => Answer.Allows(feature);

    /// <summary>The ceiling over a shared screen.</summary>
    public ShareCeiling Ceiling => Answer.Ceiling;

    /// <summary>The notice for the lobby, while the lobby is the screen: none in a call, none once it has run out, and none once closed here.</summary>
    public LobbyNotice? Notice
    {
        get
        {
            if (!_inLobby) return null;
            if (Answer.Notice is not { } notice) return null;
            if (_closed.Contains(notice.Id)) return null;
            if (notice.Until is { } until && until <= _now()) return null;
            return notice;
        }
    }

    /// <summary>The notice's close: this id is not shown again on this device, even after the app is started again.</summary>
    public void Close(LobbyNotice notice)
    {
        if (_closed.Contains(notice.Id)) return;
        _closed.Add(notice.Id);
        if (_closed.Count > ClosedHeld) _closed.RemoveRange(0, _closed.Count - ClosedHeld);
        _store?.Set(ClosedKey, JsonSerializer.Serialize(_closed));
        Changed?.Invoke();
    }

    /// <summary>Applies the answer kept from before, for the first screen to be drawn with: called before it is, as the language is settled before it is.</summary>
    public void Restore()
    {
        if (_source == null || _store?.String(KeptKey) is not { } kept) return;
        RemoteAnswer? read;
        try
        {
            read = _source.Read(kept);
        }
        catch (Exception)
        {
            read = null;
        }
        if (read != null) Apply(read);
    }

    /// <summary>Starts asking: now, and every half hour after. Once.</summary>
    public void Start()
    {
        if (_started || _source == null) return;
        _started = true;
        _ = Asking();

        async Task Asking()
        {
            while (true)
            {
                await Ask();
                await _clock.Wait(Every);
            }
        }
    }

    /// <summary>
    /// Asks again, on coming back to the front or to the lobby, unless a
    /// question is out or one went a moment ago: the window comes to the
    /// front and lands in the lobby in the same second it starts, which would
    /// be three questions for one answer.
    /// </summary>
    public void Refresh()
    {
        if (!_started || _asking) return;
        if (_lastAsked is { } last && _now() - last < Soonest) return;
        _ = Ask();
    }

    /// <summary>Told whenever the screen changes. Coming back to the lobby asks again, since a call can be long and the lobby is where what was said shows.</summary>
    public void Lobby(bool here)
    {
        if (here == _inLobby) return;
        _inLobby = here;
        Changed?.Invoke();
        if (here) Refresh();
    }

    private async Task Ask()
    {
        if (_source == null || _asking) return;
        _asking = true;
        _lastAsked = _now();
        RemoteAnswer heard;
        try
        {
            heard = await _source.Fetch();
        }
        catch (Exception)
        {
            // Kept quiet: what applied goes on applying.
            return;
        }
        finally
        {
            _asking = false;
        }
        _store?.Set(KeptKey, heard.Kept);
        Apply(heard);
    }

    private void Apply(RemoteAnswer heard)
    {
        // The same answer again, as every half hour mostly is: nothing to
        // draw again, and the notice's wait goes on as it was.
        if (heard.Kept.Length > 0 && heard.Kept == Answer.Kept && heard.Notice?.Id == Answer.Notice?.Id) return;
        Answer = heard;
        _localization.LaidOver = heard.Phrases;
        Expire();
        Changed?.Invoke();
    }

    /// <summary>
    /// Takes the notice down when it runs out, if nothing is heard before: a
    /// notice that says the server is down until nine would still be saying so
    /// at ten to somebody who had not left the lobby, since nothing else draws
    /// it again then.
    /// </summary>
    private void Expire()
    {
        var mine = ++_expiry;
        if (Answer.Notice is not { Until: { } until } notice) return;
        _ = Waiting();

        async Task Waiting()
        {
            while (until - _now() is var left && left > TimeSpan.Zero)
            {
                await _clock.Wait(left < LongestWait ? left : LongestWait);
                if (mine != _expiry) return;
            }
            if (mine != _expiry || Answer.Notice?.Id != notice.Id) return;
            Answer = Answer with { Notice = null };
            Changed?.Invoke();
        }
    }

    private static List<string> ReadClosed(IDefaults? store)
    {
        if (store?.String(ClosedKey) is not { } raw) return new List<string>();
        try
        {
            return JsonSerializer.Deserialize<List<string>>(raw)?.Where(id => id.Length > 0).ToList() ?? new List<string>();
        }
        catch (JsonException)
        {
            return new List<string>();
        }
    }
}

/// <summary>The features the deployment can switch off. A name the app does not know is a feature of a later app, and nothing to it.</summary>
public enum RemoteFeature
{
    ScreenShare,
    BackgroundBlur,
    WatchTogether,
    Music,
    AppUpdates,
}

public static class RemoteFeatures
{
    /// <summary>The deployment's name for a feature, in its answer's <c>off</c>.</summary>
    public static string Key(this RemoteFeature feature) => feature switch
    {
        RemoteFeature.ScreenShare => "screen-share",
        RemoteFeature.BackgroundBlur => "background-blur",
        RemoteFeature.WatchTogether => "watch-together",
        RemoteFeature.Music => "music",
        RemoteFeature.AppUpdates => "app-updates",
        _ => "",
    };

    /// <summary>
    /// Whether a feature's way in is shown (the call bar's watch and music):
    /// where the feature offers it, and the deployment allows it or the
    /// feature is under way. A show playing or music going out is not
    /// interrupted by being switched off, and keeps the button that stops it.
    /// </summary>
    public static bool Entry(bool offered, bool allowed, bool underWay) => offered && (allowed || underWay);

    /// <summary>
    /// The background blur from here on (CallMedia.Blurring). In a call it
    /// stays as it is: a blur switched off by the deployment mid-call is not
    /// taken off a camera that has it, which would show everybody the room
    /// behind somebody who had chosen not to show it. Out of one, it is what
    /// was chosen where blur is allowed, so that from the next meeting a blur
    /// switched off counts as off, whatever was remembered, and comes back
    /// when it is switched on again.
    /// </summary>
    public static bool Blur(bool chosen, bool allowed, bool inCall, bool blurring) => inCall ? blurring : chosen && allowed;
}

/// <summary>What the deployment said, as the app uses it: worked out for each of the app's languages by the core when it was heard (the app's CoreRemoteSource), so that a change of language needs nothing asked or read again.</summary>
public sealed record RemoteAnswer
{
    private static readonly IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> NoPhrases = new Dictionary<Language, IReadOnlyDictionary<string, string>>();
    private static readonly IReadOnlyDictionary<string, double> NoParams = new Dictionary<string, double>();

    /// <summary>The deployment's name for this answer.</summary>
    public string Revision { get; init; } = "";

    /// <summary>The phrases laid over the app's own, by language and then by English key (the core's <c>remote_phrases</c> for each language).</summary>
    public IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> Phrases { get; init; } = NoPhrases;

    /// <summary>The features switched off, by the deployment's names.</summary>
    public IReadOnlySet<string> Off { get; init; } = new HashSet<string>();

    /// <summary>The numbers, by the deployment's names.</summary>
    public IReadOnlyDictionary<string, double> Params { get; init; } = NoParams;

    public LobbyNotice? Notice { get; init; }

    /// <summary>The answer as it came, to keep and read back (the core's <c>kept</c>).</summary>
    public string Kept { get; init; } = "";

    /// <summary>What applies when nothing has ever been heard: the app as it was built.</summary>
    public static readonly RemoteAnswer Nothing = new();

    public bool Allows(RemoteFeature feature) => !Off.Contains(feature.Key());

    public ShareCeiling Ceiling => ShareCeiling.Of(Params);

    /// <summary>
    /// How long between looks for an update (update_check_hours), or null
    /// for the app's own six hours. Never under ten minutes, the wait after a
    /// look that failed (Updates.Retry): a fraction of an hour written by
    /// mistake would otherwise have every copy of the app asking the
    /// deployment every few seconds.
    /// </summary>
    public TimeSpan? UpdateInterval
    {
        get
        {
            if (!Params.TryGetValue("update_check_hours", out var hours) || !double.IsFinite(hours) || hours <= 0) return null;
            // A year at most, which TimeSpan holds and a timer can be made of.
            var asked = TimeSpan.FromHours(Math.Min(hours, 24 * 366));
            return asked < Updates.Retry ? Updates.Retry : asked;
        }
    }
}

/// <summary>A notice the lobby shows above everything else in it.</summary>
/// <param name="Id">The same while the notice is the same one; a closed notice is remembered by it.</param>
/// <param name="Warning">Something to take care about (the deployment's "warning"), drawn as the app draws a problem, rather than only to know.</param>
/// <param name="Text">
/// The words by language: that language, else it without its region or
/// script, else English (the core's <c>remote_notice_text</c> for each). A
/// language missing here has nothing it can read, and is shown nothing.
/// </param>
/// <param name="Until">When it stops being shown; null until it is taken down.</param>
public sealed record LobbyNotice(string Id, bool Warning, IReadOnlyDictionary<Language, string> Text, DateTimeOffset? Until = null)
{
    /// <summary>The words for somebody reading <paramref name="language"/>, or null for no notice at all.</summary>
    public string? In(Language language) => Text.TryGetValue(language, out var said) ? said : null;
}

/// <summary>Where the answers come from: the core, a debug launch's file, or a test's stand-in.</summary>
public interface IRemoteSource
{
    /// <summary>What the deployment says now; throws where it could not be asked or did not answer with something that can be read.</summary>
    Task<RemoteAnswer> Fetch();

    /// <summary>An answer kept from before, read again; null where it is not one. A notice that has run out since is left out.</summary>
    RemoteAnswer? Read(string kept);
}
