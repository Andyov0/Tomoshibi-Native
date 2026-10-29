package app.tomoshibi.ui.kit

import android.os.SystemClock
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController

/**
 * Puts the keyboard away on a tap on nothing in particular and on a scroll
 * (Kit.swift `putsKeyboardAway`): tapped into the lobby's join bar or the
 * link field, the keyboard stayed over half the screen with no way to put it
 * away but pressing Join. The tap is taken only where nothing inside took
 * it, so that a tap on a field or a button is still theirs: a tap laid over
 * everything put the keyboard away again as a field was pressed to bring it
 * up. Put it on the screen's root, around the scrolling content.
 *
 * Only a scroll somebody is making puts it away: a finger dragging the
 * content, or a wheel turned over it. Compose reports the scroll that brings
 * a focused field up above the keyboard as user input too, so taken at its
 * word the keyboard put itself away a second after it came up, wherever it
 * had to move the field to show it: on the sign-in under the deployment's
 * notice and the update, and on the start sheet over a camera picture, where
 * nothing could be typed at all ([ScrollMaker] and the health check of
 * 2026-10-29).
 */
@Composable
fun Modifier.putsKeyboardAway(): Modifier {
    val focus by rememberUpdatedState(LocalFocusManager.current)
    val keyboard by rememberUpdatedState(LocalSoftwareKeyboardController.current)
    val maker = remember { ScrollMaker() }
    val scrolling = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source == NestedScrollSource.UserInput && available != Offset.Zero && maker.somebodyScrolls(SystemClock.uptimeMillis())) {
                    keyboard?.hide()
                    focus.clearFocus()
                }
                return Offset.Zero
            }
        }
    }
    return this
        // Watched on the way down, before anything inside takes the events,
        // and never taken: whether a finger is on the screen is all this asks.
        .pointerInput(Unit) {
            awaitPointerEventScope {
                while (true) {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    maker.saw(event.type == PointerEventType.Scroll, event.changes.any { it.pressed }, event.changes.firstOrNull()?.uptimeMillis ?: SystemClock.uptimeMillis())
                }
            }
        }
        .nestedScroll(scrolling)
        .pointerInput(Unit) {
            detectTapGestures {
                keyboard?.hide()
                focus.clearFocus()
            }
        }
}

/**
 * Whether a scroll is somebody's: a finger is down, or a wheel turned in the
 * last [WHEEL_MILLIS] (a wheel's scroll is animated over the frames after
 * its event). A scroll with neither is the content moving itself, which is
 * how a focused field is brought above the keyboard.
 */
internal class ScrollMaker {
    private var down = false
    private var wheelAt: Long? = null

    /** A pointer event on its way in: whether it was a wheel's, whether anything is pressed, and when (uptime). */
    fun saw(wheel: Boolean, pressed: Boolean, at: Long) {
        down = pressed
        if (wheel) wheelAt = at
    }

    /** Whether a scroll at [now] (uptime) is somebody's. */
    fun somebodyScrolls(now: Long): Boolean = down || wheelAt?.let { now - it <= WHEEL_MILLIS } == true

    companion object {
        const val WHEEL_MILLIS = 300L
    }
}
