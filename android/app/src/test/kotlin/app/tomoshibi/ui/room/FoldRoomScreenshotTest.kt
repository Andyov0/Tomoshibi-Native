package app.tomoshibi.ui.room

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.Hinge
import app.tomoshibi.ui.InMeeting
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.LocalCallCameras
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
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_protocol.MediaSource
import java.util.Locale
import java.util.TimeZone

/**
 * The call on a Galaxy Z Fold6, as RoomView lays it out for each way the
 * phone is held: the cover screen (372 by 800 points), the inner screen flat
 * (840 by 980) and turned (980 by 840), standing on a table with the hinge
 * across the turned inner screen, and held open as a book with the hinge
 * down the upright one. Robolectric folds nothing, so the hinge is provided
 * around the room (LocalHinge) where the window would report it. In English
 * and in simplified Chinese, into src/test/screenshots/fold/room, to be read
 * for anything lying across the hinge.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = COVER)
@OptIn(ExperimentalRoborazziApi::class)
class FoldRoomScreenshotTest(private val language: Language, private val held: Held) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}-{1}")
        fun cases() = listOf(Language.En, Language.ZhHans).flatMap { language -> Held.entries.map { arrayOf<Any>(language, it) } }

        const val DIRECTORY = "src/test/screenshots/fold/room"
    }

    @get:Rule
    val compose = createComposeRule()

    private var hinge by mutableStateOf<Hinge?>(null)

    @Before
    fun setUp() {
        RuntimeEnvironment.setQualifiers(held.qualifiers)
        hinge = held.hinge
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

    private fun shoot(name: String, call: CallModel, before: () -> Unit = {}) {
        val app = app()
        compose.setContent {
            InMeeting { CompositionLocalProvider(LocalRoomPictures provides standInPictures, LocalHinge provides hinge, LocalCallCameras provides FakeCameras) { RoomView(app, call) } }
        }
        compose.mainClock.advanceTimeBy(600)
        compose.waitForIdle()
        before()
        compose.mainClock.advanceTimeBy(2500)
        compose.waitForIdle()
        captureScreenRoboImage("$DIRECTORY/$name-${held.tag}-${language.code}.png")
    }

    private fun room(people: Int, deployment: FakeDeployment = FakeDeployment(), camerasOff: Set<Int> = emptySet()): CallModel {
        val call = callModel(deployment = deployment)
        PEOPLE.take(people).forEachIndexed { index, (id, name) -> call.joins(id, name, camera = index !in camerasOff, microphone = index % 3 != 2) }
        call.settled = true
        return call
    }

    private fun chatted(call: CallModel): CallModel {
        call.apply(CallEvent.Chat("1", "p-mei", "Mei Lin", "Morning! The deck is at https://example.com/deck", 1_760_000_000_000))
        call.apply(CallEvent.Chat("2", "p-mei", "Mei Lin", "Slide 4 is the new one.", 1_760_000_030_000))
        call.apply(CallEvent.Chat("3", "p-kenji", "Kenji Sato", "Looks good to me", 1_760_000_090_000))
        return call
    }

    @Test fun alone() = shoot("alone", room(0))

    @Test fun two() = shoot("two", room(1))

    @Test fun four() = shoot("four", room(3, camerasOff = setOf(1)))

    @Test fun nine() = shoot("nine", room(8, camerasOff = setOf(2, 5)))

    @Test fun sixteen() = shoot("sixteen", room(15, camerasOff = setOf(4, 9)))

    @Test
    fun share() {
        val call = room(3)
        call.apply(CallEvent.TrackPublished("p-mei", track("TR_p-mei_screen", MediaSource.SCREEN_SHARE)))
        shoot("share", call)
    }

    @Test
    fun shareWithMessages() {
        val call = chatted(room(3))
        call.apply(CallEvent.TrackPublished("p-mei", track("TR_p-mei_screen", MediaSource.SCREEN_SHARE)))
        RoomState.of(call).panel = RoomPanel.Messages
        shoot("share-messages", call)
    }

    @Test
    fun messages() {
        val call = chatted(room(2))
        RoomState.of(call).panel = RoomPanel.Messages
        shoot("messages", call)
    }

    @Test
    fun host() {
        val call = room(3, FakeDeployment(hosting = true))
        RoomState.of(call).panel = RoomPanel.Host
        shoot("host", call)
    }

    @Test
    fun sound() {
        val call = room(3)
        RoomState.of(call).panel = RoomPanel.Sound
        shoot("sound", call)
    }

    @Test
    fun devices() {
        val call = room(3)
        RoomState.of(call).panel = RoomPanel.Devices
        shoot("devices", call)
    }

    @Test
    fun watch() {
        val call = room(3)
        RoomState.of(call).panel = RoomPanel.Watch
        shoot("watch", call)
    }

    @Test
    fun music() {
        val call = room(3)
        RoomState.of(call).panel = RoomPanel.Music
        shoot("music", call)
    }

    @Test
    fun more() {
        val call = room(3, FakeDeployment(hosting = true))
        shoot("more", call) { RoomState.of(call).popovers.show(MORE) }
    }

    @Test
    fun leave() {
        val call = room(3, FakeDeployment(hosting = true))
        shoot("leave", call) { RoomState.of(call).popovers.show(LEAVE) }
    }

    @Test
    fun shareMenu() {
        val call = room(3)
        shoot("share-menu", call) { RoomState.of(call).popovers.show(SHARE) }
    }

    @Test
    fun reactions() {
        val call = room(3)
        shoot("reactions", call) { RoomState.of(call).popovers.show(app.tomoshibi.ui.call.TRAY) }
    }

    @Test
    fun toasts() {
        val call = room(3)
        call.announce(CallToast(t("{name} joined", mapOf("name" to "Ola Nordmann")), lasts = null))
        call.announce(CallToast(t("{name} started sharing", mapOf("name" to "Mei Lin")), kind = CallToast.Kind.Signal, lasts = null))
        call.apply(CallEvent.Chat("1", "p-mei", "Mei Lin", "Can everybody see the slides?", 1_760_000_000_000))
        shoot("toasts", call)
    }

    @Test
    fun side() {
        Appearance.shared.choose(ControlsPlacement.Side)
        shoot("side", room(3))
    }

    /**
     * The phone stood up on the table in the middle of a call, and laid flat
     * again: half way through the arrival curve the pictures, the panel and
     * the bar are on their way rather than already there, and nothing was
     * drawn again (the same call, the same draft).
     */
    @Test
    fun standingUp() {
        if (held != Held.Table) return
        val call = chatted(room(3))
        call.apply(CallEvent.TrackPublished("p-mei", track("TR_p-mei_screen", MediaSource.SCREEN_SHARE)))
        RoomState.of(call).panel = RoomPanel.Messages
        compose.runOnUiThread { hinge = null }
        Snapshot.sendApplyNotifications()
        compose.mainClock.advanceTimeByFrame()
        val app = app()
        compose.setContent {
            InMeeting { CompositionLocalProvider(LocalRoomPictures provides standInPictures, LocalHinge provides hinge, LocalCallCameras provides FakeCameras) { RoomView(app, call) } }
        }
        compose.mainClock.advanceTimeBy(3000)
        compose.waitForIdle()
        captureScreenRoboImage("$DIRECTORY/standing-0-flat-${language.code}.png")
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { hinge = TABLE_HINGE }
        Snapshot.sendApplyNotifications()
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(130)
        captureScreenRoboImage("$DIRECTORY/standing-1-moving-${language.code}.png")
        compose.mainClock.advanceTimeBy(600)
        captureScreenRoboImage("$DIRECTORY/standing-2-table-${language.code}.png")
        compose.runOnUiThread { hinge = null }
        Snapshot.sendApplyNotifications()
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(130)
        captureScreenRoboImage("$DIRECTORY/standing-3-laying-${language.code}.png")
        compose.mainClock.autoAdvance = true
        compose.mainClock.advanceTimeBy(600)
        compose.waitForIdle()
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
}

/** The ways a Fold6 is held, with the screen each has and where its hinge is, in pixels at a point and a half to the point. */
enum class Held(val tag: String, val qualifiers: String, val hinge: Hinge?) {
    Cover("cover", COVER, null),
    Inner("inner", INNER, null),
    Turned("turned", TURNED, null),
    Table("table", TURNED, TABLE_HINGE),
    Book("book", INNER, BOOK_HINGE),
}

/** The cover screen: 968 by 2376 pixels, about 372 by 800 points once the bars are off. */
const val COVER = "w372dp-h800dp-hdpi"
/** The inner screen upright: 1856 by 2160 pixels, about 840 by 980 points. */
const val INNER = "w840dp-h980dp-hdpi"
/** The inner screen turned on its side, which is how it stands on a table. */
const val TURNED = "w980dp-h840dp-land-hdpi"
/** The Fold's hinge across the turned inner screen, half way down; a Fold reports it as a line, no thicker than nothing. */
val TABLE_HINGE = Hinge(left = 0f, top = 630f, right = 1470f, bottom = 630f, vertical = false)
/** The same hinge down the upright inner screen, held as a book. */
val BOOK_HINGE = Hinge(left = 630f, top = 0f, right = 630f, bottom = 1470f, vertical = true)
