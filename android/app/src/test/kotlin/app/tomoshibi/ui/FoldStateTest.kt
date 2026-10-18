package app.tomoshibi.ui

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.lobby.LobbyDebug
import app.tomoshibi.ui.lobby.LobbyView
import app.tomoshibi.ui.lobby.ada
import app.tomoshibi.ui.lobby.fakeApp
import app.tomoshibi.ui.room.BOOK_HINGE
import app.tomoshibi.ui.room.LocalRoomPictures
import app.tomoshibi.ui.room.PEOPLE
import app.tomoshibi.ui.room.RoomPanel
import app.tomoshibi.ui.room.RoomState
import app.tomoshibi.ui.room.RoomView
import app.tomoshibi.ui.room.SHARE
import app.tomoshibi.ui.room.TABLE_HINGE
import app.tomoshibi.ui.room.callModel
import app.tomoshibi.ui.room.joins
import app.tomoshibi.ui.room.standInPictures
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.LocalCallCameras
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * A Fold folded and opened out, and stood up and laid flat, with something
 * open: nothing open is closed, nothing chosen is forgotten, nothing typed is
 * lost. The activity turns round for itself (the manifest's configChanges),
 * so a fold reaches the app as its window changing size, and a half-open
 * phone as its hinge appearing: the same here, in a window big enough for
 * the inner screen, with the app drawn at the size of whichever screen it is
 * on and the hinge provided where Robolectric has none.
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w980dp-h980dp-hdpi")
class FoldStateTest {
    @get:Rule
    val compose = createComposeRule()

    private var screen by mutableStateOf(COVER_SIZE)
    private var hinge by mutableStateOf<Hinge?>(null)

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Appearance.install(context).keeps = false
        Appearance.shared.choose(ControlsPlacement.Always)
        standInRules()
        LobbyDebug.open = null
        LobbyDebug.section = null
        LobbyDebug.popover = null
    }

    @After
    fun tearDown() {
        LobbyDebug.open = null
        LobbyDebug.section = null
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
    }

    /** Words typed into the field inside the one tagged [tag]. */
    private fun typed(tag: String, words: String) = compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag(tag))).performTextInput(words)

    /**
     * The field inside the one tagged [tag], holding exactly [words]; a
     * passphrase's field says only as many dots, which an emptied field
     * does not.
     */
    private fun holds(tag: String, words: String) {
        val matcher = SemanticsMatcher("holds \"$words\"") {
            val text = it.config.getOrNull(SemanticsProperties.EditableText)?.text
            text == words || (SemanticsProperties.Password in it.config && text?.length == words.length && text.isNotEmpty())
        }
        compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag(tag)) and matcher).assertExists()
    }

    private fun fold(to: DpSize, hinge: Hinge? = null) {
        screen = to
        this.hinge = hinge
        settle()
    }

    // MARK: The settings

    private fun lobby() {
        val app = fakeApp()
        compose.setContent {
            CompositionLocalProvider(LocalHinge provides hinge, LocalSettingsPlatform provides FakePlatform()) {
                Box(Modifier.size(screen)) { GlassBackdrop { LobbyView(app, ada) } }
            }
        }
        settle()
    }

    /**
     * The fault the owner found: with the settings open, folding or opening
     * the phone took them back to their first page and emptied what was
     * being typed, because the phone's settings and the wide ones kept their
     * own state, and the lobby's sheet and its dialog kept theirs.
     */
    @Test
    fun theSettingsKeepTheirPageAndWhatIsTypedThroughAFold() {
        LobbyDebug.open = "settings"
        LobbyDebug.section = "account"
        lobby()
        compose.onNodeWithTag("settings.back").assertExists()
        typed("settings.passphrase.current", "the old one")
        typed("settings.passphrase.next", "a new one, long")
        settle()

        // Opened out: the wide settings, at the same section.
        fold(INNER_SIZE)
        compose.onNodeWithTag("settings.section.account").assertIsSelected()
        holds("settings.passphrase.current", "the old one")
        holds("settings.passphrase.next", "a new one, long")

        // Stood on a table, then held as a book.
        fold(TURNED_SIZE, TABLE_HINGE)
        compose.onNodeWithTag("settings.section.account").assertIsSelected()
        holds("settings.passphrase.next", "a new one, long")
        fold(INNER_SIZE, BOOK_HINGE)
        holds("settings.passphrase.next", "a new one, long")

        // Folded again: the phone's page drilled into the same section.
        fold(COVER_SIZE)
        compose.onNodeWithTag("settings.back").assertExists()
        holds("settings.passphrase.current", "the old one")
        holds("settings.passphrase.next", "a new one, long")
    }

    @Test
    fun aSectionChosenOpenedOutIsTheSectionFolded() {
        LobbyDebug.open = "settings"
        fold(INNER_SIZE)
        lobby()
        compose.onNodeWithTag("settings.section.media").performClick()
        settle()
        fold(COVER_SIZE)
        compose.onNodeWithTag("settings.back").assertExists()
        compose.onNodeWithTag("settings.start.microphone").assertExists()
        // And back out to the list, which opened out is the first section.
        compose.onNodeWithTag("settings.back").performClick()
        settle()
        compose.onNodeWithTag("settings.topic.media").assertExists()
        fold(INNER_SIZE)
        compose.onNodeWithTag("settings.section.general").assertIsSelected()
    }

    // MARK: The call

    private fun room(people: Int): CallModel {
        val call = callModel()
        PEOPLE.take(people).forEach { (id, name) -> call.joins(id, name) }
        call.settled = true
        return call
    }

    private fun show(call: CallModel) {
        val app = app.tomoshibi.ui.room.app()
        compose.setContent {
            CompositionLocalProvider(LocalRoomPictures provides standInPictures, LocalHinge provides hinge, LocalCallCameras provides FakeCameras) {
                Box(Modifier.size(screen)) { RoomView(app, call) }
            }
        }
        settle()
    }

    /** Every way the phone is held, one after another, ending as it began. */
    private fun everyWay(check: () -> Unit) {
        for ((size, hinge) in listOf(INNER_SIZE to null, TURNED_SIZE to TABLE_HINGE, INNER_SIZE to BOOK_HINGE, TURNED_SIZE to null, COVER_SIZE to null)) {
            fold(size, hinge)
            check()
        }
    }

    @Test
    fun theCallKeepsWhatIsOnTheStageAndTheOpenPanel() {
        val call = room(3)
        val state = RoomState.of(call)
        show(call)
        state.pin = state.pin.pin("p-kenji/camera")
        state.panel = RoomPanel.Messages
        call.chatDraft = "half a thought"
        settle()
        everyWay {
            assertEquals("p-kenji/camera", state.pin.pinned)
            assertEquals(RoomPanel.Messages, state.panel)
            assertEquals(1, compose.onAllNodesWithText("half a thought").fetchSemanticsNodes().size)
        }
    }

    /**
     * What is typed into the watch panel lives in the panel itself rather
     * than in the call, so it is the check that the panel is never drawn
     * again as the room moves it from the side of the call to the lower half
     * of the table and to the second page of the book.
     */
    @Test
    fun theWatchPanelKeepsWhatWasPasted() {
        val call = room(2)
        RoomState.of(call).panel = RoomPanel.Watch
        show(call)
        compose.onAllNodes(hasSetTextAction())[0].performTextInput("https://video.example/film.mp4")
        settle()
        everyWay {
            assertEquals(1, compose.onAllNodesWithText("https://video.example/film.mp4").fetchSemanticsNodes().size)
        }
    }

    @Test
    fun theMusicPanelKeepsWhatWasAskedFor() {
        val call = room(2)
        RoomState.of(call).panel = RoomPanel.Music
        show(call)
        compose.onAllNodes(hasSetTextAction())[0].performTextInput("Clair de lune")
        settle()
        everyWay {
            assertEquals(1, compose.onAllNodesWithText("Clair de lune").fetchSemanticsNodes().size)
        }
    }

    @Test
    fun theShareMenuStaysOpen() {
        val call = room(3)
        val state = RoomState.of(call)
        show(call)
        state.popovers.show(SHARE)
        settle()
        everyWay {
            assertEquals(SHARE, state.popovers.open)
            compose.onAllNodesWithText("Share your screen")[0].assertExists()
        }
    }

    private companion object {
        val COVER_SIZE = DpSize(372.dp, 800.dp)
        val INNER_SIZE = DpSize(840.dp, 980.dp)
        val TURNED_SIZE = DpSize(980.dp, 840.dp)
    }
}
