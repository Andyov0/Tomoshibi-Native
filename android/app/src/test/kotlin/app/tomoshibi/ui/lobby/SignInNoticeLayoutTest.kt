package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Pace
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
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.room.COVER
import app.tomoshibi.ui.room.Held
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalCallCameras
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import com.github.takahirom.roborazzi.captureScreenRoboImage
import java.io.File
import java.util.Locale
import java.util.TimeZone
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The deployment's notice on the sign-in, which is for whoever has the app
 * signed in or not, on a Fold6 held each way it is held (Held): at the head
 * of the page, inside the window and clear of the language menu, the update
 * offered at the same time under it rather than on it, and the greeting
 * below them both. Pictures into src/test/screenshots/remote, recorded with
 * `./gradlew :app:recordRoborazziDebug`.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = COVER)
class SignInNoticeLayoutTest(private val held: Held) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun cases() = Held.entries.map { arrayOf<Any>(it) }

        const val DIRECTORY = "src/test/screenshots/remote"
    }

    @get:Rule
    val compose = createComposeRule()

    private var remote: Remote? = null

    private class Says(val answer: RemoteAnswer) : RemoteSource {
        override suspend fun fetch() = answer
        override fun read(kept: String): RemoteAnswer? = null
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

    @Before
    fun setUp() {
        RuntimeEnvironment.setQualifiers(held.qualifiers)
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(Language.En)
        Localization.shared.keeps = false
        Appearance.install(context).keeps = false
        PlainRoomNames.install()
        LobbyDebug.open = null
        LobbyDebug.section = null
    }

    @After
    fun tearDown() {
        remote?.stop()
        PlainRoomNames.remove()
    }

    private fun bounds(tag: String): Rect = compose.onNodeWithTag(tag).fetchSemanticsNode().boundsInRoot

    @Test
    fun theNoticeIsAtTheHeadOfTheSignInWithTheUpdateUnderIt() {
        val app = fakeApp(signedIn = false)
        val updates = Updates(Nowhere, Nowhere, "0.1.4", CoroutineScope(Dispatchers.Unconfined), Pace(null, whole = 0))
        app.updates = updates
        updates.stage("step", Step.Offered(Offer("0.1.4.2", 56_093_610, label = "0.1.4 (2)")))
        val said = Remote(
            Says(RemoteAnswer(notice = LobbyNotice("n1", warning = true, text = mapOf(Language.En to "The server restarts at 23:00 for about ten minutes.")))),
            MemoryDefaults(), CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
        )
        remote = said
        app.remote = said
        said.start()
        compose.setContent {
            CompositionLocalProvider(LocalHinge provides held.hinge, LocalSettingsPlatform provides FakePlatform(), LocalCallCameras provides FakeCameras) {
                TestApp(app)
            }
        }
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
        captureScreenRoboImage("$DIRECTORY/sign-in-${held.tag}.png")

        val window = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val notice = bounds("lobby.notice")
        val update = bounds("update.banner")
        assertTrue("the notice is not inside the window: $notice in $window", notice.top >= window.top && notice.bottom <= window.bottom)
        assertTrue("the update is not under the notice: $update, $notice", update.top >= notice.bottom)
        val menu = compose.onNodeWithContentDescription("Language").fetchSemanticsNode().boundsInRoot
        assertFalse("the language menu lies over the notice: $menu, $notice", menu.overlaps(notice))
        assertFalse("the language menu lies over the update: $menu, $update", menu.overlaps(update))
        // The sign-in itself where it was, whole, and on a table above the hinge.
        val button = compose.onAllNodesWithText("Sign in").fetchSemanticsNodes().maxBy { it.boundsInRoot.top }.boundsInRoot
        val floor = held.hinge?.takeIf { !it.vertical }?.top ?: window.bottom
        assertTrue("the sign-in is pushed off its half: $button, under $floor", button.bottom <= floor)
        val greeting = compose.onAllNodesWithText("Sign in").fetchSemanticsNodes().minBy { it.boundsInRoot.top }.boundsInRoot
        assertTrue("the greeting lies under the notice: $greeting, $notice", greeting.top >= update.bottom || greeting.left >= update.right || greeting.right <= update.left)
    }
}
