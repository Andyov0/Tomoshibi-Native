package app.tomoshibi.ui.lobby

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Pace
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.Installed
import app.tomoshibi.model.Offer
import app.tomoshibi.model.Transfer
import app.tomoshibi.model.UpdateInstaller
import app.tomoshibi.model.UpdateSource
import app.tomoshibi.model.Updates
import app.tomoshibi.model.Updates.Said
import app.tomoshibi.model.Updates.Step
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import app.tomoshibi.ui.settings.SettingsTopic
import app.tomoshibi.ui.settings.SettingsView
import com.github.takahirom.roborazzi.captureRoboImage
import com.github.takahirom.roborazzi.captureScreenRoboImage
import java.io.File
import java.util.Locale
import java.util.TimeZone
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Pictures of the update as the lobby says it, step by step, at a phone's
 * width (412 points) and an unfolded Fold's (840), in English and simplified
 * Chinese, and of the About page's row: what the Mac's are set beside.
 * Recorded with `./gradlew :app:recordRoborazziDebug` into
 * src/test/screenshots/updates.
 *
 * Also the one check that the banner is there at all in the lobby a person
 * sees and gone from it outside the lobby: a model test cannot see a view
 * that forgot to read `shown`.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = PHONE)
class UpdateScreenshotTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf<Any>(Language.En), arrayOf<Any>(Language.ZhHans))

        const val DIRECTORY = "src/test/screenshots/updates"

        private val NOTES = mapOf(
            "en" to "- In-app updates: a newer version is offered in the lobby, then downloaded and installed once you agree.\n- Several fixes.",
            "zh-Hans" to "· 新增应用内更新：发现新版本会在大厅提示，同意后自动下载并安装。\n· 修复若干问题。",
        )
    }

    private object Nowhere : UpdateSource, UpdateInstaller {
        override suspend fun latest(running: String): Offer? = null
        override fun download(offer: Offer): Transfer = error("nothing is downloaded in a picture")
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

    // A later build of a version, as small fixes are numbered, so that the
    // pictures show the name people are told: "0.1.4 (2)".
    private val offer = Offer("0.1.4.2", 56_093_610, { NOTES[it] }, label = "0.1.4 (2)")

    @Before
    fun setUp() {
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        val localization = Localization.install(context)
        localization.keeps = false
        localization.choose(language)
        Appearance.install(context).keeps = false
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.section = null
    }

    private fun updating(app: AppModel, step: Step, said: Said? = null): Updates {
        val updates = Updates(Nowhere, Nowhere, "0.1.4", CoroutineScope(Dispatchers.Unconfined), Pace(null, whole = 0))
        app.updates = updates
        updates.stage("step", step)
        // The bar where the download is, as the model's reports would put it.
        if (step is Step.Downloading) updates.downloadBar.follow(step.received.toFloat() / step.total)
        updates.stage("said", said)
        return updates
    }

    private fun shoot(name: String, step: Step, wide: Boolean = false, signedIn: Boolean = true) {
        val app = fakeApp(signedIn = signedIn)
        updating(app, step)
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithTag("update.banner").assertExists()
        captureScreenRoboImage("$DIRECTORY/$name-${if (wide) "840" else "412"}-${language.code}.png")
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(2000)
        compose.waitForIdle()
    }

    @Test fun offered() = shoot("offered", Step.Offered(offer))
    @Test @Config(qualifiers = WIDE) fun offeredWide() = shoot("offered", Step.Offered(offer), wide = true)
    @Test fun offeredSignedOut() = shoot("offered-signed-out", Step.Offered(offer), signedIn = false)
    @Test fun downloading() = shoot("downloading", Step.Downloading(offer, 23_400_000, 56_093_610))
    @Test @Config(qualifiers = WIDE) fun downloadingWide() = shoot("downloading", Step.Downloading(offer, 23_400_000, 56_093_610), wide = true)
    @Test fun permission() = shoot("permission", Step.Permission(offer, File("/x.apk")))
    @Test @Config(qualifiers = WIDE) fun permissionWide() = shoot("permission", Step.Permission(offer, File("/x.apk")), wide = true)
    @Test fun refused() = shoot("permission-refused", Step.Permission(offer, File("/x.apk"), asked = true, refused = true))
    @Test fun installing() = shoot("installing", Step.Installing(offer))
    @Test fun failed() = shoot("failed", Step.Offered(offer, "The download was damaged and has been deleted. Try again."))

    /** Outside the lobby there is no banner, whatever there is to say. */
    @Test
    fun notOverACall() {
        val app = fakeApp(screen = AppModel.Screen.Prejoin)
        updating(app, Step.Offered(offer))
        compose.setContent { TestApp(app) }
        settle()
        compose.onNodeWithTag("update.banner").assertDoesNotExist()
    }

    private fun about(name: String, step: Step, said: Said?) {
        val app = fakeApp()
        updating(app, step, said)
        compose.setContent {
            CompositionLocalProvider(LocalSettingsPlatform provides FakePlatform()) {
                SettingsView(app, {}, Modifier, SettingsTopic.About)
            }
        }
        settle()
        compose.onNodeWithTag("settings.updates").assertExists()
        compose.onRoot().captureRoboImage("$DIRECTORY/about-$name-412-${language.code}.png")
    }

    @Test fun aboutCurrent() = about("current", Step.None, Said.Current)
    @Test fun aboutOffered() = about("offered", Step.Offered(offer), null)
    @Test fun aboutDownloading() = about("downloading", Step.Downloading(offer, 23_400_000, 56_093_610), null)
}
