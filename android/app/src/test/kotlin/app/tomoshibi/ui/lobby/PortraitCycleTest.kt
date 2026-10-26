package app.tomoshibi.ui.lobby

import androidx.compose.runtime.MonotonicFrameClock
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The camera card's frame loop (PreviewGlass `CycleState.run`) against a
 * display of its own, on the test's virtual clock: it moves the drawing with
 * the frames, no more than sixty times a second on a display of 125, asks
 * for no frames at all while nothing moves, and goes on from where it was
 * stopped. The first of these is what the card on an emulator showed to be
 * missing: the loop ran every frame and drew nothing.
 */
class PortraitCycleTest {
    /** The book of PortraitArtTest: first's stroke from 0.25 to 1.25 after a 0.5 gap, held from 2.0 to 3.0 after it. */
    private val book = PortraitBook.parse(
        """
        {"format": 1, "canvas": {"width": 120, "height": 100}, "lineWidth": 3, "placement": "bottom-center", "gap": 0.5,
         "portraits": [
           {"name": "first", "hold": 1, "fade": 0.6, "strokes": [
             {"d": "M 10 10 L 50 10", "start": 0.25, "duration": 1},
             {"d": "M 10 20 L 40 20", "start": 1.5, "duration": 0.5}]},
           {"name": "second", "hold": 2, "fade": 0.4, "strokes": [{"d": "M 0 0 L 10 0", "start": 0, "duration": 0.8}]}]}
        """.trimIndent(),
    )

    /**
     * A display drawing every 8 milliseconds, whose frame times are far from
     * nought as a phone's are, and which notes where the drawing stood at
     * each frame it was asked for.
     */
    private class Display(private val test: TestScope, private val cycle: () -> CycleState) : MonotonicFrameClock {
        /** Virtual milliseconds of each frame asked for, and how far the drawing was written then. */
        val frames = mutableListOf<Pair<Long, Double>>()

        override suspend fun <R> withFrameNanos(onFrame: (frameTimeNanos: Long) -> R): R {
            delay(8)
            val ms = test.testScheduler.currentTime
            frames += ms to cycle().written
            return onFrame(nanos(ms))
        }

        companion object {
            /** About two months of uptime in nanoseconds: a frame time as a phone's monotonic clock gives it. */
            const val UPTIME = 5_000_000_000_000_000L
            fun nanos(ms: Long) = UPTIME + ms * 1_000_000L
        }
    }

    private fun TestScope.cycleOnADisplay(): Pair<CycleState, Display> {
        lateinit var cycle: CycleState
        cycle = CycleState(PortraitTimeline(book)) { Display.nanos(testScheduler.currentTime) }
        return cycle to Display(this) { cycle }
    }

    @Test
    fun theDrawingMovesWithTheFramesAtSixtyASecond() = runTest {
        val (cycle, display) = cycleOnADisplay()
        val job = launch(display) { cycle.run() }
        // 1.108 seconds after the first frame at 8: 0.6 after the gap.
        advanceTimeBy(1_116)
        assertEquals(0, cycle.person)
        assertEquals(0.6, cycle.written, 0.02)
        job.cancel()
        // While the first stroke is drawn (0.75 to 1.75 into the cycle), each
        // frame the display draws shows the drawing further on, sixty times a
        // second and no more: a display at 125 a second draws every other one.
        val drawing = display.frames.filter { (ms, _) -> ms in 850..1_100 }.map { it.second }
        val moves = drawing.zipWithNext().count { (a, b) -> b != a }
        assertTrue("$moves moves in a quarter of a second", moves in 13..16)
    }

    @Test
    fun nothingIsAskedOfTheDisplayWhileNothingMoves() = runTest {
        val (cycle, display) = cycleOnADisplay()
        val job = launch(display) { cycle.run() }
        advanceTimeBy(4_000)
        job.cancel()
        // The first child is held finished from 2.5 to 3.5 into the cycle,
        // and the pen is lifted between their strokes from 1.75 to 2.0.
        assertTrue(display.frames.count { (ms, _) -> ms in 2_560..3_480 } <= 1)
        assertTrue(display.frames.count { (ms, _) -> ms in 1_800..1_990 } <= 1)
        // While the first stroke is drawn, every frame is asked for.
        assertTrue(display.frames.count { (ms, _) -> ms in 900..1_100 } >= 20)
    }

    @Test
    fun stoppedAndStartedAgainItGoesOnFromWhereItWas() = runTest {
        val (cycle, display) = cycleOnADisplay()
        var job = launch(display) { cycle.run() }
        advanceTimeBy(1_116)
        job.cancel()
        val stopped = cycle.written
        // Five seconds with the card covered or the app behind another.
        advanceTimeBy(5_000)
        job = launch(display) { cycle.run() }
        advanceTimeBy(216)
        job.cancel()
        assertEquals(stopped + 0.2, cycle.written, 0.03)
    }
}
