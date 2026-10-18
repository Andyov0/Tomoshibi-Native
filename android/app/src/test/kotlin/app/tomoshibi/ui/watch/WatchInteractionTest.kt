package app.tomoshibi.ui.watch

import android.content.Context
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.feature.music.SongDesk
import app.tomoshibi.feature.music.TestDeskMachinery
import app.tomoshibi.feature.music.deskState
import app.tomoshibi.feature.music.entry
import app.tomoshibi.feature.watch.TestDeployment
import app.tomoshibi.feature.watch.TestMachinery
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.feature.watch.WatchInfo
import app.tomoshibi.feature.watch.WatchPlayer
import app.tomoshibi.feature.watch.callModel
import app.tomoshibi.feature.watch.resolved
import app.tomoshibi.feature.watch.show
import app.tomoshibi.feature.watch.video
import app.tomoshibi.ui.music.MusicPanelContent
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * That the panels' and the stage's presses reach the show and the desk, by
 * the words a screen reader says for each, under Robolectric: the debug
 * presses of the Apple apps reach the model and not the hit testing, which
 * these do.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w412dp-h900dp")
class WatchInteractionTest {
    @get:Rule
    val compose = createComposeRule()

    private val machinery = TestMachinery()
    private val theatre get() = machinery.theatre

    @Before
    fun setUp() {
        Localization.install(ApplicationProvider.getApplicationContext<Context>()).choose(Language.En)
    }

    private fun engine(): WatchEngine =
        WatchEngine(callModel(deployment = TestDeployment(), context = Dispatchers.Main.immediate), machinery, live = false)

    @Test
    fun aLinkPastedAndAddedGoesToTheShowAndTheFieldEmpties() {
        val engine = engine()
        engine.show = show(null)
        compose.setContent { WatchPanelContent(engine, close = {}) }
        compose.onNode(hasSetTextAction()).performTextInput("https://archive.example/general")
        compose.onNodeWithContentDescription("Add a video").performClick()
        compose.waitForIdle()
        assertEquals(listOf("addLink https://archive.example/general"), theatre.asked)
        compose.onNode(hasSetTextAction()).assert(SemanticsMatcher.expectValue(SemanticsProperties.EditableText, AnnotatedString("")))
    }

    @Test
    fun theStagesControlsDriveTheShowAndAVideoInTheQueueComesOff() {
        val engine = engine()
        val info = resolved("t-now")
        val now = video("now", "t-now")
        engine.show = show(now, listOf(video("next")), playing = false)
        engine.onStage = true
        engine.infos = mapOf("t-now" to WatchInfo.Known(info), "t-next" to WatchInfo.Known(resolved("t-next", title = "Big Buck Bunny")))
        engine.player = WatchPlayer(now, info, engine, machinery, "https://meet.example", false).also { it.waiting = false }
        compose.setContent {
            androidx.compose.foundation.layout.Column {
                WatchStageContent(engine, Modifier.size(412.dp, 232.dp))
                WatchPanelContent(engine, close = {})
            }
        }
        compose.onNodeWithContentDescription("Play for everybody").performClick()
        compose.onNodeWithContentDescription("Next video").performClick()
        compose.onNodeWithContentDescription("Take Big Buck Bunny off").performClick()
        compose.waitForIdle()
        assertEquals(listOf("play", "skip now", "remove next"), theatre.asked)
    }

    @Test
    fun theDesksPressesReachTheDesk() {
        val machinery = TestDeskMachinery()
        val desk = SongDesk(callModel(deployment = TestDeployment(), context = Dispatchers.Main.immediate), machinery, live = false)
        desk.state = deskState(now = entry("1"), queue = listOf(entry("2")))
        compose.setContent { MusicPanelContent(desk, close = {}) }
        compose.onNodeWithText("Vote to skip (0/2)").performClick()
        compose.onNode(hasSetTextAction()).performTextInput("clair de lune")
        compose.onNode(hasContentDescription("Ask for a song") and SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button)).performClick()
        compose.waitForIdle()
        assertEquals(listOf("vote", "ask clair de lune"), machinery.desk.told)
        assertTrue(desk.panelOpen)
    }
}
