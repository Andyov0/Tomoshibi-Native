package app.tomoshibi.ui.kit

import androidx.compose.ui.geometry.Rect
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The corner's rules, held to the numbers the Apple apps' tests hold theirs
 * to (TomoshibiBasicsTests/CornerTests.swift): how long notices stay, which
 * one a fourth pushes out, where a pile of them sits, and where the notices
 * rise above what was said; and the web's cases for finding addresses
 * (Linked.test.tsx).
 */
class CornerRulesTest {
    @Test
    fun theWebsDurations() {
        assertEquals(2.2, NoticeTimes.briefly, 0.0)
        assertEquals(3.0, NoticeTimes.aMoment, 0.0)
        assertEquals(10.0, NoticeTimes.atMost, 0.0)
        assertEquals(3, NoticeTimes.shown)
    }

    @Test
    fun theFourthPushesOutTheOldestThatGoesByItself() {
        assertEquals(emptyList<Int>(), NoticeTimes.overflow(listOf(false, false, false)))
        assertEquals(listOf(0), NoticeTimes.overflow(listOf(false, false, false, false)))
        // Somebody at the door stays; the next oldest goes instead.
        assertEquals(listOf(1), NoticeTimes.overflow(listOf(true, false, false, false)))
        assertEquals(listOf(0, 1), NoticeTimes.overflow(listOf(false, false, false, false, false)))
        // Nothing that stays is ever dropped, even past the limit.
        assertEquals(emptyList<Int>(), NoticeTimes.overflow(listOf(true, true, true, true)))
    }

    @Test
    fun aClockWaitsWhileHeldAndKeepsWhatWasLeft() {
        val clock = NoticeClock(3.0, 100.0, running = true)
        assertEquals(2.0, clock.due(101.0)!!, 1e-9)
        clock.hold(101.0)
        assertNull(clock.due(150.0))
        assertEquals(2.0, clock.left, 1e-9)
        clock.run(150.0)
        assertEquals(2.0, clock.due(150.0)!!, 1e-9)
        assertEquals(0.5, clock.due(151.5)!!, 1e-9)
    }

    @Test
    fun aNoticeReadToItsLastMomentIsGivenASecondMore() {
        val clock = NoticeClock(2.2, 0.0, running = true)
        clock.hold(2.15)
        clock.run(9.0)
        assertEquals(NoticeTimes.afterHolding, clock.due(9.0)!!, 1e-9)
    }

    @Test
    fun aClockStartedHeldDoesNotRun() {
        val clock = NoticeClock(2.2, 0.0, running = false)
        assertNull(clock.due(5.0))
        assertFalse(clock.running)
    }

    @Test
    fun aPilePeeksAndASpreadColumnStacks() {
        val heights = listOf(44f, 60f, 52f)
        assertEquals(listOf(0f, NoticePile.peek, 2 * NoticePile.peek), NoticePile.lifts(heights, spread = false))
        assertEquals(44 + 2 * NoticePile.peek, NoticePile.height(heights, spread = false))
        assertEquals(listOf(0f, 44 + NoticePile.gap, 44 + 60 + 2 * NoticePile.gap), NoticePile.lifts(heights, spread = true))
        assertEquals(44 + 60 + 52 + 2 * NoticePile.gap, NoticePile.height(heights, spread = true))
        assertEquals(emptyList<Float>(), NoticePile.lifts(emptyList(), spread = true))
        assertEquals(0f, NoticePile.height(emptyList(), spread = false))
        assertEquals(1f, NoticePile.scale(0, spread = false))
        assertEquals(0.9f, NoticePile.scale(2, spread = false), 0.0001f)
        assertEquals(1f, NoticePile.scale(2, spread = true))
    }

    @Test
    fun noticesRiseOverMessagesOnlyWhereTheyShareWidth() {
        // A wide window: messages at the left, notices far to the right.
        val said = Rect(12f, 600f, 12f + 320f, 690f)
        assertEquals(0f, CornerLanes.lift(1200f, said))
        // A phone: the notices' column starts inside the messages'.
        assertEquals(90 + CornerLanes.gap, CornerLanes.lift(60f, said))
        // Closer than the gap counts as touching.
        assertEquals(90 + CornerLanes.gap, CornerLanes.lift(said.right + 4, said))
        assertEquals(0f, CornerLanes.lift(60f, null))
        assertEquals(0f, CornerLanes.lift(60f, Rect.Zero))
    }

    @Test
    fun findsACompleteAddress() {
        assertEquals(listOf("https://example.com/a?b=1"), Linked.links("see https://example.com/a?b=1"))
        assertEquals(listOf("http://a.test", "https://b.test"), Linked.links("http://a.test and https://b.test"))
    }

    // A false link is worse than a missed one: it is clickable, it goes
    // somewhere, and it looks deliberate.
    @Test
    fun doesNotGuessAtAnythingThatIsNotOne() {
        assertEquals(emptyList<String>(), Linked.links("see example.com or www.example.com"))
        assertEquals(emptyList<String>(), Linked.links("port 4443 on the box"))
        assertEquals(emptyList<String>(), Linked.links("ftp://files.test"))
        // The web's `\b`: not in the middle of a word.
        assertEquals(emptyList<String>(), Linked.links("xhttps://example.com"))
    }

    @Test
    fun leavesTrailingPunctuationOutOfTheAddress() {
        assertEquals(listOf("https://example.com/docs"), Linked.links("it is at https://example.com/docs."))
        assertEquals(listOf("https://example.com"), Linked.links("(https://example.com)"))
        assertEquals(listOf("https://example.com"), Linked.links("https://example.com, then"))
    }

    @Test
    fun keepsTheWordsAroundIt() {
        val pieces = Linked.pieces("go to https://example.com now")
        assertEquals(
            listOf(Linked.Piece.Words("go to "), Linked.Piece.Link("https://example.com"), Linked.Piece.Words(" now")),
            pieces,
        )
    }

    // In a browser `\b` is about ASCII, so an address typed straight after
    // Chinese or Japanese is found.
    @Test
    fun findsAnAddressStraightAfterChineseOrJapanese() {
        assertEquals(listOf("https://example.com/x"), Linked.links("看这个https://example.com/x"))
        assertEquals(listOf("https://example.jp"), Linked.links("これhttps://example.jp を"))
    }

    // The web's `\s` is every Unicode space: an ideographic space ends an
    // address as an ordinary one does, where Java's ASCII `\s` would have run
    // on into the words after it.
    @Test
    fun anIdeographicSpaceEndsAnAddress() {
        assertEquals(listOf("https://example.com/x"), Linked.links("https://example.com/x　好的"))
    }

    @Test
    fun caseDoesNotMatter() {
        assertEquals(listOf("HTTPS://Example.com/A"), Linked.links("HTTPS://Example.com/A"))
    }

    @Test
    fun aLimitNeverCutsACharacterInTwo() {
        assertEquals("abc", clampUnits("abcdef", 3))
        assertEquals("short", clampUnits("short", 10))
        // One short of the limit, then an emoji of two UTF-16 units.
        assertEquals("a".repeat(9), clampUnits("a".repeat(9) + "😀", 10))
    }
}
