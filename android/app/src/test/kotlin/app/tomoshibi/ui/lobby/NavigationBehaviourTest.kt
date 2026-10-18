package app.tomoshibi.ui.lobby

import app.tomoshibi.model.host.standInRules
import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.click
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.media.CallAudio
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.ui.TomoshibiApp
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/*
 * What Back, a swipe and a fold do on the way into a call. What these guard
 * is somebody leaving the app when they meant to close a sheet, leaving a
 * call when they meant to close a panel, and losing what they typed because
 * the phone was opened.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = PHONE)
class NavigationBehaviourTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var audio: CallAudio? = null

    @Before
    fun setUp() {
        standInRules()
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        LobbyDebug.open = null
        LobbyDebug.room = null
        LobbyDebug.popover = null
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        LobbyDebug.open = null
        audio?.stop()
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        settle()
    }

    private val backIsTaken: Boolean get() = compose.activity.onBackPressedDispatcher.hasEnabledCallbacks()

    @Test
    fun backInTheLobbyIsTheSystemsAndLeavesTheApp() {
        compose.setContent { TestApp(fakeApp()) }
        settle()
        assertFalse("nothing in the lobby may hold Back with nothing open", backIsTaken)
    }

    @Test
    fun backClosesTheStartSheetAndCallsOffItsJoin() {
        LobbyDebug.open = "start"
        compose.setContent { TestApp(fakeApp()) }
        settle()
        compose.onNodeWithText("Start new meeting").assertExists()
        assertTrue(backIsTaken)
        back()
        assertEquals(0, compose.onAllNodesWithText("Start new meeting").fetchSemanticsNodes().size)
        assertFalse(backIsTaken)
    }

    @Test
    fun backClosesTheScheduleSheet() {
        LobbyDebug.open = "scheduled"
        compose.setContent { TestApp(fakeApp()) }
        settle()
        compose.onNodeWithText("Arrange").assertExists()
        back()
        assertEquals(0, compose.onAllNodesWithText("Arrange").fetchSemanticsNodes().size)
    }

    @Test
    fun backTakesThePushedServerPageAway() {
        LobbyDebug.open = "servers"
        compose.setContent { TestApp(fakeApp()) }
        settle()
        compose.onNodeWithText("Measure again").assertExists()
        back()
        assertEquals(0, compose.onAllNodesWithText("Measure again").fetchSemanticsNodes().size)
    }

    @Test
    fun backOnTheJoinScreenGoesBackToTheLobby() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        compose.setContent { TestApp(app) }
        settle()
        back()
        assertEquals(Screen.Lobby, app.screen)
    }

    @Test
    fun backInACallDoesNotLeaveIt() {
        val app = fakeApp(screen = Screen.Call)
        val made = silentAudio().also { audio = it }
        val call = fakeCall(made)
        app.stage("call", call)
        compose.setContent { TestApp(app) }
        settle()
        assertTrue("Back in a call must be held, or the activity finishes and leaves the call", backIsTaken)
        back()
        assertEquals(Screen.Call, app.screen)
        assertNotNull(app.call)
    }

    @Test
    fun aRecentRoomOpensAgainAndAsksWhetherItIsLive() {
        val app = fakeApp()
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithText("friday-standup").performClick()
        settle()
        compose.onNodeWithText("Join a meeting").assertExists()
        assertEquals(listOf("friday-standup"), (app.deployment as FakeDeployment).asked)
    }

    @Test
    fun aRecentRoomSwipedAllTheWayIsForgotten() {
        val app = fakeApp()
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithText("friday-standup").performTouchInput { swipeLeft(startX = right - 4f, endX = left - 200f, durationMillis = 300) }
        settle()
        assertFalse(app.lobby.recentHere.any { it.room == "friday-standup" })
        assertEquals(0, compose.onAllNodesWithText("friday-standup").fetchSemanticsNodes().size)
    }

    @Test
    fun aShortSwipeOnlyShowsRemove() {
        val app = fakeApp()
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithText("friday-standup").performTouchInput { swipeLeft(startX = right - 10f, endX = right - 260f, durationMillis = 400) }
        settle()
        assertTrue(app.lobby.recentHere.any { it.room == "friday-standup" })
        compose.onNodeWithContentDescription("Remove friday-standup from recent").performClick()
        settle()
        assertFalse(app.lobby.recentHere.any { it.room == "friday-standup" })
    }

    @Test
    fun aPressOnASheetIsTheSheets() {
        LobbyDebug.open = "settings"
        compose.setContent { TestApp(fakeApp()) }
        settle()
        // On nothing in particular on the sheet, over the lobby's own Start
        // button and the dimming that closes the sheet when it is pressed.
        compose.onRoot().performTouchInput { click(Offset(200f, 537f)) }
        settle()
        // The settings themselves, not the stand-in this was first written
        // against: the sheet is still up.
        assertTrue(compose.onAllNodesWithText("Settings").fetchSemanticsNodes().isNotEmpty())
        assertEquals(0, compose.onAllNodesWithText("Start new meeting").fetchSemanticsNodes().size)
    }

    /** In a window as wide as an unfolded Fold, the app drawn first in a phone's width of it and then in all of it. */
    @Test
    @Config(qualifiers = WIDE)
    fun openingThePhoneKeepsTheSheetAndWhatWasTypedInIt() {
        LobbyDebug.open = "join"
        var wide by mutableStateOf(false)
        val app = fakeApp()
        compose.setContent {
            Box(Modifier.width(if (wide) 840.dp else 412.dp).fillMaxHeight()) { TestApp(app) }
        }
        settle()
        compose.onNodeWithContentDescription("Room name or invite link").performTextInput("team-sync")
        settle()
        wide = true
        settle()
        // The wide window's sheet, with its Cancel beside Join, and the room still typed.
        compose.onNodeWithText("Cancel").assertExists()
        compose.onNodeWithText("team-sync").assertExists()
    }
}
