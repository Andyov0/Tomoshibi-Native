package app.tomoshibi.ui.lobby

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.media.CallAudio
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.TomoshibiApp
import app.tomoshibi.ui.room.LocalRoomPictures
import app.tomoshibi.ui.room.standInPictures
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The day mode as the person meets it: the screens follow the system's
 * appearance while the app runs, without the activity being made again
 * (it turns round for itself on a change of night mode), and the call stays
 * dark in either (Theme.MEETING_STAYS_DARK). Read off the drawn pixels: a
 * screen that kept a colour of its own, or a palette that was never handed
 * down, shows here and in no model test.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = PHONE)
class DaylightTest {
    @get:Rule
    val compose = createComposeRule()

    private var night by mutableStateOf(true)
    private var audio: CallAudio? = null

    @Before
    fun setUp() {
        standInRules()
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        audio?.stop()
    }

    /** The app, the system's night mode said by [night] as the activity's configuration would say it. */
    private fun show(app: AppModel) {
        compose.setContent {
            val base = LocalConfiguration.current
            val mode = if (night) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
            val configuration = remember(base, mode) {
                Configuration(base).apply { uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or mode }
            }
            CompositionLocalProvider(
                LocalConfiguration provides configuration,
                LocalSettingsPlatform provides FakePlatform(),
                LocalRoomPictures provides standInPictures,
            ) { TomoshibiApp(app) }
        }
        settle()
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
    }

    /** How light the screen is on average, 0 to 1, over a grid of its pixels. */
    private fun lightness(): Float {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        var sum = 0f
        var count = 0
        for (y in 0 until pixels.height step 12) for (x in 0 until pixels.width step 12) {
            sum += pixels[x, y].luminance()
            count++
        }
        return sum / count
    }

    @Test
    fun theSignInFollowsTheSystemAsItTurns() {
        show(fakeApp(signedIn = false))
        val dark = lightness()
        assertTrue("the sign-in in the dark should be dark, was $dark", dark < 0.15f)
        night = false
        settle()
        val light = lightness()
        assertTrue("the sign-in should have turned light with the system, was $light", light > 0.7f)
        night = true
        settle()
        val back = lightness()
        assertTrue("the sign-in should have turned dark again, was $back", back < 0.15f)
    }

    @Test
    fun theLobbyFollowsTheSystemAsItTurns() {
        show(fakeApp())
        assertTrue("the lobby in the dark should be dark", lightness() < 0.15f)
        night = false
        settle()
        val light = lightness()
        assertTrue("the lobby should have turned light with the system, was $light", light > 0.7f)
    }

    @Test
    fun theCallStaysDarkInTheDaylight() {
        assertTrue("the meeting is to stay dark (Theme.MEETING_STAYS_DARK)", Theme.MEETING_STAYS_DARK)
        val app = fakeApp(screen = Screen.Call)
        app.stage("call", fakeCall(silentAudio().also { audio = it }))
        night = false
        show(app)
        val shown = lightness()
        assertTrue("the call should stay dark in the daylight, was $shown", shown < 0.2f)
    }
}
