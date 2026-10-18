package app.tomoshibi.ui.room

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import app.tomoshibi.ui.Hinge
import app.tomoshibi.ui.Posture
import app.tomoshibi.ui.kit.paneAround
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Where the call's parts go on a foldable half open (RoomShape): the rule
 * the layout exists for is that no picture lies across the hinge, whoever is
 * in the call and whatever is on the stage, and that the controls and the
 * panels stand on the half they belong to. Worked out in points, at a point
 * and a half to the pixel, the Fold6's inner screen turned (980 by 840) for
 * the table and upright (840 by 980) for the book.
 */
class RoomShapeTest {
    private val density = Density(1.5f)

    private fun ids(people: Int) = listOf(ME) + (1..people).map { "p$it/camera" }

    private fun crosses(spot: Rect, fold: RoomFold): Boolean = when (fold) {
        is RoomFold.Table -> spot.top < fold.bottom && spot.bottom > fold.top
        is RoomFold.Book -> spot.left < fold.end && spot.right > fold.start
        is RoomFold.Above -> spot.bottom > fold.top
    }

    @Test
    fun aPhoneOnATableIsTheHingeAcrossItsMiddle() {
        val fold = RoomFold.of(TABLE_HINGE, density, LayoutDirection.Ltr, 0f, 0f, 980f, 840f)
        assertEquals(RoomFold.Table(420f, 420f), fold)
    }

    @Test
    fun theKeyboardOverTheLowerHalfKeepsTheRoomToTheUpper() {
        // The keyboard takes 350 of the lower 420 points, leaving the room 490.
        val fold = RoomFold.of(TABLE_HINGE, density, LayoutDirection.Ltr, 0f, 0f, 980f, 490f)
        assertEquals(RoomFold.Above(420f), fold)
        // Taller than the lower half, the keyboard leaves a room that ends above the hinge, which is flat.
        assertNull(RoomFold.of(TABLE_HINGE, density, LayoutDirection.Ltr, 0f, 0f, 980f, 400f))
    }

    @Test
    fun aBookReadsFromItsStartWhicheverWayItReads() {
        assertEquals(RoomFold.Book(420f, 420f), RoomFold.of(BOOK_HINGE, density, LayoutDirection.Ltr, 0f, 0f, 840f, 980f))
        // A hinge a little left of the middle, read from the right.
        val off = Hinge(left = 600f, top = 0f, right = 630f, bottom = 1470f, vertical = true)
        assertEquals(RoomFold.Book(420f, 440f), RoomFold.of(off, density, LayoutDirection.Rtl, 0f, 0f, 840f, 980f))
    }

    @Test
    fun aHingeNearAnEdgeIsNoHinge() {
        // A window in split screen that only just reaches over the fold.
        val edge = Hinge(left = 120f, top = 0f, right = 120f, bottom = 1470f, vertical = true)
        assertNull(RoomFold.of(edge, density, LayoutDirection.Ltr, 0f, 0f, 840f, 980f))
    }

    @Test
    fun noPictureLiesAcrossTheHingeOnATable() {
        val fold = RoomFold.Table(420f, 420f)
        for (people in 0..15) for (pinned in listOf(false, true)) for (panel in listOf(false, true)) {
            val pictures = ids(people)
            val stage = if (pinned && people > 0) pictures[1] else null
            val shape = RoomShape.of(Size(980f, 840f), fold, panel, false, false, pinned = stage != null)
            val room = shape.arrange(pictures, ME, stage, false, 0)
            for ((id, spot) in room.spots) {
                assertTrue("$people people, pinned $pinned, panel $panel: $id at $spot", !crosses(spot, fold))
            }
            // The stage above, everything else of the call below.
            if (stage != null) {
                assertTrue(room.spots.getValue(stage).bottom <= fold.top)
                for (id in room.shown) assertTrue("$id in the strip", room.spots.getValue(id).top >= fold.bottom)
            }
            assertTrue(shape.bar.top >= fold.bottom && shape.panel.top >= fold.bottom)
        }
    }

    @Test
    fun noPictureLiesAcrossTheHingeOfABook() {
        val fold = RoomFold.Book(420f, 420f)
        for (people in 0..15) for (pinned in listOf(false, true)) for (panel in listOf(false, true)) for (page in 0..1) {
            val pictures = ids(people)
            val stage = if (pinned && people > 0) pictures[1] else null
            val shape = RoomShape.of(Size(840f, 980f), fold, panel, false, false, pinned = stage != null)
            val room = shape.arrange(pictures, ME, stage, false, page)
            for ((id, spot) in room.spots) {
                assertTrue("$people people, pinned $pinned, panel $panel: $id at $spot", !crosses(spot, fold))
            }
            // An open panel has the second page to itself.
            if (panel) for (spot in room.spots.values) assertTrue(spot.right <= fold.start)
            assertTrue(shape.bar.right <= fold.start)
        }
    }

    @Test
    fun theStageFillingTheScreenFillsAHalf() {
        val table = RoomShape.of(Size(980f, 840f), RoomFold.Table(420f, 420f), false, true, false, pinned = true)
        assertEquals(Rect(0f, 0f, 980f, 420f), table.arrange(ids(1), ME, "p1/camera", true, 0).spots["p1/camera"])
        val book = RoomShape.of(Size(840f, 980f), RoomFold.Book(420f, 420f), false, true, false, pinned = true)
        assertEquals(Rect(0f, 0f, 420f, 980f), book.arrange(ids(1), ME, "p1/camera", true, 0).spots["p1/camera"])
        assertEquals(Posture.Book, book.posture)
    }

    @Test
    fun aPopoverKeepsToTheHalfItsControlIsOn() {
        val window = IntSize(1470, 1260)
        val table = Hinge(0f, 630f, 1470f, 630f, vertical = false)
        assertEquals(androidx.compose.ui.unit.IntRect(0, 630, 1470, 1260), paneAround(table, window, 700f, 1200f, 300f))
        assertEquals(androidx.compose.ui.unit.IntRect(0, 0, 1470, 630), paneAround(table, window, 700f, 100f, 300f))
        val book = Hinge(735f, 0f, 735f, 1260f, vertical = true)
        assertEquals(androidx.compose.ui.unit.IntRect(0, 0, 735, 1260), paneAround(book, window, 300f, 1200f, 300f))
        assertEquals(androidx.compose.ui.unit.IntRect(0, 0, 1470, 1260), paneAround(null, window, 300f, 1200f, 300f))
    }

    private companion object {
        const val ME = "me/camera"
    }
}
