package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import uniffi.tomoshibi_protocol.ConnectionGrade
import uniffi.tomoshibi_protocol.ConnectionReading
import uniffi.tomoshibi_protocol.ShareLimitation
import java.util.Locale

/**
 * How the call is going, in the corner (components/room/Signal.tsx;
 * Signal.swift `SignalBadge`).
 *
 * Two readings of the same thing, and which one somebody wants depends on
 * why they looked: most of the time "is it me?", answered by bars; now and
 * then "what exactly is wrong?", which only the figures answer. The bars
 * alone by default, the figures with `detailed` (the owner's choice,
 * 2026-10-15), opened from the badge. `relay` is what the machine this call
 * was sent to is called (the join answer's `relay`); `holding`, where the
 * meeting is on another, that one's name (the web's `carrying`).
 */
@Composable
fun SignalBadge(
    reading: ConnectionReading,
    detailed: Boolean,
    open: Boolean,
    onOpenChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    relay: String? = null,
    holding: String? = reading.holding,
) {
    val title = when (reading.grade) {
        ConnectionGrade.LOST -> t("Connection lost")
        ConnectionGrade.POOR -> t("Poor connection")
        ConnectionGrade.FAIR -> t("Fair connection")
        ConnectionGrade.GOOD -> t("Good connection")
    }
    val press = rememberPress()
    val sides by animateDpAsState(if (detailed) 11.dp else 9.dp, Theme.settle(), label = "sides")
    Column(modifier, horizontalAlignment = Alignment.Start, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier
                .height(28.dp)
                .glassSurface(Capsule, interactive = detailed)
                .pressable(press, Capsule, true, if (detailed) Role.Button else null, title) {
                    if (detailed) onOpenChange(!open)
                }
                .padding(horizontal = sides),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Bars(reading.grade)
            AnimatedVisibility(detailed, enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                KitText(reading.rttMs?.let { "${it.toInt()} ms" } ?: "—", font = Font(11.5f, Font.Medium, digits = true),
                    color = Theme.muted, lines = 1)
            }
        }
        AnimatedVisibility(
            detailed && open,
            enter = scaleIn(Theme.settle(), initialScale = 0.94f, transformOrigin = TransformOrigin(1f, 0f)),
            exit = ExitTransition.None,
        ) {
            val fade by glassFade()
            Detail(reading, relay, holding, Modifier.glassLayer({ fade }))
        }
    }
}

/**
 * Three bars, as a phone draws them: colour and height say the same thing,
 * which keeps it legible to somebody who cannot tell the colours apart.
 */
@Composable
private fun Bars(grade: ConnectionGrade) {
    val lit = when (grade) {
        ConnectionGrade.GOOD -> 3
        ConnectionGrade.FAIR -> 2
        ConnectionGrade.POOR -> 1
        ConnectionGrade.LOST -> 0
    }
    val colour = when (grade) {
        ConnectionGrade.GOOD -> Theme.good
        ConnectionGrade.FAIR -> Theme.tally
        else -> Theme.danger
    }
    Row(Modifier.clearAndSetSemantics {}, horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.Bottom) {
        for ((index, height) in listOf(6, 10, 14).withIndex()) {
            val fill by animateColorAsState(
                if (index < lit) colour else Theme.muted.copy(alpha = Theme.muted.alpha * 0.35f),
                Theme.settle(), label = "bar",
            )
            Box(Modifier.size(3.5.dp, height.dp).background(fill, RoundedCornerShape(1.dp)))
        }
    }
}

/**
 * The figures, as Signal.tsx `Detail` lists them, each arriving a moment
 * after the one above it, from under the bars they belong to: a panel that
 * is simply there on the next frame reads as the screen having redrawn itself.
 */
@Composable
private fun Detail(reading: ConnectionReading, relay: String?, holding: String?, modifier: Modifier) {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    Column(
        modifier
            // A fixed width: left to itself the row's spacer took the whole
            // of the room's width, and the figures ended up under the side panel.
            .width(236.dp)
            .glassSurface(rounded(14f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        // Said by the server, which picked both machines, and by nothing else
        // (Signal.tsx `relayed`): a second name arrives only when there is a
        // second machine, so its absence is the answer.
        if (!relay.isNullOrEmpty() && !holding.isNullOrEmpty()) {
            Figure(shown, 0f, t("Relay server"), say(relay))
            Figure(shown, 1f, t("Original server"), say(holding))
        } else if (!relay.isNullOrEmpty()) {
            Figure(shown, 0f, t("Server"), say(relay))
        }
        reading.ownAddress?.let { Figure(shown, 2f, t("Your address"), it) }
        Figure(shown, 3f, t("Round trip"), reading.rttMs?.let { "${it.toInt()} ms" } ?: "—")
        Figure(shown, 4f, t("Lost"), reading.lossPercent?.let { String.format(Locale.ROOT, "%.1f%%", it) } ?: "—",
            warn = (reading.lossPercent ?: 0.0) > 2)
        Figure(shown, 5f, t("Jitter"), reading.jitterMs?.let { "${it.toInt()} ms" } ?: "—")
        reading.share?.let { share ->
            val height = share.height
            val fps = share.fps
            Figure(
                shown, 6f,
                if (share.sending) t("Sharing") else t("Watching"),
                if (height != null && fps != null) "${height.toInt()}p · ${fps.toInt()} fps" else t("Measuring…"),
                warn = (share.fps ?: 30.0) < 10,
            )
            share.limited?.let { limited ->
                Figure(
                    shown, 6.5f, t("Held back by"),
                    when (limited) {
                        ShareLimitation.CPU -> t("this machine")
                        ShareLimitation.BANDWIDTH -> t("the connection")
                        ShareLimitation.OTHER -> t("something the browser will not name")
                    },
                    warn = true,
                )
            }
        }
        Figure(shown, 7f, t("Sending"), rate(reading.upKbps))
        Figure(shown, 8f, t("Receiving"), rate(reading.downKbps))
        if (!reading.measured) {
            val alpha by animateFloatAsState(if (shown) 1f else 0f, tween(320, 270, Theme.settleEasing), label = "measuring")
            KitText(t("Measuring…"), Modifier.graphicsLayer { this.alpha = alpha }, font = Font(11.5f), color = Theme.muted)
        }
    }
}

/**
 * One line, `order` places down the list: thirty milliseconds apart, as the
 * web staggers its `animate-arrive`. The value changes colour rather than
 * snapping, because these numbers change while somebody reads them.
 */
@Composable
private fun Figure(shown: Boolean, order: Float, label: String, value: String, warn: Boolean = false) {
    val spec = tween<Float>(320, (order * 30).toInt(), Theme.settleEasing)
    val alpha by animateFloatAsState(if (shown) 1f else 0f, spec, label = "figure")
    val drop by animateFloatAsState(if (shown) 0f else -4f, spec, label = "drop")
    val ink by animateColorAsState(if (warn) Theme.tallyInk else Theme.foreground, tween(300, easing = Motion.easeOut), label = "warn")
    Row(
        Modifier
            .fillMaxWidth()
            .graphicsLayer {
                this.alpha = alpha
                translationY = drop.dp.toPx()
            },
        verticalAlignment = Alignment.Bottom,
    ) {
        KitText(label, font = Font(11.5f), color = Theme.muted, lines = 1)
        Spacer(Modifier.weight(1f).padding(start = 16.dp))
        Box(Modifier.padding(start = 16.dp)) {
            KitText(value, font = Font(11.5f, digits = true), color = ink, lines = 1)
        }
    }
}

private fun rate(kbps: Double?): String {
    kbps ?: return "—"
    return if (kbps >= 1000) String.format(Locale.ROOT, "%.1f Mbps", kbps / 1000) else "${Math.round(kbps)} kbps"
}
