package app.tomoshibi.ui.kit

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
 */
@Composable
fun Modifier.putsKeyboardAway(): Modifier {
    val focus by rememberUpdatedState(LocalFocusManager.current)
    val keyboard by rememberUpdatedState(LocalSoftwareKeyboardController.current)
    val scrolling = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source == NestedScrollSource.UserInput && available != Offset.Zero) {
                    keyboard?.hide()
                    focus.clearFocus()
                }
                return Offset.Zero
            }
        }
    }
    return nestedScroll(scrolling).pointerInput(Unit) {
        detectTapGestures {
            keyboard?.hide()
            focus.clearFocus()
        }
    }
}
