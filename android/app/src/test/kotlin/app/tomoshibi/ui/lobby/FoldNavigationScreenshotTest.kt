package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.media.CallAudio
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.room.COVER
import app.tomoshibi.ui.room.Held
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalCallCameras
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureScreenRoboImage
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.tomoshibi_protocol.Arrangement
import java.time.Instant
import java.util.Locale
import java.util.TimeZone

/**
 * Every screen on the way into a call and out of it, and every sheet the
 * lobby opens, on a Galaxy Z Fold6 held each way it is held (Held): the
 * cover screen, the inner screen flat and turned, standing on a table and
 * held as a book. In English and in simplified Chinese, into
 * src/test/screenshots/fold/navigation, to be read for anything lying
 * across the hinge and anything cut off on the cover screen.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = COVER)
@OptIn(ExperimentalRoborazziApi::class)
class FoldNavigationScreenshotTest(private val language: Language, private val held: Held) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}-{1}")
        fun cases() = listOf(Language.En, Language.ZhHans).flatMap { language -> Held.entries.map { arrayOf<Any>(language, it) } }

        const val DIRECTORY = "src/test/screenshots/fold/navigation"
    }

    @get:Rule
    val compose = createComposeRule()

    private var audio: CallAudio? = null

    @Before
    fun setUp() {
        RuntimeEnvironment.setQualifiers(held.qualifiers)
        standInRules()
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
        LobbyDebug.open = null
        LobbyDebug.room = null
        LobbyDebug.popover = null
        LobbyDebug.section = null
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.room = null
        LobbyDebug.popover = null
        LobbyDebug.section = null
        audio?.stop()
    }

    private fun shoot(name: String, app: AppModel) {
        compose.setContent {
            // The settings' pages ask the platform for what the core says
            // (a room name's rules, its version), which a computer's JVM
            // cannot load: the settings' own stand-in answers instead.
            CompositionLocalProvider(LocalHinge provides held.hinge, LocalSettingsPlatform provides FakePlatform(), LocalCallCameras provides FakeCameras) {
                TestApp(app)
            }
        }
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        captureScreenRoboImage("$DIRECTORY/$name-${held.tag}-${language.code}.png")
    }

    private fun opening(part: String, popover: String? = null, section: String? = null): AppModel {
        LobbyDebug.open = part
        LobbyDebug.popover = popover
        LobbyDebug.section = section
        return fakeApp()
    }

    @Test fun lobby() = shoot("lobby", fakeApp())

    @Test fun lobbyEmpty() = shoot("lobby-empty", fakeApp(withRelays = false, withRecents = false, withMeetings = false))

    @Test
    fun lobbyProblem() {
        val app = fakeApp()
        app.tell(t("That invitation is no longer good. Ask for another."))
        shoot("lobby-problem", app)
    }

    @Test fun startSheet() = shoot("sheet-start", opening("start"))

    @Test fun joinSheet() = shoot("sheet-join", opening("join"))

    @Test fun startSheetServers() = shoot("sheet-start-servers", opening("start", popover = "sheet.server"))

    @Test fun schedule() = shoot("schedule", opening("scheduled"))

    @Test fun servers() = shoot("servers", opening("servers"))

    @Test fun settings() = shoot("settings", opening("settings"))

    @Test fun settingsAccount() = shoot("settings-account", opening("settings", section = "account"))

    @Test fun settingsMedia() = shoot("settings-media", opening("settings", section = "media"))

    @Test fun settingsGeneral() = shoot("settings-general", opening("settings", section = "general"))

    @Test fun accountMenu() = shoot("account-menu", opening("", popover = "account"))

    @Test fun starting() = shoot("starting", fakeApp(screen = Screen.Starting))

    @Test fun unreachable() = shoot("unreachable", fakeApp(signedIn = false, screen = Screen.Unreachable))

    @Test fun signIn() = shoot("sign-in", fakeApp(signedIn = false))

    @Test
    fun languageMenu() {
        LobbyDebug.popover = "language"
        shoot("sign-in-language", fakeApp(signedIn = false))
    }

    @Test
    fun prejoin() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        shoot("prejoin", app)
    }

    @Test
    fun prejoinInvited() {
        val app = fakeApp(signedIn = false, screen = Screen.Prejoin)
        app.stage("invitation", invitation(name = "Grace"))
        app.room = "design-review"
        shoot("prejoin-invited", app)
    }

    @Test
    fun knocking() {
        val app = fakeApp(screen = Screen.Knocking)
        app.room = "design-review"
        shoot("knocking", app)
    }

    @Test
    fun waiting() {
        val app = fakeApp(screen = Screen.Waiting)
        val soon = Arrangement("m9", "design-review", Instant.now().plusSeconds(3 * 3600).toString(), null, false, false, null, null, false, null)
        app.stage("arrangement", soon)
        app.room = "design-review"
        shoot("waiting", app)
    }

    @Test
    fun moving() {
        val app = fakeApp(screen = Screen.Call)
        app.stage("movingTo", "Tokyo")
        shoot("moving", app)
    }

    @Test
    fun ended() {
        val app = fakeApp(screen = Screen.Ended)
        app.room = "design-review"
        shoot("ended", app)
    }
}
