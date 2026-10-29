package app.tomoshibi.ui.lobby

import app.tomoshibi.model.host.standInRules
import android.content.Context
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.media.CallAudio
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.ui.Hinge
import app.tomoshibi.ui.InAppearance
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.TomoshibiApp
import app.tomoshibi.ui.kit.GlassBackdrop
import androidx.compose.runtime.CompositionLocalProvider
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureScreenRoboImage
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.tomoshibi_protocol.Arrangement
import java.time.Instant
import java.util.Locale
import java.util.TimeZone

/**
 * Pictures of every screen on the way into a call and out of it, and of
 * every sheet the lobby opens, at a phone's width (412 points) and at an
 * unfolded Fold's (840), in English and in simplified Chinese, drawn from a
 * model set as a deployment's replies would set it. Recorded with
 * `./gradlew :app:recordRoborazziDebug` into src/test/screenshots/navigation,
 * to be looked at beside the iPhone's and the Mac's.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = PHONE)
@OptIn(ExperimentalRoborazziApi::class)
class NavigationScreenshotTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf<Any>(Language.En), arrayOf<Any>(Language.ZhHans))

        const val DIRECTORY = "src/test/screenshots/navigation"
    }

    @get:Rule
    val compose = createComposeRule()

    private var audio: CallAudio? = null

    @Before
    fun setUp() {
        standInRules()
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
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
        LobbyDebug.room = null
        LobbyDebug.popover = null
        audio?.stop()
    }

    private fun shoot(name: String, app: AppModel, wide: Boolean = false, before: () -> Unit = {}) {
        compose.setContent { TestApp(app) }
        settle()
        before()
        settle()
        captureScreenRoboImage("$DIRECTORY/$name-${if (wide) "840" else "412"}-${language.code}.png")
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
    }

    // The lobby and what it opens.

    @Test fun lobby() = shoot("lobby", fakeApp())

    @Test @Config(qualifiers = WIDE)
    fun lobbyWide() = shoot("lobby", fakeApp(), wide = true)

    @Test fun lobbyEmpty() = shoot("lobby-empty", fakeApp(withRelays = false, withRecents = false, withMeetings = false))

    @Test
    fun lobbyRecentSwiped() = shoot("lobby-swiped", fakeApp()) {
        compose.onNodeWithText("friday-standup").performTouchInput { swipeLeft(startX = right - 10f, endX = right - 260f, durationMillis = 400) }
    }

    @Test
    fun lobbyProblem() {
        val app = fakeApp()
        app.tell(t("That invitation is no longer good. Ask for another."))
        shoot("lobby-problem", app)
    }

    @Test
    fun startSheet() {
        LobbyDebug.open = "start"
        shoot("sheet-start", fakeApp())
    }

    @Test @Config(qualifiers = WIDE)
    fun startSheetWide() {
        LobbyDebug.open = "start"
        shoot("sheet-start", fakeApp(), wide = true)
    }

    @Test
    fun joinSheet() {
        LobbyDebug.open = "join"
        shoot("sheet-join", fakeApp())
    }

    @Test
    fun startSheetServers() {
        LobbyDebug.open = "start"
        LobbyDebug.popover = "sheet.server"
        shoot("sheet-start-servers", fakeApp())
    }

    @Test
    fun schedule() {
        LobbyDebug.open = "scheduled"
        shoot("schedule", fakeApp())
    }

    @Test @Config(qualifiers = WIDE)
    fun scheduleWide() {
        LobbyDebug.open = "scheduled"
        shoot("schedule", fakeApp(), wide = true)
    }

    @Test
    fun scheduleEmpty() {
        LobbyDebug.open = "scheduled"
        shoot("schedule-empty", fakeApp(withMeetings = false))
    }

    @Test
    fun servers() {
        LobbyDebug.open = "servers"
        shoot("servers", fakeApp())
    }

    @Test @Config(qualifiers = WIDE)
    fun serversWide() {
        LobbyDebug.open = "servers"
        shoot("servers", fakeApp(), wide = true)
    }

    @Test
    fun settings() {
        LobbyDebug.open = "settings"
        shoot("settings", fakeApp())
    }

    @Test @Config(qualifiers = WIDE)
    fun settingsWide() {
        LobbyDebug.open = "settings"
        shoot("settings", fakeApp(), wide = true)
    }

    @Test @Config(qualifiers = WIDE)
    fun accountMenuWide() {
        LobbyDebug.popover = "account"
        shoot("account-menu", fakeApp(), wide = true)
    }

    /**
     * A Fold held open as a book, its hinge down the middle: the sidebar
     * keeps to the left half, the rest begins after the hinge, and a sheet
     * opens in the right half rather than across the fold.
     */
    @Test @Config(qualifiers = WIDE)
    fun lobbyOverAHinge() {
        LobbyDebug.open = "start"
        val app = fakeApp()
        compose.setContent {
            CompositionLocalProvider(LocalHinge provides BOOK) {
                InAppearance { GlassBackdrop { LobbyView(app, ada) } }
            }
        }
        settle()
        captureScreenRoboImage("$DIRECTORY/lobby-hinge-840-${language.code}.png")
    }

    // Before the lobby.

    @Test fun starting() = shoot("starting", fakeApp(screen = Screen.Starting))

    @Test fun unreachable() = shoot("unreachable", fakeApp(signedIn = false, screen = Screen.Unreachable))

    @Test fun signIn() = shoot("sign-in", fakeApp(signedIn = false))

    @Test @Config(qualifiers = WIDE)
    fun signInWide() = shoot("sign-in", fakeApp(signedIn = false), wide = true)

    @Test
    fun languageMenu() {
        LobbyDebug.popover = "language"
        shoot("sign-in-language", fakeApp(signedIn = false))
    }

    // On the way in.

    @Test
    fun prejoin() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        shoot("prejoin", app)
    }

    /** The microphone and the camera both off on the lobby's card: the last moment to notice before a call. */
    @Test
    fun lobbyDevicesOff() {
        val app = fakeApp()
        app.startWithMicrophone = false
        app.startWithCamera = false
        shoot("lobby-devices-off", app)
    }

    @Test
    fun prejoinDevicesOff() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        app.startWithMicrophone = false
        app.startWithCamera = false
        shoot("prejoin-devices-off", app)
    }

    @Test @Config(qualifiers = WIDE)
    fun prejoinWide() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        shoot("prejoin", app, wide = true)
    }

    @Test
    fun prejoinInvited() {
        val app = fakeApp(signedIn = false, screen = Screen.Prejoin)
        app.stage("invitation", invitation(name = "Grace"))
        app.room = "design-review"
        shoot("prejoin-invited", app)
    }

    /**
     * Signed in and come by an invitation's link: the server picker is on
     * the join screen, as it is for somebody signed out, since neither saw
     * the lobby's.
     */
    @Test
    fun prejoinInvitedSignedIn() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.stage("invitation", invitation(name = null))
        app.stage("arrivedByLink", true)
        app.room = "design-review"
        shoot("prejoin-invited-signed-in", app)
    }

    /** The picker opened on the join screen: Automatic, the readings, the relay out of service greyed. */
    @Test
    fun prejoinServers() {
        LobbyDebug.popover = "prejoin.server"
        val app = fakeApp(signedIn = false, screen = Screen.Prejoin)
        app.stage("invitation", invitation(name = "Grace"))
        app.stage("arrivedByLink", true)
        app.room = "design-review"
        shoot("prejoin-servers", app)
    }

    @Test
    fun prejoinRefused() {
        val app = fakeApp(screen = Screen.Prejoin)
        app.room = "design-review"
        // The one refusal a knock answers, which offers it.
        app.tell(t("{room} is by invitation. Ask the organiser for a link.", mapOf("room" to "design-review")))
        shoot("prejoin-refused", app)
    }

    @Test
    fun knocking() {
        val app = fakeApp(screen = Screen.Knocking)
        app.room = "design-review"
        shoot("knocking", app)
    }

    // The waiting screen is only ever reached by a meeting's link, so these
    // stand where `open` leaves the model, with the server picker on it.
    private fun waitingApp(): AppModel {
        val app = fakeApp(screen = Screen.Waiting)
        val soon = Arrangement("m9", "design-review", Instant.now().plusSeconds(3 * 3600).toString(), null, false, false, null, null, false, null)
        app.stage("arrangement", soon)
        app.stage("arrivedByLink", true)
        app.room = "design-review"
        return app
    }

    @Test
    fun waiting() = shoot("waiting", waitingApp())

    @Test @Config(qualifiers = WIDE)
    fun waitingWide() = shoot("waiting", waitingApp(), wide = true)

    @Test
    fun waitingServers() {
        LobbyDebug.popover = "waiting.server"
        shoot("waiting-servers", waitingApp())
    }

    // In and out.

    @Test
    fun call() {
        val app = fakeApp(screen = Screen.Call)
        val made = silentAudio().also { audio = it }
        app.stage("call", fakeCall(made))
        shoot("call", app)
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

    @Test @Config(qualifiers = WIDE)
    fun endedWide() {
        val app = fakeApp(screen = Screen.Ended)
        app.room = "design-review"
        shoot("ended", app, wide = true)
    }

    @Test
    fun endedClosed() {
        val app = fakeApp(screen = Screen.Ended)
        app.stage("ending", AppModel.Ending.Closed)
        app.room = "design-review"
        shoot("ended-closed", app)
    }
}

/** An upright hinge a little left of the middle of 840 points at a point and a half to the point, as a book is held open. */
val BOOK = Hinge(left = 600f, top = 0f, right = 630f, bottom = 1350f, vertical = true)

const val PHONE = "w412dp-h892dp-hdpi"
const val WIDE = "w840dp-h900dp-hdpi"
