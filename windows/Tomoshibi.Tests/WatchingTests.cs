using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>The stall rule of the video watched together, read on a clock whose readings within one beat are not in order (Android's WatchStallTest).</summary>
public class WatchStallTests
{
    /// <summary>The crash on Apple: a player built during a beat began after the beat read the clock, and its first look at the stall is always due.</summary>
    [Fact]
    public void ASourceThatBeganAfterTheBeatIsNotLate()
    {
        var stall = new WatchStall(started: 1_000_002);
        Assert.True(stall.Due(1_000_000));
        Assert.False(stall.Late(1_000_000));
        Assert.Equal(0L, WatchStall.Since(1_000_002, 1_000_000));
    }

    [Fact]
    public void IsLateOnlyOnceTheWaitHasPassed()
    {
        var stall = new WatchStall(started: 10_000);
        Assert.False(stall.Late(18_000));
        Assert.True(stall.Late(18_001));
    }

    [Fact]
    public void LooksOnceASecondEvenWhenAReadingRunsBackwards()
    {
        var stall = new WatchStall(started: 0);
        Assert.True(stall.Due(5_000));
        Assert.False(stall.Due(4_999));
        Assert.False(stall.Due(5_999));
        Assert.True(stall.Due(6_000));
    }

    /// <summary>A new source restarts the wait and not the once-a-second clock.</summary>
    [Fact]
    public void RestartingKeepsTheLastLook()
    {
        var stall = new WatchStall(started: 0);
        Assert.True(stall.Due(9_000));
        Assert.True(stall.Late(9_000));
        stall.Restart(9_400);
        Assert.False(stall.Late(9_300));
        Assert.False(stall.Due(9_500));
        Assert.False(stall.Late(17_400));
        Assert.True(stall.Late(17_401));
    }
}

/// <summary>The web's own cases for the watch ceiling (web/src/live/preferences.test.ts; Android's WatchCeilingTest), with null for its -1.</summary>
public class WatchCeilingTests
{
    private static readonly WatchCeilings.Level P360 = new(640, 360);
    private static readonly WatchCeilings.Level P720 = new(1280, 720);
    private static readonly WatchCeilings.Level P1080 = new(1920, 1080);
    private static readonly WatchCeilings.Level P2160 = new(3840, 2160);
    private static readonly WatchCeilings.Level[] Ladder = { P360, P720, P1080, P2160 };

    [Fact]
    public void AllowsEveryLevelUpToTheCeilingAndNoneAboveIt()
    {
        Assert.Null(WatchCeiling.Auto.Cap(Ladder));
        Assert.Equal(2, WatchCeiling.P1080.Cap(Ladder));
        Assert.Equal(1, WatchCeiling.P720.Cap(Ladder));
        // Nothing above it to keep out, so no ceiling at all.
        Assert.Null(WatchCeiling.P1080.Cap(Ladder.Take(3).ToList()));
        Assert.Null(WatchCeiling.P720.Cap(Array.Empty<WatchCeilings.Level>()));
    }

    [Fact]
    public void MeasuresTheShorterSideWithRoomForRounding()
    {
        Assert.Equal(1, WatchCeiling.P1080.Cap(new[] { P720, new WatchCeilings.Level(1920, 1088), P2160 }));
        var scope = new[] { new WatchCeilings.Level(1920, 800), new WatchCeilings.Level(3840, 1600) };
        Assert.Equal(0, WatchCeiling.P1080.Cap(scope));
        Assert.Equal(0, WatchCeiling.P720.Cap(scope));
        // A level that does not say its size is not refused for it.
        Assert.Null(WatchCeiling.P720.Cap(new[] { new WatchCeilings.Level(null, 0), P720 }));
        // A portrait 1080p picture is a 1080p picture.
        Assert.Null(WatchCeiling.P1080.Cap(new[] { new WatchCeilings.Level(1080, 1920) }));
    }

    [Fact]
    public void StopsBelowTheFirstLevelThatIsTooTall() => Assert.Equal(0, WatchCeiling.P1080.Cap(new[] { P720, P2160, P1080 }));

    [Fact]
    public void LeavesTheLeastThereIs() => Assert.Equal(0, WatchCeiling.P720.Cap(new[] { P1080, P2160 }));

    [Fact]
    public void OnlyEverLowersThePlayersOwnCeiling()
    {
        Assert.Equal(1, WatchCeiling.P720.Capping(null, Ladder));
        Assert.Equal(1, WatchCeiling.P720.Capping(3, Ladder));
        Assert.Equal(0, WatchCeiling.P720.Capping(0, Ladder));
        Assert.Equal(3, WatchCeiling.Auto.Capping(3, Ladder));
        Assert.Null(WatchCeiling.Auto.Capping(null, Ladder));
    }

    [Fact]
    public void ReadsOnlyTheWordsItWrites()
    {
        var defaults = new MemoryDefaults();
        Assert.Equal(WatchCeiling.Auto, WatchCeilings.Stored(defaults));
        foreach (var garbage in new[] { "4k", "1080P", "", "auto ", "480p" })
        {
            defaults.Set(WatchCeilings.StorageKey, garbage);
            Assert.Equal(WatchCeiling.Auto, WatchCeilings.Stored(defaults));
        }
        WatchCeiling.P1080.Remember(defaults);
        Assert.Equal("1080p", defaults.String(WatchCeilings.StorageKey));
        Assert.Equal(WatchCeiling.P1080, WatchCeilings.Stored(defaults));
        WatchCeiling.Auto.Remember(defaults);
        Assert.Null(defaults.String(WatchCeilings.StorageKey));
        // A rehearsal keeps nothing and reads nothing.
        Assert.Equal(WatchCeiling.Auto, WatchCeilings.Stored(null));
    }
}

/// <summary>What is said about a video, how its time and site are written, and the screen share's rates.</summary>
public class WatchWordsTests
{
    /// <summary>The core's own cases for <c>music_clock</c> (api/music.rs), which Clock restates.</summary>
    [Fact]
    public void ClockRoundsHalfUpAndNeverGoesBelowNought()
    {
        Assert.Equal("4:29", WatchWords.Clock(269.0));
        Assert.Equal("0:05", WatchWords.Clock(5.0));
        Assert.Equal("0:03", WatchWords.Clock(2.5));
        Assert.Equal("0:00", WatchWords.Clock(-4.0));
        Assert.Equal("0:00", WatchWords.Clock(double.NaN));
        // Minutes are not carried into hours, as the web's are not.
        Assert.Equal("61:05", WatchWords.Clock(3665.0));
    }

    [Fact]
    public void ASiteIsSaidAsPeopleSayIt()
    {
        Assert.Equal("youtube.com", WatchWords.Site("https://www.youtube.com/watch?v=abc"));
        Assert.Equal("bilibili.com", WatchWords.Site("https://bilibili.com/video/BV1xx 中文"));
        Assert.Equal("example.org", WatchWords.Site("http://user@example.org:8080/a.mp4"));
        // Nothing that is not a web address is a site, and nothing of it is offered to be opened.
        Assert.Equal("", WatchWords.Site("javascript:alert(1)"));
        Assert.Equal("", WatchWords.Site("look at this video"));
        Assert.False(WatchWords.Openable("intent://scan/#Intent;scheme=zxing;end"));
        Assert.True(WatchWords.Openable("https://vimeo.com/1"));
    }

    [Fact]
    public void ALinkIsCutAsJavaScriptCutsIt()
    {
        Assert.Equal("ab", WatchWords.Cut("abc", 2));
        Assert.Equal("abc", WatchWords.Cut("abc", 2000));
    }

    [Fact]
    public void EveryRefusalTheProtocolNamesHasItsOwnSentence()
    {
        var codes = new[] { "not_a_video", "unavailable", "members_only", "copy_protected", "region_locked", "not_found", "not_allowed", "emby_not_allowed" };
        var said = codes.Select(WatchWords.Refusal).ToList();
        Assert.Equal(codes.Length, said.Distinct().Count());
        // Anything else is the web's "could not be read", which is no other code's.
        Assert.Equal(WatchWords.Refusal("failed"), WatchWords.Refusal("something new"));
        Assert.DoesNotContain(WatchWords.Refusal("failed"), said);
    }

    [Fact]
    public void AShareKeepsItsRateWhereTheSizeAllowsIt()
    {
        Assert.Equal(new[] { 60 }, ShareRates.Allowed(0));
        Assert.Equal(240, ShareRates.Clamp(240, 1080));
        // 4K at 120 is a billion pixels a second: the size's highest instead.
        Assert.Equal(60, ShareRates.Clamp(120, 2160));
        Assert.Equal(120, ShareRates.Clamp(240, 1440));
        Assert.Equal(ShareRates.Automatic, ShareRates.Clamp(15, 0));
    }
}
