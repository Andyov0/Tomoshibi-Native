package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import kotlin.math.roundToInt

// The pieces the song desk and watching together are drawn with
// (Kit+Media.swift): a slider, a spinner, a field (MediaField, in Fields.kt),
// a panel's header and its small round buttons, artwork, and the listener's
// row for one sound. Drawn rather than borrowed for the reason Kit.swift
// gives, and with the same colour and curves as everything else.

/**
 * A capsule track that fills as it rises, with a knob: for a volume and for
 * a place in a video (`MediaSlider`). Dragged anywhere along it, and
 * adjusted by a screen reader a twentieth at a time.
 *
 * `editing` is told when a change starts and ends, for a scrubber that only
 * seeks once it is let go. Every way of changing the value ends what it
 * started: a drag let go, a drag the system takes away (a scroll taking the
 * finger, the view going from under it), and a step from a screen reader,
 * which is a whole change on its own. On Apple only a drag let go ended one,
 * and an interrupted drag left the watch scrubber editing for good: it never
 * sought, its place stood still, and the controls over the video never hid
 * again.
 */
@Composable
fun MediaSlider(
    value: Double,
    onValueChange: (Double) -> Unit,
    modifier: Modifier = Modifier,
    range: ClosedFloatingPointRange<Double> = 0.0..1.0,
    step: Double? = null,
    tint: Color = Theme.foreground,
    enabled: Boolean = true,
    label: String? = null,
    editing: (Boolean) -> Unit = {},
) {
    val on = enabled && LocalEnabled.current
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    var dragging by remember { mutableStateOf(false) }
    val currentEditing by rememberUpdatedState(editing)
    val current by rememberUpdatedState(value)
    val span = range.endInclusive - range.start
    val fraction = if (span > 0) ((value - range.start) / span).coerceIn(0.0, 1.0).toFloat() else 0f
    val track by animateDpAsState(if (dragging || hovering) 6.dp else 4.dp, Motion.hover(), label = "track")
    val knobScale by animateFloatAsState(if (dragging) 1.18f else 1f, Theme.settle(), label = "knob")
    fun set(raw: Float) {
        var next = range.start + raw.coerceIn(0f, 1f) * span
        if (step != null && step > 0) next = (next / step).roundToInt() * step
        if (next != current) onValueChange(next)
    }
    fun finish() {
        if (!dragging) return
        dragging = false
        currentEditing(false)
    }
    // The view going from under a drag ends it.
    DisposableEffect(Unit) { onDispose { if (dragging) currentEditing(false) } }
    val knob = 14.dp
    BoxWithConstraints(
        modifier
            .height(22.dp)
            .hoverable(hover, on)
            .sliderSemantics(label, fraction.toDouble(), 0.05, on) { target ->
                // A screen reader's step: begun, moved and finished at once,
                // so a scrubber seeks to it as it would to a drag let go there.
                currentEditing(true)
                set(target.toFloat())
                currentEditing(false)
            }
            .pointerInput(on, range, step) {
                if (!on) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val half = knob.toPx() / 2
                    val travel = maxOf(1f, size.width - knob.toPx())
                    dragging = true
                    currentEditing(true)
                    try {
                        set((down.position.x - half) / travel)
                        horizontalDrag(down.id) { change ->
                            set((change.position.x - half) / travel)
                            change.consume()
                        }
                    } finally {
                        finish()
                    }
                }
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        val width = maxWidth.coerceAtLeast(1.dp)
        Box(Modifier.fillMaxWidth().height(track).background(Color.White.copy(alpha = 0.14f), Capsule))
        Box(
            Modifier
                .size(maxOf(track, (width - knob) * fraction + knob / 2), track)
                .background(tint.copy(alpha = tint.alpha * if (on) 1f else 0.5f), Capsule),
        )
        Box(
            Modifier
                .offset(x = (width - knob) * fraction)
                .size(knob)
                .graphicsLayer {
                    scaleX = knobScale
                    scaleY = knobScale
                }
                .dropShadow(CircleShape, Shadow(radius = 2.dp, offset = DpOffset(0.dp, 1.dp), alpha = 0.35f))
                .background(Color.White, CircleShape),
        )
    }
}

/**
 * An arc that turns: the app's own spinner, light on glass and on a picture
 * alike, where the system's is a grey that disappears on either
 * (`MediaSpinner`).
 */
@Composable
fun MediaSpinner(modifier: Modifier = Modifier, size: Float = 16f, tint: Color = Theme.foreground) {
    val turning = rememberInfiniteTransition(label = "spinner")
    val angle by turning.animateFloat(0f, 360f, infiniteRepeatable(tween(850, easing = LinearEasing), RepeatMode.Restart), label = "angle")
    Canvas(modifier.size(size.dp).clearAndSetSemantics {}) {
        val line = maxOf(1.6f, size / 9).dp.toPx()
        // `Circle().trim(from: 0.1, to: 0.78)`: a circle's path starts at
        // three o'clock in SwiftUI as an arc's angle does here.
        rotate(angle) {
            drawArc(
                tint.copy(alpha = tint.alpha * 0.85f),
                startAngle = 0.1f * 360f,
                sweepAngle = (0.78f - 0.1f) * 360f,
                useCenter = false,
                topLeft = Offset(line / 2, line / 2),
                size = Size(this.size.width - line, this.size.height - line),
                style = Stroke(line, cap = StrokeCap.Round),
            )
        }
    }
}

/** How far through something is, as a thin bar that fills with the chosen colour (`MediaProgress`). */
@Composable
fun MediaProgress(fraction: Float, modifier: Modifier = Modifier, tint: Color = Theme.tally) {
    val shown by animateFloatAsState(fraction.coerceIn(0f, 1f), tween(500, easing = LinearEasing), label = "progress")
    Box(modifier.fillMaxWidth().height(3.dp).clearAndSetSemantics {}.background(Color.White.copy(alpha = 0.12f), Capsule)) {
        Box(Modifier.fillMaxWidth(shown).height(3.dp).background(tint, Capsule))
    }
}

/** The top of a panel beside the call: its name, and the button that closes it (`MediaPanelHeader`). */
@Composable
fun MediaPanelHeader(title: String, closeLabel: String, close: () -> Unit, modifier: Modifier = Modifier, symbol: String? = null) {
    Row(
        modifier.fillMaxWidth().padding(start = 14.dp, end = 10.dp, top = 10.dp, bottom = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Box(Modifier.size(24.dp).background(Theme.tally, rounded(7f)), contentAlignment = Alignment.Center) {
                Symbol(symbol, size = 11.5f, weight = Font.Bold, tint = Theme.tallyForeground)
            }
        }
        KitText(title, Modifier.weight(1f), font = Font(14f, Font.Semibold), lines = 1)
        MediaIconButton("xmark", closeLabel, close)
    }
}

/**
 * The glass a panel beside the call is drawn on, darkened from inside
 * (`.mediaPanelGlass(cornerRadius:)`). These panels sit over the stage, and
 * the stage is often a film: over a bright picture the clear glass let the
 * colours straight through, and grey words on it could not be read (seen
 * with the watch panel over a test card). A shade of the call's own black
 * under the content keeps the glass's edge and light and gives the words a
 * ground. A shell: its insides are content.
 */
@Composable
fun MediaPanelGlass(modifier: Modifier = Modifier, cornerRadius: Dp = 22.dp, content: @Composable BoxScope.() -> Unit) {
    val shape = ContinuousShape(cornerRadius)
    Box(modifier.liquidGlass(shape, null, LocalGlassBackdrop.current).background(Theme.background.copy(alpha = 0.68f))) {
        CompositionLocalProvider(LocalContentLayer provides true) { content() }
    }
}

/** A hairline between the parts of a panel (`MediaRule`). */
@Composable
fun MediaRule(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.07f)))
}

/**
 * A small round button that lights under the pointer and the finger:
 * closing, taking a song off, playing one next (`MediaIconButton`). `lit`
 * fills it with the chosen colour, for something that is on.
 */
@Composable
fun MediaIconButton(
    symbol: String,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 26.dp,
    lit: Boolean = false,
    prominent: Boolean = false,
    enabled: Boolean = true,
    /** How large it is pressed at, round the size it is drawn at (`fingertip`). */
    reach: Dp = size,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val filled = lit || prominent
    val fill by animateColorAsState(
        if (filled) Theme.tally else Color.White.copy(alpha = if (press.lit && on) 0.12f else 0f),
        if (filled) Theme.settle() else Motion.hover(), label = "fill",
    )
    val ink by animateColorAsState(
        if (filled) Theme.tallyForeground else if (press.lit) Theme.foreground else Theme.muted,
        Motion.hover(), label = "ink",
    )
    Box(
        modifier
            .alpha(if (on) 1f else 0.4f)
            .size(maxOf(size, reach))
            .pressable(press, CircleShape, on, Role.Button, label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(size).background(fill, CircleShape), contentAlignment = Alignment.Center) {
            SwappingSymbol(symbol, size = size.value * 0.42f, weight = Font.Semibold, tint = ink)
        }
    }
}

/**
 * A button inside a panel: a capsule of light, the chosen colour for the one
 * thing the panel is for, or no fill at all for a quiet one
 * (`MediaButtonStyle`, `.media(prominent:quiet:height:)`).
 */
@Composable
fun MediaButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    prominent: Boolean = false,
    quiet: Boolean = false,
    height: Dp = 30.dp,
    enabled: Boolean = true,
    label: String? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val fill by animateColorAsState(
        if (prominent) Theme.tally else if (quiet) Color.Transparent else Color.White.copy(alpha = if (press.pressed) 0.16f else 0.10f),
        Theme.settle(), label = "fill",
    )
    val scale by animateFloatAsState(if (press.pressed) 0.97f else 1f, Theme.settle(), label = "press")
    Row(
        modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .alpha(if (on) 1f else 0.4f)
            .heightIn(min = height)
            .background(fill, Capsule)
            .border(1.dp, Color.White.copy(alpha = if (prominent || quiet) 0f else 0.08f), Capsule)
            .pressable(press, Capsule, on, Role.Button, label, onClick = onClick)
            .padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KitInk(if (prominent) Theme.tallyForeground else if (quiet) Theme.muted else Theme.foreground) {
            KitFont(Font(12.5f, Font.Semibold)) { content() }
        }
    }
}

/**
 * Something said quietly in a panel where there is nothing to show yet: a
 * symbol in a soft circle, and a sentence (`MediaEmpty`).
 */
@Composable
fun MediaEmpty(symbol: String, text: String, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 28.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.size(44.dp).background(Color.White.copy(alpha = 0.07f), CircleShape), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 17f, weight = Font.Medium, tint = Theme.muted)
        }
        KitText(text, font = Font(12f), color = Theme.muted, align = TextAlign.Center)
    }
}

/** A line of red words in a panel, for what did not work (`MediaProblem`). */
@Composable
fun MediaProblem(text: String, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .background(Theme.danger.copy(alpha = 0.12f), rounded(10f))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Symbol("exclamationmark.circle.fill", Modifier.padding(top = 2.dp), size = 11f, tint = Theme.danger)
        KitText(text, font = Font(12f), color = Theme.danger)
    }
}

/**
 * A row of a list in a panel that lights under the pointer, and while it is
 * pressed when it does something (`MediaRow`). `content` is told whether it
 * is lit, as the Swift row tells its content it is hovered.
 */
@Composable
fun MediaRow(
    modifier: Modifier = Modifier,
    lit: Boolean = false,
    onClick: (() -> Unit)? = null,
    content: @Composable RowScope.(hovering: Boolean) -> Unit,
) {
    val press = rememberPress()
    val shape = rounded(10f)
    val light by animateFloatAsState(if (lit) 0.09f else if (press.lit) 0.06f else 0f, Motion.hover(), label = "light")
    Row(
        modifier
            .fillMaxWidth()
            .background(Color.White.copy(alpha = light), shape)
            .then(if (onClick != null) Modifier.pressable(press, shape, true, Role.Button, onClick = onClick) else Modifier.hoverable(press.source))
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) { content(press.lit) }
}

/**
 * A cover or a poster where there is one, and a soft square in the chosen
 * colour with a symbol where there is not; faded in when it arrives
 * (`MediaArtwork`). Its size is the modifier's.
 */
@Composable
fun MediaArtwork(url: String, modifier: Modifier = Modifier, symbol: String = "music.note", corner: Float = 8f) {
    val shape = rounded(corner)
    Box(
        modifier
            .clearAndSetSemantics {}
            .clip(shape)
            .background(Brush.linearGradient(listOf(Theme.tally.copy(alpha = 0.32f), Theme.surfaceHigh)))
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape),
        contentAlignment = Alignment.Center,
    ) {
        Symbol(symbol, size = 13f, weight = Font.Semibold, tint = Theme.foreground.copy(alpha = 0.7f))
        if (url.startsWith("http")) RemoteImage(url, Modifier.fillMaxSize())
    }
}

/** A few words in a small capsule: what a song is, that it is paused (`MediaBadge`). */
@Composable
fun MediaBadge(text: String, modifier: Modifier = Modifier, symbol: String? = null, lit: Boolean = false) {
    val ink = if (lit) Theme.tally else Theme.foreground.copy(alpha = 0.75f)
    Row(
        modifier
            .background(if (lit) Theme.tally.copy(alpha = 0.16f) else Color.White.copy(alpha = 0.09f), Capsule)
            .border(1.dp, if (lit) Theme.tally.copy(alpha = 0.35f) else Color.Transparent, Capsule)
            .padding(horizontal = 7.dp, vertical = 3.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) Symbol(symbol, size = 8.5f, weight = Font.Bold, tint = ink)
        KitText(text, font = Font(10.5f, Font.Semibold), color = ink, lines = 1)
    }
}

/**
 * The listener's own setting for one sound, as the sound panel's row has it
 * (SoundPanel.tsx `SoundRow`; `HearingRow`): a button that blocks it at the
 * media server, a volume, and the volume said in figures, because a slider
 * near one end tells nobody whether it is at nought. The slider is disabled
 * rather than hidden while blocked: nothing arrives to be made louder, and a
 * slider that moves while the sound stays off is a control that lies.
 */
@Composable
fun HearingRow(
    name: String,
    volume: Double,
    blocked: Boolean,
    setVolume: (Double) -> Unit,
    setBlocked: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val silenced = blocked || volume == 0.0
    val nameInk by animateColorAsState(if (silenced) Theme.muted else Theme.foreground, Theme.settle(), label = "name")
    val sliderShown by animateFloatAsState(if (blocked) 0.45f else 1f, Theme.settle(), label = "slider")
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        KitText(name, font = Font(12f, Font.Medium), color = nameInk, lines = 1)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            // Named for what pressing it does, as the sound panel's is.
            MediaIconButton(
                if (blocked) "speaker.slash.fill" else "speaker.wave.2.fill",
                if (blocked) t("Unmute {name}", mapOf("name" to name)) else t("Mute {name}", mapOf("name" to name)),
                { setBlocked(!blocked) },
                size = 28.dp,
                reach = fingertip(28.dp),
                lit = blocked,
            )
            MediaSlider(
                volume,
                setVolume,
                Modifier.weight(1f).alpha(sliderShown),
                step = 0.05,
                enabled = !blocked,
                label = t("{name}'s volume", mapOf("name" to name)),
            )
            AnimatedContent(
                if (blocked) t("Muted") else "${(volume * 100).roundToInt()}%",
                Modifier.width(42.dp),
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                contentAlignment = Alignment.CenterEnd,
                label = "volume",
            ) { said ->
                KitText(said, Modifier.fillMaxWidth(), font = Font(11f, Font.Medium, digits = true), color = Theme.muted,
                    lines = 1, align = TextAlign.End)
            }
        }
    }
}
