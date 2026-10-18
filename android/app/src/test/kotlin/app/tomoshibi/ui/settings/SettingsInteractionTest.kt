package app.tomoshibi.ui.settings

import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.basics.Accent
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ConnectionBadge
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.AppModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import kotlin.math.abs

/**
 * The settings answering a finger: a colour chosen is the colour drawn, a
 * language chosen is the language read, the passphrase cannot be sent until
 * both fields say enough, and the list drills in and back out. Driven through
 * the composables, with a model that has nothing behind it.
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h2000dp-hdpi")
class SettingsInteractionTest {
    @get:Rule
    val compose = createComposeRule()

    private val platform = FakePlatform()

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val localization = Localization.install(context)
        localization.keeps = false
        localization.choose(Language.En)
        val appearance = Appearance.install(context)
        appearance.keeps = false
        appearance.choose(Accent.Amber)
        appearance.choose(ConnectionBadge.Signal)
        appearance.choose(ControlsPlacement.Always)
    }

    /** Whether the settings are on screen: taken away, they leave the composition as a closed page does. */
    private var shown by mutableStateOf(true)

    private fun show(opening: SettingsTopic?, app: AppModel = fakeApp(), close: () -> Unit = {}) {
        compose.setContent {
            if (shown) {
                CompositionLocalProvider(LocalSettingsPlatform provides platform) {
                    SettingsView(app, close, Modifier, opening)
                }
            }
        }
        settle()
    }

    /** A field with nothing typed in it, whatever its prompt says. */
    private val empty = SemanticsMatcher("nothing typed") { it.config.getOrNull(SemanticsProperties.EditableText)?.text == "" }

    /** The field inside the one tagged, where the words are typed. */
    private fun field(tag: String) = compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag(tag)))

    private fun settle() {
        compose.mainClock.advanceTimeBy(1200)
        compose.waitForIdle()
    }

    /**
     * How many of the preview's pixels are drawn in [colour]: the Join
     * button, the switch, the speaker's ring and the raised hand are all in
     * the chosen one.
     */
    private fun inPreview(colour: Color): Int {
        val pixels = compose.onNodeWithTag("settings.accent.preview").captureToImage().toPixelMap()
        var count = 0
        for (x in 0 until pixels.width) for (y in 0 until pixels.height) {
            val p = pixels[x, y]
            if (abs(p.red - colour.red) < 0.03f && abs(p.green - colour.green) < 0.03f && abs(p.blue - colour.blue) < 0.03f) count++
        }
        return count
    }

    @Test
    fun choosingAnAccentRedrawsWhatItIsSpentOn() {
        show(SettingsTopic.General)
        compose.onNodeWithTag("settings.accent.amber").assertIsSelected()
        val before = inPreview(Accent.Amber.color)
        assertTrue("the preview is not drawn in the first colour ($before pixels)", before > 2000)
        assertEquals(0, inPreview(Accent.Blue.color))

        compose.onNodeWithTag("settings.accent.blue").performClick()
        settle()
        assertEquals(Accent.Blue, Appearance.shared.accent)
        compose.onNodeWithTag("settings.accent.blue").assertIsSelected()
        compose.onNodeWithTag("settings.accent.amber").assertIsNotSelected()
        // Redrawn in the new colour, with nothing of the old one left.
        val after = inPreview(Accent.Blue.color)
        assertTrue("the preview was not redrawn in blue ($after pixels)", after > 2000)
        assertEquals(0, inPreview(Accent.Amber.color))
    }

    @Test
    fun aLanguageChosenIsTheLanguageRead() {
        show(SettingsTopic.General)
        compose.onNodeWithText("General").assertExists()
        compose.onNodeWithTag("settings.language.zh-Hans").performClick()
        settle()
        assertEquals(Language.ZhHans, Localization.shared.language)
        assertFalse(Localization.shared.followsSystem)
        compose.onNodeWithText("通用").assertExists()
        compose.onNodeWithText("语言").assertExists()
        compose.onAllNodesWithText("General").assertCountEquals(0)
        // The languages' own names are never translated.
        compose.onNodeWithTag("settings.language.en").assert(hasText("English"))
        compose.onNodeWithTag("settings.language.ja").assert(hasText("日本語"))
        compose.onNodeWithTag("settings.language.zh-Hans").assertIsSelected()

        // Back to the system's, which on this test's system is English.
        compose.onNodeWithTag("settings.language.system").performClick()
        settle()
        assertTrue(Localization.shared.followsSystem)
        compose.onNodeWithText("General").assertExists()
        compose.onNodeWithTag("settings.language.system").assertIsSelected()
        compose.onNodeWithTag("settings.language.en").assertIsNotSelected()
    }

    @Test
    fun thePassphraseIsSentOnlyWithSomethingNowAndEightNext() {
        show(SettingsTopic.Account)
        val change = compose.onNodeWithTag("settings.passphrase.change")
        change.performScrollTo().assertIsNotEnabled()

        field("settings.passphrase.next").performTextInput("seven77")
        settle()
        change.assertIsNotEnabled()
        field("settings.passphrase.current").performTextInput("the old one")
        settle()
        // Seven is one short.
        change.assertIsNotEnabled()
        field("settings.passphrase.next").performTextReplacement("eight888")
        settle()
        change.assertIsEnabled()
        field("settings.passphrase.current").performTextReplacement("")
        settle()
        change.assertIsNotEnabled()

        // Taken (a model with no deployment refuses nothing): the fields are
        // emptied and it says so.
        field("settings.passphrase.current").performTextReplacement("the old one")
        settle()
        change.assertIsEnabled().performClick()
        settle()
        compose.onNodeWithText("Changed.").assertExists()
        field("settings.passphrase.current").assert(empty)
        field("settings.passphrase.next").assert(empty)
        change.assertIsNotEnabled()
    }

    @Test
    fun theListDrillsInAndBackAndCloses() {
        var closed = 0
        show(null, close = { closed++ })
        compose.onNodeWithTag("settings.topic.music").performClick()
        settle()
        compose.onNodeWithText("Lower the music while anybody talks").assertExists()
        compose.onNodeWithTag("settings.back").performClick()
        settle()
        compose.onNodeWithText("Lower the music while anybody talks").assertDoesNotExist()
        compose.onNodeWithTag("settings.account").performClick()
        settle()
        compose.onNodeWithText("Ada Lovelace").assertExists()
        compose.onNodeWithTag("settings.back").performClick()
        settle()
        compose.onNodeWithTag("settings.close").performClick()
        assertEquals(1, closed)
    }

    @Test
    fun aSectionSlidesInOverTheListRatherThanAppearing() {
        show(null)
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("settings.topic.about").performClick()
        compose.mainClock.advanceTimeBy(100)
        // Partway: the list still leaving, the page on its way in from the side.
        compose.onNodeWithTag("settings.topic.about").assertExists()
        val partway = compose.onNodeWithTag("settings.back").getBoundsInRoot().left
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithTag("settings.topic.about").assertDoesNotExist()
        val there = compose.onNodeWithTag("settings.back").getBoundsInRoot().left
        assertTrue("the page did not slide in: $partway then $there", partway > there + 40.dp)
        compose.onNodeWithText("Media core").assertExists()
    }

    @Test
    @Config(qualifiers = "w840dp-h1400dp-hdpi")
    fun aWideScreenHasTheSectionsBesideThePage() {
        show(null)
        compose.onNodeWithTag("settings.section.general").assertIsSelected()
        compose.onNodeWithText("Accent colour").assertExists()
        compose.onNodeWithTag("settings.section.music").performClick()
        settle()
        compose.onNodeWithTag("settings.section.music").assertIsSelected()
        compose.onNodeWithTag("settings.section.general").assertIsNotSelected()
        compose.onNodeWithText("Lower the music while anybody talks").assertExists()
        compose.onNodeWithText("Accent colour").assertDoesNotExist()
    }

    @Test
    fun theMediaPageChecksTheDevicesWhileItShowsAndRemembersTheChoices() {
        val app = fakeApp()
        show(SettingsTopic.Media, app)
        assertTrue("the check did not start", platform.check.started)
        compose.onNodeWithTag("settings.start.microphone").performClick()
        settle()
        assertFalse(app.startWithMicrophone)
        // The page's sentence about picked devices arrives with the first one picked.
        compose.onNodeWithTag("settings.forget").assertDoesNotExist()
        app.preferences.choose("audioinput", "31")
        settle()
        assertEquals(2, platform.check.starts)
        compose.onNodeWithTag("settings.forget").performScrollTo().performClick()
        settle()
        assertTrue(app.preferences.devices.isEmpty())
        // Gone, the page stops the camera and the microphone.
        shown = false
        compose.waitForIdle()
        assertFalse("the check outlived its page", platform.check.started)
    }
}
