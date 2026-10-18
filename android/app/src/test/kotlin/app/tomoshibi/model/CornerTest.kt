package app.tomoshibi.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/*
 * The web's cases for finding addresses in a message (Linked.test.tsx), and
 * the corner's rules: how long notices stay, which one a fourth pushes out,
 * where a pile of them sits, and the chat box's limit; the Apple apps'
 * CornerTests, case for case.
 */
class CornerTest {
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
        assertEquals(emptyList<String>(), Linked.links("xhttps://example.com"))
    }

    @Test
    fun leavesTrailingPunctuationOutOfTheAddress() {
        assertEquals(listOf("https://example.com/docs"), Linked.links("it is at https://example.com/docs."))
        assertEquals(listOf("https://example.com"), Linked.links("(https://example.com)"))
        assertEquals(listOf("https://example.com"), Linked.links("https://example.com, then"))
        // JavaScript's \s, not Java's: an ideographic space ends the address.
        assertEquals(listOf("https://example.com/x"), Linked.links("https://example.com/x　次"))
    }

    @Test
    fun keepsTheWordsAroundIt() {
        val pieces = Linked.pieces("go to https://example.com now")
        assertEquals(listOf(Linked.Piece.Text("go to "), Linked.Piece.Link("https://example.com"), Linked.Piece.Text(" now")), pieces)
        assertEquals("go to https://example.com now", pieces.joinToString("") { if (it is Linked.Piece.Text) it.text else (it as Linked.Piece.Link).href })
    }

    @Test
    fun findsAnAddressStraightAfterChineseOrJapanese() {
        assertEquals(listOf("https://example.com/x"), Linked.links("看这个https://example.com/x"))
        assertEquals(listOf("https://example.jp"), Linked.links("これhttps://example.jp を"))
        assertEquals(listOf("HTTPS://Example.com/A"), Linked.links("HTTPS://Example.com/A"))
    }

    @Test
    fun theWebsDurations() {
        assertEquals(2.2, NoticeTimes.BRIEFLY, 0.0)
        assertEquals(3.0, NoticeTimes.A_MOMENT, 0.0)
        assertEquals(10.0, NoticeTimes.AT_MOST, 0.0)
        assertEquals(3, NoticeTimes.SHOWN)
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
        var clock = NoticeClock(lasts = 3.0, now = 100.0, running = true)
        assertEquals(2.0, clock.due(101.0)!!, 1e-9)
        clock = clock.hold(101.0)
        assertNull(clock.due(150.0))
        assertEquals(2.0, clock.left, 1e-9)
        clock = clock.run(150.0)
        assertEquals(2.0, clock.due(150.0)!!, 1e-9)
        assertEquals(0.5, clock.due(151.5)!!, 1e-9)
    }

    @Test
    fun aNoticeReadToItsLastMomentIsGivenASecondMore() {
        val clock = NoticeClock(lasts = 2.2, now = 0.0, running = true).hold(2.15).run(9.0)
        assertEquals(NoticeTimes.AFTER_HOLDING, clock.due(9.0)!!, 1e-9)
    }

    @Test
    fun aClockStartedHeldDoesNotRun() {
        val clock = NoticeClock(lasts = 2.2, now = 0.0, running = false)
        assertNull(clock.due(5.0))
        assertFalse(clock.running)
    }

    @Test
    fun aPilePeeksAndASpreadColumnStacks() {
        val heights = listOf(44f, 60f, 52f)
        assertEquals(listOf(0f, NoticePile.PEEK, 2 * NoticePile.PEEK), NoticePile.lifts(heights, spread = false))
        assertEquals(44 + 2 * NoticePile.PEEK, NoticePile.height(heights, spread = false))
        assertEquals(listOf(0f, 44 + NoticePile.GAP, 44 + 60 + 2 * NoticePile.GAP), NoticePile.lifts(heights, spread = true))
        assertEquals(44 + 60 + 52 + 2 * NoticePile.GAP, NoticePile.height(heights, spread = true))
        assertEquals(emptyList<Float>(), NoticePile.lifts(emptyList(), spread = true))
        assertEquals(0f, NoticePile.height(emptyList(), spread = false))
        assertEquals(1f, NoticePile.scale(0, spread = false))
        assertEquals(0.9f, NoticePile.scale(2, spread = false), 0.0001f)
        assertEquals(1f, NoticePile.scale(2, spread = true))
    }

    @Test
    fun noticesRiseOverMessagesOnlyWhereTheyShareWidth() {
        // Messages at the left, 320 wide from 12, 90 high.
        assertEquals(0f, CornerLanes.lift(1200f, 332f, 90f))
        assertEquals(90 + CornerLanes.GAP, CornerLanes.lift(60f, 332f, 90f))
        // Closer than the gap counts as touching.
        assertEquals(90 + CornerLanes.GAP, CornerLanes.lift(336f, 332f, 90f))
        assertEquals(0f, CornerLanes.lift(60f, null, null))
        assertEquals(0f, CornerLanes.lift(60f, 0f, 0f))
    }

    @Test
    fun theBoxStopsAtTwoThousandWithoutSplittingACharacter() {
        assertEquals(2000, ChatRules.clamp("a".repeat(2100)).length)
        assertEquals("short", ChatRules.clamp("short"))
        // One short of the limit, then an emoji of two UTF-16 units.
        assertEquals("a".repeat(1999), ChatRules.clamp("a".repeat(1999) + "😀"))
        assertEquals(2000, ChatRules.clamp("你".repeat(2001)).length)
    }

    @Test
    fun aRunFromOnePersonNamesThemOnce() {
        val speakers = listOf("a", "a", "b", "a", "a", "a")
        assertEquals(listOf(false, true, false, false, true, true), speakers.indices.map { ChatRules.continues(speakers, it) })
        assertFalse(ChatRules.continues(emptyList(), 0))
    }
}
