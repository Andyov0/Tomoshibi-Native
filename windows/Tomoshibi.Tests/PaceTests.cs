using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The update bar's pace, frame by frame (Pace.cs). What these guard went
/// wrong in front of the owner: a download that arrived in a few large
/// reports drew the bar across in leaps. Each frame here is a sixtieth of a
/// second unless a test says otherwise, as CompositionTarget.Rendering gives
/// them.
/// </summary>
public class PaceTests
{
    private static readonly TimeSpan Frame = TimeSpan.FromTicks(TimeSpan.TicksPerSecond / 60);
    private const double Near = 1e-9;

    /// <summary>Frames from <paramref name="at"/> until the bar stands still (or for <paramref name="most"/> frames), each frame's shown value in turn.</summary>
    private static List<double> Run(Pace pace, ref TimeSpan at, int most = 1000, TimeSpan? every = null)
    {
        var seen = new List<double>();
        for (var i = 0; i < most; i++)
        {
            var moving = pace.Advance(at);
            seen.Add(pace.Shown);
            at += every ?? Frame;
            if (!moving) break;
        }
        return seen;
    }

    [Fact]
    public void NeverDrawsMoreThanHasArrived()
    {
        var pace = new Pace();
        var at = TimeSpan.FromSeconds(10);
        pace.Aim(0.3);
        foreach (var shown in Run(pace, ref at)) Assert.True(shown <= 0.3 + Near, $"{shown} drawn with 0.3 arrived");
        Assert.Equal(0.3, pace.Shown, 9);
        Assert.False(pace.Moving);
        // Further frames with nothing more arrived move nothing.
        Run(pace, ref at, most: 30);
        Assert.Equal(0.3, pace.Shown, 9);
    }

    [Fact]
    public void GoesAcrossAtOneSpeedAndNoFasterThanTheWholeBarAllows()
    {
        // Everything arrived at once, as a fast download's first report says.
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(1);
        var seen = Run(pace, ref at);
        var perFrame = Frame / Pace.Whole;
        // The first frame only starts the count; every one after moves the
        // same, the last only as far as is left.
        Assert.Equal(0, seen[0], 9);
        for (var i = 1; i < seen.Count - 1; i++) Assert.Equal(perFrame, seen[i] - seen[i - 1], 9);
        Assert.True(seen[^1] - seen[^2] <= perFrame + Near);
        Assert.True(pace.Full);
        // Nought to full in the whole bar's time, to the frame.
        var frames = seen.Count - 1;
        Assert.InRange(frames * Frame.TotalMilliseconds, Pace.Whole.TotalMilliseconds, Pace.Whole.TotalMilliseconds + 2 * Frame.TotalMilliseconds);
    }

    [Fact]
    public void ALateFrameMovesNoFurtherThanTheLongestFrame()
    {
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(1);
        pace.Advance(at);
        // Half a second without a frame (the window dragged, the machine busy).
        at += TimeSpan.FromMilliseconds(500);
        pace.Advance(at);
        Assert.Equal(Pace.MostAFrame, pace.Shown, 9);
        Assert.True(Pace.MostAFrame < 0.05);
    }

    [Fact]
    public void FollowsWhatHasArrivedBackAtOnceRatherThanDrawingItRunBackwards()
    {
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(0.6);
        Run(pace, ref at);
        Assert.Equal(0.6, pace.Shown, 9);
        // A download begun again.
        pace.Aim(0);
        Assert.Equal(0, pace.Shown, 9);
        pace.Aim(0.2);
        Assert.Equal(0, pace.Shown, 9);
        Assert.True(pace.Moving);
    }

    [Fact]
    public void TwoViewsOfOneBarInOneFrameMoveItOnce()
    {
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(1);
        pace.Advance(at);
        at += Frame;
        pace.Advance(at);
        var once = pace.Shown;
        pace.Advance(at);
        Assert.Equal(once, pace.Shown, 12);
        // Nor does a frame given out of order move it back or on.
        pace.Advance(at - Frame);
        Assert.Equal(once, pace.Shown, 12);
    }

    [Fact]
    public void ABarThatCaughtUpDoesNotLeapAcrossTheTimeItWaited()
    {
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(0.1);
        Run(pace, ref at);
        Assert.False(pace.Moving);
        // The next report a while later; nothing drew it meanwhile.
        at += TimeSpan.FromMilliseconds(400);
        pace.Aim(0.5);
        pace.Advance(at);
        Assert.Equal(0.1, pace.Shown, 9);
        at += Frame;
        pace.Advance(at);
        Assert.Equal(0.1 + Frame / Pace.Whole, pace.Shown, 9);
    }

    /// <summary>
    /// A download like the owner's: fifty-odd megabytes in under half a
    /// second, reported every tenth of a second (core/protocol/src/api/updates.rs,
    /// PROGRESS_EVERY). The bar never says more than has arrived, never moves
    /// more than a frame's worth, and is full only once the whole bar's time
    /// has passed.
    /// </summary>
    [Fact]
    public void AFastDownloadIsDrawnEvenlyBehindItsReports()
    {
        var pace = new Pace();
        var reports = new[] { (At: 0, Real: 0.0), (At: 100, Real: 0.31), (At: 200, Real: 0.64), (At: 300, Real: 0.93), (At: 400, Real: 1.0) };
        var next = 0;
        var before = 0.0;
        double? full = null;
        for (var i = 0; i < 180; i++)
        {
            var at = Frame * i;
            var ms = at.TotalMilliseconds;
            while (next < reports.Length && reports[next].At <= ms) pace.Aim(reports[next++].Real);
            pace.Advance(at);
            Assert.True(pace.Shown <= pace.Real + Near, $"{pace.Shown} drawn with {pace.Real} arrived at {ms} ms");
            Assert.True(pace.Shown - before <= Frame / Pace.Whole + Near, $"moved {pace.Shown - before} in a frame at {ms} ms");
            before = pace.Shown;
            if (pace.Full) full ??= ms;
        }
        Assert.NotNull(full);
        Assert.True(full >= Pace.Whole.TotalMilliseconds - Frame.TotalMilliseconds, $"full at {full} ms");
    }

    [Fact]
    public void SaysWhenItIsDrawnFullAndNotBefore()
    {
        var pace = new Pace();
        var at = TimeSpan.Zero;
        pace.Aim(1);
        var full = pace.WhenFull();
        Assert.False(full.IsCompleted);
        Run(pace, ref at, most: 60);
        Assert.False(pace.Full);
        Assert.False(full.IsCompleted);
        Run(pace, ref at);
        Assert.True(pace.Full);
        Assert.True(full.IsCompleted);
        Assert.True(pace.WhenFull().IsCompleted);
    }
}
