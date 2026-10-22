package app.tomoshibi.basics

import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.platform.AndroidUiDispatcher
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLooper

/**
 * The app's bar (Pace.onTheDisplay) is moved by the main thread's frames
 * with nothing else driving it, at its one speed: PaceTest's frames are the
 * test's own, and a bar given no frame clock would stand still in the app,
 * and the install waiting on it would never start.
 *
 * Measured by the frames' own times, read by a second listener on the same
 * clock: Robolectric runs a run of frames 15 ms apart within one turn of the
 * main looper, whose clock says nothing of them.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class PaceOnTheDisplayTest {
    @Test
    fun theAppsBarIsMovedByTheDisplaysFrames() {
        val pace = Pace.onTheDisplay()
        val seen = mutableListOf<Pair<Long, Float>>()
        val watching = CoroutineScope(AndroidUiDispatcher.Main)
        // Two hundred frames at most, three seconds: Robolectric goes on
        // drawing frames within one turn for as long as somebody asks for one.
        watching.launch {
            repeat(200) {
                // Read once the frame is over, the bar's own move in it made.
                val time = withFrameNanos { it }
                seen += time to pace.value
                if (pace.value >= 1f) return@launch
            }
        }
        pace.follow(1f)
        repeat(20) { ShadowLooper.idleMainLooper(16, TimeUnit.MILLISECONDS) }
        watching.cancel()
        val full = seen.indexOfFirst { it.second >= 1f }
        assertTrue("never full: ${seen.lastOrNull()}", full >= 0)
        val took = (seen[full].first - seen.first().first) / 1_000_000
        assertTrue("full in $took ms of frames", took >= 1_150)
        assertTrue("full only in $took ms of frames", took <= 1_300)
        assertTrue("never moved between: $seen", seen.count { it.second > 0f && it.second < 1f } >= 60)
    }
}
