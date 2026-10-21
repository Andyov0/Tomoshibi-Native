package app.tomoshibi.ui.settings

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.height
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.media.CameraChoice
import app.tomoshibi.model.CallModel
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlin.coroutines.CoroutineContext

/*
 * The call's devices panel opening without a stutter. On a Fold the panel
 * stuttered through its slide because, in the frame it opened in, it asked
 * the camera service for every camera's description twice and the audio
 * service for every device, on the main thread, and again mid-slide when its
 * listeners were first answered; meanwhile each of its rows grew from
 * nothing, so the panel was measured, blurred and shadowed again at a new
 * size every frame. What these guard: nothing asked as the panel arrives,
 * and all of it asked off the main thread; no rows growing in it, and no
 * "No devices found" before anything was asked. The room's side (the lists
 * read ahead, the panel moving in whole) is PanelArrivalTest's.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], qualifiers = "w412dp-h892dp-hdpi")
class DevicesPanelOpeningTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context get() = ApplicationProvider.getApplicationContext()

    @Before
    fun inEnglish() {
        val localization = Localization.install(context)
        localization.keeps = false
        localization.choose(Language.En)
    }

    private fun open(call: CallModel, cameras: CallCameras) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalCallCameras provides cameras) { DevicesPanel(fakeApp(), call, {}) }
        }
    }

    /** The call's lists read as the room reads them ahead of the panel. */
    private fun readAhead(call: CallModel, cameras: CallCameras) = runBlocking { DeviceLists.of(call, cameras).refresh(context) }

    @Test
    fun aPanelOpenedOnListsAlreadyReadAsksNothingAsItArrives() {
        val cameras = CountedCameras()
        val call = fakeCall()
        readAhead(call, cameras)
        assertEquals(1, cameras.asked)
        open(call, cameras)
        compose.mainClock.advanceTimeByFrame()
        // Drawn from the first frame, from what was read.
        compose.onNodeWithTag("devices.camera.1").assertExists()
        compose.onNodeWithTag("devices.microphone.31").assertExists()
        // Through the slide (Theme.settle's 320 ms), nothing asked.
        repeat(19) { compose.mainClock.advanceTimeByFrame() }
        assertEquals("the system was asked while the panel arrived", 1, cameras.asked)
        // Once it has arrived, asked again, once, for a headset plugged in
        // since: the listeners' first answer and that read are one.
        compose.mainClock.advanceTimeBy(DeviceLists.SETTLING + DeviceLists.BURST + 300)
        assertEquals(2, cameras.asked)
        assertEquals("asked on the main thread", 0, cameras.askedElsewhere)
    }

    @Test
    fun theRowsListedAsItOpensComeWithThePanelRatherThanGrowingInIt() {
        val cameras = CountedCameras()
        val call = fakeCall()
        readAhead(call, cameras)
        open(call, cameras)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val early = compose.onNodeWithTag("devices.camera.0").getBoundsInRoot()
        compose.mainClock.advanceTimeBy(1500)
        val settled = compose.onNodeWithTag("devices.camera.0").getBoundsInRoot()
        assertTrue("no row to measure: ${settled.height}", settled.height.value > 30f)
        assertEquals("the row was still growing", settled.height.value, early.height.value, 0.5f)
    }

    @Test
    fun listsNotReadYetAreReadOffTheMainThreadWithoutSayingThereAreNone() {
        val cameras = CountedCameras().apply { holding = true }
        val call = fakeCall()
        open(call, cameras)
        repeat(6) { compose.mainClock.advanceTimeByFrame() }
        // Not known yet, which is not the same as none.
        compose.onAllNodesWithText("No devices found").assertCountEquals(0)
        compose.onNodeWithTag("devices.camera.1").assertDoesNotExist()
        assertEquals(0, cameras.asked)
        cameras.release()
        compose.mainClock.advanceTimeBy(1500)
        compose.onNodeWithTag("devices.camera.1").assertExists()
        compose.onNodeWithTag("devices.speakers.32").assertExists()
        compose.onAllNodesWithText("No devices found").assertCountEquals(0)
        assertEquals("asked on the main thread", 0, cameras.askedElsewhere)
    }
}

/**
 * The cameras as the camera service would give them, counting each time it
 * is asked and whether that was on the thread the lists are read on.
 * [holding] keeps reads waiting until [release], as a slow service would.
 */
internal class CountedCameras : CallCameras {
    var asked = 0
    var askedElsewhere = 0
    var holding = false
    private var inside = false
    private val waiting = ArrayList<Runnable>()

    override val reading: CoroutineContext = object : CoroutineDispatcher() {
        override fun dispatch(context: CoroutineContext, block: Runnable) {
            if (holding) waiting.add(block) else run(block)
        }
    }

    private fun run(block: Runnable) {
        inside = true
        try {
            block.run()
        } finally {
            inside = false
        }
    }

    fun release() {
        holding = false
        val now = waiting.toList()
        waiting.clear()
        now.forEach(::run)
    }

    override fun list(context: Context, call: CallModel): List<CameraChoice> {
        asked++
        if (!inside) askedElsewhere++
        return listOf(CameraChoice("1", CameraChoice.Facing.Front), CameraChoice("0", CameraChoice.Facing.Back))
    }

    override fun current(call: CallModel, present: List<CameraChoice>) = present.firstOrNull()?.id
}
