package app.tomoshibi.basics

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The web's own cases for where pictures go (web/src/live/plan.test.ts), as
 * the Apple apps' PlanTests hold the Swift port to them, case for case. The
 * faults they guard are the ones the web found: a filmstrip passing under the
 * controls, a phone showing one person in a letterbox, two strangers side by
 * side where a conversation should be.
 */
class PlanTest {
    private val window = Size(1600f, 900f)
    private val phone = Size(393f, 852f)

    private fun ids(count: Int) = (0 until count).map { "p$it" }

    private fun overlaps(a: Rect, b: Rect): Boolean {
        val tolerance = 0.5f
        return a.right - tolerance > b.left && b.right - tolerance > a.left && a.bottom - tolerance > b.top && b.bottom - tolerance > a.top
    }

    @Test
    fun gridPlacesEverybodyOnceWithoutOverlap() {
        for (count in 1..9) {
            val placed = Plan.grid(window, ids(count)).values.toList()
            assertEquals(count, placed.size)
            for (i in placed.indices) for (j in placed.indices) {
                if (j > i) assertFalse("$count people", overlaps(placed[i], placed[j]))
            }
        }
    }

    /** Clear of the controls whole, and of the window's edges by the gap. */
    @Test
    fun gridKeepsClearOfTheControlsAndInsideTheWindow() {
        for (size in listOf(window, Size(1280f, 772f), phone)) {
            for (count in 1..9) {
                for (spot in Plan.grid(size, ids(count)).values) {
                    assertTrue("$count in $size", spot.bottom <= size.height - Plan.ISLAND + 0.5f)
                    assertTrue("$count in $size", spot.left >= Plan.GAP - 0.5f)
                    assertTrue(spot.top >= 0)
                    assertTrue("$count in $size", spot.right <= size.width - Plan.GAP + 0.5f)
                }
            }
        }
    }

    /** The paging arrows sit beside the pictures, not over the first and last. */
    @Test
    fun aRoomThatPagesLeavesItsArrowsRoom() {
        val grid = Plan.room(window, listOf("me") + ids(20), "me", null)
        for (spot in grid.spots.values) {
            assertTrue(spot.left >= Plan.PAGER_RESERVE - 0.5f)
            assertTrue(spot.right <= window.width - Plan.PAGER_RESERVE + 0.5f)
        }
        val strip = Plan.room(window, listOf("me", "stage") + ids(20), "me", "stage")
        for (id in strip.shown) {
            assertTrue(strip.spots.getValue(id).left >= Plan.PAGER_RESERVE - 0.5f)
            assertTrue(strip.spots.getValue(id).right <= window.width - Plan.PAGER_RESERVE + 0.5f)
        }
    }

    @Test
    fun gridCentresAShortLastRow() {
        val plan = Plan.grid(Size(1200f, 900f), ids(5))
        val lowest = plan.values.maxOf { it.top }
        val lastRow = plan.values.filter { it.top == lowest }
        assertEquals(lastRow.minOf { it.left }, 1200f - lastRow.maxOf { it.right }, 0.001f)
    }

    @Test
    fun gridPlansNothingUnmeasured() {
        assertTrue(Plan.grid(Size.Zero, ids(4)).isEmpty())
        assertTrue(Plan.grid(window, emptyList()).isEmpty())
    }

    @Test
    fun focusPutsTheStageAboveTheStripAndBothClearOfTheControls() {
        val plan = Plan.focus(window, "stage", ids(4))
        val stage = plan.getValue("stage")
        for (id in ids(4)) {
            val spot = plan.getValue(id)
            assertTrue(spot.top >= stage.bottom)
            assertTrue(spot.bottom <= window.height - Plan.ISLAND + 0.5f)
        }
    }

    @Test
    fun focusFillsTheScreenWhenAsked() {
        val plan = Plan.focus(window, "stage", ids(3), fullscreen = true)
        assertEquals(1, plan.size)
        assertEquals(Rect(Offset.Zero, window), plan["stage"])
    }

    @Test
    fun stripCapacityHoldsSomebodyAlways() {
        assertEquals(1, Plan.stripCapacity(Size(10f, 900f)))
        assertTrue(Plan.stripCapacity(Size(1600f, 900f)) > Plan.stripCapacity(Size(800f, 900f)))
    }

    @Test
    fun aPhoneIsFilledRatherThanBanded() {
        // The web measured 0.28 before it learned to turn tiles upright.
        for (count in listOf(3, 4, 6)) {
            val used = Plan.grid(phone, ids(count)).values.sumOf { (it.width * it.height).toDouble() }
            assertTrue("$count people", used / (phone.width * (phone.height - Plan.ISLAND)) > 0.45)
        }
    }

    @Test
    fun uprightPicturesAreUpright() {
        val spot = Plan.grid(phone, ids(4)).values.first()
        assertTrue(spot.height > spot.width)
        val wide = Plan.grid(window, ids(4)).values.first()
        assertTrue(wide.width > wide.height)
    }

    @Test
    fun twoPeopleAreAConversation() {
        val plan = Plan.close(phone, "them", listOf("me"))
        assertEquals(Rect(Offset.Zero, phone), plan["them"])
        val me = plan.getValue("me")
        assertTrue(me.right <= phone.width)
        assertTrue(me.bottom <= phone.height - Plan.ISLAND)
        assertTrue(me.width < phone.width / 2)
    }

    @Test
    fun aCardThatWouldNotFitIsDropped() {
        val short = Size(393f, 300f)
        for ((id, spot) in Plan.close(short, "them", ids(6))) {
            if (id != "them") assertTrue(spot.bottom <= short.height - Plan.ISLAND)
        }
    }

    // The whole room (Room.tsx)

    /** Whichever page is showing, your own picture is on it, and it takes one of the nine places rather than making a tenth. */
    @Test
    fun yourOwnPictureNeverPagesAway() {
        val everybody = listOf("me") + ids(20)
        val seen = HashSet<String>()
        val first = Plan.room(window, everybody, "me", null)
        assertEquals(3, first.pages)
        for (page in 0 until first.pages) {
            val room = Plan.room(window, everybody, "me", null, page = page)
            assertNotNull("page $page", room.spots["me"])
            assertEquals("me", room.shown.first())
            assertTrue(room.spots.size <= Plan.TILES_PER_PAGE)
            seen += room.shown
        }
        assertEquals("every picture is on some page", everybody.toSet(), seen)
    }

    @Test
    fun yourOwnPictureStaysInTheStripToo() {
        val room = Plan.room(window, listOf("me", "stage") + ids(30), "me", "stage", page = 2)
        assertNotNull(room.spots["me"])
        assertNotNull(room.spots["stage"])
        val places = Plan.stripCapacity(Size(window.width - 2 * Plan.PAGER_RESERVE, window.height))
        assertEquals(places + 1, room.spots.size)
    }

    /**
     * A strip that pages holds what fits beside the arrows at the window's
     * own shape: counted at the width left between them, a window a little
     * short of upright (an unfolded phone, 840 by 1000) read as upright, its
     * strip counted as narrow upright cards and then drawn as wide ones,
     * running off both sides.
     */
    @Test
    fun aStripThatPagesStaysInsideItsWindow() {
        for (size in listOf(Size(840f, 1000f), Size(700f, 820f), window, phone)) {
            for (count in 3..12) {
                val room = Plan.room(size, listOf("me", "stage") + ids(count), "me", "stage")
                for (id in room.shown) {
                    val spot = room.spots.getValue(id)
                    assertTrue("$count in $size: $spot", spot.left >= -0.5f && spot.right <= size.width + 0.5f)
                }
            }
        }
    }

    @Test
    fun aPageThatNoLongerExistsShowsTheLastOne() {
        val room = Plan.room(window, listOf("me") + ids(10), "me", null, page = 7)
        assertEquals(room.pages - 1, room.page)
        assertFalse(room.shown.isEmpty())
    }

    @Test
    fun twoPeopleAreDrawnAsACallOfTwo() {
        val room = Plan.room(phone, listOf("me", "them"), "me", null)
        assertEquals(Rect(Offset.Zero, phone), room.spots["them"])
        assertTrue(room.spots.getValue("me").width < phone.width / 2)
    }

    @Test
    fun fillingTheScreenDrawsTheStageAlone() {
        val room = Plan.room(window, listOf("me", "stage") + ids(4), "me", "stage", fullscreen = true)
        assertEquals(mapOf("stage" to Rect(Offset.Zero, window)), room.spots)
    }

    @Test
    fun aStageThatIsNotInTheRoomIsTheGrid() {
        val room = Plan.room(window, listOf("me") + ids(4), "me", "gone")
        assertEquals(5, room.spots.size)
        assertNull(room.spots["gone"])
    }

    /** The strain card, stacked above the notices, is lifted clear of the filmstrip's pictures and of nothing else. */
    @Test
    fun theStrainCardEndsAboveTheFilmstrip() {
        val room = Plan.room(window, listOf("me", "stage") + ids(3), "me", "stage")
        val clear = Plan.clear(room.spots, room.shown, window)
        val stripTop = room.shown.mapNotNull { room.spots[it]?.top }.min()
        assertEquals(window.height - stripTop + Plan.GAP, clear)
        val ends = Plan.ISLAND + 8 + 10
        val lift = Plan.lift(clear, ends)
        assertTrue(lift > 0)
        assertTrue(window.height - (ends + lift) <= stripTop - Plan.GAP)
        assertEquals(0f, Plan.lift(clear, clear!! + 40))
        val grid = Plan.room(window, listOf("me") + ids(3), "me", null)
        assertNull(Plan.clear(grid.spots, emptyList(), window))
        assertEquals(0f, Plan.lift(null, ends))
    }

    // Who has the stage (usePin.test.tsx)

    @Test
    fun aShareTakesTheStageWhenItStartsAndGivesItBack() {
        var pin = StagePin()
        pin = pin.update(listOf("a", "b"), emptyList())
        assertNull(pin.pinned)
        pin = pin.update(listOf("a", "b", "a/screen"), listOf("a/screen"))
        assertEquals("a/screen", pin.pinned)
        pin = pin.update(listOf("a", "b"), emptyList())
        assertNull(pin.pinned)
    }

    @Test
    fun aShareAlreadyRunningOnArrivalTakesTheStage() {
        assertEquals("a/screen", StagePin().update(listOf("a", "a/screen"), listOf("a/screen")).pinned)
    }

    /** The way back to the grid: pressing the share on the stage shows everybody, and the share does not climb straight back on. */
    @Test
    fun theGridCanBeChosenWhileSomebodyShares() {
        var pin = StagePin().update(listOf("a", "b", "a/screen"), listOf("a/screen"))
        pin = pin.toggle("a/screen")
        pin = pin.update(listOf("a", "b", "a/screen"), listOf("a/screen"))
        assertNull(pin.pinned)
        // Until a share that is new starts.
        pin = pin.update(listOf("a", "b", "a/screen", "b/screen"), listOf("a/screen", "b/screen"))
        assertEquals("b/screen", pin.pinned)
    }

    @Test
    fun aShareDoesNotTakeAStagePinnedByHand() {
        var pin = StagePin().update(listOf("a", "b"), emptyList()).pin("b")
        pin = pin.update(listOf("a", "b", "a/screen"), listOf("a/screen"))
        assertEquals("b", pin.pinned)
        pin = pin.update(listOf("a", "b"), emptyList())
        assertEquals("an unrelated share ending keeps a pin made by hand", "b", pin.pinned)
    }

    @Test
    fun aPinnedPersonLeavingEmptiesTheStage() {
        val pin = StagePin().update(listOf("a", "b"), emptyList()).pin("b").update(listOf("a"), emptyList())
        assertNull(pin.pinned)
    }

    @Test
    fun theSameTileTwiceTogglesOff() {
        var pin = StagePin().toggle("a")
        assertEquals("a", pin.pinned)
        pin = pin.toggle("a")
        assertNull(pin.pinned)
    }

    @Test
    fun theFirstShareKeepsTheStageAndHandsItOn() {
        var pin = StagePin().update(listOf("a", "b", "a/screen"), listOf("a/screen"))
        pin = pin.update(listOf("a", "b", "a/screen", "b/screen"), listOf("a/screen", "b/screen"))
        assertEquals("a second share does not fight the first for the stage", "a/screen", pin.pinned)
        pin = pin.update(listOf("a", "b", "b/screen"), listOf("b/screen"))
        assertEquals("the remaining share takes over rather than the grid", "b/screen", pin.pinned)
    }

    // Who spoke last (useSpeakingOrder.test.tsx)

    @Test
    fun somebodyTalkingComesForwardAndStaysThroughAPause() {
        val room = listOf("a", "b", "c")
        var record = SpeakingRecord().hear(emptyList(), room, 1.0)
        assertEquals(listOf("a", "b", "c"), record.order(room))
        record = record.hear(listOf("c"), room, 2.0)
        assertEquals(listOf("c", "a", "b"), record.order(room))
        record = record.hear(emptyList(), room, 3.0)
        assertEquals(listOf("c", "a", "b"), record.order(room))
        record = record.hear(listOf("b"), room, 4.0)
        assertEquals(listOf("b", "c", "a"), record.order(room))
    }

    @Test
    fun somebodyWhoLeftIsForgotten() {
        val record = SpeakingRecord().hear(listOf("c"), listOf("a", "c"), 1.0).hear(emptyList(), listOf("a"), 2.0)
        assertEquals(listOf("a", "c"), record.order(listOf("a", "c")))
    }
}
