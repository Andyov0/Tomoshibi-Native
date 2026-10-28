package app.tomoshibi.ui.lobby

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The camera card's portraits as the shared file says them (PortraitArt.kt):
 * the file read, its paths read, and who is on the card at each moment of
 * the cycle, which is what keeps this card in step with the iPhone's.
 */
class PortraitArtTest {
    /** Two portraits: one of two strokes with a lift between, one of a single stroke. */
    private val two = """
        {
          "format": 1,
          "canvas": {"width": 120, "height": 100},
          "lineWidth": 3,
          "placement": "bottom-center",
          "gap": 0.5,
          "portraits": [
            {"name": "first", "hold": 1, "fade": 0.6, "strokes": [
              {"d": "M 10 10 L 50 10", "start": 0.25, "duration": 1},
              {"d": "M 10,20 C 20 30 30 30 40 20 Q 45 15 50 20 Z", "start": 1.5, "duration": 0.5}
            ]},
            {"name": "second", "hold": 2, "fade": 0.4, "strokes": [
              {"d": "M0 0L10 0 10 10", "start": 0, "duration": 0.8}
            ]}
          ]
        }
    """.trimIndent()

    private fun book(json: String = two) = PortraitBook.parse(json)

    @Test
    fun readsTheFile() {
        val book = book()
        assertEquals(120f, book.width)
        assertEquals(100f, book.height)
        assertEquals(3f, book.lineWidth)
        assertEquals(PortraitBook.Placement.BottomCenter, book.placement)
        assertEquals(0.5, book.gap, 0.0)
        assertEquals(listOf("first", "second"), book.portraits.map { it.name })
        val first = book.portraits[0]
        assertEquals(1.0, first.hold, 0.0)
        assertEquals(0.6, first.fade, 0.0)
        // The drawing lasts until its last stroke is finished.
        assertEquals(2.0, first.writing, 1e-9)
        assertEquals(0.8, book.portraits[1].writing, 1e-9)
    }

    @Test
    fun readsThePathCommands() {
        val stroke = book().portraits[0].strokes[1].path
        assertEquals(
            listOf(
                PathData.Segment.Move(10f, 20f),
                PathData.Segment.Cubic(20f, 30f, 30f, 30f, 40f, 20f),
                PathData.Segment.Quad(45f, 15f, 50f, 20f),
                PathData.Segment.Close,
            ),
            stroke,
        )
        // Numbers run together and repeated, as SVG allows: pairs after a
        // move are lines, and a command's numbers again are another of it.
        assertEquals(
            listOf(PathData.Segment.Move(0f, 0f), PathData.Segment.Line(10f, 0f), PathData.Segment.Line(10f, 10f)),
            book().portraits[1].strokes[0].path,
        )
        assertEquals(
            listOf(PathData.Segment.Move(-1.5f, 2e-1f), PathData.Segment.Line(3f, 4f), PathData.Segment.Line(5f, 6f)),
            PathData.parse("M-1.5,.2 L3 4 5 6"),
        )
        assertEquals(
            listOf(PathData.Segment.Move(1f, 2f), PathData.Segment.Line(3f, 4f), PathData.Segment.Move(5f, 6f), PathData.Segment.Line(7f, 8f)),
            PathData.parse("M 1 2 3 4 M 5 6 7 8"),
        )
    }

    @Test
    fun refusesWhatItCannotDrawAsWritten() {
        // Relative commands and the commands the format leaves out.
        for (d in listOf("m 0 0 l 1 1", "M 0 0 H 10", "M 0 0 V 10", "M 0 0 S 1 1 2 2", "M 0 0 T 1 1", "M 0 0 A 1 1 0 0 1 2 2")) {
            assertThrows(d, PortraitFormatException::class.java) { PathData.parse(d) }
        }
        // A stroke must start with a move, have whole commands, and say something.
        assertThrows(PortraitFormatException::class.java) { PathData.parse("L 1 1") }
        assertThrows(PortraitFormatException::class.java) { PathData.parse("1 2 L 3 4") }
        assertThrows(PortraitFormatException::class.java) { PathData.parse("M 0 0 C 1 1 2 2") }
        assertThrows(PortraitFormatException::class.java) { PathData.parse("M 0 0 L 1 x") }
        assertThrows(PortraitFormatException::class.java) { PathData.parse("  ") }
        // The file's own promises.
        assertThrows(PortraitFormatException::class.java) { book(two.replace("\"format\": 1", "\"format\": 2")) }
        assertThrows(PortraitFormatException::class.java) { book(two.replace("bottom-center", "center")) }
        assertThrows(PortraitFormatException::class.java) { book(two.replace("\"duration\": 0.8", "\"duration\": 0")) }
        assertThrows(PortraitFormatException::class.java) { book(two.replace("\"hold\": 2", "\"hold\": -1")) }
        assertThrows(PortraitFormatException::class.java) { book(two.replace("\"width\": 120", "\"width\": 0")) }
        assertThrows(PortraitFormatException::class.java) { book("""{"format": 1, "canvas": {"width": 1, "height": 1}, "lineWidth": 1, "placement": "bottom-center", "gap": 0, "portraits": []}""") }
        assertThrows(PortraitFormatException::class.java) { book("not json") }
        assertThrows(PortraitFormatException::class.java) { book(two.replace("\"lineWidth\": 3,", "")) }
    }

    @Test
    fun aStrokeIsDrawnAlongItsTimeEasedInAndOut() {
        val stroke = book().portraits[0].strokes[0] // 0.25 after the gap, for one second
        assertEquals(0f, stroke.drawn(-1.0), 0f)
        assertEquals(0f, stroke.drawn(0.25), 0f)
        assertEquals(0.5f, stroke.drawn(0.75), 1e-6f)
        assertEquals(1f, stroke.drawn(1.25), 0f)
        assertEquals(1f, stroke.drawn(9.0), 0f)
        // The README's smoothstep, u * u * (3 - 2u), which the iPhone's pen
        // follows too: a quarter of the way in time is 0.15625 of the line.
        assertEquals(0.15625f, stroke.drawn(0.5), 1e-6f)
        assertEquals(0.84375f, stroke.drawn(1.0), 1e-6f)
    }

    @Test
    fun theCycleGoesThroughEachPersonInTurn() {
        val timeline = PortraitTimeline(book())
        // Each turn is the gap first, then the drawing, the hold and the fade:
        // first 0.5 + 2 + 1 + 0.6 = 4.1, second 0.5 + 0.8 + 2 + 0.4 = 3.7.
        assertEquals(7.8, timeline.round, 1e-9)

        // The gap: empty glass, still until the first stroke starts.
        timeline.at(0.1).let {
            assertEquals(0, it.person)
            assertEquals(-0.4, it.tau, 1e-9)
            assertFalse(it.moving)
            assertEquals(0.75, it.until, 1e-9)
        }
        // Drawing the first stroke: moving until it is finished.
        timeline.at(1.1).let {
            assertEquals(0.6, it.tau, 1e-9)
            assertTrue(it.moving)
            assertEquals(1.75, it.until, 1e-9)
        }
        // The pen lifted between strokes: still until the next.
        timeline.at(1.8).let {
            assertFalse(it.moving)
            assertEquals(2.0, it.until, 1e-9)
        }
        // Held finished: still, all of it shown, until the fade.
        timeline.at(3.0).let {
            assertFalse(it.moving)
            assertEquals(3.5, it.until, 1e-9)
            assertEquals(1f, it.shown)
            assertEquals(2.5, it.tau, 1e-9)
        }
        // Fading: moving, half gone half way through, on the same curve.
        timeline.at(3.8).let {
            assertTrue(it.moving)
            assertEquals(4.1, it.until, 1e-9)
            assertEquals(0.5f, it.shown, 1e-6f)
        }
        timeline.at(3.5 + 0.15).let { assertEquals(1f - 0.15625f, it.shown, 1e-6f) }
        // The second portrait's gap, then its stroke.
        timeline.at(4.1 + 0.2).let {
            assertEquals(1, it.person)
            assertFalse(it.moving)
            assertEquals(4.6, it.until, 1e-9)
        }
        timeline.at(4.1 + 0.9).let {
            assertEquals(1, it.person)
            assertEquals(0.4, it.tau, 1e-9)
            assertTrue(it.moving)
            assertEquals(5.4, it.until, 1e-9)
        }
        // And round again, the moments of the next round those of the first.
        timeline.at(7.8 + 1.1).let {
            assertEquals(0, it.person)
            assertEquals(0.6, it.tau, 1e-9)
            assertEquals(7.8 + 1.75, it.until, 1e-9)
        }
        timeline.at(3 * 7.8 + 5.0).let {
            assertEquals(1, it.person)
            assertEquals(0.4, it.tau, 1e-6)
        }
    }

    @Test
    fun aMomentIsNeverOutsideTheCycle() {
        val timeline = PortraitTimeline(book())
        for (t in listOf(-5.0, Double.NaN, Double.POSITIVE_INFINITY, timeline.round, timeline.round * 1000)) {
            val moment = timeline.at(t)
            assertTrue("$t: ${moment.person}", moment.person in 0..1)
            assertTrue("$t: ${moment.tau}", moment.tau >= -0.5)
        }
        // The very end of a round is the start of the next, not past the last turn.
        timeline.at(timeline.round).let {
            assertEquals(0, it.person)
            assertEquals(-0.5, it.tau, 1e-9)
        }
    }

    @Test
    fun theStillMomentsLastUntilSomethingMoves() {
        // Walking the cycle by its own `until`, every still stretch ends
        // where something moves or a turn begins, and the walk never stands
        // still: what the card's frame loop sleeps through is all there is.
        val timeline = PortraitTimeline(book())
        var t = 0.0
        var steps = 0
        while (t < timeline.round * 2) {
            val moment = timeline.at(t)
            assertTrue("stuck at $t", moment.until > t)
            t = moment.until + 1e-9
            steps++
        }
        // Six stretches for the first (gap, stroke, lift, stroke, hold,
        // fade) and four for the second, twice.
        assertEquals(20, steps)
    }

    /**
     * The file every platform draws from, as the build copies it into the
     * app: it must be one this can draw, or the card would be the pane alone.
     */
    @Test
    fun theSharedFileIsOneThisCanDraw() {
        val file = File("../../art/portraits.json")
        assertTrue("${file.absolutePath} is missing", file.isFile)
        val book = PortraitBook.parse(file.readText())
        assertTrue(book.portraits.isNotEmpty())
        assertEquals(book.portraits.size, book.portraits.map { it.name }.toSet().size)
        for (art in book.portraits) {
            assertTrue(art.name, art.writing > 0)
            assertTrue(art.name, art.strokes.all { it.path.first() is PathData.Segment.Move })
        }
        val timeline = PortraitTimeline(book)
        assertNotEquals(0.0, timeline.round)
    }
}
