package app.tomoshibi.ui.settings

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Accent
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ConnectionBadge
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.Initial
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.SignalBadge
import app.tomoshibi.ui.kit.SwitchTrack
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import uniffi.tomoshibi_protocol.ConnectionGrade
import uniffi.tomoshibi_protocol.ConnectionReading

// The General page (Settings.swift `GeneralSettings`): the language, the
// colour and what it is spent on, how the connection is shown in a call,
// and where its controls sit.

@Composable
internal fun GeneralSettings() {
    val appearance = Appearance.shared
    Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
        Section(t("Language")) {
            LanguageTiles(Localization.shared, Modifier.padding(12.dp))
        }
        Section(t("Accent colour")) {
            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Swatches(appearance)
                AccentPreview(appearance.accent)
            }
        }
        Section(t("Connection badge")) {
            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                BadgePreview(appearance.connectionBadge)
                SegmentedChoice(
                    ConnectionBadge.entries.map { it to it.displayName },
                    appearance.connectionBadge,
                    { appearance.choose(it) },
                    height = 32.dp,
                )
            }
        }
        Section(t("Controls")) {
            PlacementCards(appearance, Modifier.padding(12.dp))
        }
    }
}

// MARK: - The language

/**
 * The languages as tiles, each in its own words, with the system's first
 * (Settings.swift `LanguageTiles`).
 */
@Composable
private fun LanguageTiles(localization: Localization, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LanguageTile(
            chosen = localization.followsSystem,
            symbol = "globe",
            title = t("Same as the system"),
            detail = Language.detect(Language.systemPreferences()).displayName,
            tag = "settings.language.system",
        ) { localization.followSystem() }
        AdaptiveGrid(minimum = 120.dp, spacing = 8.dp) {
            for (one in Language.entries) {
                LanguageTile(
                    chosen = !localization.followsSystem && localization.language == one,
                    symbol = null,
                    title = one.displayName,
                    detail = one.code,
                    tag = "settings.language.${one.code}",
                ) { localization.choose(one) }
            }
        }
    }
}

@Composable
private fun LanguageTile(chosen: Boolean, symbol: String?, title: String, detail: String, tag: String, pick: () -> Unit) {
    val press = rememberPress()
    val tally = Theme.tally
    val fill by animateColorAsState(if (chosen) tally.copy(alpha = 0.13f) else Color.White.copy(alpha = 0.05f), Theme.settle(), label = "fill")
    val edge by animateColorAsState(if (chosen) tally.copy(alpha = 0.55f) else Color.White.copy(alpha = 0.07f), Theme.settle(), label = "edge")
    val line by animateDpAsState(if (chosen) 1.5.dp else 1.dp, Theme.settle(), label = "line")
    val tick by animateFloatAsState(if (chosen) 1f else 0f, Theme.settle(), label = "tick")
    val ink by animateColorAsState(if (chosen) tally else Theme.muted, Theme.settle(), label = "symbol")
    val shape = rounded(13f)
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 50.dp)
            .background(fill, shape)
            .border(line, edge, shape)
            .pressable(press, shape, true, Role.RadioButton, selected = chosen, onClick = pick)
            .padding(horizontal = 12.dp)
            .testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) Symbol(symbol, size = 15f, weight = Font.Medium, tint = ink)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            KitText(title, font = Font(14f, if (chosen) Font.Semibold else Font.Regular), color = Theme.foreground, lines = 1)
            KitText(detail, font = Font(11f), color = Theme.muted, lines = 1)
        }
        Symbol(
            "checkmark.circle.fill",
            Modifier.padding(start = 4.dp).graphicsLayer {
                alpha = tick
                scaleX = 0.6f + 0.4f * tick
                scaleY = 0.6f + 0.4f * tick
            },
            size = 15f,
            tint = tally,
        )
    }
}

/**
 * As many columns of at least [minimum] as fit, sharing the width
 * (`LazyVGrid(columns: [GridItem(.adaptive(minimum:))])`).
 */
@Composable
private fun AdaptiveGrid(minimum: Dp, spacing: Dp, content: @Composable () -> Unit) {
    Layout(content) { measurables, constraints ->
        val gap = spacing.roundToPx()
        val width = constraints.maxWidth
        val columns = maxOf(1, (width + gap) / (minimum.roundToPx() + gap))
        val cell = (width - gap * (columns - 1)) / columns
        val placeables = measurables.map { it.measure(Constraints.fixedWidth(cell.coerceAtLeast(0))) }
        val rows = placeables.chunked(columns)
        val height = rows.sumOf { row -> row.maxOf { it.height } } + gap * (rows.size - 1).coerceAtLeast(0)
        layout(width, height) {
            var y = 0
            for (row in rows) {
                row.forEachIndexed { index, placeable -> placeable.place(index * (cell + gap), y) }
                y += row.maxOf { it.height } + gap
            }
        }
    }
}

// MARK: - The colour

/**
 * The colours, as lit glass beads (Settings.swift `Swatches`): all nine in a
 * row where they fit, as on the Mac; five and four on a phone, which reads
 * as two rows chosen rather than eight and one left over.
 */
@Composable
private fun Swatches(appearance: Appearance) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val all = Accent.entries.size
        val columns = if (maxWidth >= 42.dp * all + 6.dp * (all - 1)) all else 5
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            for (row in Accent.entries.chunked(columns)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (accent in row) {
                        Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                            Swatch(accent, appearance.accent == accent) { appearance.choose(accent) }
                        }
                    }
                    // The short row keeps the long row's columns.
                    repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
private fun Swatch(accent: Accent, chosen: Boolean, pick: () -> Unit) {
    val press = rememberPress()
    val lit by animateFloatAsState(if (chosen) 1f else 0f, Theme.settle(), label = "chosen")
    val pressed by animateFloatAsState(if (press.hovered && !chosen) 1.08f else 1f, Motion.hover(), label = "hover")
    Box(
        Modifier
            .size(42.dp)
            .graphicsLayer {
                scaleX = pressed
                scaleY = pressed
            }
            .pressable(press, CircleShape, true, Role.RadioButton, accent.displayName, selected = chosen, onClick = pick)
            .testTag("settings.accent.${accent.key}"),
        contentAlignment = Alignment.Center,
    ) {
        // The ring that says this one is chosen, grown in from inside.
        Box(
            Modifier
                .matchParentSize()
                .graphicsLayer {
                    alpha = lit
                    scaleX = 0.8f + 0.2f * lit
                    scaleY = 0.8f + 0.2f * lit
                }
                .border(2.dp, accent.color, CircleShape),
        )
        Box(
            Modifier
                .size(34.dp)
                .dropShadow(
                    CircleShape,
                    Shadow(
                        radius = (4f + 6f * lit).dp,
                        color = accent.color,
                        offset = DpOffset(0.dp, 2.dp),
                        alpha = 0.25f + 0.30f * lit,
                    ),
                )
                .background(accent.color, CircleShape)
                .drawBehind { bead() },
            contentAlignment = Alignment.Center,
        ) {
            androidx.compose.animation.AnimatedVisibility(
                chosen,
                enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle()),
                exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave()),
            ) { Symbol("checkmark", size = 13f, weight = FontWeight.ExtraBold, tint = accent.onColor) }
        }
    }
}

/** A light from the upper left and a shade at the foot, so each colour reads as a bead of glass. */
private fun DrawScope.bead() {
    val radius = size.minDimension / 2
    drawCircle(
        Brush.radialGradient(
            listOf(Color.White.copy(alpha = 0.42f), Color.White.copy(alpha = 0f)),
            center = Offset(size.width * 0.32f, size.height * 0.26f),
            radius = 15.dp.toPx(),
        ),
        radius,
    )
    drawCircle(
        Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0f), Color.Black.copy(alpha = 0.2f)), startY = size.height / 2, endY = size.height),
        radius,
    )
    drawCircle(Color.White.copy(alpha = 0.25f), radius - 0.25.dp.toPx(), style = Stroke(0.5.dp.toPx()))
}

/**
 * What the colour is spent on, drawn small: somebody speaking, the button
 * that joins, a switch that is on, something unread (Settings.swift
 * `AccentPreview`). The colour's name beside it where both fit, above it
 * where they do not.
 */
@Composable
private fun AccentPreview(accent: Accent) {
    Box(
        Modifier
            .fillMaxWidth()
            .testTag("settings.accent.preview")
            .clearAndSetSemantics {}
            .background(Color.Black.copy(alpha = 0.22f), rounded(16f))
            .padding(12.dp),
    ) {
        FitsAcross(
            first = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    AccentScene()
                    Spacer(Modifier.padding(start = 12.dp).widthIn(min = 8.dp).weight(1f))
                    AccentName(accent, Modifier.padding(start = 12.dp, end = 6.dp))
                }
            },
            second = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    AccentName(accent)
                    AccentScene()
                }
            },
        )
    }
}

/** The colour's name, in itself. */
@Composable
private fun AccentName(accent: Accent, modifier: Modifier = Modifier) {
    Crossfade(accent, modifier, animationSpec = Theme.settle(), label = "name") { shown ->
        KitText(shown.displayName, font = Font(17f, Font.Semibold), color = Theme.tally, lines = 1)
    }
}

@Composable
private fun AccentScene() {
    val tally = Theme.tally
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier
                .size(104.dp, 66.dp)
                .background(Brush.verticalGradient(listOf(Color(0.2f, 0.25f, 0.32f), Color(0.1f, 0.12f, 0.16f))), rounded(12f))
                .border(2.dp, tally, rounded(12f)),
        ) {
            Initial("Mika", Modifier.align(Alignment.Center), size = 30.dp)
            Row(
                Modifier
                    .align(Alignment.BottomStart)
                    .padding(6.dp)
                    .background(Color.Black.copy(alpha = 0.45f), Capsule)
                    .padding(horizontal = 6.dp, vertical = 3.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Symbol("waveform", size = 8f, weight = Font.Bold, tint = tally)
                KitText("Mika", font = Font(9f, Font.Semibold), lines = 1)
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.height(28.dp).background(tally, Capsule).padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) { KitText(t("Join"), font = Font(12.5f, Font.Semibold), color = Theme.tallyForeground, lines = 1) }
                SwitchTrack(checked = true, enabled = true)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Box {
                    Symbol("bubble.left.fill", size = 14f, tint = Theme.foreground.copy(alpha = 0.85f))
                    Box(
                        Modifier
                            .align(Alignment.TopEnd)
                            .offset(x = 3.dp, y = (-2).dp)
                            .size(7.dp)
                            .background(tally, CircleShape),
                    )
                }
                Symbol("hand.raised.fill", size = 14f, tint = tally)
            }
        }
    }
}

// MARK: - The connection badge

/** The badge in the corner of a small picture of a call, as it will be (Settings.swift `BadgePreview`). */
@Composable
private fun BadgePreview(mode: ConnectionBadge) {
    val shape = rounded(14f)
    Box(
        Modifier
            .fillMaxWidth()
            .height(132.dp)
            .clip(shape)
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .clearAndSetSemantics {},
    ) {
        Backdrop(Modifier.matchParentSize())
        Row(
            Modifier.matchParentSize().padding(start = 14.dp, end = 14.dp, top = 46.dp, bottom = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            for (index in 0 until 3) {
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(Color.White.copy(alpha = if (index == 1) 0.13f else 0.08f), rounded(7f)),
                )
            }
        }
        Box(Modifier.padding(10.dp)) {
            AnimatedVisibility(mode != ConnectionBadge.Hidden, enter = Theme.arrive, exit = Theme.depart) {
                SignalBadge(PreviewReading, detailed = mode == ConnectionBadge.Detailed, open = false, onOpenChange = {})
            }
        }
        // A drawing of the badge, not the badge: nothing in it is pressed.
        Box(Modifier.matchParentSize().pointerInput(Unit) { detectTapGestures { } })
    }
}

/** A healthy reading, for showing what the badge looks like (Settings.swift `ConnectionReading.preview`). */
private val PreviewReading = ConnectionReading(
    grade = ConnectionGrade.GOOD,
    rttMs = 32.0,
    lossPercent = 0.0,
    upKbps = 820.0,
    downKbps = 2400.0,
    jitterMs = 4.0,
    share = null,
    ownAddress = null,
    holding = null,
    measured = true,
)

// MARK: - Where the controls sit

/**
 * The three places the controls can sit (controls.ts), each as a small
 * drawing of a call with them there (Settings.swift `PlacementCards`).
 */
@Composable
private fun PlacementCards(appearance: Appearance, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
        for (placement in ControlsPlacement.entries) {
            val chosen = appearance.controlsPlacement == placement
            PlacementCard(placement, chosen, Modifier.weight(1f)) { appearance.choose(placement) }
        }
    }
}

@Composable
private fun PlacementCard(placement: ControlsPlacement, chosen: Boolean, modifier: Modifier, pick: () -> Unit) {
    val press = rememberPress()
    val tally = Theme.tally
    val fill by animateColorAsState(if (chosen) tally.copy(alpha = 0.12f) else Color.White.copy(alpha = 0.04f), Theme.settle(), label = "fill")
    val edge by animateColorAsState(if (chosen) tally.copy(alpha = 0.6f) else Color.White.copy(alpha = 0.07f), Theme.settle(), label = "edge")
    val line by animateDpAsState(if (chosen) 1.5.dp else 1.dp, Theme.settle(), label = "line")
    val ink by animateColorAsState(if (chosen) Theme.foreground else Theme.muted, Theme.settle(), label = "ink")
    val shape = rounded(14f)
    Column(
        modifier
            .background(fill, shape)
            .border(line, edge, shape)
            .pressable(press, shape, true, Role.RadioButton, selected = chosen, onClick = pick)
            .padding(8.dp)
            .testTag("settings.controls.${placement.key}"),
        verticalArrangement = Arrangement.spacedBy(9.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Schematic(placement, chosen, Modifier.fillMaxWidth().aspectRatio(1.45f))
        KitText(
            placement.displayName,
            Modifier.fillMaxWidth(),
            font = Font(11.5f, if (chosen) Font.Semibold else Font.Regular),
            color = ink,
            align = TextAlign.Center,
        )
    }
}

/** A call, small: four pictures, narrower where the controls take a strip of width, and the controls. */
@Composable
private fun Schematic(placement: ControlsPlacement, lit: Boolean, modifier: Modifier) {
    val colour by animateColorAsState(if (lit) Theme.tally else Color.White.copy(alpha = 0.55f), Theme.settle(), label = "bar")
    Canvas(modifier) {
        val unit = 1.dp.toPx()
        val side = placement == ControlsPlacement.Side
        drawRoundRect(Color.Black.copy(alpha = 0.35f), cornerRadius = CornerRadius(8 * unit))
        val width = size.width - (if (side) 26 else 12) * unit
        val height = size.height - (if (side) 12 else 24) * unit
        val gap = 3 * unit
        val tileW = (width - gap) / 2
        val tileH = (height - gap) / 2
        for (row in 0 until 2) for (column in 0 until 2) {
            drawRoundRect(
                Color.White.copy(alpha = 0.13f),
                Offset(6 * unit + column * (tileW + gap), 6 * unit + row * (tileH + gap)),
                Size(tileW, tileH),
                CornerRadius(3 * unit),
            )
        }
        when (placement) {
            ControlsPlacement.Always -> {
                val w = size.width * 0.42f
                drawRoundRect(colour, Offset((size.width - w) / 2, size.height - 9 * unit - 3.5f * unit), Size(w, 7 * unit), CornerRadius(3.5f * unit))
            }
            ControlsPlacement.Idle -> {
                val w = size.width * 0.42f
                val line = 1.2f * unit
                drawRoundRect(
                    colour.copy(alpha = colour.alpha * 0.8f),
                    Offset((size.width - w) / 2 + line / 2, size.height - 9 * unit - 3.5f * unit + line / 2),
                    Size(w - line, 7 * unit - line),
                    CornerRadius(3.5f * unit),
                    style = Stroke(line, pathEffect = PathEffect.dashPathEffect(floatArrayOf(2.5f * unit, 2 * unit))),
                )
            }
            ControlsPlacement.Side -> {
                val h = size.height * 0.5f
                drawRoundRect(colour, Offset(size.width - 9 * unit - 3.5f * unit, (size.height - h) / 2), Size(7 * unit, h), CornerRadius(3.5f * unit))
            }
        }
    }
}
