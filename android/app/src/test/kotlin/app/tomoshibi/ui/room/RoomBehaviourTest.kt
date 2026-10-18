package app.tomoshibi.ui.room

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.CallModel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/*
 * What a finger does in the room, pressed through the composition the way a
 * person presses it: a picture onto the stage and a second press filling the
 * screen, Back closing what is open and then asking about leaving rather
 * than leaving, the messages sent from the box, the hand raised, the
 * controls stepping aside and coming back, a panel leaving rather than
 * vanishing. What these guard is the room as the Swift one behaves.
 */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], qualifiers = PHONE)
class RoomBehaviourTest {
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

    @After
    fun tearDown() {
        Appearance.shared.choose(ControlsPlacement.Always)
    }

    private fun room(people: Int, fake: FakeCall = FakeCall()): CallModel {
        val call = callModel(fake)
        PEOPLE.take(people).forEach { (id, name) -> call.joins(id, name) }
        call.settled = true
        return call
    }

    private fun show(call: CallModel) {
        val app = app()
        compose.setContent {
            CompositionLocalProvider(LocalRoomPictures provides standInPictures) { RoomView(app, call) }
        }
        settle()
    }

    private fun settle(millis: Long = 800) {
        compose.mainClock.advanceTimeBy(millis)
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        settle()
    }

    @Test
    fun aPressPutsAPictureOnTheStageAndASecondFillsTheScreen() {
        val call = room(3)
        val state = RoomState.of(call)
        show(call)
        compose.onNodeWithContentDescription("Show Mei Lin larger").performClick()
        settle()
        assertEquals("p-mei/camera", state.pin.pinned)
        assertFalse(state.filled)
        // The second press lands on whatever is under the finger now; it
        // counts for the picture the first one moved.
        compose.onNodeWithContentDescription("Show everybody").performClick()
        settle()
        assertTrue(state.filled)
        assertEquals("p-mei/camera", state.pin.pinned)
    }

    @Test
    fun backClosesWhatIsOpenThenAsksAboutLeavingAndNeverLeaves() {
        val fake = FakeCall()
        val call = room(2, fake)
        val state = RoomState.of(call)
        state.panel = RoomPanel.Sound
        state.pin = state.pin.pin("p-mei/camera")
        state.filled = true
        show(call)
        back()
        assertNull(state.panel)
        assertTrue(state.filled)
        back()
        assertFalse(state.filled)
        back()
        assertEquals(LEAVE, state.popovers.open)
        compose.onNodeWithText("Leave the call").assertIsDisplayed()
        assertFalse(fake.left)
        // Back again puts the question away; still in the call.
        back()
        assertNull(state.popovers.open)
        assertFalse(fake.left)
    }

    @Test
    fun theLeaveSheetLeaves() {
        val fake = FakeCall()
        val call = room(1, fake)
        show(call)
        compose.onNodeWithContentDescription("Leave").performClick()
        settle()
        compose.onNodeWithText("Leave the call").performClick()
        settle()
        assertTrue(fake.left)
    }

    @Test
    fun whoeverRunsTheRoomIsOfferedTheEndOfItForEverybody() {
        val call = callModel(deployment = FakeDeployment(hosting = true))
        call.joins("p-mei", "Mei Lin")
        show(call)
        settle(1500)
        compose.onNodeWithContentDescription("Leave").performClick()
        settle()
        compose.onNodeWithText("End for everybody").assertIsDisplayed()
    }

    @Test
    fun aMessageGoesFromTheBox() {
        val fake = FakeCall()
        val call = room(1, fake)
        show(call)
        compose.onNodeWithContentDescription("Show messages").performClick()
        settle()
        assertEquals(RoomPanel.Messages, RoomState.of(call).panel)
        assertTrue(call.chatOpen)
        compose.onNodeWithContentDescription("Say something").performTextInput("Hello from the phone")
        compose.onNodeWithContentDescription("Send").performClick()
        settle()
        assertEquals(listOf("Hello from the phone"), fake.sent)
        assertEquals("Hello from the phone", call.messages.single().text)
        assertEquals("", call.chatDraft)
    }

    @Test
    fun theHandGoesUpAndDown() {
        val fake = FakeCall()
        val call = room(1, fake)
        show(call)
        compose.onNodeWithContentDescription("Raise your hand").performClick()
        settle()
        assertTrue(call.handUp)
        assertEquals(mapOf("hand" to "up"), fake.written.last())
        compose.onNodeWithContentDescription("Lower your hand").performClick()
        settle()
        assertFalse(call.handUp)
    }

    @Test
    fun theControlsStepAsideWhenNothingHappensAndComeBackOnATouch() {
        Appearance.shared.choose(ControlsPlacement.Idle)
        val call = room(2)
        show(call)
        compose.onNodeWithContentDescription("Leave").assertIsDisplayed()
        settle(3500)
        assertTrue(compose.onAllNodesWithContentDescription("Leave").fetchSemanticsNodes().isEmpty())
        assertTrue(RoomState.of(call).idle.idle)
        compose.onRoot().performTouchInput { down(center); up() }
        settle()
        assertFalse(RoomState.of(call).idle.idle)
        compose.onNodeWithContentDescription("Leave").assertIsDisplayed()
    }

    @Test
    fun aPanelLeavesRatherThanVanishing() {
        val call = room(1)
        val state = RoomState.of(call)
        state.panel = RoomPanel.Messages
        show(call)
        compose.onNodeWithText("Messages").assertIsDisplayed()
        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { state.panel = null }
        val there = (1..30).map {
            compose.mainClock.advanceTimeBy(16)
            compose.onAllNodesWithText("Messages").fetchSemanticsNodes().isNotEmpty()
        }
        assertTrue("gone before its departure had played: $there", there.take(8).all { it })
        assertFalse("still there long after its departure: $there", there.last())
    }
}
