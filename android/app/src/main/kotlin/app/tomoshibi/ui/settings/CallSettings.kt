package app.tomoshibi.ui.settings

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallPreferences
import app.tomoshibi.ui.kit.Chip
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassSwitch
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.SettingsRow
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.rounded
import kotlin.math.abs
import kotlin.math.sin

// The pages about what a call fetches and plays (Settings.swift
// `WatchingSettings`, `MusicSettings`), each over a small drawing of the
// choice, so that it is seen as well as read.

/**
 * Offered to everybody, whether or not they may start a showing: it is about
 * what this screen fetches, and everybody in a room is a viewer (Account.tsx
 * `WatchSettings`).
 */
@Composable
internal fun WatchingSettings(preferences: CallPreferences) {
    val quality = preferences.watchQuality
    Column(verticalArrangement = Arrangement.spacedBy(18.dp)) {
        WatchPreview(quality)
        Section(
            t("Watch together"),
            footer = if (quality == "auto") null
            else t("Where a video comes in several qualities, it is never fetched sharper than this here. It applies from the next video you open."),
        ) {
            AdaptiveRow(t("Picture quality"), symbol = "sparkles.tv", last = true) {
                SegmentedChoice(
                    CallPreferences.WATCH_QUALITIES.map { it to watchLabel(it) },
                    quality,
                    { preferences.watchQuality = it },
                    Modifier.width(IntrinsicSize.Max),
                    height = 30.dp,
                )
            }
        }
    }
}

/**
 * A film on the backdrop, wearing the ceiling chosen for it: drawn
 * softer the lower the ceiling (Settings.swift `WatchPreview`).
 */
@Composable
private fun WatchPreview(quality: String) {
    val shape = rounded(18f)
    val softness by animateDpAsState(
        when (quality) {
            "720p" -> 6.dp
            "1080p" -> 2.5.dp
            else -> 0.dp
        },
        Theme.settle(),
        label = "softness",
    )
    Box(
        Modifier
            .fillMaxWidth()
            .aspectRatio(2.6f)
            .clip(shape)
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .clearAndSetSemantics {},
    ) {
        FilmStill(Modifier.matchParentSize().blur(softness, BlurredEdgeTreatment.Rectangle))
        Box(Modifier.align(Alignment.Center).size(54.dp).glassSurface(CircleShape), contentAlignment = Alignment.Center) {
            Symbol("play.fill", size = 20f, weight = Font.Bold, tint = Theme.foreground)
        }
        Crossfade(quality, Modifier.align(Alignment.BottomEnd).padding(12.dp), animationSpec = Theme.settle(), label = "chip") { shown ->
            Chip(watchLabel(shown), symbol = "sparkles.tv", lit = shown != "auto")
        }
    }
}

/**
 * The film [WatchPreview] softens: a still of flat shapes, the call's grey
 * with a lighter band along its foot and a pale disc high on the right
 * (Settings.swift `FilmStill`). It was the screens' glowing backdrop,
 * blurred; once that went flat a blur of it changed nothing that could be
 * seen, and the ceiling's choice was read but no longer seen. Shapes with
 * edges show the softening without a gradient.
 */
@Composable
private fun FilmStill(modifier: Modifier = Modifier) {
    Canvas(modifier) {
        drawRect(Theme.surface)
        drawRect(Theme.surfaceHigh, topLeft = Offset(0f, size.height * 0.6f), size = Size(size.width, size.height * 0.4f))
        drawCircle(Theme.foreground.copy(alpha = 0.18f), radius = size.height * 0.14f, center = Offset(size.width * 0.75f, size.height * 0.35f))
    }
}

/**
 * Lowering it under speech is the listener's choice, and somebody who may
 * not open the library still hears whatever another person plays, so it is
 * offered to everybody; the library's quality only to whoever may fetch from
 * it (Account.tsx `MusicSettings`).
 */
@Composable
internal fun MusicSettings(app: AppModel, preferences: CallPreferences) {
    val library = app.holds("music")
    Column(verticalArrangement = Arrangement.spacedBy(18.dp)) {
        DuckPreview(preferences.musicDucks)
        Section(
            t("Music"),
            footer = if (preferences.musicQuality == "best" && library) t("Hi-Res and masters are large, and take longer to start.") else null,
        ) {
            SettingsRow(t("Lower the music while anybody talks"), symbol = "speaker.wave.1.fill", last = !library) {
                GlassSwitch(preferences.musicDucks, { preferences.musicDucks = it }, label = t("Lower the music while anybody talks"))
            }
            Arriving(library) {
                AdaptiveRow(t("Quality"), symbol = "waveform.path", last = true) {
                    SegmentedChoice(
                        listOf("lossless" to t("Lossless"), "best" to t("Highest")),
                        preferences.musicQuality,
                        { preferences.musicQuality = it },
                        Modifier.width(IntrinsicSize.Max),
                        height = 30.dp,
                    )
                }
            }
        }
    }
}

/**
 * The music as a row of bars and a voice under it: where somebody speaks the
 * music sinks, if it is asked to (duck.ts), and stays put if not
 * (Settings.swift `DuckPreview`).
 */
@Composable
private fun DuckPreview(ducks: Boolean) {
    val sunk by animateFloatAsState(if (ducks) 1f else 0f, Theme.settle(), label = "duck")
    val shape = rounded(18f)
    val tally = Theme.tally
    Column(
        Modifier
            .fillMaxWidth()
            .clearAndSetSemantics {}
            .background(Color.Black.copy(alpha = 0.22f), shape)
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Bars("music.note", { index ->
            val base = 0.45 + 0.4 * abs(sin(index * 0.55)) + 0.15 * abs(sin(index * 1.7))
            if (speaking(index)) base * (1 - 0.68 * sunk) else base
        }) { tally }
        Bars("person.wave.2.fill", { index -> if (speaking(index)) 0.3 + 0.6 * abs(sin(index * 1.3)) else 0.05 }) { index ->
            if (speaking(index)) Theme.foreground.copy(alpha = 0.85f) else Color.White.copy(alpha = 0.18f)
        }
    }
}

private const val BARS = 44

private fun speaking(index: Int) = index >= BARS * 2 / 5 && index < BARS * 4 / 5

@Composable
private fun Bars(symbol: String, height: (Int) -> Double, colour: (Int) -> Color) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(20.dp), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 13f, weight = Font.Semibold, tint = Theme.muted)
        }
        Canvas(Modifier.weight(1f).height(36.dp)) {
            val gap = 3.dp.toPx()
            val width = (size.width - gap * (BARS - 1)) / BARS
            for (index in 0 until BARS) {
                val tall = maxOf(3.dp.toPx(), 34.dp.toPx() * height(index).toFloat())
                drawRoundRect(
                    colour(index),
                    Offset(index * (width + gap), (size.height - tall) / 2),
                    Size(width, tall),
                    CornerRadius(width / 2),
                )
            }
        }
    }
}
