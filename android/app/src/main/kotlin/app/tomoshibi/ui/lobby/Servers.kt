package app.tomoshibi.ui.lobby

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.SettingsSection
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.launch

/**
 * Every relay grouped by region with its round trip from here, and the
 * automatic choice above them (ServerPicker.tsx `Rows`; `ServersView`), as
 * the app's own cards rather than a system list. On a phone a page pushed
 * over the lobby with a way back; on a wide screen a section of the window.
 */
@Composable
fun ServersView(app: AppModel, wide: Boolean, back: () -> Unit, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    LaunchedEffect(Unit) { if (app.lobby.measured == null) app.lobby.measure() }
    Box(
        modifier
            .fillMaxSize()
            // Pushed over the lobby, a press anywhere on the page is the
            // page's: Compose hands a press on a part that holds none to what
            // is under it, the lobby's buttons out of sight.
            .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } },
    ) {
        // On a phone the page is pushed over the lobby and brings its own
        // backdrop; in a wide window it stands on the window's, which the
        // sidebar floats over too, rather than drawing a second over it with
        // a seam where it begins.
        if (!wide) {
            Box(Modifier.matchParentSize().background(Theme.background))
            Backdrop(Modifier.matchParentSize().glassSource())
        }
        ContentLayer {
            Column(
                Modifier
                    .fillMaxSize()
                    // What scrolls under the bar is what its glass shows.
                    .glassSource(1f)
                    .verticalScroll(rememberScrollState())
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                if (!wide) {
                    Spacer(Modifier.windowInsetsTopHeight(WindowInsets.safeDrawing))
                    Spacer(Modifier.height(44.dp))
                } else {
                    Spacer(Modifier.windowInsetsTopHeight(WindowInsets.safeDrawing))
                }
                Column(
                    Modifier
                        .widthIn(max = 680.dp)
                        .fillMaxWidth()
                        .padding(if (wide) 28.dp else 16.dp),
                    verticalArrangement = Arrangement.spacedBy(20.dp),
                ) {
                    Row(Modifier.fillMaxWidth().rise(), verticalAlignment = Alignment.CenterVertically) {
                        if (wide) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                KitText(t("Server"), font = Font(26f, Font.Bold))
                                FleetLine(app)
                            }
                        } else {
                            Box(Modifier.weight(1f)) { FleetLine(app) }
                        }
                        GlassButton(
                            { scope.launch { app.lobby.measure() } },
                            shape = GlassButtonShape.Capsule(32.dp),
                            enabled = !app.lobby.measuring,
                        ) {
                            Symbol("arrow.clockwise", size = 12.5f, weight = Font.Semibold)
                            KitText(t("Measure again"), font = Font(12.5f, Font.Semibold), lines = 1)
                        }
                    }
                    SettingsSection(t("Automatic"), Modifier.rise(40)) {
                        Rows {
                        val detail = app.lobby.fastest?.let { "${t("Whichever answers fastest")} · ${app.lobby.label(it.name)}" } ?: t("Whichever answers fastest")
                        RelayRow(app, t("Automatic"), detail, "wand.and.stars", app.relayChoice == null, measured = null, off = false, last = true) {
                            app.relayChoice = null
                        }
                        }
                    }
                    for (group in app.lobby.grouped()) {
                        SettingsSection(group.region.ifEmpty { t("Server") }, Modifier.rise(80)) {
                            Rows {
                            for (relay in group.relays) {
                                val named = say(relay.label ?: relay.name)
                                RelayRow(
                                    app,
                                    if (relay.adminOnly) "$named [Admin]" else named,
                                    if (relay.maintenance) t("Not taking calls") else if (relay.fallback) "[Fallback]" else null,
                                    null,
                                    app.relayChoice == relay.name,
                                    measured = app.lobby.measured?.let { Measured(it[relay.name]) },
                                    off = relay.maintenance,
                                    last = relay.name == group.relays.lastOrNull()?.name,
                                ) { app.relayChoice = relay.name }
                            }
                            }
                        }
                    }
                }
            }
        }
        if (!wide) PageBar(t("Server"), back)
    }
}

/**
 * A card's rows held to its rounded corners, so that the chosen one's tint
 * does not stand square out of them (SettingsSection does not clip).
 */
@Composable
private fun Rows(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(rounded(16f))) { content() }
}

/** A round trip as measured: `millis` null for a relay that did not answer. */
private class Measured(val millis: Double?)

/**
 * One relay, or the automatic choice. `measured` is null when nothing has
 * been measured yet, and holds no number for a relay that did not answer.
 */
@Composable
private fun RelayRow(
    app: AppModel,
    title: String,
    detail: String?,
    symbol: String?,
    chosen: Boolean,
    measured: Measured?,
    off: Boolean,
    last: Boolean,
    pick: () -> Unit,
) {
    val tick by animateFloatAsState(if (chosen) 1f else 0f, Theme.settle(), label = "chosen")
    val ground by animateColorAsState(if (chosen) Theme.tally.copy(alpha = 0.08f) else Color.Transparent, Theme.settle(), label = "ground")
    SettingsRow(
        title,
        Modifier
            .alpha(if (off) 0.45f else 1f)
            .background(ground)
            .plainPress(pick, enabled = !off),
        detail = detail,
        symbol = symbol ?: "server.rack",
        last = last,
    ) {
        if (symbol == null) {
            val millis = measured?.millis
            when {
                measured == null -> if (app.lobby.measuring) MediaSpinner(size = 12f, tint = Theme.muted)
                millis != null -> KitText("${Math.round(millis)} ms", font = Font(13f, digits = true), color = latencyColour(millis), lines = 1)
                !off -> KitText(t("Timeout"), font = Font(12f), color = Theme.danger, lines = 1)
            }
        }
        Symbol("checkmark.circle.fill", Modifier.alpha(tick).scale(0.6f + 0.4f * tick), size = 15f, tint = Theme.tally)
    }
}

/**
 * The top of a page pushed over the phone's lobby, as the system's inline
 * navigation bar draws it: the way back in a circle of glass, and the page's
 * name in the middle, over the page fading into the status bar.
 */
@Composable
internal fun PageBar(title: String, back: () -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.55f), Color.Transparent))),
        ) {
            Column(Modifier.fillMaxWidth()) {
                Spacer(Modifier.windowInsetsTopHeight(WindowInsets.safeDrawing))
                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(52.dp)
                        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))
                        .padding(horizontal = 16.dp),
                ) {
                    GlassButton(back, Modifier.align(Alignment.CenterStart), shape = GlassButtonShape.Circle(40.dp), label = t("Back")) {
                        Symbol("chevron.left", size = 16f, weight = Font.Semibold)
                    }
                    KitText(title, Modifier.align(Alignment.Center), font = Font(17f, Font.Semibold), lines = 1)
                }
            }
        }
    }
}
