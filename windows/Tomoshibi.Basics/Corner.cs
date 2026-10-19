using System.Globalization;

namespace Tomoshibi.Basics;

// The rules for the corner of a call, where notices and what people said
// appear, kept apart from the views that draw them so that they can be held
// to the web's numbers by a test (TomoshibiBasics/Corner.swift, Android's
// basics/Corner.kt).

/// <summary>How long the call's notices stay (web/src/live/notices.ts).</summary>
public static class NoticeTimes
{
    /// <summary>An ordinary notice: long enough to read four words (<c>BRIEFLY</c>).</summary>
    public const double Briefly = 2.2;
    /// <summary>A share, because the picture is about to change under you (<c>A_MOMENT</c>).</summary>
    public const double AMoment = 3.0;
    /// <summary>The longest any notice stays (<c>AT_MOST</c>). Ten seconds has been seen; for ever has been ignored.</summary>
    public const double AtMost = 10.0;
    /// <summary>What fits above the controls; the fourth pushes the oldest out (Notices.tsx <c>visibleToasts</c>).</summary>
    public const int Shown = 3;
    /// <summary>
    /// The least a held notice is given once let go. Sonner resumes with
    /// whatever was left, so a notice read to its last tenth of a second
    /// vanished the instant the pointer moved off it.
    /// </summary>
    public const double AfterHolding = 1.0;

    /// <summary>
    /// Which notices to drop so that no more than <paramref name="limit"/>
    /// remain, oldest first: only ones that go by themselves
    /// (<paramref name="lasting"/> false). One that stays until it is dealt
    /// with, somebody at the door, is never pushed out by somebody arriving.
    /// </summary>
    public static IReadOnlyList<int> Overflow(IReadOnlyList<bool> lasting, int limit = Shown)
    {
        var excess = lasting.Count - limit;
        var drop = new List<int>();
        for (var index = 0; index < lasting.Count; index++)
        {
            if (excess <= 0) break;
            if (!lasting[index])
            {
                drop.Add(index);
                excess--;
            }
        }
        return drop;
    }
}

/// <summary>
/// A notice's countdown, which waits while the corner is being read (sonner
/// pauses every timer while its stack is expanded under the pointer). Times
/// are in seconds on whatever monotonic clock the caller keeps.
/// </summary>
public readonly record struct NoticeClock(double Left, double? Since)
{
    public NoticeClock(double lasts, double now, bool running) : this(lasts, running ? now : null) { }

    public bool Running => Since != null;

    /// <summary>Stops it, keeping what was left.</summary>
    public NoticeClock Hold(double now) =>
        Since is { } since ? new NoticeClock(Math.Max(0.0, Left - (now - since)), (double?)null) : this;

    /// <summary>Starts it again, with at least <see cref="NoticeTimes.AfterHolding"/> to go.</summary>
    public NoticeClock Run(double now) =>
        Since != null ? this : new NoticeClock(Math.Max(Left, NoticeTimes.AfterHolding), (double?)now);

    /// <summary>Seconds from <paramref name="now"/> until it is due; null while it is held.</summary>
    public double? Due(double now) => Since is { } since ? Math.Max(0.0, Left - (now - since)) : null;
}

/// <summary>
/// Where each notice in the corner sits: piled, the newest in front and the
/// others peeking above it, until the corner is read, when they spread into
/// a column (sonner's stack). Lengths in device-independent pixels.
/// </summary>
public static class NoticePile
{
    public const float Gap = 8f;
    public const float Peek = 9f;
    public const float Shrink = 0.05f;

    /// <summary>How far up from the bottom each notice's bottom edge sits, newest first.</summary>
    public static IReadOnlyList<float> Lifts(IReadOnlyList<float> heights, bool spread)
    {
        var lifts = new List<float>(heights.Count);
        var below = 0f;
        for (var place = 0; place < heights.Count; place++)
        {
            if (spread)
            {
                lifts.Add(below);
                below += heights[place] + Gap;
            }
            else
            {
                lifts.Add(place * Peek);
            }
        }
        return lifts;
    }

    /// <summary>The height the whole corner takes.</summary>
    public static float Height(IReadOnlyList<float> heights, bool spread)
    {
        if (heights.Count == 0) return 0f;
        if (spread) return heights.Sum() + Gap * (heights.Count - 1);
        return heights[0] + Peek * (heights.Count - 1);
    }

    /// <summary>The scale a notice is drawn at in its place.</summary>
    public static float Scale(int place, bool spread) => spread ? 1f : MathF.Max(0.5f, 1 - Shrink * place);
}

/// <summary>
/// Keeping the notices and what people said out of each other's way: where
/// both columns cover the width (a narrow window), the notices rise above
/// the messages, because a message stays six seconds and a notice two, so
/// the one that comes and goes is the one that moves.
/// </summary>
public static class CornerLanes
{
    public const float Gap = 8f;

    /// <summary>
    /// How far the notices must rise, given the messages' right edge and
    /// height and the leading edge of the notices' own column; nothing while
    /// the two do not share any width.
    /// </summary>
    public static float Lift(float notices, float? saidRight, float? saidHeight, float gap = Gap)
    {
        if (saidRight is not { } right || saidHeight is not { } height || height <= 0f) return 0f;
        if (right + gap <= notices) return 0f;
        return height + gap;
    }
}

/// <summary>The parts of the web's chat that are rules rather than drawing.</summary>
public static class ChatRules
{
    /// <summary>The longest message the box takes (ChatPanel.tsx <c>maxLength</c>).</summary>
    public const int Longest = 2000;

    /// <summary>
    /// The text cut to <paramref name="limit"/>, counted as a browser counts
    /// <c>maxLength</c>, in UTF-16 code units, but never through the middle of
    /// a character: a pasted emoji at the limit is left out whole rather than
    /// sent as half a surrogate pair.
    /// </summary>
    public static string Clamp(string text, int limit = Longest)
    {
        if (text.Length <= limit) return text;
        // The last boundary between characters at or before the limit, a
        // character being what a reader sees as one (a flag, an emoji joined
        // to another): .NET's text elements are extended grapheme clusters.
        var end = 0;
        while (end < text.Length)
        {
            var next = end + StringInfo.GetNextTextElementLength(text, end);
            if (next > limit) break;
            end = next;
        }
        return text[..end];
    }

    /// <summary>Whether the line at <paramref name="index"/> carries on a run by the same person (ChatPanel.tsx, Said.tsx).</summary>
    public static bool Continues(IReadOnlyList<string> speakers, int index) =>
        index > 0 && index < speakers.Count && speakers[index - 1] == speakers[index];
}
