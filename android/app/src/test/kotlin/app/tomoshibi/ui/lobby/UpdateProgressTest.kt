package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Pace
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.Installed
import app.tomoshibi.model.Offer
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
 * The update's download as the views draw it: the lobby's banner draws its
 * bar where the model's bar is (Updates `downloadBar`), which moves at one
 * speed, and not where the download is, which jumps; the figures beside it,
 * there and on the About page, count up with that bar rather than run ahead
 * of it, as one line whose numbers change. And a later build of a version is
 * named as people are told it, "0.1.4 (2)", wherever an update is named,
 * never as the version the deployment compares, 0.1.4.2.
 *
 * The bar's pace is PaceTest's, and what waits for it to fill UpdatesTest's;
 * this is that the views draw by it. The bar here is moved by hand.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = PHONE)
class UpdateProgressTest {
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

    @get:Rule
    val compose = createComposeRule()

    private val offer = Offer("0.1.4.2", 56_093_610, label = "0.1.4 (2)")
    private val bar = Pace(null)

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
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.section = null
    }

    private fun updating(app: AppModel, step: Step): Updates {
        val updates = Updates(Nowhere, Nowhere, "0.1.4", CoroutineScope(Dispatchers.Unconfined), bar)
        app.updates = updates
        updates.stage("step", step)
        return updates
    }

    private fun inTheLobby(step: Step): Updates {
        val app = fakeApp()
        val updates = updating(app, step)
        compose.mainClock.autoAdvance = false
        compose.setContent { TestApp(app) }
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        return updates
    }

    private fun onTheAboutPage(step: Step): Updates {
        val app = fakeApp()
        val updates = updating(app, step)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalSettingsPlatform provides FakePlatform()) {
                SettingsView(app, {}, Modifier, SettingsTopic.About)
            }
        }
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        return updates
    }

    /**
     * A frame: the bar moved on by one, and the screen drawn with it. The
     * main looper is run between, which is what hands a change made here to
     * the composition.
     */
    private fun frame() {
        bar.advance(16_000_000)
        compose.waitForIdle()
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
    }

    /** How much of the banner's bar is filled, from nought to one. */
    private fun filled(): Float {
        val fill = compose.onAllNodesWithTag("update.progress.fill", useUnmergedTree = true).fetchSemanticsNodes().single()
        val track = compose.onAllNodesWithTag("update.progress", useUnmergedTree = true).fetchSemanticsNodes().single()
        return fill.size.width.toFloat() / track.size.width
    }

    /** One part in the bar's width, the most rounding to whole pixels can add. */
    private fun pixel(): Float = 1f / compose.onAllNodesWithTag("update.progress", useUnmergedTree = true).fetchSemanticsNodes().single().size.width

    @Test
    fun theBannersBarIsTheModelsAndNotTheDownloads() {
        // Most of it has arrived; the bar has a tenth drawn.
        bar.follow(0.1f)
        repeat(8) { bar.advance(16_000_000) }
        inTheLobby(Step.Downloading(offer, offer.size * 9 / 10, offer.size))
        bar.follow(0.9f)
        val pixel = pixel()
        repeat(60) {
            frame()
            assertEquals("the bar drawn at ${filled()} with the model's at ${bar.value}", bar.value, filled(), pixel)
        }
        assertTrue(bar.value < 0.9f)
    }

    @Test
    fun theFiguresCountUpWithTheBar() {
        bar.follow(0.5f)
        while (!bar.caughtUp) bar.advance(16_000_000)
        bar.follow(1f)
        inTheLobby(Step.Downloading(offer, offer.size, offer.size))
        // Half of 56.1 MB drawn: the figures say so rather than the whole.
        compose.onNodeWithText("28.0 MB of 56.1 MB").assertExists()
        repeat(40) { frame() }
        compose.onNodeWithText("56.1 MB of 56.1 MB").assertExists()
    }

    @Test
    fun theAboutPagesFiguresAreOneLineThatCountsUp() {
        bar.follow(1f)
        onTheAboutPage(Step.Downloading(offer, offer.size, offer.size))
        compose.onNodeWithText("0.0 MB of 56.1 MB").assertExists()
        // A line for each frame faded in over the one before would be several at once.
        var lines = 0
        repeat(80) {
            frame()
            lines = maxOf(lines, compose.onAllNodesWithText(" MB of ", substring = true).fetchSemanticsNodes().size)
        }
        compose.onNodeWithText("56.1 MB of 56.1 MB").assertExists()
        assertEquals("lines of figures at once", 1, lines)
    }

    @Test
    fun aLaterBuildIsNamedWithItsBuildInBrackets() {
        inTheLobby(Step.Offered(offer))
        compose.onNodeWithText("Tomoshibi 0.1.4 (2) is available").assertExists()
        assertTrue(compose.onAllNodesWithText("0.1.4.2", substring = true).fetchSemanticsNodes().isEmpty())
    }

    @Test
    fun theAboutPageNamesALaterBuildTheSame() {
        onTheAboutPage(Step.Offered(offer))
        compose.onNodeWithText("Version 0.1.4 (2) is available.").assertExists()
        assertTrue(compose.onAllNodesWithText("0.1.4.2", substring = true).fetchSemanticsNodes().isEmpty())
    }
}
