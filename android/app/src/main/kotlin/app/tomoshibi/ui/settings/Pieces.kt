package app.tomoshibi.ui.settings

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.Hairline
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LocalEnabled
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.SettingsSection
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.barPress
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.delay

// The small pieces the settings are drawn from that the kit does not have:
// the product's mark, the rise every part of these screens arrives with, a
// row whose control goes under its words where the two will not fit side by
// side, and a section whose line under it arrives and leaves.

/**
 * The product's own mark, as the favicon and the app icon draw it
 * (Lobby.swift `Mark`): four tiles on a dark square, the last one lit.
 */
@Composable
internal fun Mark(modifier: Modifier = Modifier, size: Dp = 20.dp) {
    Canvas(modifier.size(size).clearAndSetSemantics {}) { drawMark() }
}

private fun DrawScope.drawMark() {
    val unit = size.width / 32
    drawOutline(rounded(8f * unit / density).createOutline(size, layoutDirection, this), MarkGround)
    val inner = unit * 0.86f
    val offset = (size.width - 32 * inner) / 2
    for ((x, y, live) in listOf(Triple(6f, 8f, false), Triple(17f, 8f, false), Triple(6f, 17f, false), Triple(17f, 17f, true))) {
        drawRoundRect(
            if (live) MarkLive else MarkIdle,
            Offset(offset + x * inner, offset + y * inner),
            Size(9 * inner, 7 * inner),
            CornerRadius(2 * inner),
        )
    }
}

private val MarkGround = Color(0x0F / 255f, 0x0E / 255f, 0x0D / 255f)
private val MarkIdle = Color(0x55 / 255f, 0x52 / 255f, 0x4E / 255f)
private val MarkLive = Color(0xFF / 255f, 0xA2 / 255f, 0x3B / 255f)

/**
 * Everything on these screens arrives in an order: in from fourteen points
 * below, over 420 ms on the arriving curve, `delay` milliseconds after the
 * screen does (Lobby.swift `rise`). Faded on each thing drawn rather than as
 * a layer: a layer made translucent the ordinary way is drawn offscreen and
 * cut the shadow of any glass inside it off square (Glass.kt `glassLayer`).
 */
@Composable
internal fun Modifier.rise(delayMillis: Int = 0): Modifier {
    val shown = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        if (delayMillis > 0) delay(delayMillis.toLong())
        shown.animateTo(1f, tween(420, easing = Theme.settleEasing))
    }
    return graphicsLayer {
        alpha = shown.value
        translationY = (1f - shown.value) * 14.dp.toPx()
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }
}

/**
 * A settings card under a heading, with a line under it that arrives and
 * leaves (the kit's [SettingsSection], whose line is simply there or not):
 * the footer of the devices card comes with the first device picked, and
 * one that appeared between two frames would be the one thing on the page
 * that pops.
 */
@Composable
internal fun Section(
    title: String,
    modifier: Modifier = Modifier,
    footer: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    // The last words said, kept while they leave.
    var said by remember { mutableStateOf(footer) }
    if (footer != null) said = footer
    Column(modifier.fillMaxWidth()) {
        SettingsSection(title, content = content)
        AnimatedVisibility(
            footer != null,
            enter = Theme.arrive + expandVertically(Theme.settle()),
            exit = Theme.depart + shrinkVertically(Theme.leave()),
        ) {
            KitText(said.orEmpty(), Modifier.padding(start = 6.dp, end = 6.dp, top = 8.dp), font = Font(11.5f), color = Theme.muted)
        }
    }
}

/** A row that arrives and leaves inside a card, its room opening and closing with it. */
@Composable
internal fun ColumnScope.Arriving(visible: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(
        visible,
        enter = Theme.arrive + expandVertically(Theme.settle()),
        exit = Theme.depart + shrinkVertically(Theme.leave()),
    ) { content() }
}

/**
 * A settings row whose control goes under its words where the two will not
 * fit side by side, as on a phone held upright (Settings.swift
 * `AdaptiveRow`, a `ViewThatFits`). Laid out here rather than by composing
 * both arrangements and keeping the one that fits: the control is a choice
 * that opens a popover, and a second, unshown copy of it would open its
 * popover too.
 */
@Composable
internal fun AdaptiveRow(
    title: String,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    last: Boolean = false,
    control: @Composable () -> Unit,
) {
    val lead = if (symbol == null) 14.dp else 52.dp
    Column(modifier.fillMaxWidth()) {
        Layout(
            content = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (symbol != null) {
                        Box(
                            Modifier
                                .padding(end = 12.dp)
                                .size(26.dp)
                                .background(Color.White.copy(alpha = 0.10f), rounded(7f)),
                            contentAlignment = Alignment.Center,
                        ) { Symbol(symbol, size = 12.5f, weight = Font.Semibold, tint = Theme.foreground) }
                    }
                    KitText(title, font = Font(13.5f))
                }
                Box { control() }
            },
        ) { measurables, constraints ->
            val (words, choice) = measurables
            val width = constraints.maxWidth
            val side = 14.dp.roundToPx()
            val gap = 22.dp.roundToPx()
            val rowHeight = 48.dp.roundToPx()
            val vertical = 9.dp.roundToPx()
            val wordsIdeal = words.maxIntrinsicWidth(Constraints.Infinity)
            val control = choice.measure(Constraints(maxWidth = (width - lead.roundToPx() - side).coerceAtLeast(0)))
            val fits = side + wordsIdeal + gap + control.width + side <= width
            if (fits) {
                val placed = words.measure(Constraints(maxWidth = (width - side * 2 - gap - control.width).coerceAtLeast(0)))
                val height = maxOf(rowHeight, maxOf(placed.height, control.height) + vertical * 2)
                layout(width, height) {
                    placed.place(side, (height - placed.height) / 2)
                    control.place(width - side - control.width, (height - control.height) / 2)
                }
            } else {
                val placed = words.measure(Constraints(maxWidth = (width - side * 2).coerceAtLeast(0)))
                val top = maxOf(rowHeight, placed.height + vertical * 2)
                val below = 12.dp.roundToPx()
                layout(width, top + control.height + below) {
                    placed.place(side, (top - placed.height) / 2)
                    control.place(lead.roundToPx(), top)
                }
            }
        }
        if (!last) Hairline(lead)
    }
}

/**
 * The first arrangement where it fits across the width it is given, the
 * second where it does not (SwiftUI's `ViewThatFits(in: .horizontal)`), for
 * content with nothing in it that opens or listens: both may be composed,
 * and only one is placed.
 */
@Composable
internal fun FitsAcross(modifier: Modifier = Modifier, first: @Composable () -> Unit, second: @Composable () -> Unit) {
    SubcomposeLayout(modifier) { constraints ->
        val wide = subcompose("first", first).firstOrNull()
        val fits = wide == null || wide.maxIntrinsicWidth(Constraints.Infinity) <= constraints.maxWidth
        val chosen = if (fits) wide else subcompose("second", second).firstOrNull()
        val placeable = chosen?.measure(constraints.copy(minHeight = 0))
        layout(placeable?.width ?: 0, placeable?.height ?: 0) { placeable?.place(0, 0) }
    }
}

/**
 * The small round close button at the top of a panel (DevicesPanel.swift
 * `CloseButton`): a faint plate that brightens, with the symbol, under the
 * finger and the pointer.
 */
@Composable
internal fun CloseButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val press = rememberPress()
    val on = LocalEnabled.current
    val ink by animateColorAsState(if (press.lit) Theme.foreground else Theme.muted, Motion.hover(), label = "ink")
    val fill by animateColorAsState(Color.White.copy(alpha = if (press.lit) 0.16f else 0.08f), Motion.hover(), label = "fill")
    Box(
        modifier
            .barPress(press)
            .size(26.dp)
            .background(fill, CircleShape)
            .pressable(press, CircleShape, on, Role.Button, label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Symbol("xmark", size = 10.5f, weight = Font.Bold, tint = ink) }
}

/** The chevron at the end of a row that opens a page. */
@Composable
internal fun Chevron() {
    Symbol("chevron.right", size = 13f, weight = Font.Semibold, tint = Theme.muted.copy(alpha = Theme.muted.alpha * 0.7f))
}

/** Settings.swift `watchLabel`: what a watch ceiling is called. */
internal fun watchLabel(quality: String): String = when (quality) {
    "1080p" -> t("1080p")
    "720p" -> t("720p")
    else -> t("Automatic")
}
