package app.tomoshibi.ui.kit

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.ui.kit.gallery.GalleryOptions
import app.tomoshibi.ui.kit.gallery.gallerySections
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureRoboImage
import com.github.takahirom.roborazzi.captureScreenRoboImage
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.Locale
import java.util.TimeZone

/**
 * Pictures of every piece of the kit in its states, at a phone's width
 * (412 points, a pixel and a half to the point, which keeps the pictures
 * small enough to keep beside the code), in English and in simplified
 * Chinese (the symbols, which say nothing, in English alone), over the
 * evening room the screens before a call stand on. Recorded with
 * `./gradlew :app:recordRoborazziDebug` into src/test/screenshots/kit and
 * held to them with `verifyRoborazziDebug`.
 *
 * Drawn by Robolectric's native graphics through the hardware renderer, which
 * carries the RenderEffect blur of the glass; an Android 15 system.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h2600dp-hdpi")
@OptIn(ExperimentalRoborazziApi::class)
class KitScreenshotTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf<Any>(Language.En), arrayOf<Any>(Language.ZhHans))

        const val DIRECTORY = "src/test/screenshots/kit"
    }

    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() {
        // The calendar's week and the moment's zone, the same wherever the
        // test is run.
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
    }

    @After
    fun tearDown() {
        GlassSupport.forcedFallback = false
        GlassSupport.everywhere = false
    }

    private fun file(name: String) = "$DIRECTORY/$name-${language.code}.png"

    /** The page on the evening room, as wide as a phone and as tall as it is. */
    private fun page(content: @Composable () -> Unit) {
        compose.setContent {
            GlassBackdrop {
                Box(Modifier.fillMaxWidth().testTag("page")) {
                    Box(Modifier.matchParentSize().background(Theme.background))
                    Backdrop(Modifier.matchParentSize().glassSource())
                    content()
                }
            }
        }
        settle()
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
    }

    private fun shoot(key: String, options: GalleryOptions = GalleryOptions(), name: String = key) {
        val section = gallerySections.first { it.key == key }
        page { section.content(options) }
        compose.onNodeWithTag("page").captureRoboImage(file(name))
    }

    @Test fun glass() = shoot("glass")

    /** The same page on a system with no blur: the stand-in Android before 12 draws. */
    @Test
    fun glassWithoutBlur() {
        GlassSupport.forcedFallback = true
        shoot("glass", name = "glass-fallback")
    }

    @Test fun menus() = shoot("menus")
    @Test fun choosing() = shoot("choosing")
    @Test fun settings() = shoot("settings")
    @Test fun bar() = shoot("bar")
    @Test fun panels() = shoot("panels")
    @Test fun fields() = shoot("fields")
    @Test fun people() = shoot("people")
    @Test fun calendar() = shoot("calendar")
    @Test fun stage() = shoot("stage")
    @Test fun media() = shoot("media")
    @Test fun toasts() = shoot("toasts")
    @Test fun signal() = shoot("signal")
    @Test fun said() = shoot("said")
    @Test
    fun symbols() {
        assumeTrue(language == Language.En)
        shoot("symbols")
    }

    /** Popovers are windows of their own, so these are pictures of the whole screen. */
    @Test
    @Config(qualifiers = "w412dp-h480dp-hdpi")
    fun popoverOffTheBar() = screen("popovers", GalleryOptions(openPopover = "more"), "popover-bar")

    @Test
    @Config(qualifiers = "w412dp-h480dp-hdpi")
    fun popoverAtTheEndOfTheBar() = screen("popovers", GalleryOptions(openPopover = "leave"), "popover-bar-end")

    @Test
    @Config(qualifiers = "w412dp-h480dp-hdpi")
    fun popoverHungBelow() = screen("popovers", GalleryOptions(openPopover = "hang"), "popover-hanging")

    @Test
    @Config(qualifiers = "w412dp-h600dp-hdpi")
    fun choiceOpen() = screen("choosing", GalleryOptions(openPopover = "choice"), "popover-choice")

    @Test
    @Config(qualifiers = "w412dp-h1000dp-hdpi")
    fun momentOpen() = screen("calendar", GalleryOptions(openPopover = "moment"), "popover-moment")

    @Test
    @Config(qualifiers = "w412dp-h800dp-hdpi")
    fun clockOpen() = screen("calendar", GalleryOptions(openPopover = "clock.hour"), "popover-clock")

    private fun screen(key: String, options: GalleryOptions, name: String) {
        val section = gallerySections.first { it.key == key }
        compose.setContent {
            GlassBackdrop {
                Box(Modifier.fillMaxSize()) {
                    Box(Modifier.matchParentSize().background(Theme.background))
                    Backdrop(Modifier.matchParentSize().glassSource())
                    section.content(options)
                }
            }
        }
        settle()
        captureScreenRoboImage(file(name))
    }

    /**
     * What a press looks like: glass gives a little, a plain surface gives
     * and brightens, a menu row and a bar button light. Held down, then
     * photographed before it is let go.
     */
    @Test
    fun pressed() {
        page {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    GlassButton({}, Modifier.testTag("glass"), prominent = true) { KitText(t("Join")) }
                    GlassButton({}) { KitText(t("Join")) }
                }
                ContentLayer {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        GlassButton({}, Modifier.testTag("plain")) { KitText(t("Copy link")) }
                        GlassButton({}) { KitText(t("Copy link")) }
                    }
                }
                GlassShell(Modifier.width(280.dp), cornerRadius = 18.dp) {
                    MenuCard {
                        MenuItem(t("Share your screen"), {}, Modifier.testTag("item"), symbol = "rectangle.on.rectangle")
                        MenuItem(t("Share only sound"), {}, symbol = "waveform")
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    BarButton("mic.fill", t("Mute microphone"), {}, Modifier.testTag("bar"))
                    BarButton("mic.fill", t("Mute microphone"), {})
                    RoundButton("xmark", t("Close"), {}, Modifier.testTag("round"))
                    RoundButton("xmark", t("Close"), {})
                    PillButton(t("Let in"), {}, Modifier.testTag("pill"), role = PillRole.Prominent)
                    PillButton(t("Let in"), {}, role = PillRole.Prominent)
                }
            }
        }
        for ((index, tag) in listOf("glass", "plain", "item", "bar", "round", "pill").withIndex()) {
            compose.onNodeWithTag(tag, useUnmergedTree = true).performTouchInput { down(index, center) }
        }
        settle()
        compose.onNodeWithTag("page").captureRoboImage(file("pressed"))
    }
}
