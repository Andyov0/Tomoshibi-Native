using Tomoshibi.Basics;
using Xunit;
using static Tomoshibi.Basics.Updates;

namespace Tomoshibi.Tests;

/// <summary>
/// What the deployment says to the apps between releases (Remote.cs), case
/// for case with Android's RemoteTest where Windows has the same rule: what
/// is kept and read back before the first screen, when the deployment is
/// asked, what a question that is not answered changes (nothing), the
/// phrases laid over the app's own, the features switched off and what is
/// under way going on regardless, the ceilings, and the lobby's notice,
/// closed for good by its id. What these guard goes wrong in front of
/// everybody with the app at once: a switch that does not stay off on a
/// train, a notice that comes back after it was closed, a notice over a call.
///
/// Driven by hand: the source's answers and the clock complete their tasks
/// on this thread, and the model's continuations run inline.
/// </summary>
public class RemoteTests
{
    private sealed class FakeClock : IUpdateClock
    {
        private readonly List<(DateTimeOffset Due, TaskCompletionSource Done)> _waits = new();
        public DateTimeOffset Now { get; private set; } = new(2026, 10, 22, 12, 0, 0, TimeSpan.Zero);

        public Task Wait(TimeSpan time)
        {
            var done = new TaskCompletionSource();
            _waits.Add((Now + time, done));
            return done.Task;
        }

        public void Advance(TimeSpan by)
        {
            Now += by;
            while (_waits.FirstOrDefault(w => w.Due <= Now) is { Done: not null } due)
            {
                _waits.Remove(due);
                due.Done.SetResult();
            }
        }
    }

    private sealed class FakeSource : IRemoteSource
    {
        public Func<RemoteAnswer> Answer = () => throw new IOException("no network");
        public int Asked;
        /// <summary>What the core makes of a kept answer: here, the answers made by these tests, by their Kept.</summary>
        public readonly Dictionary<string, RemoteAnswer> Readable = new();
        /// <summary>A question that is out and not yet answered, where a test holds one.</summary>
        public TaskCompletionSource<RemoteAnswer>? Holding;

        public Task<RemoteAnswer> Fetch()
        {
            Asked++;
            if (Holding is { } holding) return holding.Task;
            try
            {
                return Task.FromResult(Answer());
            }
            catch (Exception fault)
            {
                return Task.FromException<RemoteAnswer>(fault);
            }
        }

        public RemoteAnswer? Read(string kept) => Readable.GetValueOrDefault(kept);
    }

    private readonly FakeClock _clock = new();
    private readonly FakeSource _source = new();
    private readonly MemoryDefaults _store = new();
    private readonly Localization _words = Localization.ForTesting(new Dictionary<Language, IReadOnlyDictionary<string, string>>
    {
        [Language.Ja] = new Dictionary<string, string> { ["Join"] = "参加", ["Start"] = "開始" },
        [Language.ZhHans] = new Dictionary<string, string> { ["Join"] = "加入", ["Start"] = "开始" },
    }, Language.Ja);

    private Remote Make(IDefaults? store = null) => new(_source, store ?? _store, _clock, () => _clock.Now, _words);

    private RemoteAnswer Said(string revision, IEnumerable<string>? off = null, LobbyNotice? notice = null, Dictionary<string, double>? numbers = null,
        Dictionary<Language, IReadOnlyDictionary<string, string>>? phrases = null)
    {
        var answer = new RemoteAnswer
        {
            Revision = revision,
            Off = (off ?? Array.Empty<string>()).ToHashSet(),
            Notice = notice,
            Params = numbers ?? new Dictionary<string, double>(),
            Phrases = phrases ?? new Dictionary<Language, IReadOnlyDictionary<string, string>>(),
            Kept = "{\"revision\":\"" + revision + "\"}",
        };
        _source.Readable[answer.Kept] = answer;
        return answer;
    }

    private static LobbyNotice Notice(string id, bool warning = false, DateTimeOffset? until = null) =>
        new(id, warning, new Dictionary<Language, string> { [Language.En] = $"Notice {id}", [Language.Ja] = $"お知らせ {id}" }, until);

    [Fact]
    public void WithNothingKeptOrHeardTheAppIsWhatItWasBuiltAs()
    {
        var remote = Make();
        remote.Restore();
        remote.Start();
        Assert.Equal(1, _source.Asked);
        foreach (var feature in Enum.GetValues<RemoteFeature>()) Assert.True(remote.Allows(feature), feature.ToString());
        Assert.Equal(ShareCeiling.None, remote.Ceiling);
        Assert.Null(remote.Answer.UpdateInterval);
        remote.Lobby(true);
        Assert.Null(remote.Notice);
        Assert.Equal("参加", _words.T("Join"));
    }

    [Fact]
    public void AnAnswerAppliesAndIsKeptAndAQuestionNotAnsweredChangesNothing()
    {
        _source.Answer = () => Said("r1", off: new[] { "music" });
        var remote = Make();
        var told = 0;
        remote.Changed += () => told++;
        remote.Start();
        Assert.False(remote.Allows(RemoteFeature.Music));
        Assert.Equal("{\"revision\":\"r1\"}", _store.String(Remote.KeptKey));
        Assert.Equal(1, told);

        // Half an hour later the deployment cannot be reached: nothing changes, nothing is said.
        _source.Answer = () => throw new IOException("no network");
        _clock.Advance(Remote.Every);
        Assert.Equal(2, _source.Asked);
        Assert.False(remote.Allows(RemoteFeature.Music));
        Assert.Equal("{\"revision\":\"r1\"}", _store.String(Remote.KeptKey));
        Assert.Equal(1, told);

        // The same answer again draws nothing again.
        _source.Answer = () => Said("r1", off: new[] { "music" });
        _clock.Advance(Remote.Every);
        Assert.Equal(1, told);
    }

    /// <summary>Off a train: the answer kept from the run before applies before the first screen, and stays when the deployment does not answer.</summary>
    [Fact]
    public void OfflineTheKeptAnswerApplies()
    {
        var earlier = Said("r1", off: new[] { "screen-share" }, phrases: new() { [Language.Ja] = new Dictionary<string, string> { ["Join"] = "参加する" } });
        _store.Set(Remote.KeptKey, earlier.Kept);
        var remote = Make();
        remote.Restore();
        Assert.False(remote.Allows(RemoteFeature.ScreenShare));
        Assert.Equal("参加する", _words.T("Join"));
        remote.Start();
        Assert.Equal(1, _source.Asked);
        Assert.False(remote.Allows(RemoteFeature.ScreenShare));
        Assert.Equal("参加する", _words.T("Join"));
        Assert.Equal(earlier.Kept, _store.String(Remote.KeptKey));
    }

    [Fact]
    public void AKeptAnswerThatCannotBeReadIsNothing()
    {
        _store.Set(Remote.KeptKey, "not an answer");
        var remote = Make();
        remote.Restore();
        Assert.True(remote.Allows(RemoteFeature.Music));
    }

    [Fact]
    public void AsksEveryHalfHourAndOnComingBackButNotTwiceInAMoment()
    {
        _source.Answer = () => Said("r1");
        var remote = Make();
        remote.Refresh();
        Assert.Equal(0, _source.Asked);
        remote.Start();
        Assert.Equal(1, _source.Asked);
        // The window comes to the front and lands in the lobby in the same second.
        remote.Refresh();
        remote.Lobby(true);
        Assert.Equal(1, _source.Asked);
        _clock.Advance(Remote.Soonest);
        remote.Lobby(false);
        remote.Lobby(true);
        Assert.Equal(2, _source.Asked);
        // A question still out is not asked again.
        _source.Holding = new TaskCompletionSource<RemoteAnswer>();
        _clock.Advance(Remote.Soonest);
        remote.Refresh();
        remote.Refresh();
        Assert.Equal(3, _source.Asked);
        _source.Holding.SetResult(Said("r2"));
        _source.Holding = null;
        _clock.Advance(Remote.Every - Remote.Soonest - Remote.Soonest - TimeSpan.FromMilliseconds(1));
        Assert.Equal(3, _source.Asked);
        _clock.Advance(TimeSpan.FromMilliseconds(2));
        Assert.Equal(4, _source.Asked);
    }

    [Fact]
    public void WithNoSourceNothingIsAskedOrRead()
    {
        _store.Set(Remote.KeptKey, Said("r1", off: new[] { "music" }).Kept);
        var remote = new Remote(null, _store, _clock, () => _clock.Now, _words);
        remote.Restore();
        remote.Start();
        remote.Refresh();
        Assert.Equal(0, _source.Asked);
        Assert.True(remote.Allows(RemoteFeature.Music));
    }

    // Phrases

    [Fact]
    public void LaidOverPhrasesComeBeforeTheAppsOwnAndFollowTheLanguage()
    {
        _source.Answer = () => Said("r1", phrases: new()
        {
            [Language.Ja] = new Dictionary<string, string> { ["Join"] = "参加する" },
            [Language.En] = new Dictionary<string, string> { ["Start"] = "Begin" },
        });
        var remote = Make();
        remote.Start();
        Assert.Equal("参加する", _words.T("Join"));
        Assert.Equal("参加する", _words.Say("Join"));
        // Reworded in English alone: a reader of Japanese keeps the app's own Japanese.
        Assert.Equal("開始", _words.T("Start"));
        _words.Choose(Language.ZhHans);
        Assert.Equal("加入", _words.T("Join"));
        _words.Choose(Language.En);
        Assert.Equal("Begin", _words.T("Start"));
        Assert.Equal("Join", _words.T("Join"));
    }

    [Fact]
    public void AChangeInTheLanguageOnScreenIsSaidAndOneInAnotherIsNot()
    {
        var remote = Make();
        var said = 0;
        _words.Changed += () => said++;
        _source.Answer = () => Said("r1", phrases: new() { [Language.ZhHans] = new Dictionary<string, string> { ["Join"] = "进入" } });
        remote.Start();
        Assert.Equal(0, said);
        _source.Answer = () => Said("r2", phrases: new() { [Language.Ja] = new Dictionary<string, string> { ["Join"] = "参加する" } });
        _clock.Advance(Remote.Every);
        Assert.Equal(1, said);
        Assert.Equal("参加する", _words.T("Join"));
        // Taken back: the app's own again, said again.
        _source.Answer = () => Said("r3");
        _clock.Advance(Remote.Every);
        Assert.Equal(2, said);
        Assert.Equal("参加", _words.T("Join"));
    }

    [Fact]
    public void PlaceholdersAreFilledInALaidOverPhraseAsInTheAppsOwn()
    {
        _words.LaidOver = new Dictionary<Language, IReadOnlyDictionary<string, string>> { [Language.Ja] = new Dictionary<string, string> { ["Could not join {room}."] = "{room} に入れませんでした" } };
        Assert.Equal("standup に入れませんでした", _words.T("Could not join {room}.", new Dictionary<string, object?> { ["room"] = "standup" }));
    }

    // Features

    [Fact]
    public void AFeatureSwitchedOffIsNotOfferedAndANameNotKnownIsNothing()
    {
        _source.Answer = () => Said("r1", off: new[] { "watch-together", "teleport" });
        var remote = Make();
        remote.Start();
        Assert.False(remote.Allows(RemoteFeature.WatchTogether));
        Assert.True(remote.Allows(RemoteFeature.Music));
        Assert.True(remote.Allows(RemoteFeature.ScreenShare));
        Assert.True(remote.Allows(RemoteFeature.BackgroundBlur));
        Assert.True(remote.Allows(RemoteFeature.AppUpdates));
    }

    /// <summary>The way in to a feature switched off goes, unless the feature is under way, which goes on and keeps what stops it.</summary>
    [Fact]
    public void AnEntrySwitchedOffGoesUnlessItIsUnderWay()
    {
        Assert.True(RemoteFeatures.Entry(offered: true, allowed: true, underWay: false));
        Assert.False(RemoteFeatures.Entry(offered: true, allowed: false, underWay: false));
        Assert.True(RemoteFeatures.Entry(offered: true, allowed: false, underWay: true));
        // Never more than the feature itself offers.
        Assert.False(RemoteFeatures.Entry(offered: false, allowed: true, underWay: true));
    }

    /// <summary>A blur on as it is switched off stays on for this call; from the next meeting it counts as off, and the choice comes back with the switch.</summary>
    [Fact]
    public void ABlurSwitchedOffStaysForTheCallAndIsOffFromTheNext()
    {
        Assert.True(RemoteFeatures.Blur(chosen: true, allowed: false, inCall: true, blurring: true));
        Assert.False(RemoteFeatures.Blur(chosen: true, allowed: false, inCall: false, blurring: true));
        Assert.True(RemoteFeatures.Blur(chosen: true, allowed: true, inCall: false, blurring: false));
        Assert.False(RemoteFeatures.Blur(chosen: false, allowed: true, inCall: false, blurring: true));
        // Nor turned on in a call that began without it.
        Assert.False(RemoteFeatures.Blur(chosen: true, allowed: true, inCall: true, blurring: false));
    }

    // Numbers

    [Fact]
    public void TheShareCeilingIsReadFromTheNumbers()
    {
        var ceiling = Said("r1", numbers: new() { ["screen_share_max_fps"] = 30, ["screen_share_max_height"] = 1080, ["screen_share_hot_fps"] = 15 }).Ceiling;
        Assert.Equal(new ShareCeiling(30, 1080), ceiling);
        // Nought, a negative or a fraction under one is no ceiling at all.
        Assert.Equal(ShareCeiling.None, Said("r2", numbers: new() { ["screen_share_max_fps"] = 0, ["screen_share_max_height"] = -1 }).Ceiling);
        Assert.Equal(ShareCeiling.None, Said("r3", numbers: new() { ["screen_share_max_fps"] = 0.5 }).Ceiling);
    }

    [Fact]
    public void ASharePublishesOneOfItsSizesRatesUnderTheCeiling()
    {
        var ceiling = new ShareCeiling(Fps: 30);
        // min(chosen, ceiling), as one of the size's own rates.
        Assert.Equal(30, ceiling.Published(240, 1080));
        Assert.Equal(15, ceiling.Published(15, 1080));
        Assert.Equal(15, new ShareCeiling(Fps: 24).Published(60, 1440));
        // Under every rate of the size: its lowest, never no frames.
        Assert.Equal(15, new ShareCeiling(Fps: 5).Published(60, 2160));
        // Automatic keeps its one rate and is held to the ceiling as it is captured.
        Assert.Equal(60, ceiling.Published(60, 0));
        Assert.Equal(30, ceiling.Captured(60));
        Assert.Equal(60, ShareCeiling.None.Captured(60));
        Assert.Equal(240, ShareCeiling.None.Published(240, 1080));
    }

    [Fact]
    public void AShareIsCapturedNoTallerThanTheCeilingItsShapeKept()
    {
        var ceiling = new ShareCeiling(Height: 720);
        Assert.Equal((1280, 720), ceiling.Size(2560, 1440));
        Assert.Equal((1280, 720), ceiling.Size(1280, 720));
        Assert.Equal((800, 600), ceiling.Size(800, 600));
        // Even, as the encoder wants them.
        Assert.Equal((1150, 720), ceiling.Size(1919, 1201));
        Assert.Equal((2560, 1440), ShareCeiling.None.Size(2560, 1440));
    }

    [Fact]
    public void TheUpdateIntervalIsTheDeploymentsAndNeverUnderTheRetry()
    {
        Assert.Equal(TimeSpan.FromHours(12), Said("r1", numbers: new() { ["update_check_hours"] = 12 }).UpdateInterval);
        Assert.Equal(Updates.Retry, Said("r2", numbers: new() { ["update_check_hours"] = 0.01 }).UpdateInterval);
        Assert.Null(Said("r3", numbers: new() { ["update_check_hours"] = 0 }).UpdateInterval);
        Assert.Null(Said("r4", numbers: new() { ["update_check_hours"] = -3 }).UpdateInterval);
        Assert.Null(Said("r5").UpdateInterval);
    }

    // The notice

    [Fact]
    public void TheNoticeIsShownInTheLobbyAndNeverInACall()
    {
        _source.Answer = () => Said("r1", notice: Notice("n1", warning: true));
        var remote = Make();
        remote.Start();
        Assert.Null(remote.Notice);
        remote.Lobby(true);
        Assert.Equal("n1", remote.Notice?.Id);
        Assert.True(remote.Notice!.Warning);
        Assert.Equal("お知らせ n1", remote.Notice.In(Language.Ja));
        Assert.Null(remote.Notice.In(Language.ZhHant));
        remote.Lobby(false);
        Assert.Null(remote.Notice);
    }

    [Fact]
    public void ANoticeClosedStaysClosedEvenAfterAStartAndANewOneIsShown()
    {
        _source.Answer = () => Said("r1", notice: Notice("n1"));
        var remote = Make();
        remote.Start();
        remote.Lobby(true);
        var notice = remote.Notice!;
        var told = 0;
        remote.Changed += () => told++;
        remote.Close(notice);
        Assert.Null(remote.Notice);
        Assert.Equal(1, told);

        // The app started again, the same notice still up.
        var again = Make();
        again.Restore();
        again.Start();
        again.Lobby(true);
        Assert.Null(again.Notice);

        // Another notice is shown.
        _source.Answer = () => Said("r2", notice: Notice("n2"));
        _clock.Advance(Remote.Every);
        Assert.Equal("n2", again.Notice?.Id);
    }

    [Fact]
    public void OnlyTheLastNoticesClosedAreRemembered()
    {
        var remote = Make();
        for (var i = 0; i < Remote.ClosedHeld + 5; i++) remote.Close(Notice($"n{i}"));
        var kept = System.Text.Json.JsonSerializer.Deserialize<List<string>>(_store.String(Remote.ClosedKey)!)!;
        Assert.Equal(Remote.ClosedHeld, kept.Count);
        Assert.Equal("n5", kept[0]);
        Assert.Equal($"n{Remote.ClosedHeld + 4}", kept[^1]);
    }

    [Fact]
    public void ANoticeIsTakenDownWhenItRunsOutWithNothingHeard()
    {
        _source.Answer = () => Said("r1", notice: Notice("n1", until: _clock.Now + TimeSpan.FromMinutes(5)));
        var remote = Make();
        remote.Start();
        remote.Lobby(true);
        _source.Answer = () => throw new IOException("no network");
        var told = 0;
        remote.Changed += () => told++;
        _clock.Advance(TimeSpan.FromMinutes(5) - TimeSpan.FromMilliseconds(1));
        Assert.Equal("n1", remote.Notice?.Id);
        _clock.Advance(TimeSpan.FromMilliseconds(2));
        Assert.Null(remote.Notice);
        Assert.Null(remote.Answer.Notice);
        Assert.Equal(1, told);
    }

    // Updates

    private sealed class Shelf : IUpdateSource
    {
        public Func<Offer?> Answer = () => null;
        public int Asked;
        public readonly List<Transfer> Transfers = new();

        public Task<Offer?> Latest(string running)
        {
            Asked++;
            return Task.FromResult(Answer());
        }

        public IUpdateTransfer Download(Offer offer)
        {
            var transfer = new Transfer(offer);
            Transfers.Add(transfer);
            return transfer;
        }

        public void Clear()
        {
        }
    }

    private sealed class Transfer(Offer offer) : IUpdateTransfer
    {
        public string File { get; } = Path.Combine(Path.GetTempPath(), $"Tomoshibi-{offer.Version}-{Guid.NewGuid():N}.exe");
        public readonly TaskCompletionSource<Exception?> Outcome = new();
        public bool Cancelled;

        public async Task Run(Action<long, long> progress)
        {
            if (await Outcome.Task is { } fault) throw fault;
        }

        public void Cancel()
        {
            Cancelled = true;
            Outcome.TrySetResult(new UpdateFailure.Cancelled());
        }
    }

    private sealed class Installer : IUpdateInstaller
    {
        public Task<Installed> Install(string file, string version, Action started) => new TaskCompletionSource<Installed>().Task;
        public bool Stop() => true;
    }

    [Fact]
    public void UpdatesSwitchedOffAreNotLookedForOrOfferedAndBackOnTheyAreLookedForAtOnce()
    {
        var shelf = new Shelf { Answer = () => new Offer("0.1.4.2", 1000) };
        var updates = new Updates(shelf, new Installer(), "0.1.4", _clock);
        updates.Allow(false);
        updates.Lobby(true);
        updates.Start();
        Assert.Equal(0, shelf.Asked);
        Assert.False(updates.Shown);
        Assert.False(updates.Speaks);
        updates.CheckNow();
        Assert.Equal(0, shelf.Asked);
        _clock.Advance(Updates.Every + TimeSpan.FromMinutes(1));
        Assert.Equal(0, shelf.Asked);

        updates.Allow(true);
        Assert.Equal(1, shelf.Asked);
        Assert.True(updates.Shown);
        // Off again while offered: the offer goes, and the looking stops.
        updates.Allow(false);
        Assert.False(updates.Shown);
        updates.Accept();
        Assert.Empty(shelf.Transfers);
        _clock.Advance(Updates.Every + TimeSpan.FromMinutes(1));
        Assert.Equal(1, shelf.Asked);
    }

    [Fact]
    public void ADownloadUnderWayWhenUpdatesAreSwitchedOffGoesOn()
    {
        var shelf = new Shelf { Answer = () => new Offer("0.1.4.2", 1000) };
        var updates = new Updates(shelf, new Installer(), "0.1.4", _clock);
        updates.Lobby(true);
        updates.Start();
        updates.Accept();
        var transfer = Assert.Single(shelf.Transfers);
        updates.Allow(false);
        Assert.False(transfer.Cancelled);
        Assert.IsType<Step.Downloading>(updates.Now);
        Assert.True(updates.Shown);
        Assert.True(updates.Speaks);
    }

    [Fact]
    public void TheDeploymentSaysHowOftenToLookFromTheNextWait()
    {
        var shelf = new Shelf();
        var updates = new Updates(shelf, new Installer(), "0.1.4", _clock);
        updates.Lobby(true);
        updates.Start();
        Assert.Equal(1, shelf.Asked);
        updates.CheckEvery(TimeSpan.FromHours(1));
        Assert.Equal(1, shelf.Asked);
        _clock.Advance(TimeSpan.FromHours(1));
        Assert.Equal(2, shelf.Asked);
        _clock.Advance(TimeSpan.FromHours(1));
        Assert.Equal(3, shelf.Asked);
        _clock.Advance(TimeSpan.FromHours(1));
        _clock.Advance(TimeSpan.FromHours(1));
        _clock.Advance(TimeSpan.FromHours(1));
        Assert.Equal(6, shelf.Asked);
        // The six hours' wait from before runs out in nothing: one look at
        // the sixth hour, the new schedule's, not two.
        _clock.Advance(TimeSpan.FromMinutes(59));
        Assert.Equal(6, shelf.Asked);
        _clock.Advance(TimeSpan.FromMinutes(2));
        Assert.Equal(7, shelf.Asked);
    }
}
