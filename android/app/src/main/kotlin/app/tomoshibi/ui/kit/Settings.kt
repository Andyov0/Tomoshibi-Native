package app.tomoshibi.ui.kit

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme

// Settings cards and their rows (Kit.swift, Settings; Kit+Settings.swift,
// Pages of settings), and the level meter.

/** A card of settings rows under a heading, with perhaps a line under it (`SettingsSection`). */
@Composable
fun SettingsSection(
    title: String,
    modifier: Modifier = Modifier,
    footer: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        KitText(title, Modifier.padding(start = 6.dp), font = Font(12f, Font.Semibold), color = Theme.muted)
        val shape = rounded(16f)
        Column(
            Modifier
                .fillMaxWidth()
                .background(Color.White.copy(alpha = 0.05f), shape)
                .border(1.dp, Color.White.copy(alpha = 0.07f), shape),
            content = content,
        )
        if (footer != null) {
            KitText(footer, Modifier.padding(horizontal = 6.dp), font = Font(11.5f), color = Theme.muted)
        }
    }
}

/** One row of a settings card: a symbol in a tile, words, and the control (`SettingsRow`). */
@Composable
fun SettingsRow(
    title: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    symbol: String? = null,
    last: Boolean = false,
    control: @Composable RowScope.() -> Unit = {},
) {
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .padding(horizontal = 14.dp, vertical = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (symbol != null) {
                Box(
                    Modifier
                        .size(26.dp)
                        .background(Color.White.copy(alpha = 0.10f), rounded(7f)),
                    contentAlignment = Alignment.Center,
                ) { Symbol(symbol, size = 12.5f, weight = Font.Semibold, tint = Theme.foreground) }
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(title, font = Font(13.5f))
                if (detail != null) KitText(detail, font = Font(11.5f), color = Theme.muted)
            }
            // The Swift row's spacer, at least ten points, and the row's
            // spacing on either side of it.
            Row(
                Modifier.padding(start = 22.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
                content = control,
            )
        }
        if (!last) Hairline(if (symbol == null) 14.dp else 52.dp)
    }
}

/**
 * The heading a page of settings opens with: its symbol in a lit tile, its
 * name large, and a line saying what it is for (`SettingsPageHeading`).
 */
@Composable
fun SettingsPageHeading(title: String, symbol: String, modifier: Modifier = Modifier, detail: String? = null) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
        SectionTile(symbol, lit = true, size = 44f)
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            KitText(title, font = Font(24f, Font.Bold))
            if (detail != null) KitText(detail, font = Font(12.5f), color = Theme.muted)
        }
    }
}

/**
 * A section's symbol in a rounded tile, lit in the person's colour where it
 * is the one chosen (`SectionTile`).
 */
@Composable
fun SectionTile(symbol: String, modifier: Modifier = Modifier, lit: Boolean = false, size: Float = 28f) {
    val glow by animateFloatAsState(if (lit) 1f else 0f, Theme.settle(), label = "lit")
    val ink by animateColorAsState(if (lit) Theme.tallyForeground else Theme.foreground, Theme.settle(), label = "ink")
    val shape = rounded(size * 0.28f)
    val tally = Theme.tally
    Box(
        modifier
            .size(size.dp)
            .background(Color.White.copy(alpha = 0.10f * (1f - glow)), shape)
            .drawBehind {
                if (glow > 0f) {
                    val outline = shape.createOutline(this.size, layoutDirection, this)
                    drawOutline(
                        outline,
                        Brush.verticalGradient(listOf(tally.copy(alpha = 0.95f * glow), tally.copy(alpha = 0.72f * glow))),
                    )
                }
            }
            .border(1.dp, Color.White.copy(alpha = 0.06f + 0.16f * glow), shape),
        contentAlignment = Alignment.Center,
    ) { Symbol(symbol, size = size * 0.44f, weight = Font.Semibold, tint = ink) }
}

/**
 * How loud something is, 0 to 1, as a row of segments that light in turn:
 * the "Hearing you" meter of the web's join screen (MicLevel.tsx, `LevelMeter`).
 */
@Composable
fun LevelMeter(level: Float, modifier: Modifier = Modifier, segments: Int = 14) {
    Row(modifier.clearAndSetSemantics {}, horizontalArrangement = Arrangement.spacedBy(2.5.dp)) {
        for (index in 0 until segments) {
            val lit = index.toFloat() / segments < level
            val fill by animateColorAsState(if (lit) Theme.tally else Color.White.copy(alpha = 0.12f), Motion.follow(), label = "segment")
            Box(Modifier.size(4.dp, 14.dp).background(fill, Capsule))
        }
    }
}
