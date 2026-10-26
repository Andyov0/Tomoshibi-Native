package app.tomoshibi.ui.settings

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Accent
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ConnectionBadge
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.InAppearance
import app.tomoshibi.ui.InMeeting
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.glassSource
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.Locale
import java.util.TimeZone

/**
 * Pictures of every page of the settings and of the devices panel, at a
 * phone's width (412 points, the iPhone's list and pages) and at a tablet's
 * (840, the sidebar beside the page, as on the Mac), in English and in
 * simplified Chinese, drawn with a model that has somebody signed in and no
 * core behind it. Recorded with `./gradlew :app:recordRoborazziDebug` into
 * src/test/screenshots/settings, to be set beside what the Swift draws.
 *
 * The windows are taller than a phone's so that a whole page is in one
 * picture; the pages scroll on a device.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h2000dp-hdpi")
class SettingsScreenshotTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf<Any>(Language.En), arrayOf<Any>(Language.ZhHans))

        const val DIRECTORY = "src/test/screenshots/settings"
    }

    @get:Rule
    val compose = createComposeRule()

    private val platform = FakePlatform()

    @Before
    fun setUp() {
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        // Not inside `apply`, where `language` would be the Localization's own.
        val localization = Localization.install(context)
        localization.keeps = false
        localization.choose(language)
        Appearance.install(context).apply {
            keeps = false
            choose(Accent.Amber)
            choose(ConnectionBadge.Signal)
            choose(ControlsPlacement.Always)
        }
    }

    private fun file(name: String) = "$DIRECTORY/$name-${language.code}.png"

    private fun shoot(name: String, opening: SettingsTopic?, app: AppModel = fakeApp()) {
        compose.setContent {
            CompositionLocalProvider(LocalSettingsPlatform provides platform) {
                InAppearance { SettingsView(app, {}, Modifier, opening) }
            }
        }
        settle()
        compose.onRoot().captureRoboImage(file(name))
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
    }

    @Test fun home() = shoot("phone-home", null)
    @Test fun homeSignedOut() = shoot("phone-home-signed-out", null, fakeApp(account = null))
    @Test fun general() = shoot("phone-general", SettingsTopic.General)
    @Test fun media() = shoot("phone-media", SettingsTopic.Media)
    /** Devices picked: the footer and the way to forget them arrive, and each choice says its device. */
    @Test
    fun mediaPicked() {
        val app = fakeApp()
        app.preferences.choose("audioinput", "31")
        app.preferences.choose("videoinput", "0")
        shoot("phone-media-picked", SettingsTopic.Media, app)
    }

    @Test fun watching() = shoot("phone-watching", SettingsTopic.Watching)
    @Test fun music() = shoot("phone-music", SettingsTopic.Music)
    @Test fun account() = shoot("phone-account", SettingsTopic.Account)
    @Test fun accountSignedOut() = shoot("phone-account-signed-out", SettingsTopic.Account, fakeApp(account = null))
    @Test fun about() = shoot("phone-about", SettingsTopic.About)

    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideGeneral() = shoot("wide-general", null)
    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideMedia() = shoot("wide-media", SettingsTopic.Media)
    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideWatching() = shoot("wide-watching", SettingsTopic.Watching)
    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideMusic() = shoot("wide-music", SettingsTopic.Music)
    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideAccount() = shoot("wide-account", SettingsTopic.Account)
    @Test @Config(qualifiers = "w840dp-h1500dp-hdpi") fun wideAbout() = shoot("wide-about", SettingsTopic.About)

    /** The panel as the call room hangs it: beside the call's corner, as wide as RoomView.swift's `panelWidth`. */
    private fun panel(name: String, width: Dp) {
        val app = fakeApp()
        val call = fakeCall()
        compose.setContent {
            // Opened from inside a call, and drawn as the call is.
            CompositionLocalProvider(LocalSettingsPlatform provides platform, LocalCallCameras provides FakeCameras) { InMeeting {
                GlassBackdrop {
                    Box(Modifier.fillMaxSize()) {
                        Box(Modifier.matchParentSize().background(Theme.background))
                        Backdrop(Modifier.matchParentSize().glassSource())
                        Box(
                            Modifier
                                .align(Alignment.TopEnd)
                                .padding(12.dp)
                                .width(width)
                                .verticalScroll(rememberScrollState()),
                        ) { DevicesPanel(app, call, {}) }
                    }
                }
            } }
        }
        settle()
        compose.onRoot().captureRoboImage(file(name))
    }

    @Test @Config(qualifiers = "w412dp-h1400dp-hdpi") fun devicesPanel() = panel("phone-devices", 388.dp)
    @Test @Config(qualifiers = "w840dp-h1400dp-hdpi") fun wideDevicesPanel() = panel("wide-devices", 360.dp)
}
