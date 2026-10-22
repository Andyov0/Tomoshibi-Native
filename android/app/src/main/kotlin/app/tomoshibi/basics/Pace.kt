package app.tomoshibi.basics

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.platform.AndroidUiDispatcher
import kotlin.coroutines.coroutineContext
import kotlin.math.min
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * How far something has got, drawn at one steady speed rather than as it was
 * reported: the update's download (Updates `downloadBar`), which the lobby's
 * banner and the About page draw as it is. SteadyFill.swift on Apple and
 * Pace.cs on Windows follow the same rule.
 *
 * The core reports a download up to ten times a second (UpdateProgress), and
 * on a fast connection each report is far past the one before. Drawn as they
 * came, or eased toward each on the arriving curve as the bar was until
 * 2026-10-22, it went in jumps, quick and slow by turns, which the owner found
 * neither smooth nor even. Here what is drawn moves toward what was reported
 * at one speed, the whole bar in [WHOLE] and never faster, and never past what
 * was reported, so that it never shows more arrived than has. A report that
 * goes back (a download begun again, another build) is drawn there at once:
 * a bar going backwards looks like something being undone.
 *
 * Moved by the display's frames ([frames], whose context has a frame clock),
 * by the time since the frame before rather than by a step a frame, so that
 * a phone drawing 120 frames a second fills it no faster than one drawing
 * 60. A frame longer than [LONGEST_FRAME] counts as that long, so that after
 * a stall the bar goes on from where it stood rather than leaping. Nothing
 * runs between frames once it has caught up.
 *
 * [filled] is for the step after the bar: installing waits on it, so that the
 * system's confirmation is not put up over a bar still partway, as though the
 * download had been cut short.
 *
 * For the main thread only.
 */
@Stable
class Pace(
    /**
     * Where it is moved from, frame by frame: the display's ([onTheDisplay])
     * in the app. Null where it is moved by hand ([advance]), as the tests of
     * what waits on it move it.
     */
    private val frames: CoroutineScope?,
    /**
     * The whole bar at the fastest, in nanoseconds. Nought draws every report
     * as it comes and is full the moment the last arrives, for the tests of
     * the update's steps, which are not about the bar.
     */
    private val whole: Long = WHOLE,
) {
    /** What to draw, from nought to one. */
    var value by mutableFloatStateOf(0f)
        private set

    /** Where it is going: the last report, from nought to one. */
    var target by mutableFloatStateOf(0f)
        private set

    /** Whether what is drawn is what was last reported. */
    val caughtUp: Boolean get() = value >= target

    /** When the frame before was, while it is moving; [STILL] while it is not. */
    private var lastFrame = STILL
    private var moving: Job? = null
    /** Completed while the bar is full, or once [reset] lets its waiters go. */
    private var full = CompletableDeferred<Unit>()

    /**
     * Takes a report. One that went back is drawn there at once: left to
     * the frames, which only ever move it forward, the bar would stand where
     * it was, past what has arrived, until the report overtook it again.
     */
    fun follow(report: Float) {
        val to = if (report.isNaN()) 0f else report.coerceIn(0f, 1f)
        target = to
        if (to < value || whole <= 0) value = to
        settle()
        if (frames != null && value < target && moving == null) moving = frames.launch { move() }
    }

    /** Moves the bar on by what [elapsed] nanoseconds allow, no further than the report. */
    fun advance(elapsed: Long) {
        value = step(value, target, elapsed, whole)
        settle()
    }

    /**
     * Empty again, for the next download. Whoever was waiting for the bar to
     * fill is let go: what they were waiting to follow is over, and they look
     * again at what there is to do.
     */
    fun reset() {
        moving?.cancel()
        moving = null
        lastFrame = STILL
        target = 0f
        value = 0f
        full.complete(Unit)
        full = CompletableDeferred()
    }

    /** Returns once the bar is drawn full, at once where it already is; or when it is [reset]. */
    suspend fun filled() {
        if (value < 1f) full.await()
    }

    private suspend fun move() {
        val mine = coroutineContext[Job]
        try {
            while (value < target) {
                withFrameNanos { now ->
                    if (lastFrame != STILL) advance(now - lastFrame)
                    lastFrame = now
                }
            }
        } finally {
            // Not where [reset] has stopped this one and the next report
            // started another, which would be left with nothing moving it.
            if (moving === mine) {
                moving = null
                lastFrame = STILL
            }
        }
    }

    private fun settle() {
        if (value >= 1f) {
            full.complete(Unit)
        } else if (full.isCompleted) {
            full = CompletableDeferred()
        }
    }

    companion object {
        /**
         * The whole bar, from nought to one, at the fastest: 1.2 seconds, long
         * enough to be seen filling when the build arrives in a moment, short
         * enough that nobody waits on it.
         */
        const val WHOLE = 1_200_000_000L

        /** The longest a frame counts for, in nanoseconds: a frame at twenty a second. */
        const val LONGEST_FRAME = 50_000_000L

        private const val STILL = Long.MIN_VALUE

        /**
         * One move from [at] toward [target], [elapsed] nanoseconds after the
         * one before: never past it, which is what keeps the bar from saying
         * more has arrived than has, and onto it exactly at the end, which is
         * what [filled] waits for.
         */
        internal fun step(at: Float, target: Float, elapsed: Long, whole: Long = WHOLE): Float =
            if (whole <= 0) target else min(target, at + elapsed.coerceIn(0, LONGEST_FRAME).toFloat() / whole)

        /**
         * A pace moved by the display's frames: the main thread's frame clock,
         * the one Compose draws by, so that the bar moves once a frame, in
         * step with what is drawn, and not at all while nothing is.
         */
        fun onTheDisplay(): Pace = Pace(CoroutineScope(SupervisorJob() + AndroidUiDispatcher.Main))
    }
}
