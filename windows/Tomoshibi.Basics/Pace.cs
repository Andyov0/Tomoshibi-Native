namespace Tomoshibi.Basics;

/// <summary>
/// A bar drawn at an even pace behind what it shows: an update's download.
///
/// The owner, 2026-10-22, of every platform's update bars: the builds arrive
/// fast, and the bar went across in three or four leaps, each eased into
/// place, which read as a jerk rather than progress. So what is drawn
/// (<see cref="Shown"/>) moves towards what has arrived (<see cref="Real"/>) at
/// one speed, the whole bar in <see cref="Whole"/> and never faster, and a
/// frame late after a stall moves it no further than <see cref="LongestFrame"/>
/// would. It never runs ahead of what has arrived, so it never says more
/// than is true; what has arrived going back (a download begun again) is
/// followed at once rather than drawn running backwards.
///
/// Driven by the frames it is drawn in, with their times on any monotonic
/// clock: several views drawing one bar in the same frame give it the same
/// time, and only the first moves it.
/// </summary>
public sealed class Pace
{
    /// <summary>Nought to full in no less than this.</summary>
    public static readonly TimeSpan Whole = TimeSpan.FromMilliseconds(1200);

    /// <summary>The most a frame is counted as: 50 ms, three frames at sixty a second.</summary>
    public static readonly TimeSpan LongestFrame = TimeSpan.FromMilliseconds(50);

    /// <summary>The most the bar moves in one frame, however late the frame.</summary>
    public static double MostAFrame => LongestFrame / Whole;

    /// <summary>What has arrived, from nought to one.</summary>
    public double Real { get; private set; }

    /// <summary>What is drawn, from nought to <see cref="Real"/>.</summary>
    public double Shown { get; private set; }

    /// <summary>Whether a frame would move it.</summary>
    public bool Moving => Shown < Real;

    /// <summary>Whether it is drawn full.</summary>
    public bool Full => Shown >= 1;

    private TimeSpan? _last;
    private TaskCompletionSource? _full;

    /// <summary>Told what has arrived. Further on is gone towards frame by frame; back is gone to at once.</summary>
    public void Aim(double real)
    {
        real = double.IsNaN(real) ? 0 : Math.Clamp(real, 0, 1);
        Real = real;
        if (Shown > real) Shown = real;
    }

    /// <summary>
    /// A frame drawn at <paramref name="now"/>: moves what is shown on by the
    /// time since the last frame, at most <see cref="LongestFrame"/> of it,
    /// and no further than what has arrived. True while there is still
    /// further to go.
    /// </summary>
    public bool Advance(TimeSpan now)
    {
        if (Shown < Real)
        {
            if (_last is { } last && now > last)
            {
                var frame = now - last;
                if (frame > LongestFrame) frame = LongestFrame;
                Shown = Math.Min(Real, Shown + frame / Whole);
            }
            if (_last is not { } seen || now > seen) _last = now;
        }
        // A bar that has caught up counts again from its next frame, so that
        // the time it stood waiting for the next report is not taken for a
        // frame and leapt across.
        if (Shown >= Real) _last = null;
        if (Full && _full is { } full)
        {
            _full = null;
            full.SetResult();
        }
        return Shown < Real;
    }

    /// <summary>Done once the bar is drawn full; at once where it already is.</summary>
    public Task WhenFull()
    {
        if (Full) return Task.CompletedTask;
        _full ??= new TaskCompletionSource();
        return _full.Task;
    }
}
