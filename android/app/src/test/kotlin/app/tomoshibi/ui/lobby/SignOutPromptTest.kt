package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.width
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import com.github.takahirom.roborazzi.captureScreenRoboImage
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/*
 * Sign out asked before it is done, from both of its places (the account
 * menu on a wide screen, the account's settings on a phone): Sign out once
 * signed out on the press, and a press meant for the item beside it did it
 * too. What these guard: nothing signed out until the question is answered
 * yes; Cancel, a press on the glass and Back each leave you signed in, Back
 * closing the question rather than the settings under it; the card growing
 * in rather than appearing, and leaving rather than vanishing.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = PHONE)
class SignOutPromptTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Before
    fun setUp() {
        standInRules()
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        LobbyDebug.open = null
        LobbyDebug.section = null
        LobbyDebug.popover = null
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.section = null
        LobbyDebug.popover = null
    }

    private fun settle(millis: Long = 2000) {
        compose.mainClock.advanceTimeBy(millis)
        compose.waitForIdle()
    }

    private val AppModel.signedOut: Int get() = (deployment as FakeDeployment).signedOut

    /** The app, with the settings' platform stood in: its account page asks the core's rules otherwise. */
    @Composable
    private fun Shown(app: AppModel) {
        CompositionLocalProvider(LocalSettingsPlatform provides FakePlatform()) { TestApp(app) }
    }

    /** The phone's settings open at the account, and its Sign out pressed. */
    private fun askFromTheSettings(app: AppModel) {
        LobbyDebug.open = "settings"
        LobbyDebug.section = "account"
        compose.setContent { Shown(app) }
        settle()
        compose.onNodeWithTag("settings.signout").performScrollTo().performClick()
        settle()
    }

    @Test @Config(qualifiers = WIDE)
    fun theAccountMenusSignOutAsksAndYesSignsOut() {
        val app = fakeApp()
        LobbyDebug.popover = "account"
        compose.setContent { Shown(app) }
        settle()
        compose.onNodeWithText("Sign out").performClick()
        settle()
        assertTrue(app.signOutAsked)
        assertNotNull("signed out on the press", app.account)
        assertEquals(0, app.signedOut)
        compose.onNodeWithText("Sign out of Tomoshibi?").assertIsDisplayed()
        compose.onNodeWithText("To come back, you will sign in again with your name and passphrase.").assertIsDisplayed()
        compose.onNodeWithTag("signout.confirm").performClick()
        settle()
        assertFalse(app.signOutAsked)
        assertEquals(1, app.signedOut)
        assertNull(app.account)
        assertEquals(0, compose.onAllNodesWithTag("signout.prompt").fetchSemanticsNodes().size)
    }

    @Test
    fun cancelLeavesYouSignedIn() {
        val app = fakeApp()
        askFromTheSettings(app)
        assertTrue(app.signOutAsked)
        compose.onNodeWithTag("signout.cancel").performClick()
        settle()
        assertFalse(app.signOutAsked)
        assertNotNull(app.account)
        assertEquals(0, app.signedOut)
        assertEquals(0, compose.onAllNodesWithTag("signout.prompt").fetchSemanticsNodes().size)
    }

    @Test
    fun aPressOnTheGlassIsCancel() {
        val app = fakeApp()
        askFromTheSettings(app)
        // Over the settings sheet, well clear of the card in the middle.
        compose.onNodeWithTag("signout.glass").performTouchInput { click(Offset(width / 2f, height - 40f)) }
        settle()
        assertFalse(app.signOutAsked)
        assertNotNull(app.account)
        assertEquals(0, app.signedOut)
        // The press went to the glass and no further: the settings are still up.
        compose.onNodeWithTag("settings.signout").assertExists()
    }

    @Test
    fun backIsCancelAndLeavesTheSettingsOpen() {
        val app = fakeApp()
        askFromTheSettings(app)
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        settle()
        assertFalse(app.signOutAsked)
        assertNotNull(app.account)
        assertEquals(0, app.signedOut)
        compose.onNodeWithTag("settings.signout").assertExists()
    }

    @Test
    fun theCardGrowsInAndLeavesRatherThanVanishing() {
        val app = fakeApp()
        LobbyDebug.open = "settings"
        LobbyDebug.section = "account"
        compose.setContent { Shown(app) }
        settle()
        compose.onNodeWithTag("settings.signout").performScrollTo()
        settle()
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { app.signOutAsked = true }
        // Its first frames: the state is taken up a frame or two after it is set.
        var frames = 0
        while (compose.onAllNodesWithTag("signout.prompt").fetchSemanticsNodes().isEmpty() && frames++ < 10) compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val early = compose.onNodeWithTag("signout.prompt").getBoundsInRoot().width.value
        compose.mainClock.advanceTimeBy(1000)
        val settled = compose.onNodeWithTag("signout.prompt").getBoundsInRoot().width.value
        assertTrue("not grown from 96 per cent: $early of $settled", early < settled * 0.995f && early > settled * 0.955f)
        compose.runOnUiThread { app.signOutAsked = false }
        // A frame at a time: one long step lets it be taken away within it
        // whatever its departure, and passes either way.
        val there = (1..25).map {
            compose.mainClock.advanceTimeBy(16)
            compose.onAllNodesWithTag("signout.prompt").fetchSemanticsNodes().isNotEmpty()
        }
        assertTrue("gone before its departure had played: $there", there.take(8).all { it })
        assertFalse("still there long after its departure: $there", there.last())
    }

    /** The question over the settings, for looking at beside the Swift's. */
    @Test
    fun picture() {
        askFromTheSettings(fakeApp())
        captureScreenRoboImage("src/test/screenshots/navigation/signout-prompt-412-en.png")
    }
}
