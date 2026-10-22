package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Pace
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.Installed
import app.tomoshibi.model.LobbyNotice
import app.tomoshibi.model.MemoryDefaults
import app.tomoshibi.model.Offer
import app.tomoshibi.model.Remote
import app.tomoshibi.model.RemoteAnswer
import app.tomoshibi.model.RemoteSource
import app.tomoshibi.model.Transfer
import app.tomoshibi.model.UpdateInstaller
import app.tomoshibi.model.UpdateSource
import app.tomoshibi.model.Updates
import app.tomoshibi.model.Updates.Step
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import app.tomoshibi.ui.settings.SettingsTopic
import app.tomoshibi.ui.settings.SettingsView
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.json.JSONArray
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * What the deployment says, as the screens before a call show it (Remote):
 * a phrase it rewords in place of the app's own in the language showing,
 * and the app's own again in a language it wrote nothing in; its notice at
 * the top of the lobby under the title, closed for good by its close and
 * back only as another notice, and never anywhere but the lobby; the blur
 * switch and the updates' row gone from the settings while it has them
 * switched off. The deployment here is a stand-in that answers at once.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = PHONE)
class RemoteLobbyTest {
    @get:Rule
    val compose = createComposeRule()

    /** Says whatever [answer] is, as the deployment would. */
    private class Says(var answer: RemoteAnswer) : RemoteSource {
        override suspend fun fetch() = answer
        override fun read(kept: String): RemoteAnswer? = null
    }

    private val made = mutableListOf<Remote>()

    @Before
    fun setUp() {
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        val localization = Localization.install(context)
        localization.keeps = false
        localization.choose(Language.En)
        Appearance.install(context).keeps = false
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        made.forEach { it.stop() }
        Localization.shared.laidOver = emptyMap()
        Localization.shared.choose(Language.En)
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.section = null
    }

    /** [app] told what the deployment says, and asking now; again whenever asked, with no minute's wait between. */
    private fun hearing(app: AppModel, says: Says, store: MemoryDefaults = MemoryDefaults()): Remote {
        val remote = Remote(says, store, CoroutineScope(SupervisorJob() + Dispatchers.Unconfined), soonest = 0)
        made += remote
        app.remote = remote
        remote.start()
        return remote
    }

    /** Two seconds of frames, after handing the composition whatever was changed here. */
    private fun settle() {
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
    }

    private fun gone(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isEmpty()

    @Test
    fun aPhraseTheDeploymentRewordsIsShownInTheLanguageItWasWrittenIn() {
        val app = fakeApp()
        hearing(app, Says(RemoteAnswer(phrases = mapOf(Language.En to mapOf("Upcoming" to "Coming up"), Language.ZhHans to mapOf("Upcoming" to "接下来")))))
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithText("Coming up").assertExists()
        assertTrue(compose.onAllNodesWithText("Upcoming").fetchSemanticsNodes().isEmpty())
        Localization.shared.choose(Language.ZhHans)
        settle()
        compose.onNodeWithText("接下来").assertExists()
        // Nothing written in Japanese: the app's own Japanese, not the deployment's English.
        Localization.shared.choose(Language.Ja)
        val own = Localization.shared.t("Upcoming")
        assertTrue("the app has a Japanese of its own for this", own != "Upcoming" && own != "Coming up")
        settle()
        compose.onNodeWithText(own).assertExists()
    }

    @Test
    fun theNoticeIsAtTheTopOfTheLobbyAndClosedForGoodUntilAnotherIsPutUp() {
        val app = fakeApp()
        val store = MemoryDefaults()
        val says = Says(RemoteAnswer(notice = LobbyNotice("n1", text = mapOf(Language.En to "Down tonight from nine"))))
        val remote = hearing(app, says, store)
        compose.mainClock.autoAdvance = false
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithTag("lobby.notice").assertExists()
        compose.onNodeWithText("Down tonight from nine").assertExists()
        // Under the title, above everything else in the page.
        // The large title, and the navigation bar's, which waits above it.
        val title = compose.onAllNodesWithText("Tomoshibi").fetchSemanticsNodes().maxBy { it.boundsInRoot.bottom }.boundsInRoot
        val notice = compose.onNodeWithTag("lobby.notice").fetchSemanticsNode().boundsInRoot
        val hero = compose.onNodeWithText("Start a meeting").fetchSemanticsNode().boundsInRoot
        assertTrue("the notice is not under the title: $notice, $title", notice.top >= title.bottom)
        assertTrue("the notice lies over the page: $notice, $hero", notice.bottom <= hero.top)

        compose.onNodeWithTag("lobby.notice.close").performClick()
        // It leaves rather than vanishing.
        compose.mainClock.advanceTimeBy(48)
        assertTrue("gone at once, with no leaving", !gone("lobby.notice"))
        settle()
        assertTrue("still there after its close", gone("lobby.notice"))
        assertEquals(listOf("n1"), JSONArray(store.string(Remote.CLOSED_KEY)).let { a -> (0 until a.length()).map { a.getString(it) } })

        // Asked again, the same notice is not shown again.
        remote.refresh()
        settle()
        assertTrue("a closed notice came back", gone("lobby.notice"))
        // Another one is.
        says.answer = RemoteAnswer(notice = LobbyNotice("n2", warning = true, text = mapOf(Language.En to "Moved to a new server")))
        remote.refresh()
        settle()
        compose.onNodeWithText("Moved to a new server").assertExists()
    }

    @Test
    fun theNoticeIsTheLobbysAndNotTheWayIntoACall() {
        val app = fakeApp(screen = AppModel.Screen.Prejoin)
        hearing(app, Says(RemoteAnswer(notice = LobbyNotice("n1", text = mapOf(Language.En to "Down tonight from nine")))))
        compose.setContent { TestApp(app) }
        settle()
        assertTrue("a notice on the way into a call", gone("lobby.notice"))
        assertTrue(app.remote.notice == null)
        // Back in the lobby, which the model tells it of.
        compose.runOnIdle { app.backToLobby() }
        settle()
        compose.onNodeWithTag("lobby.notice").assertExists()
    }

    @Test
    fun aNoticeInTheReadersLanguageOrNoneAtAll() {
        val app = fakeApp()
        hearing(app, Says(RemoteAnswer(notice = LobbyNotice("n1", text = mapOf(Language.En to "Down tonight", Language.ZhHans to "今晚维护")))))
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithText("Down tonight").assertExists()
        Localization.shared.choose(Language.ZhHans)
        settle()
        compose.onNodeWithText("今晚维护").assertExists()
    }

    private object Nowhere : UpdateSource, UpdateInstaller {
        override suspend fun latest(running: String): Offer? = null
        override fun download(offer: Offer): Transfer = error("nothing is downloaded here")
        override fun clear(keepingKept: Boolean) {}
        override fun keep(file: File?) {}
        override suspend fun kept(offer: Offer): File? = null
        override val allowed = false
        override fun askToBeAllowed() {}
        override suspend fun install(file: File): Installed = Installed.Declined
        override fun stop() {}
    }

    private fun settings(app: AppModel, topic: SettingsTopic) {
        compose.setContent {
            CompositionLocalProvider(LocalSettingsPlatform provides FakePlatform()) { SettingsView(app, {}, Modifier, topic) }
        }
        settle()
    }

    @Test
    fun theBlurSwitchGoesFromTheSettingsWhileBlurIsSwitchedOff() {
        val app = fakeApp()
        val says = Says(RemoteAnswer(off = setOf("background-blur")))
        val remote = hearing(app, says)
        settings(app, SettingsTopic.Media)
        assertTrue("the blur switch is offered while switched off", gone("settings.blur"))
        says.answer = RemoteAnswer()
        compose.runOnIdle { remote.refresh() }
        settle()
        compose.onNodeWithTag("settings.blur").assertExists()
    }

    @Test
    fun theUpdatesRowGoesWhileUpdatesAreSwitchedOffButNotFromADownloadUnderWay() {
        val app = fakeApp()
        val updates = Updates(Nowhere, Nowhere, "0.1.4", CoroutineScope(Dispatchers.Unconfined), Pace(null, whole = 0))
        app.updates = updates
        hearing(app, Says(RemoteAnswer(off = setOf("app-updates"))))
        settings(app, SettingsTopic.About)
        assertTrue("the updates' row is there with updates switched off", gone("settings.updates"))
        updates.stage("step", Step.Downloading(Offer("0.1.4.2", 56_093_610), 1_000_000, 56_093_610))
        settle()
        compose.onNodeWithTag("settings.updates").assertExists()
    }
}
