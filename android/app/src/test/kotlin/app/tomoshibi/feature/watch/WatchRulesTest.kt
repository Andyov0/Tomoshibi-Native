package app.tomoshibi.feature.watch

import app.tomoshibi.model.MemoryDefaults
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The stall rule of the video watched together, read on a clock whose
 * readings within one beat are not in order (WatchStallTests.swift).
 */
class WatchStallTest {
    /** The crash on Apple: a player built during a beat began after the beat read the clock, and its first look at the stall is always due. */
    @Test
    fun aSourceThatBeganAfterTheBeatIsNotLate() {
        val stall = WatchStall(started = 1_000_002)
        assertTrue(stall.due(1_000_000))
        assertFalse(stall.late(1_000_000))
        assertEquals(0L, WatchStall.since(1_000_002, 1_000_000))
    }

    @Test
    fun isLateOnlyOnceTheWaitHasPassed() {
        val stall = WatchStall(started = 10_000)
        assertFalse(stall.late(18_000))
        assertTrue(stall.late(18_001))
    }

    @Test
    fun looksOnceASecondEvenWhenAReadingRunsBackwards() {
        val stall = WatchStall(started = 0)
        assertTrue(stall.due(5_000))
        assertFalse(stall.due(4_999))
        assertFalse(stall.due(5_999))
        assertTrue(stall.due(6_000))
    }

    /** A new source restarts the wait and not the once-a-second clock. */
    @Test
    fun restartingKeepsTheLastLook() {
        val stall = WatchStall(started = 0)
        assertTrue(stall.due(9_000))
        assertTrue(stall.late(9_000))
        stall.restart(9_400)
        assertFalse(stall.late(9_300))
        assertFalse(stall.due(9_500))
        assertFalse(stall.late(17_400))
        assertTrue(stall.late(17_401))
    }
}

/** The web's own cases for the watch ceiling (web/src/live/preferences.test.ts; WatchCeilingTests.swift), with null for its -1. */
class WatchCeilingTest {
    private val p360 = WatchCeiling.Level(640.0, 360.0)
    private val p720 = WatchCeiling.Level(1280.0, 720.0)
    private val p1080 = WatchCeiling.Level(1920.0, 1080.0)
    private val p2160 = WatchCeiling.Level(3840.0, 2160.0)
    private val ladder = listOf(p360, p720, p1080, p2160)

    @Test
    fun allowsEveryLevelUpToTheCeilingAndNoneAboveIt() {
        assertNull(WatchCeiling.Auto.cap(ladder))
        assertEquals(2, WatchCeiling.P1080.cap(ladder))
        assertEquals(1, WatchCeiling.P720.cap(ladder))
        // Nothing above it to keep out, so no ceiling at all.
        assertNull(WatchCeiling.P1080.cap(ladder.take(3)))
        assertNull(WatchCeiling.P720.cap(emptyList()))
    }

    @Test
    fun measuresTheShorterSideWithRoomForRounding() {
        assertEquals(1, WatchCeiling.P1080.cap(listOf(p720, WatchCeiling.Level(1920.0, 1088.0), p2160)))
        val scope = listOf(WatchCeiling.Level(1920.0, 800.0), WatchCeiling.Level(3840.0, 1600.0))
        assertEquals(0, WatchCeiling.P1080.cap(scope))
        assertEquals(0, WatchCeiling.P720.cap(scope))
        // A level that does not say its size is not refused for it.
        assertNull(WatchCeiling.P720.cap(listOf(WatchCeiling.Level(null, 0.0), p720)))
        // A portrait 1080p picture is a 1080p picture.
        assertNull(WatchCeiling.P1080.cap(listOf(WatchCeiling.Level(1080.0, 1920.0))))
    }

    @Test
    fun stopsBelowTheFirstLevelThatIsTooTall() {
        assertEquals(0, WatchCeiling.P1080.cap(listOf(p720, p2160, p1080)))
    }

    @Test
    fun leavesTheLeastThereIs() {
        assertEquals(0, WatchCeiling.P720.cap(listOf(p1080, p2160)))
    }

    @Test
    fun onlyEverLowersThePlayersOwnCeiling() {
        assertEquals(1, WatchCeiling.P720.capping(null, ladder))
        assertEquals(1, WatchCeiling.P720.capping(3, ladder))
        assertEquals(0, WatchCeiling.P720.capping(0, ladder))
        assertEquals(3, WatchCeiling.Auto.capping(3, ladder))
        assertNull(WatchCeiling.Auto.capping(null, ladder))
    }

    @Test
    fun readsOnlyTheWordsItWrites() {
        val defaults = MemoryDefaults()
        assertEquals(WatchCeiling.Auto, WatchCeiling.stored(defaults))
        for (garbage in listOf("4k", "1080P", "", "auto ", "480p")) {
            defaults.set(WatchCeiling.STORAGE_KEY, garbage)
            assertEquals(garbage, WatchCeiling.Auto, WatchCeiling.stored(defaults))
        }
        WatchCeiling.P1080.remember(defaults)
        assertEquals("1080p", defaults.string(WatchCeiling.STORAGE_KEY))
        assertEquals(WatchCeiling.P1080, WatchCeiling.stored(defaults))
        WatchCeiling.Auto.remember(defaults)
        assertNull(defaults.string(WatchCeiling.STORAGE_KEY))
        // A rehearsal keeps nothing and reads nothing.
        assertEquals(WatchCeiling.Auto, WatchCeiling.stored(null))
    }
}

/** What is said about a video, and how its time and site are written. */
class WatchWordsTest {
    /** The core's own cases for `music_clock` (api/music.rs), which [clockOf] restates. */
    @Test
    fun clockRoundsHalfUpAndNeverGoesBelowNought() {
        assertEquals("4:29", clockOf(269.0))
        assertEquals("0:05", clockOf(5.0))
        assertEquals("0:03", clockOf(2.5))
        assertEquals("0:00", clockOf(-4.0))
        assertEquals("0:00", clockOf(Double.NaN))
        // Minutes are not carried into hours, as the web's are not.
        assertEquals("61:05", clockOf(3665.0))
    }

    @Test
    fun aSiteIsSaidAsPeopleSayIt() {
        assertEquals("youtube.com", WatchWords.site("https://www.youtube.com/watch?v=abc"))
        assertEquals("bilibili.com", WatchWords.site("https://bilibili.com/video/BV1xx 中文"))
        assertEquals("example.org", WatchWords.site("http://user@example.org:8080/a.mp4"))
        // Nothing that is not a web address is a site, and nothing of it is offered to be opened.
        assertEquals("", WatchWords.site("javascript:alert(1)"))
        assertEquals("", WatchWords.site("look at this video"))
        assertFalse(WatchWords.openable("intent://scan/#Intent;scheme=zxing;end"))
        assertTrue(WatchWords.openable("https://vimeo.com/1"))
    }

    @Test
    fun aLinkIsCutAsJavaScriptCutsIt() {
        assertEquals("ab", WatchWords.cut("abc", 2))
        assertEquals("abc", WatchWords.cut("abc", 2000))
    }

    @Test
    fun everyRefusalTheProtocolNamesHasItsOwnSentence() {
        val codes = listOf("not_a_video", "unavailable", "members_only", "copy_protected", "region_locked", "not_found", "not_allowed", "emby_not_allowed")
        val said = codes.map { WatchWords.refusal(it) }
        assertEquals(codes.size, said.toSet().size)
        // Anything else is the web's "could not be read", which is no other code's.
        assertEquals(WatchWords.refusal("failed"), WatchWords.refusal("something new"))
        assertFalse(WatchWords.refusal("failed") in said)
    }
}
