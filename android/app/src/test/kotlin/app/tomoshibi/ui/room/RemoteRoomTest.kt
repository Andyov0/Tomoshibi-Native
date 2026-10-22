package app.tomoshibi.ui.room

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.Remote
import app.tomoshibi.model.RemoteAnswer
import app.tomoshibi.model.RemoteSource
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.lobby.WIDE
import app.tomoshibi.ui.lobby.stage
import app.tomoshibi.ui.settings.DevicesPanel
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.LocalCallCameras
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/**
 * The call's controls as the deployment switches features off (Remote): the
 * screen's half of the share menu, the watch and music buttons and the blur
 * switch gone, and each back when it is switched on; and nothing under way
 * taken from whoever has to stop it: a share running keeps its Stop, music
 * playing its button. Drawn at an unfolded Fold's width, where the bar has
 * its panels' own buttons.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], qualifiers = WIDE)
class RemoteRoomTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private class Says(var answer: RemoteAnswer) : RemoteSource {
        override suspend fun fetch() = answer
        override fun read(kept: String): RemoteAnswer? = null
    }

    /** What a feature says to the bar, set by the test rather than by the feature. */
    private class Offer(override var offered: Boolean = true, override var playing: Boolean = false) : BarOffer

    private val made = mutableListOf<Remote>()

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        Appearance.shared.choose(ControlsPlacement.Always)
        standInRules()
    }

    @After
    fun tearDown() {
        made.forEach { it.stop() }
    }

    private fun hearing(app: AppModel, says: Says): Remote {
        val remote = Remote(says, null, CoroutineScope(SupervisorJob() + Dispatchers.Unconfined), soonest = 0)
        made += remote
        app.remote = remote
        remote.start()
        return remote
    }

    private fun room(music: Offer = Offer()): CallModel {
        val call = callModel()
        call.parts[BarOffer.WATCH] = Offer()
        call.parts[BarOffer.MUSIC] = music
        PEOPLE.take(2).forEach { (id, name) -> call.joins(id, name) }
        call.settled = true
        return call
    }

    private fun show(app: AppModel, call: CallModel) {
        compose.setContent {
            CompositionLocalProvider(LocalRoomPictures provides standInPictures) { RoomView(app, call) }
        }
        settle()
    }

    private fun settle() {
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(800)
        compose.waitForIdle()
    }

    private fun there(description: String) = compose.onAllNodesWithContentDescription(description).fetchSemanticsNodes().isNotEmpty()
    private fun said(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    @Test
    fun featuresSwitchedOffLoseTheirControlsAndComeBackWhenOn() {
        val app = app()
        val says = Says(RemoteAnswer(off = setOf("screen-share", "watch-together", "music")))
        val remote = hearing(app, says)
        val call = room()
        show(app, call)
        assertTrue("the watch button with watching switched off", !there("Show watch together"))
        assertTrue("the music button with music switched off", !there("Show music"))
        assertTrue("the share button still offers the screen", !there("Share your screen"))
        compose.onNodeWithContentDescription("Share only sound").performClick()
        settle()
        assertTrue("the share menu offers the screen with it switched off", !said("Share your screen"))
        assertTrue("the sound went with the screen", said("Share only sound"))

        says.answer = RemoteAnswer()
        compose.runOnIdle { remote.refresh() }
        settle()
        assertTrue(said("Share your screen"))
        assertTrue(there("Show watch together"))
        assertTrue(there("Show music"))
    }

    /**
     * The share button turning from the screen's into the sound's alone, as
     * screen sharing is switched off: its symbol gives way to the other over
     * frames, both drawn for a while, never one swapped for the other between
     * two frames (SwappingSymbol, on the settle and leave curves). The button
     * is a circle of its own size either way, so the bar does not move.
     */
    @Test
    fun theShareButtonChangesOverFramesNotBetweenTwo() {
        val app = app()
        val says = Says(RemoteAnswer())
        val remote = hearing(app, says)
        val call = room()
        compose.mainClock.autoAdvance = false
        show(app, call)
        fun drawn(symbol: String) = compose.onAllNodesWithTag("symbol.$symbol", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
        val button = compose.onNodeWithContentDescription("Share your screen").fetchSemanticsNode().boundsInRoot
        assertTrue(drawn("rectangle.on.rectangle"))
        says.answer = RemoteAnswer(off = setOf("screen-share"))
        compose.runOnIdle { remote.refresh() }
        compose.waitForIdle()
        val frames = (1..30).map {
            compose.mainClock.advanceTimeByFrame()
            compose.waitForIdle()
            drawn("rectangle.on.rectangle") to drawn("waveform")
        }
        val both = frames.count { it.first && it.second }
        assertTrue("the symbol was swapped between two frames: $frames", both >= 3)
        assertTrue("the old symbol never went: $frames", frames.last() == (false to true))
        assertEquals("the button changed size", button.size, compose.onNodeWithContentDescription("Share only sound").fetchSemanticsNode().boundsInRoot.size)
    }

    @Test
    fun whatIsUnderWayKeepsItsControls() {
        val app = app()
        hearing(app, Says(RemoteAnswer(off = setOf("screen-share", "music"))))
        val call = room(music = Offer(playing = true))
        call.sharing.stage("active", true)
        show(app, call)
        assertTrue("music playing lost its button", there("Show music"))
        compose.onNodeWithContentDescription("Screen sharing settings").performClick()
        settle()
        // The menu's own: its sizes, and its Stop (the shared tile has a Stop of its own).
        assertTrue("a share running lost its settings", said("4K"))
        assertEquals("a share running lost the menu's Stop", 2, compose.onAllNodesWithText("Stop sharing").fetchSemanticsNodes().size)
    }

    @Test
    fun theBlurSwitchGoesFromTheCallWhileBlurIsSwitchedOff() {
        val app = app()
        val says = Says(RemoteAnswer(off = setOf("background-blur")))
        val remote = hearing(app, says)
        val call = room()
        call.remote = remote
        compose.setContent {
            CompositionLocalProvider(LocalCallCameras provides FakeCameras) { DevicesPanel(app, call, {}) }
        }
        settle()
        assertTrue("the blur switch with blur switched off", compose.onAllNodesWithTag("devices.blur").fetchSemanticsNodes().isEmpty())
        says.answer = RemoteAnswer()
        compose.runOnIdle { remote.refresh() }
        settle()
        compose.onNodeWithTag("devices.blur").assertExists()
    }
}
