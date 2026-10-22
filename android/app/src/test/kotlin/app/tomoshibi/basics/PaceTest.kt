package app.tomoshibi.basics

import androidx.compose.runtime.BroadcastFrameClock
import kotlin.math.abs
import kotlin.math.min
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The update's bar at its one speed (Pace), moved by a frame clock the test
 * sends frames of, 16 ms apart: it never draws more than was reported, fills
 * the whole of itself in no less than 1.2 s however suddenly the report gets
 * there, moving the same distance every frame as it does, goes straight back
 * to a report that went back rather than easing down to it, and says it is
 * full only once it is drawn full.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PaceTest {
    private val clock = BroadcastFrameClock()
    private var now = 0L

    private fun TestScope.pace() = Pace(CoroutineScope(backgroundScope.coroutineContext + clock))

    /** Reports [fraction], and lets what that starts run. */
    private fun TestScope.report(pace: Pace, fraction: Float) {
        pace.follow(fraction)
        runCurrent()
    }

    /** What is drawn after each of [count] frames. */
    private fun TestScope.frames(pace: Pace, count: Int): List<Float> = (1..count).map {
        now += 16_000_000
        clock.sendFrame(now)
        runCurrent()
        pace.value
    }

    /** A download reported ten times a second (every sixth frame), [by] further on each time: what was drawn at each frame, beside what had been reported. */
    private fun TestScope.download(pace: Pace, from: Float, to: Float, by: Float): List<Pair<Float, Float>> {
        val seen = mutableListOf<Pair<Float, Float>>()
        var at = from
        while (at != to) {
            at = min(to, at + by)
            report(pace, at)
            for (drawn in frames(pace, 6)) seen += drawn to at
        }
        return seen
    }

    private val even = 16f / 1200f

    @Test
    fun neverDrawsMoreThanWasReported() = runTest {
        val pace = pace()
        // Slowly, where the bar catches up with every report and would run
        // past it; quickly, where it never catches up; and back to the
        // start, as a download begun again is.
        val seen = download(pace, 0f, 0.3f, 0.02f) + download(pace, 0.3f, 1f, 0.15f) +
            run { report(pace, 0.1f); frames(pace, 30).map { it to 0.1f } } +
            download(pace, 0.1f, 0.4f, 0.01f)
        for ((drawn, then) in seen) assertTrue("drew $drawn with $then reported", drawn <= then)
        assertEquals("caught up in the end", 0.4f, seen.last().first, 1e-6f)
    }

    @Test
    fun fillsAtOneSpeedNoFasterThanTheWholeInOnePointTwoSeconds() = runTest {
        val pace = pace()
        // All of it at once: a build that arrived within one report.
        report(pace, 1f)
        val drawn = frames(pace, 120)
        val full = drawn.indexOfFirst { it >= 1f }
        assertTrue("never full: ${drawn.last()}", full >= 0)
        // 1.2 s is 75 frames of 16 ms.
        assertTrue("full after ${full + 1} frames, ${(full + 1) * 16} ms", full + 1 >= 75)
        assertTrue("full only after ${full + 1} frames, ${(full + 1) * 16} ms", full + 1 <= 80)
        val moves = drawn.take(full + 1).zipWithNext { a, b -> b - a }.filter { it > 0f }
        for (move in moves.dropLast(1)) assertTrue("moved $move in a frame, not $even", abs(move - even) < 1e-4f)
        assertTrue("moved ${moves.last()} in its last frame", moves.last() <= even + 1e-4f)
        // Full, and asking the display for nothing more.
        assertFalse("still asking for frames once full", clock.hasAwaiters)
    }

    @Test
    fun keepsItsSpeedThroughReportsThatComeTenTimesASecond() = runTest {
        val pace = pace()
        // Ten reports, each ahead of the bar: none of them may cost it a frame.
        val seen = download(pace, 0f, 1f, 0.1f).map { it.first }
        val moves = (listOf(0f) + seen).zipWithNext { a, b -> b - a }
        val moving = moves.drop(2).takeWhile { it > 0f }.dropLast(1)
        assertTrue("stopped after ${moving.size} frames: $moves", moving.size >= 50)
        for (move in moving) assertTrue("moved $move in a frame, not $even: $moves", abs(move - even) < 1e-4f)
    }

    @Test
    fun goesStraightBackToAReportThatWentBack() = runTest {
        val pace = pace()
        report(pace, 0.8f)
        assertEquals(0.8f, frames(pace, 90).last(), 1e-6f)
        report(pace, 0.2f)
        // At once, and never anywhere between.
        assertEquals(0.2f, pace.value)
        assertEquals(List(10) { 0.2f }, frames(pace, 10))
        // And from there it fills at its pace again rather than jumping.
        report(pace, 0.5f)
        val after = frames(pace, 10)
        assertTrue("jumped to ${after.first()}", after.first() < 0.21f)
        assertTrue("did not move: $after", after.last() > 0.25f)
    }

    @Test
    fun saysItIsFullOnlyOnceItIsDrawnFull() = runTest {
        val pace = pace()
        var full = false
        backgroundScope.launch {
            pace.filled()
            full = true
        }
        report(pace, 0.4f)
        frames(pace, 40)
        report(pace, 1f)
        val seen = (1..100).map { frames(pace, 1).single() to full }
        val said = seen.indexOfFirst { it.second }
        assertTrue("never said so", said >= 0)
        assertEquals("said so with the bar at ${seen[said].first}", 1f, seen[said].first)
        assertFalse("said so a frame late", said > 0 && seen[said - 1].first >= 1f)
    }

    @Test
    fun aResetLetsGoOfWhoeverWaitedAndStopsMovingTheBar() = runTest {
        val pace = pace()
        var letGo = false
        backgroundScope.launch {
            pace.filled()
            letGo = true
        }
        report(pace, 1f)
        frames(pace, 20)
        pace.reset()
        runCurrent()
        assertTrue("still waiting after the reset", letGo)
        assertEquals(0f, pace.value)
        // Nothing left asking the display for a frame for a bar with nowhere to go.
        assertFalse("still asking for frames after the reset", clock.hasAwaiters)
        // The next download's bar, at one speed: not moved twice a frame by
        // the one before the reset as well.
        report(pace, 1f)
        val moves = (listOf(0f) + frames(pace, 30)).zipWithNext { a, b -> b - a }.drop(2)
        for (move in moves) assertTrue("moved $move in a frame, not $even", abs(move - even) < 1e-4f)
    }
}
