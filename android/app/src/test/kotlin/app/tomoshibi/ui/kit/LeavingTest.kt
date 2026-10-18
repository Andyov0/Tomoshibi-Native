package app.tomoshibi.ui.kit

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * Glass leaves as it arrives: still on screen partway through Theme.leave's
 * 200 ms, gone after. GlassAppear fades by hand rather than with fadeOut
 * (whose offscreen layer cut the glass's shadow off), so nothing but this
 * says the fade is still waited for and the view does not vanish at once.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class LeavingTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun glassStaysWhileItFadesAndThenGoes() {
        var shown by mutableStateOf(true)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            GlassAppear(shown) { Box(Modifier.size(40.dp).glassSurface(Capsule).testTag("glass")) }
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithTag("glass").assertIsDisplayed()
        shown = false
        // A frame at a time: one long step lets the leaving view be taken
        // away within it whatever its fade, and passes either way.
        val there = (1..25).map {
            compose.mainClock.advanceTimeBy(16)
            compose.onAllNodesWithTag("glass").fetchSemanticsNodes().isNotEmpty()
        }
        assertTrue("gone before its fade had played: $there", there.take(8).all { it })
        assertFalse("still there long after its fade: $there", there.last())
    }
}
