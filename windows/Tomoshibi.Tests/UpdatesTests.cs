using Tomoshibi.Basics;
using Xunit;
using static Tomoshibi.Basics.Updates;

namespace Tomoshibi.Tests;

/// <summary>
/// When the app looks for an update, what it says, and above all where it
/// says it and does anything about it (Android's UpdatesTest, case for case
/// where Windows has the same step). What these guard would go wrong in front
/// of somebody without any test of the core noticing: an offer, a download or
/// an install in the middle of a call; a version put away with "Later" coming
/// back an hour later; a check that failed shouting about it, or a check
/// somebody asked for saying nothing; a build downloaded twice where it is
/// already here, or kept after the system refused it.
///
/// Everything is driven by hand: the fakes' answers and the clock complete
/// their tasks on this thread, and the model's continuations run inline, so
/// six hours pass in an instant and every step is where the test left it.
/// </summary>
public class UpdatesTests
{
    private sealed class FakeClock : IUpdateClock
    {
        private readonly List<(TimeSpan Due, TaskCompletionSource Done)> _waits = new();
        public TimeSpan Now { get; private set; }

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

    private sealed class FakeSource : IUpdateSource
    {
        public Func<Offer?> Answer = () => null;
        public int Asked;
        public int Cleared;
        public readonly List<FakeTransfer> Transfers = new();

        public Task<Offer?> Latest(string running)
        {
            Asked++;
            try
            {
                return Task.FromResult(Answer());
            }
            catch (Exception fault)
            {
                return Task.FromException<Offer?>(fault);
            }
        }

        public IUpdateTransfer Download(Offer offer)
        {
            var transfer = new FakeTransfer(offer);
            Transfers.Add(transfer);
            return transfer;
        }

        public void Clear()
        {
            Cleared++;
            foreach (var transfer in Transfers) System.IO.File.Delete(transfer.File);
        }
    }

    private sealed class FakeTransfer : IUpdateTransfer
    {
        /// <summary>A real file, since a build kept for a second try is used only while it is still there.</summary>
        public string File { get; }
        public readonly TaskCompletionSource<Exception?> Outcome = new();
        public Action<long, long>? Progress;
        public bool Cancelled;
        /// <summary>A cancel that arrives after the download has already finished, as one can.</summary>
        public bool Deaf;

        public FakeTransfer(Offer offer)
        {
            File = Path.Combine(Path.GetTempPath(), $"Tomoshibi-{offer.Version}-{Guid.NewGuid():N}.msixbundle");
            System.IO.File.WriteAllText(File, "build");
        }

        public async Task Run(Action<long, long> progress)
        {
            Progress = progress;
            if (await Outcome.Task is { } fault) throw fault;
        }

        public void Cancel()
        {
            Cancelled = true;
            if (!Deaf) Outcome.TrySetResult(new UpdateFailure.Cancelled());
        }
    }

    private sealed class FakeInstaller : IUpdateInstaller
    {
        public int Stopped;
        public readonly List<string> Installs = new();
        public TaskCompletionSource<Installed> Answer = new();

        public Task<Installed> Install(string file, string version)
        {
            Installs.Add(file);
            return Answer.Task;
        }

        public void Stop()
        {
            Stopped++;
            Answer.TrySetResult(new Installed.Stopped());
        }
    }

    private readonly FakeSource _source = new();
    private readonly FakeInstaller _installer = new();
    private readonly FakeClock _clock = new();

    private static Offer MakeOffer(string version) => new(version, 56_093_610, language => language == "en" ? "Fixed." : null);

    private Updates Make() => new(_source, _installer, "0.1.0.7", _clock);

    /// <summary>Started in the lobby with <paramref name="version"/> on offer.</summary>
    private Updates Offered(string version = "0.1.0.8")
    {
        _source.Answer = () => MakeOffer(version);
        var updates = Make();
        updates.Lobby(true);
        updates.Start();
        Assert.Equal(new Step.Offered(MakeOffer(version)), updates.Now);
        return updates;
    }

    /// <summary>Offered, accepted, and the download finished.</summary>
    private (Updates Updates, FakeTransfer Transfer) Downloaded()
    {
        var updates = Offered();
        updates.Accept();
        var transfer = _source.Transfers[^1];
        transfer.Outcome.SetResult(null);
        return (updates, transfer);
    }

    [Fact]
    public void AsksAtStartAndEverySixHoursAndRetriesAFailureQuietly()
    {
        var fail = true;
        _source.Answer = () => fail ? throw new IOException("no network") : null;
        var updates = Make();
        updates.Lobby(true);
        updates.Start();
        Assert.Equal(1, _source.Asked);
        Assert.Equal(1, _source.Cleared);
        // Nothing said about a check nobody asked for.
        Assert.Equal(new Step.None(), updates.Now);
        Assert.Null(updates.Answer);
        Assert.False(updates.Shown);

        _clock.Advance(Updates.Retry - TimeSpan.FromMilliseconds(1));
        Assert.Equal(1, _source.Asked);
        _clock.Advance(TimeSpan.FromMilliseconds(2));
        Assert.Equal(2, _source.Asked);

        fail = false;
        _clock.Advance(Updates.Retry);
        Assert.Equal(3, _source.Asked);
        _clock.Advance(Updates.Every - TimeSpan.FromMilliseconds(1));
        Assert.Equal(3, _source.Asked);
        _clock.Advance(TimeSpan.FromMilliseconds(2));
        Assert.Equal(4, _source.Asked);
        updates.Start();
        Assert.Equal(4, _source.Asked);
    }

    [Fact]
    public void AnOfferIsShownInTheLobbyAndNowhereElse()
    {
        _source.Answer = () => MakeOffer("0.1.0.8");
        var updates = Make();
        updates.Start();
        // Found during a call: kept until the lobby.
        Assert.IsType<Step.Offered>(updates.Now);
        Assert.False(updates.Shown);
        updates.Lobby(true);
        Assert.True(updates.Shown);
        updates.Lobby(false);
        Assert.False(updates.Shown);
        Assert.Equal("Fixed.", updates.Offer?.Notes("en"));
    }

    [Fact]
    public void LaterPutsThatVersionAwayAndANewerOneIsOfferedAllTheSame()
    {
        var updates = Offered("0.1.0.8");
        updates.Later();
        Assert.False(updates.Shown);
        _clock.Advance(Updates.Every + TimeSpan.FromMilliseconds(1));
        Assert.Equal(2, _source.Asked);
        Assert.False(updates.Shown);

        _source.Answer = () => MakeOffer("0.1.0.9");
        _clock.Advance(Updates.Every + TimeSpan.FromMilliseconds(1));
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.9")), updates.Now);
        Assert.True(updates.Shown);
    }

    [Fact]
    public void ACheckSomebodyAsksForAlwaysAnswers()
    {
        var updates = Make();
        updates.Lobby(true);
        _source.Answer = () => null;
        updates.CheckNow();
        Assert.Equal(Said.Current, updates.Answer);

        _source.Answer = () => throw new IOException("no network");
        updates.CheckNow();
        Assert.Equal(Said.Failed, updates.Answer);

        // A version put away is brought back by asking.
        _source.Answer = () => MakeOffer("0.1.0.8");
        updates.CheckNow();
        updates.Later();
        Assert.False(updates.Shown);
        updates.CheckNow();
        Assert.Null(updates.Answer);
        Assert.True(updates.Shown);
    }

    [Fact]
    public void ADownloadIsHeldOutsideTheLobbyAndCarriesOnWhenBack()
    {
        var updates = Offered();
        updates.Accept();
        var first = Assert.Single(_source.Transfers);
        first.Progress!(1_000, 56_093_610);
        Assert.Equal(new Step.Downloading(MakeOffer("0.1.0.8"), 1_000, 56_093_610), updates.Now);
        Assert.True(updates.Shown);

        updates.Lobby(false);
        Assert.True(first.Cancelled);
        Assert.IsType<Step.Held>(updates.Now);
        Assert.False(updates.Shown);

        updates.Lobby(true);
        Assert.Equal(2, _source.Transfers.Count);
        Assert.IsType<Step.Downloading>(updates.Now);
        Assert.True(updates.Shown);
    }

    [Fact]
    public void CancelStopsTheDownloadAndOffersItAgain()
    {
        var updates = Offered();
        updates.Accept();
        updates.Cancel();
        Assert.True(Assert.Single(_source.Transfers).Cancelled);
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.8")), updates.Now);
        Assert.True(updates.Shown);
        Assert.Empty(_installer.Installs);
    }

    [Fact]
    public void AFailedInstallLetsGoOfTheBuild()
    {
        var (updates, _) = Downloaded();
        var cleared = _source.Cleared;
        _installer.Answer.SetResult(new Installed.Failed("The update does not match the app that is installed."));
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.8"), "The update does not match the app that is installed."), updates.Now);
        Assert.Equal(cleared + 1, _source.Cleared);
        updates.Accept();
        Assert.Equal(2, _source.Transfers.Count);
    }

    [Fact]
    public void AnInstallIsGivenUpWhenTheLobbyIsLeftAndTheBuildKept()
    {
        var (updates, transfer) = Downloaded();
        Assert.IsType<Step.Installing>(updates.Now);
        updates.Lobby(false);
        Assert.Equal(1, _installer.Stopped);
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.8")), updates.Now);
        Assert.False(updates.Shown);
        updates.Lobby(true);
        _installer.Answer = new TaskCompletionSource<Installed>();
        updates.Accept();
        Assert.Single(_source.Transfers);
        Assert.Equal(new[] { transfer.File, transfer.File }, _installer.Installs);
    }

    /// <summary>
    /// A join pressed in the lobby leaves it only once it lands, and an
    /// install that came to its swap in those seconds saw a lobby, replaced
    /// the app and ended the call being entered. A join out counts as the
    /// lobby left, from the press.
    /// </summary>
    [Fact]
    public void AJoinPressedInTheLobbyGivesUpTheInstallBeforeItLands()
    {
        var (updates, transfer) = Downloaded();
        Assert.IsType<Step.Installing>(updates.Now);
        updates.Joining(true);
        Assert.Equal(1, _installer.Stopped);
        Assert.False(updates.Shown);
        updates.Joining(false);
        _installer.Answer = new TaskCompletionSource<Installed>();
        updates.Accept();
        Assert.Equal(new[] { transfer.File, transfer.File }, _installer.Installs);
    }

    /// <summary>
    /// A download held for a join that is refused or called off carries on
    /// when the lobby is back, and one that is out is not downloaded through.
    /// </summary>
    [Fact]
    public void ADownloadIsHeldWhileAJoinFromTheLobbyIsOutAndGoesOnWhenItFails()
    {
        var updates = Offered();
        updates.Accept();
        updates.Joining(true);
        Assert.True(Assert.Single(_source.Transfers).Cancelled);
        Assert.IsType<Step.Held>(updates.Now);
        Assert.False(updates.Shown);
        updates.Joining(false);
        Assert.Equal(2, _source.Transfers.Count);
        Assert.IsType<Step.Downloading>(updates.Now);
    }

    [Fact]
    public void AnInstallGivenUpByCancelSaysSoAndKeepsTheBuild()
    {
        var (updates, transfer) = Downloaded();
        Assert.IsType<Step.Installing>(updates.Now);
        updates.StopInstalling();
        Assert.Equal(1, _installer.Stopped);
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.8"), "The update was not installed."), updates.Now);
        _installer.Answer = new TaskCompletionSource<Installed>();
        updates.Accept();
        Assert.Single(_source.Transfers);
        Assert.Equal(new[] { transfer.File, transfer.File }, _installer.Installs);
    }

    [Fact]
    public void ADamagedDownloadSaysSoAndASupersededOneAsksAgain()
    {
        var updates = Offered();
        updates.Accept();
        _source.Transfers[^1].Outcome.SetResult(new UpdateFailure.Damaged("its SHA-256 is …"));
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.8"), "The download was damaged and has been deleted. Try again."), updates.Now);
        Assert.Empty(_installer.Installs);

        updates.Accept();
        var asked = _source.Asked;
        _source.Answer = () => MakeOffer("0.1.0.9");
        _source.Transfers[^1].Outcome.SetResult(new UpdateFailure.Superseded());
        Assert.Equal(asked + 1, _source.Asked);
        Assert.Equal(new Step.Offered(MakeOffer("0.1.0.9")), updates.Now);
    }

    [Fact]
    public void ABuildThatArrivesAsTheLobbyIsLeftIsNotInstalledUntilAskedThere()
    {
        var updates = Offered();
        updates.Accept();
        var transfer = Assert.Single(_source.Transfers);
        transfer.Deaf = true;
        updates.Lobby(false);
        transfer.Outcome.SetResult(null);
        Assert.Empty(_installer.Installs);
        Assert.False(updates.Shown);

        updates.Lobby(true);
        Assert.True(updates.Shown);
        Assert.Empty(_installer.Installs);
        updates.Accept();
        Assert.Single(_source.Transfers);
        Assert.Equal(new[] { transfer.File }, _installer.Installs);
    }

    [Fact]
    public void ABuildTakenBackIsNotOfferedAnyMore()
    {
        var updates = Offered();
        _source.Answer = () => null;
        _clock.Advance(Updates.Every + TimeSpan.FromMilliseconds(1));
        Assert.Equal(new Step.None(), updates.Now);
        Assert.False(updates.Shown);
    }
}
