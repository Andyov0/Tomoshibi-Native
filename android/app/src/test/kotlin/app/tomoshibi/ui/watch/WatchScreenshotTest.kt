package app.tomoshibi.ui.watch

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.feature.watch.TestDeployment
import app.tomoshibi.feature.watch.TestMachinery
import app.tomoshibi.feature.watch.WatchEngine
import app.tomoshibi.feature.watch.WatchInfo
import app.tomoshibi.feature.watch.WatchPlayer
import app.tomoshibi.feature.watch.callModel
import app.tomoshibi.feature.watch.resolved
import app.tomoshibi.feature.watch.show
import app.tomoshibi.feature.watch.video
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.glassSource
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.Dispatchers
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.tomoshibi_protocol.MediaItem
import uniffi.tomoshibi_protocol.MediaServer
import uniffi.tomoshibi_protocol.ResolvedPlayKind

/**
 * Pictures of the watch panel and the watch stage in their states, drawn
 * from a show stood in for (nothing of the core is loaded), at a phone's
 * width and at a tablet's, in English and in simplified Chinese. The panel is
 * as wide as the room makes it beside the call (RoomView.swift
 * `panelWidth`): the screen less its margins on a phone, 360 points beside
 * the stage on a tablet. Recorded with `./gradlew :app:recordRoborazziDebug`
 * into src/test/screenshots/watch.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h900dp-hdpi")
@OptIn(ExperimentalRoborazziApi::class)
class WatchScreenshotTest(private val language: Language, private val width: Int) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}-{1}")
        fun cases() = listOf(Language.En, Language.ZhHans).flatMap { language -> listOf(412, 840).map { arrayOf<Any>(language, it) } }

        const val DIRECTORY = "src/test/screenshots/watch"
    }

    @get:Rule
    val compose = createComposeRule()

    private val deployment = TestDeployment()
    private val machinery = TestMachinery()

    @Before
    fun setUp() {
        RuntimeEnvironment.setQualifiers("w${width}dp-h${if (width > 600) 1100 else 900}dp-hdpi")
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
    }

    private fun file(name: String) = "$DIRECTORY/$name-${language.code}-$width.png"

    private val panelWidth: Dp get() = if (width < 360 + 24 + 172 + 22) (width - 24).dp else 360.dp

    private fun engine(sealed: Boolean = false): WatchEngine {
        val model = callModel(deployment = deployment, context = Dispatchers.Main.immediate, sealed = sealed)
        return WatchEngine(model, machinery, live = false)
    }

    private fun page(content: @Composable () -> Unit) {
        compose.setContent {
            GlassBackdrop {
                Box(Modifier.fillMaxWidth().testTag("page")) {
                    Box(Modifier.matchParentSize().background(Theme.background))
                    Backdrop(Modifier.matchParentSize().glassSource())
                    Box(Modifier.padding(12.dp)) { content() }
                }
            }
        }
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
    }

    private fun shoot(name: String) {
        compose.onNodeWithTag("page").captureRoboImage(file(name))
    }

    // MARK: The panel

    private fun panel(engine: WatchEngine, name: String, start: WatchTab = WatchTab.Queue) {
        page { WatchPanelContent(engine, close = {}, modifier = Modifier.width(panelWidth), start = start) }
        shoot(name)
    }

    @Test
    fun panelEmptyForSomebodyWhoMayStartAShow() {
        val engine = engine()
        engine.ready = true
        panel(engine, "panel-empty")
    }

    @Test
    fun panelEmptyForAGuest() = panel(engine(), "panel-guest")

    @Test
    fun panelPlayingWithAQueue() {
        val engine = engine()
        engine.ready = true
        engine.mine = true
        val now = video("now", "t-now", by = "Ada")
        val next = video("next", "t-next", by = "Bea", duration = 1_312.0)
        val gone = video("gone", "t-gone", by = "Cy")
        val failed = video("failed", "t-failed", by = "Dee")
        engine.show = show(now, listOf(next, gone, failed))
        engine.onStage = true
        engine.infos = mapOf(
            "t-now" to WatchInfo.Known(resolved("t-now", title = "Sherlock Jr. (1924)")),
            "t-next" to WatchInfo.Known(resolved("t-next", title = "Big Buck Bunny")),
            "t-gone" to WatchInfo.Unavailable,
            "t-failed" to WatchInfo.Failed,
        )
        panel(engine, "panel-queue")
    }

    @Test
    fun panelBrowsingAMediaServer() {
        deployment.servers = listOf(MediaServer("emby", "Home"), MediaServer("jelly", "Cabin"))
        deployment.media = listOf(
            MediaItem("1", "Films", "CollectionFolder", true, false, 0u, 0.0, ""),
            MediaItem("2", "Series", "CollectionFolder", true, false, 0u, 0.0, ""),
            MediaItem("3", "The General", "Movie", false, true, 1926u, 4_620.0, ""),
            MediaItem("4", "Nosferatu", "Movie", false, true, 1922u, 5_640.0, ""),
            MediaItem("5", "A trailer that will not play", "Trailer", false, false, 0u, 0.0, ""),
        )
        val engine = engine()
        engine.ready = true
        engine.servers = deployment.servers
        panel(engine, "panel-browsing", start = WatchTab.Media)
    }

    @Test
    fun panelInASealedCall() = panel(engine(sealed = true), "panel-sealed")

    // MARK: The stage

    private val stageWidth: Dp get() = (width - 24).dp
    private val stageHeight: Dp get() = stageWidth * 9 / 16

    private fun stage(engine: WatchEngine, name: String, compact: Boolean = false, touch: Boolean = false) {
        page {
            Box(Modifier.testTag("stage").let { if (compact) it.size(160.dp, 90.dp) else it.size(stageWidth, stageHeight) }) {
                WatchStageContent(engine, Modifier.matchParentSize())
            }
        }
        if (touch) {
            compose.onNodeWithTag("stage").performTouchInput {
                down(center)
                up()
            }
            compose.mainClock.advanceTimeBy(600)
            compose.waitForIdle()
        }
        shoot(name)
    }

    /** A show with a video in it whose player is built and holding to it, as the engine would have it. */
    private fun playing(
        playing: Boolean = false,
        info: uniffi.tomoshibi_protocol.ResolvedVideo = resolved("t-now", title = "Sherlock Jr. (1924)"),
        configure: (WatchPlayer) -> Unit = {},
    ): WatchEngine {
        val engine = engine()
        val now = video("now", info.ticket, by = "Ada", duration = info.duration)
        engine.show = show(now, playing = playing, position = 1_234.0)
        engine.onStage = true
        engine.infos = mapOf(info.ticket to WatchInfo.Known(info))
        val player = WatchPlayer(now, info, engine, machinery, "https://meet.example", false)
        player.waiting = false
        player.position = 1_234.0
        configure(player)
        engine.player = player
        return engine
    }

    @Test
    fun stageWithNothingPlaying() {
        val engine = engine()
        engine.show = show(null)
        engine.onStage = true
        stage(engine, "stage-nothing")
    }

    @Test
    fun stagePausedWithItsControls() = stage(playing(playing = false), "stage-paused")

    @Test
    fun stagePlayingAfterATouch() = stage(playing(playing = true, info = resolved("t-now", title = "Sherlock Jr. (1924)", relay = true)), "stage-playing", touch = true)

    @Test
    fun stageWaitingForThePicture() = stage(playing(playing = true) { it.waiting = true }, "stage-waiting")

    @Test
    fun stageWhereTheConnectionCannotReachIt() = stage(playing(playing = true) { it.unreachable = WatchPlayer.Lost.Network }, "stage-unreachable")

    @Test
    fun stageWhereTheSiteRefusedIt() = stage(
        playing(playing = true, info = resolved("t-y", title = "A music video", kind = ResolvedPlayKind.YOUTUBE, url = null, id = "dQw4",
            proxy = null, link = "https://www.youtube.com/watch?v=dQw4")) { it.unreachable = WatchPlayer.Lost.Refused },
        "stage-refused",
    )

    @Test
    fun stageWithAKindOfVideoThisAppCannotOpen() = stage(playing(playing = true) { it.unreachable = WatchPlayer.Lost.Format }, "stage-format")

    @Test
    fun stageWhereTheRelayStopped() = stage(playing(playing = true) { it.halted = WatchPlayer.Halted.Stopped }, "stage-stopped")

    @Test
    fun stageReconnectingToTheRelay() = stage(playing(playing = false) { it.halted = WatchPlayer.Halted.Resuming }, "stage-resuming")

    @Test
    fun stageWithAVideoThatCannotBePlayedHere() {
        val engine = engine()
        engine.show = show(video("now", "t-now"))
        engine.onStage = true
        engine.infos = mapOf("t-now" to WatchInfo.Unavailable)
        stage(engine, "stage-cannot")
    }

    @Test
    fun stageAsATileInTheStrip() = stage(playing(playing = true), "stage-tile", compact = true)
}
