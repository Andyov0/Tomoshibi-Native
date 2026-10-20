package app.tomoshibi.ui.room

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
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
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_protocol.Knocker
import java.util.Locale
import java.util.TimeZone

/**
 * Pictures of the call as the room draws it, with fake people in it: alone,
 * two, four, nine and sixteen; a share on the stage; the messages, the host's
 * panel, the narrow bar's More, the question of leaving and the corner's
 * notices; the controls in each of their three places; a phone held upright
 * (412 points) and on its side, and an unfolded phone or a tablet (840
 * points). In English and in simplified Chinese. Recorded with
 * `./gradlew :app:recordRoborazziDebug` into src/test/screenshots/room, and
 * read beside what the Swift draws.
 *
 * Whole-screen pictures, because the bar's popovers are windows of their own.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = PHONE)
@OptIn(ExperimentalRoborazziApi::class)
class RoomScreenshotTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf<Any>(Language.En), arrayOf<Any>(Language.ZhHans))

        const val DIRECTORY = "src/test/screenshots/room"
    }

    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() {
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
        Appearance.shared.choose(ControlsPlacement.Always)
        standInRules()
    }

    @After
    fun tearDown() {
        Appearance.shared.choose(ControlsPlacement.Always)
    }

    private fun file(name: String) = "$DIRECTORY/$name-${language.code}.png"

    private fun shoot(name: String, call: CallModel, settle: Long = 2500, before: () -> Unit = {}) {
        val app = app()
        compose.setContent {
            CompositionLocalProvider(LocalRoomPictures provides standInPictures) { RoomView(app, call) }
        }
        compose.mainClock.advanceTimeBy(600)
        compose.waitForIdle()
        before()
        compose.mainClock.advanceTimeBy(settle)
        compose.waitForIdle()
        captureScreenRoboImage(file(name))
    }

    private fun room(people: Int, deployment: FakeDeployment = FakeDeployment(), camerasOff: Set<Int> = emptySet()): CallModel {
        val call = callModel(deployment = deployment)
        PEOPLE.take(people).forEachIndexed { index, (id, name) -> call.joins(id, name, camera = index !in camerasOff, microphone = index % 3 != 2) }
        call.settled = true
        return call
    }

    @Test
    fun alone() = shoot("alone", room(0))

    @Test
    fun two() = shoot("two", room(1))

    @Test
    fun four() {
        val call = room(3, camerasOff = setOf(1))
        // Somebody talking, somebody with a hand up.
        call.apply(CallEvent.Speaking(listOf("p-mei")))
        call.apply(CallEvent.AttributesChanged("p-kenji", mapOf("hand" to "up")))
        shoot("four", call)
    }

    /**
     * Every camera off and one person talking: the tally's edge round the
     * tile and the name in it are all that say who it is, now that the
     * tiles are flat grey with nothing glowing in them.
     */
    @Test
    fun camerasOffSomebodySpeaking() {
        val call = room(3, camerasOff = setOf(0, 1, 2))
        call.apply(CallEvent.Speaking(listOf("p-mei")))
        shoot("cameras-off-speaking", call)
    }

    @Test
    @Config(qualifiers = WIDE)
    fun wideCamerasOffSomebodySpeaking() {
        val call = room(3, camerasOff = setOf(0, 1, 2))
        call.apply(CallEvent.Speaking(listOf("p-mei")))
        shoot("wide-cameras-off-speaking", call)
    }

    @Test
    fun nine() = shoot("nine", room(8, camerasOff = setOf(2, 5)))

    @Test
    fun sixteen() = shoot("sixteen", room(15, camerasOff = setOf(4, 9)))

    @Test
    fun shareOnTheStage() {
        val call = room(3)
        call.apply(CallEvent.TrackPublished("p-mei", track("TR_p-mei_screen", uniffi.tomoshibi_protocol.MediaSource.SCREEN_SHARE)))
        shoot("share", call)
    }

    @Test
    fun messagesOpen() {
        val call = room(2)
        call.apply(CallEvent.Chat("1", "p-mei", "Mei Lin", "Morning! The deck is at https://example.com/deck", 1_760_000_000_000))
        call.apply(CallEvent.Chat("2", "p-mei", "Mei Lin", "Slide 4 is the new one.", 1_760_000_030_000))
        call.apply(CallEvent.Chat("3", "p-kenji", "Kenji Sato", "Looks good to me", 1_760_000_090_000))
        RoomState.of(call).panel = RoomPanel.Messages
        shoot("messages", call)
    }

    /** The messages before anybody has written: the one place the empty state's plate is seen. */
    @Test
    fun messagesEmpty() {
        val call = room(2)
        RoomState.of(call).panel = RoomPanel.Messages
        shoot("messages-empty", call)
    }

    @Test
    fun hostPanelOpen() {
        val deployment = FakeDeployment(hosting = true, knockers = listOf(Knocker("k1", "Ada", "203.0.113.24", "")))
        val call = room(3, deployment)
        RoomState.of(call).panel = RoomPanel.Host
        shoot("host", call)
    }

    @Test
    fun somebodyAtTheDoor() {
        val deployment = FakeDeployment(hosting = true, knockers = listOf(Knocker("k1", "Ada", "203.0.113.24", "")))
        shoot("door", room(3, deployment))
    }

    @Test
    fun moreOpen() {
        val call = room(3, FakeDeployment(hosting = true))
        shoot("more", call) { RoomState.of(call).popovers.show(MORE) }
    }

    @Test
    fun leaving() {
        val call = room(3, FakeDeployment(hosting = true))
        shoot("leave", call) { RoomState.of(call).popovers.show(LEAVE) }
    }

    @Test
    fun sharing() {
        val call = room(3)
        shoot("share-menu", call) { RoomState.of(call).popovers.show(SHARE) }
    }

    @Test
    fun reactions() {
        val call = room(3)
        shoot("reactions", call) { RoomState.of(call).popovers.show(app.tomoshibi.ui.call.TRAY) }
    }

    @Test
    fun pictureMenu() {
        val call = room(3)
        val state = RoomState.of(call)
        shoot("picture-menu", call) {
            // A long press on Mei's picture, a little right of its middle.
            state.menu = MenuAt("p-mei/camera", Offset(700f, 650f))
            state.popovers.show(MENU)
        }
    }

    @Test
    fun toasts() {
        val call = room(3)
        call.announce(CallToast(t("{name} joined", mapOf("name" to "Ola Nordmann")), lasts = null))
        call.announce(CallToast(t("Can't use your camera"), detail = t("Something else may be using it."), kind = CallToast.Kind.Failure,
            lasts = null, closable = true))
        call.announce(CallToast(t("{name} started sharing", mapOf("name" to "Mei Lin")), kind = CallToast.Kind.Signal, lasts = null))
        shoot("toasts", call)
    }

    @Test
    fun soundOpen() {
        val call = room(3)
        RoomState.of(call).panel = RoomPanel.Sound
        shoot("sound", call)
    }

    @Test
    fun controlsHideWhenNothingHappens() {
        Appearance.shared.choose(ControlsPlacement.Idle)
        shoot("idle", room(3), settle = 5000)
    }

    @Test
    fun controlsAtTheSide() {
        Appearance.shared.choose(ControlsPlacement.Side)
        shoot("side", room(3))
    }

    @Test
    fun filled() {
        val call = room(1)
        val state = RoomState.of(call)
        shoot("filled", call) {
            state.pin = state.pin.pin("p-mei/camera")
            state.filled = true
        }
    }

    @Test
    @Config(qualifiers = LANDSCAPE)
    fun landscape() = shoot("landscape", room(3))

    @Test
    @Config(qualifiers = LANDSCAPE)
    fun landscapeAtTheSide() {
        // Not tall enough for a column: the bar stays at the foot.
        Appearance.shared.choose(ControlsPlacement.Side)
        shoot("landscape-side", room(3))
    }

    @Test
    @Config(qualifiers = WIDE)
    fun wide() = shoot("wide", room(5))

    @Test
    @Config(qualifiers = WIDE)
    fun wideWithMessages() {
        val call = room(3)
        call.apply(CallEvent.Chat("1", "p-mei", "Mei Lin", "Can everybody see the slides?", 1_760_000_000_000))
        RoomState.of(call).panel = RoomPanel.Messages
        shoot("wide-messages", call)
    }

    @Test
    @Config(qualifiers = WIDE)
    fun wideShare() {
        val call = room(4)
        call.apply(CallEvent.TrackPublished("p-kenji", track("TR_p-kenji_screen", uniffi.tomoshibi_protocol.MediaSource.SCREEN_SHARE)))
        shoot("wide-share", call)
    }

    /**
     * A share that is not on the stage, which is drawn as its card
     * (ShareCardFace) in the strip: somebody's camera is pinned instead.
     */
    @Test
    @Config(qualifiers = WIDE)
    fun wideShareOffTheStage() {
        val call = room(4)
        call.apply(CallEvent.TrackPublished("p-kenji", track("TR_p-kenji_screen", uniffi.tomoshibi_protocol.MediaSource.SCREEN_SHARE)))
        val state = RoomState.of(call)
        shoot("wide-share-card", call) { state.pin = state.pin.pin("p-mei/camera") }
    }

    @Test
    @Config(qualifiers = WIDE)
    fun wideAtTheSide() {
        Appearance.shared.choose(ControlsPlacement.Side)
        val call = room(5, FakeDeployment(hosting = true))
        RoomState.of(call).panel = RoomPanel.Host
        shoot("wide-side", call)
    }
}

/** A phone held upright, at a pixel and a half to the point, which keeps the pictures small enough to keep. */
const val PHONE = "w412dp-h892dp-hdpi"
const val LANDSCAPE = "w892dp-h412dp-land-hdpi"
/** An unfolded phone, or a small tablet. */
const val WIDE = "w840dp-h1000dp-hdpi"
