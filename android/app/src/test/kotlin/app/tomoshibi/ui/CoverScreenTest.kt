package app.tomoshibi.ui

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.toSize
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.AppModel.Screen
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.host.standInRules
import app.tomoshibi.ui.kit.HOLDS_PRESSES
import app.tomoshibi.ui.lobby.LobbyDebug
import app.tomoshibi.ui.lobby.PlainRoomNames
import app.tomoshibi.ui.lobby.TestApp
import app.tomoshibi.ui.lobby.fakeApp
import app.tomoshibi.ui.lobby.invitation
import app.tomoshibi.ui.lobby.stage
import app.tomoshibi.ui.room.COVER
import app.tomoshibi.ui.room.FakeDeployment
import app.tomoshibi.ui.room.LEAVE
import app.tomoshibi.ui.room.LocalRoomPictures
import app.tomoshibi.ui.room.MORE
import app.tomoshibi.ui.room.PEOPLE
import app.tomoshibi.ui.room.RoomPanel
import app.tomoshibi.ui.room.RoomState
import app.tomoshibi.ui.room.RoomView
import app.tomoshibi.ui.room.SHARE
import app.tomoshibi.ui.room.callModel
import app.tomoshibi.ui.room.joins
import app.tomoshibi.ui.room.standInPictures
import app.tomoshibi.ui.settings.FakeCameras
import app.tomoshibi.ui.settings.FakePlatform
import app.tomoshibi.ui.settings.LocalCallCameras
import app.tomoshibi.ui.settings.LocalSettingsPlatform
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.tomoshibi_core.CallEvent
import java.io.File
import java.util.Locale
import java.util.TimeZone

/**
 * The Fold6's cover screen, 372 points across, in each of the four
 * languages: no word the app says cut off with an ellipsis where it should
 * have been read whole, and nothing to press smaller than a fingertip,
 * which Android's guidance puts at 48 points. Compose already counts a press
 * that lands within 48 points of a smaller control as a press on it where
 * nothing else is nearer (the view configuration's minimum touch target),
 * so a control drawn at the iPhone's 44 points is a target of 48 as long as
 * nothing else to press lies within its 48; what this checks is that
 * nothing does.
 *
 * Every screen and sheet, the call with each of its panels and popovers.
 * What is found is written to build/reports/cover (one file a language) as
 * well as asserted on, so that a failure says everything at once.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = COVER)
class CoverScreenTest(private val language: Language) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = Language.entries.map { arrayOf<Any>(it) }

        /**
         * Where a cut is the design, as it is on the iPhone: a link, a room
         * held under a scope, and a person's name on a picture, which is as
         * wide as the picture and no wider whatever the name is.
         */
        private val MEANT_TO_CUT = listOf(
            Regex("https?://.*"), Regex(".*@.*"), Regex(".*Andy.*"),
        ) + PEOPLE.map { Regex(".*" + Regex.escape(it.second) + ".*") }

        /**
         * A month's days, seven to a row across the schedule's card on the
         * cover screen, are as wide as that leaves them, forty points side
         * by side, and 44 tall (MonthCalendar): a seven-day week cannot be
         * seven fingertips across a screen 372 points wide inside a card.
         */
        private val DAYS = Regex(".*\\b20\\d\\d\\b.*|.*\\d+年.*")
        private const val DAY_WIDE = 38f
        private const val DAY_TALL = 44f
    }

    @get:Rule
    val compose = createComposeRule()

    private val found = ArrayList<String>()

    @Before
    fun setUp() {
        standInRules()
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Shanghai"))
        Locale.setDefault(Locale.US)
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
        Appearance.shared.choose(ControlsPlacement.Always)
        LobbyDebug.open = null
        LobbyDebug.popover = null
        LobbyDebug.section = null
        PlainRoomNames.install()
    }

    @After
    fun tearDown() {
        PlainRoomNames.remove()
        LobbyDebug.open = null
        LobbyDebug.popover = null
        LobbyDebug.section = null
    }

    private fun settle() {
        compose.mainClock.advanceTimeBy(2500)
        compose.waitForIdle()
    }

    /** The screen being looked at, one after another in the one composition a test may set. */
    private var scene by mutableStateOf<Pair<String, @Composable () -> Unit>>("" to {})
    private var started = false

    private fun screen(name: String, content: @Composable () -> Unit, before: () -> Unit = {}) {
        if (!started) {
            started = true
            compose.setContent {
                CompositionLocalProvider(LocalSettingsPlatform provides FakePlatform(), LocalCallCameras provides FakeCameras, LocalRoomPictures provides standInPictures) {
                    val (key, shown) = scene
                    key(key) { shown() }
                }
            }
        }
        scene = name to content
        settle()
        before()
        settle()
        look(name)
    }

    private fun app(name: String, app: AppModel) = screen(name, { TestApp(app) })

    private fun opening(part: String, section: String? = null): AppModel {
        LobbyDebug.open = part
        LobbyDebug.section = section
        return fakeApp()
    }

    /** Everything on every window of the screen, merged as a screen reader reads it and as drawn. */
    private fun nodes(merged: Boolean): List<SemanticsNode> =
        compose.onAllNodes(SemanticsMatcher("anything") { true }, useUnmergedTree = !merged).fetchSemanticsNodes(atLeastOneRootRequired = false)

    private fun look(name: String) {
        val density = compose.density.density
        // Words cut off.
        for (node in nodes(merged = false)) {
            val layout = node.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action ?: continue
            val results = ArrayList<TextLayoutResult>()
            if (layout(results) != true) continue
            val result = results.firstOrNull() ?: continue
            if (result.lineCount == 0) continue
            // Cut where the words needed more lines than they were given, or
            // were cut at the end of the line they were given; not where a
            // line drawn whole is narrower than the paragraph it was laid out in.
            val cut = result.isLineEllipsized(result.lineCount - 1) || result.multiParagraph.didExceedMaxLines
            if (!cut) continue
            val text = result.layoutInput.text.text
            if (node.boundsInRoot.width <= 0f || node.boundsInRoot.height <= 0f) continue
            if (node.config.getOrNull(SemanticsProperties.EditableText) != null) continue
            if (MEANT_TO_CUT.any { it.matches(text) }) continue
            found += "$name: cut off: \"$text\" (${result.lineCount} of ${result.layoutInput.maxLines} lines, ${result.size} in ${result.layoutInput.constraints})"
        }
        // Targets smaller than a fingertip with something else to press within the fingertip's reach.
        // Only what is on the screen, at its own size rather than the part
        // a scrolling page leaves showing at its edge.
        val presses = nodes(merged = true).filter {
            it.config.getOrNull(SemanticsActions.OnClick) != null && it.boundsInRoot.width > 1f && it.boundsInRoot.height > 1f
        }
        fun whole(node: SemanticsNode) = Rect(node.positionOnScreen, node.size.toSize())
        val least = 48f * density
        for (node in presses) {
            val bounds = whole(node)
            if (bounds.width >= least - 0.5f && bounds.height >= least - 0.5f) continue
            if (DAYS.matches(describe(node)) && bounds.width >= DAY_WIDE * density - 0.5f && bounds.height >= DAY_TALL * density - 0.5f) continue
            // A pixel short of the fingertip either way, so that two
            // controls exactly a fingertip apart do not count as touching.
            val reach = Rect(
                bounds.center.x - maxOf(bounds.width, least) / 2 + 1, bounds.center.y - maxOf(bounds.height, least) / 2 + 1,
                bounds.center.x + maxOf(bounds.width, least) / 2 - 1, bounds.center.y + maxOf(bounds.height, least) / 2 - 1,
            )
            // Only what a press there could reach instead: on the same window
            // (a popover's is its own, and takes every press on it), and
            // inside whatever holds the presses made on it (a bar or a
            // panel over the call, kit `holdsPresses`), not under it.
            val holder = generateSequence(node.parent) { it.parent }.firstOrNull { it.config.getOrNull(SemanticsProperties.TestTag) == HOLDS_PRESSES }
            val window = root(node)
            val crowded = presses.filter {
                val other = whole(it)
                it.id != node.id && root(it) == window && (holder == null || inside(it, holder)) &&
                    other.overlaps(reach) && !other.contains(bounds) && !bounds.contains(other)
            }
            if (crowded.isEmpty()) continue
            val label = describe(node)
            val near = crowded.joinToString { describe(it) }
            found += "$name: small target ${"%.0f".format(bounds.width / density)}x${"%.0f".format(bounds.height / density)} \"$label\", within reach of $near"
        }
    }

    private fun root(node: SemanticsNode): Int = generateSequence(node) { it.parent }.last().id

    private fun inside(node: SemanticsNode, holder: SemanticsNode): Boolean = generateSequence(node.parent) { it.parent }.any { it.id == holder.id }

    private fun Rect.contains(other: Rect) = left <= other.left && top <= other.top && right >= other.right && bottom >= other.bottom

    private fun describe(node: SemanticsNode): String =
        node.config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString()
            ?: node.config.getOrNull(SemanticsProperties.Text)?.joinToString { it.text }
            ?: node.config.getOrNull(SemanticsProperties.TestTag)
            ?: "?"

    private fun room(people: Int, deployment: FakeDeployment = FakeDeployment()): CallModel {
        val call = callModel(deployment = deployment)
        PEOPLE.take(people).forEach { (id, name) -> call.joins(id, name) }
        call.settled = true
        return call
    }

    private fun call(name: String, call: CallModel, before: () -> Unit = {}) {
        val app = app.tomoshibi.ui.room.app()
        screen(name, { RoomView(app, call) }, before)
    }

    @Test
    fun everyScreen() {
        app("sign-in", fakeApp(signedIn = false))
        app("lobby", fakeApp())
        app("sheet-start", opening("start"))
        app("sheet-join", opening("join"))
        app("schedule", opening("scheduled"))
        app("servers", opening("servers"))
        app("settings", opening("settings"))
        for (section in listOf("general", "media", "watching", "music", "account", "about")) app("settings-$section", opening("settings", section))
        app("unreachable", fakeApp(signedIn = false, screen = Screen.Unreachable))
        app("prejoin", fakeApp(screen = Screen.Prejoin).also { it.room = "design-review" })
        app("prejoin-invited", fakeApp(signedIn = false, screen = Screen.Prejoin).also {
            it.stage("invitation", invitation(name = "Grace"))
            it.room = "design-review"
        })
        app("knocking", fakeApp(screen = Screen.Knocking).also { it.room = "design-review" })
        app("ended", fakeApp(screen = Screen.Ended).also { it.room = "design-review" })

        call("call", room(3))
        call("call-two", room(1))
        for (panel in RoomPanel.entries) {
            val one = room(3, FakeDeployment(hosting = true))
            if (panel == RoomPanel.Messages) one.apply(CallEvent.Chat("1", "p-mei", "Mei Lin", "Slide 4 is the new one.", 1_760_000_030_000))
            RoomState.of(one).panel = panel
            call("panel-${panel.name.lowercase()}", one)
        }
        for (popover in listOf(SHARE, LEAVE, MORE, app.tomoshibi.ui.call.TRAY)) {
            val one = room(3, FakeDeployment(hosting = true))
            call("popover-$popover", one) { RoomState.of(one).popovers.show(popover) }
        }

        val report = File("build/reports/cover/${language.code}.txt")
        report.parentFile.mkdirs()
        report.writeText(found.joinToString("\n", postfix = "\n"))
        assertTrue("On the cover screen in ${language.code}:\n" + found.joinToString("\n"), found.isEmpty())
    }
}
