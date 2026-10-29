package app.tomoshibi.ui.room

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.kit.HOLDS_PRESSES
import app.tomoshibi.ui.settings.CountedCameras
import app.tomoshibi.ui.settings.DeviceLists
import app.tomoshibi.ui.settings.LocalCallCameras
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/*
 * A panel of the call arriving at its full size from its first frame: in
 * from the trailing edge and faded on a wide screen, as the Swift one does,
 * and up from the foot on a phone, as the web's panels on a phone do
 * (2026-10-29). It used to grow, as well, from nothing in its corner on
 * AnimatedContent's spring, measured again every frame of its slide and its
 * glass and shadow cut by the clip that unrolled it; and the devices panel
 * asked the system for its lists in the frame it opened in. The room now
 * reads those ahead.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], qualifiers = PHONE)
class PanelArrivalTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        Appearance.shared.choose(ControlsPlacement.Always)
        standInRules()
    }

    /** The open panel: the box that holds the presses on it, around the devices panel. */
    private val panel = hasTestTag(HOLDS_PRESSES) and hasAnyDescendant(hasTestTag("devices.close"))

    @Test
    fun theRoomReadsTheDevicesAheadAndThePanelSlidesInWhole() = slidesIn(fromTheFoot = true)

    @Test
    @Config(qualifiers = WIDE)
    fun onAWideScreenThePanelComesInFromTheTrailingEdge() = slidesIn(fromTheFoot = false)

    private fun slidesIn(fromTheFoot: Boolean) {
        val cameras = CountedCameras()
        val call = callModel()
        PEOPLE.take(1).forEach { (id, name) -> call.joins(id, name) }
        call.settled = true
        val app = app()
        compose.setContent {
            CompositionLocalProvider(LocalRoomPictures provides standInPictures, LocalCallCameras provides cameras) { RoomView(app, call) }
        }
        compose.mainClock.advanceTimeBy(DeviceLists.AHEAD + 500)
        compose.waitForIdle()
        assertEquals("the room did not read the devices ahead", 1, cameras.asked)
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { RoomState.of(call).panel = RoomPanel.Devices }
        // Its first frames on screen: the state is taken up a frame or two
        // after it is set.
        var frames = 0
        while (compose.onAllNodes(panel).fetchSemanticsNodes().isEmpty() && frames++ < 10) compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val early = compose.onNode(panel).getUnclippedBoundsInRoot()
        val earlyShown = compose.onNode(panel).getBoundsInRoot()
        assertEquals("the system was asked as the panel opened", 1, cameras.asked)
        compose.mainClock.advanceTimeBy(1500)
        val settled = compose.onNode(panel).getUnclippedBoundsInRoot()
        if (fromTheFoot) {
            assertTrue("not coming up from the foot: ${early.top} against ${settled.top}", early.top > settled.top)
            assertEquals("moving sideways as it comes up", settled.left.value, early.left.value, 0.5f)
        } else {
            assertTrue("not coming in from the trailing edge: ${early.left} against ${settled.left}", early.left > settled.left)
        }
        assertEquals("measured again as it moved", settled.height.value, early.height.value, 0.5f)
        assertEquals("measured again as it moved", settled.width.value, early.width.value, 0.5f)
        // Only the screen's edge cuts it while it comes in, not a clip
        // growing from its corner.
        if (!fromTheFoot) assertEquals("unrolled from its corner", settled.height.value, earlyShown.height.value, 0.5f)
        else assertEquals("unrolled from its corner", settled.width.value, earlyShown.width.value, 0.5f)
        // And once it is in, asked again for anything plugged in since.
        assertEquals(2, cameras.asked)
        assertEquals("asked on the main thread", 0, cameras.askedElsewhere)
    }
}
