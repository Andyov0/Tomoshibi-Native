using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The web's own cases for music that makes way for speech
/// (web/src/live/duck.test.ts), as Android's MusicDuckTest holds its port to
/// them. What would go wrong unnoticed: music turned down by itself and
/// never brought back; a click counted as somebody talking; a fade-in that
/// never finishes, which is music never heard; a switch that says off while
/// the music still ducks.
/// </summary>
public class MusicDuckTests
{
    private readonly MusicDuck _duck = new();
    private double _clock;

    private void Pass(double ms, params double[] levels)
    {
        for (var t = 0.0; t < ms; t += MusicDuck.TickMs)
        {
            _clock += MusicDuck.TickMs;
            _duck.Tick(levels, _clock, MusicDuck.TickMs);
        }
    }

    [Fact]
    public void ComesDownWithinTheAttackOnceSomebodyTalksAndOnlyAsFarAsDucked()
    {
        Pass(2 * MusicDuck.TickMs + MusicDuck.AttackMs + MusicDuck.TickMs, 0.1);
        Assert.Equal(MusicDuck.Ducked, _duck.Factor("gfriend", _clock), 9);
        Assert.True(_duck.Ducking);
    }

    [Fact]
    public void WaitsAfterTheTalkingStopsThenComesBackOverTheReleaseNotAtOnce()
    {
        Pass(400, 0.1);
        Pass(MusicDuck.HoldMs - 2 * MusicDuck.TickMs, 0.0);
        Assert.Equal(MusicDuck.Ducked, _duck.Factor("gfriend", _clock), 9);
        Pass(MusicDuck.ReleaseMs / 2, 0.0);
        var midway = _duck.Factor("gfriend", _clock);
        Assert.True(midway > MusicDuck.Ducked);
        Assert.True(midway < 1);
        Pass(MusicDuck.ReleaseMs, 0.0);
        Assert.Equal(1.0, _duck.Factor("gfriend", _clock));
    }

    [Fact]
    public void TakesAClickForNothing()
    {
        Pass(MusicDuck.TickMs, 0.5);
        Pass(MusicDuck.AttackMs * 2, 0.0);
        Assert.Equal(1.0, _duck.Factor("gfriend", _clock));
    }

    [Fact]
    public void IgnoresWhatIsBelowSpeech()
    {
        Pass(2000, MusicDuck.Speech * 0.8);
        Assert.Equal(1.0, _duck.Factor("gfriend", _clock));
    }

    [Fact]
    public void AnyOneMicrophoneIsEnough()
    {
        Pass(500, 0.0, 0.1, 0.0);
        Assert.Equal(MusicDuck.Ducked, _duck.Factor("gfriend", _clock), 9);
    }

    [Fact]
    public void DoesNothingOnceSwitchedOff()
    {
        Pass(500, 0.1);
        _duck.Enabled = false;
        Assert.Equal(1.0, _duck.Factor("gfriend", _clock));
        Assert.False(_duck.Ducking);
    }

    [Fact]
    public void FadesAStreamInFromNothingAndFinishesWithNoMicrophones()
    {
        _duck.Arrived(MusicDuck.Local, _clock);
        Assert.Equal(0.0, _duck.Factor(MusicDuck.Local, _clock));
        Pass(MusicDuck.FadeInMs / 2);
        Assert.InRange(_duck.Factor(MusicDuck.Local, _clock), 0.45, 0.55);
        Pass(MusicDuck.FadeInMs);
        Assert.Equal(1.0, _duck.Factor(MusicDuck.Local, _clock));
        Assert.True(_duck.Settled(_clock));
    }

    [Fact]
    public void FadesAStreamInOnceNotAgainBetweenSongs()
    {
        _duck.Arrived("gholder", _clock);
        Pass(MusicDuck.FadeInMs + 100);
        _duck.Arrived("gholder", _clock);
        Assert.Equal(1.0, _duck.Factor("gholder", _clock));
    }

    [Fact]
    public void FadesInAgainAfterTheStreamWent()
    {
        _duck.Arrived("gholder", _clock);
        Pass(MusicDuck.FadeInMs + 100);
        _duck.Left("gholder");
        _duck.Arrived("gholder", _clock);
        Assert.Equal(0.0, _duck.Factor("gholder", _clock));
    }
}

/// <summary>The clock the music is sent by: a song handed to the core faster than it plays is heard at the wrong speed or not at all.</summary>
public class MusicPacerTests
{
    private const long Ms = 1_000_000;

    [Fact]
    public void HandsOverNoMoreThanRealTimeHowEverOftenItIsAsked()
    {
        var pacer = new MusicPacer(0);
        var blocks = 0;
        // Asked every millisecond for a second: a hundred 10 ms blocks, and
        // never one before its time.
        for (var step = 0; step <= 1000; step++)
        {
            var now = step * Ms;
            blocks += pacer.Due(now);
            Assert.True(blocks <= now / (10 * Ms));
        }
        Assert.Equal(100, blocks);
    }

    [Fact]
    public void AskedLateItMakesUpWhatIsOwed()
    {
        var pacer = new MusicPacer(0);
        Assert.Equal(3, pacer.Due(35 * Ms));
        Assert.Equal(0, pacer.Due(35 * Ms));
        Assert.Equal(1, pacer.Due(40 * Ms));
        Assert.Equal(50 * Ms, pacer.NextDeadline);
    }

    [Fact]
    public void AfterAStallItLetsTheBacklogGoAndKeepsRealTimeAfterwards()
    {
        var pacer = new MusicPacer(0, mostBehind: 20);
        Assert.Equal(10, pacer.Due(100 * Ms));
        // Five seconds held up: at most twenty blocks at once, not five
        // hundred, and then one block per ten milliseconds again.
        Assert.Equal(20, pacer.Due(5100 * Ms));
        Assert.Equal(0, pacer.Due(5100 * Ms));
        Assert.Equal(5, pacer.Due(5150 * Ms));
    }

    [Fact]
    public void AMonitorVolumeNothingWasKeptForIsFull()
    {
        Assert.Equal(1f, MusicKeys.MonitorLevel(null));
        Assert.Equal(1f, MusicKeys.MonitorLevel("loud"));
        Assert.Equal(1f, MusicKeys.MonitorLevel("1.5"));
        Assert.Equal(0.25f, MusicKeys.MonitorLevel("0.25"));
        Assert.Equal(0f, MusicKeys.MonitorLevel("0"));
    }
}
