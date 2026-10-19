using System.Diagnostics;
using System.Text.RegularExpressions;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Basics;

/// <summary>
/// How sharp a video watched together may get on this screen
/// (web/src/live/preferences.ts <c>watchQuality</c>, <c>levelCap</c>,
/// <c>capping</c>; Android's feature/watch/WatchCeiling.kt).
///
/// A ceiling and not a choice of quality: what plays is still decided by the
/// line, so somebody on a slow connection who said 1080p gets whatever their
/// connection carries, and somebody who said 720p never gets more however
/// fast it is. Kept under the web's own key and in the web's own words, so
/// the account page's setting and the apps' are one habit. The settings write
/// it; a player reads it when a video is opened, so a change applies from the
/// next video.
/// </summary>
public enum WatchCeiling { Auto, P1080, P720 }

public static class WatchCeilings
{
    public const string StorageKey = "meet-live.watch-quality";

    public static readonly WatchCeiling[] All = { WatchCeiling.Auto, WatchCeiling.P1080, WatchCeiling.P720 };

    public static string Word(this WatchCeiling ceiling) => ceiling switch
    {
        WatchCeiling.P1080 => "1080p",
        WatchCeiling.P720 => "720p",
        _ => "auto",
    };

    private static int? Lines(this WatchCeiling ceiling) => ceiling switch
    {
        WatchCeiling.P1080 => 1080,
        WatchCeiling.P720 => 720,
        _ => null,
    };

    /// <summary>One rung of a playlist, as much of it as the rule needs: its size, if the playlist says it.</summary>
    public readonly record struct Level(double? Width, double Height);

    /// <summary>
    /// The highest rung this ceiling allows, as an index into the rungs in the
    /// order the player ranks them (by bitrate), or null for no ceiling at all
    /// (<c>levelCap</c>, whose -1 this is).
    ///
    /// Measured on the shorter side, so that 1080p means what people mean by
    /// it whatever the shape; eight lines of slack, because encoders round a
    /// picture up to whole sixteen-pixel blocks. A rung that does not say its
    /// size is allowed, since refusing it could leave nothing to play; the
    /// ceiling stops below the first rung that is too tall, wherever the list
    /// puts it; and where even the first is too tall, that one is allowed.
    /// </summary>
    public static int? Cap(this WatchCeiling ceiling, IReadOnlyList<Level> levels)
    {
        if (ceiling.Lines() is not { } lines) return null;
        var most = (double)(lines + 8);
        var tooTall = -1;
        for (var i = 0; i < levels.Count; i++)
        {
            var level = levels[i];
            var side = level.Width is { } width && width > 0 ? Math.Min(width, level.Height) : level.Height;
            if (side > most)
            {
                tooTall = i;
                break;
            }
        }
        if (tooTall < 0) return null;
        return Math.Max(0, tooTall - 1);
    }

    /// <summary>What a player's own ceiling should become (<c>capping</c>): lowered to the viewer's, never raised.</summary>
    public static int? Capping(this WatchCeiling ceiling, int? current, IReadOnlyList<Level> levels)
    {
        if (ceiling.Cap(levels) is not { } most) return current;
        if (current is not { } now) return most;
        return Math.Min(now, most);
    }

    /// <summary>Writes it down; no ceiling removes the key rather than writing "auto".</summary>
    public static void Remember(this WatchCeiling ceiling, IDefaults? defaults)
    {
        if (ceiling == WatchCeiling.Auto) defaults?.Remove(StorageKey);
        else defaults?.Set(StorageKey, ceiling.Word());
    }

    /// <summary>The ceiling chosen, or none: anything that is not one of the stored words reads as none, as the web's does.</summary>
    public static WatchCeiling Stored(IDefaults? defaults)
    {
        var word = defaults?.String(StorageKey);
        if (word == null) return WatchCeiling.Auto;
        foreach (var ceiling in All) if (ceiling.Word() == word) return ceiling;
        return WatchCeiling.Auto;
    }
}

/// <summary>
/// When a source of the video watched together that has not begun playing is
/// given up on (WatchScreen.tsx <c>STALL_MS</c>), looked at once a second as
/// the web's interval looks (Android's WatchStall.kt).
///
/// Every age here is worked out saturating: within one beat the clock is
/// read at different moments, and a source that began after the beat's
/// reading has been running for no time at all, not for minus a millisecond
/// (on Apple an unsigned subtraction trapped there, and the app quit whenever
/// the show moved on by itself).
/// </summary>
public sealed class WatchStall
{
    public long Started { get; private set; }
    public long StallMs { get; }
    private long? _looked;

    public WatchStall(long started, long stallMs = 8000)
    {
        Started = started;
        StallMs = stallMs;
    }

    /// <summary>Another source is being tried, from <paramref name="at"/>; when the stall was last looked at is kept.</summary>
    public void Restart(long at) => Started = at;

    /// <summary>Whether a second has passed since the last look, which is then this one; the first look is always due.</summary>
    public bool Due(long now)
    {
        if (_looked is { } last && Since(last, now) < 1000) return false;
        _looked = now;
        return true;
    }

    /// <summary>Whether the source has gone <see cref="StallMs"/> without beginning.</summary>
    public bool Late(long now) => Since(Started, now) > StallMs;

    /// <summary>How long ago <paramref name="then"/> was at <paramref name="now"/>, and nothing for a <paramref name="then"/> read after.</summary>
    public static long Since(long then, long now) => now > then ? now - then : 0;
}

/// <summary>What is said about a video watched together, and how its time and site are written (Android's WatchWords.kt).</summary>
public static class WatchWords
{
    /// <summary>
    /// What to say about a video that was not added (WatchTab.tsx
    /// <c>refusal</c>), from the code a refusal crosses the call as.
    /// </summary>
    public static string Refusal(string why) => why switch
    {
        "not_a_video" => T("That link is not a video that can be played here."),
        "unavailable" => T("That video cannot be played here."),
        "members_only" => T("That video is for paying members only, and plays only on its own site."),
        "copy_protected" => T("That video is copy-protected, and plays only on its own site."),
        "region_locked" => T("That video is not available in this server's region."),
        "not_found" => T("That episode could not be found: it may have been taken down, or not be offered in this server's region."),
        "not_allowed" => T("Watching together has not been opened to you."),
        "emby_not_allowed" => T("The media servers have not been opened to you."),
        _ => T("The video could not be read. Try again."),
    };

    private static readonly Regex Host = new(@"^(?i:https?)://(?:[^@/?#\s]*@)?([^:/?#\s\[\]]+)", RegexOptions.Compiled);

    /// <summary>
    /// The site a link is on, as people say it: "youtube.com", not
    /// "www.youtube.com" (WatchScreen.tsx <c>siteOf</c>). Nothing for
    /// anything that is not a web address, which is never offered to be
    /// opened. Read by the address's shape, as a browser's address bar takes
    /// a path in Chinese, which is most of what Bilibili links are.
    /// </summary>
    public static string Site(string link)
    {
        var found = Host.Match(link);
        if (!found.Success) return "";
        var host = found.Groups[1].Value.ToLowerInvariant();
        return host.StartsWith("www.", StringComparison.Ordinal) ? host[4..] : host;
    }

    /// <summary>Whether a link may be opened elsewhere: only a web address, never a scheme that would start some other app.</summary>
    public static bool Openable(string link) => Site(link).Length > 0;

    /// <summary>At most <paramref name="count"/> UTF-16 units, as a JavaScript <c>slice(0, count)</c> cuts.</summary>
    public static string Cut(string text, int count) => text.Length <= count ? text : text[..count];

    /// <summary>
    /// A time as the panels and the stage say it, "4:29" (the core's
    /// <c>music_clock</c>): whole seconds rounded half up as JavaScript's
    /// <c>Math.round</c> does, minutes not carried into hours, and anything
    /// that is not a time, or before nought, nought. Restated here because a
    /// scrubber says it on every frame it is dragged.
    /// </summary>
    public static string Clock(double seconds)
    {
        long whole = 0;
        if (double.IsFinite(seconds))
        {
            var floor = Math.Floor(seconds);
            whole = (long)Math.Max(0, seconds - floor >= 0.5 ? floor + 1 : floor);
        }
        return $"{whole / 60}:{whole % 60:00}";
    }

    /// <summary>
    /// The one clock every call into the theatre and the keeper is given: a
    /// monotonic count of milliseconds. The protocol takes any such clock as
    /// long as it is always the same one; the wall clock is not one, since it
    /// jumps when the system sets it.
    /// </summary>
    public static long Now() => Stopwatch.GetTimestamp() * 1000 / Stopwatch.Frequency;
}

/// <summary>
/// The frame rates each size of screen share may be asked for (room.ts
/// <c>RATES_FOR</c>), by the shorter side the size names, nought being
/// automatic (Android's media/Sharing.kt <c>ShareRates</c>). Ceilings rather
/// than preferences: 4K at 120 frames is a billion pixels a second, which the
/// encoder does not refuse but falls behind on.
/// </summary>
public static class ShareRates
{
    public static readonly int[] Every = { 15, 30, 60, 120, 240 };

    /// <summary>What automatic sends: a ceiling of 1440p at sixty (room.ts <c>AUTOMATIC</c>).</summary>
    public const int Automatic = 60;

    public static int[] Allowed(int shorterSide) => shorterSide switch
    {
        1080 => new[] { 15, 30, 60, 120, 240 },
        1440 => new[] { 15, 30, 60, 120 },
        2160 => new[] { 15, 30, 60 },
        _ => new[] { Automatic },
    };

    /// <summary>The rate a size can carry: the one wanted where it is allowed, and otherwise the size's highest.</summary>
    public static int Clamp(int wanted, int shorterSide)
    {
        var rates = Allowed(shorterSide);
        return rates.Contains(wanted) ? wanted : rates[^1];
    }
}
