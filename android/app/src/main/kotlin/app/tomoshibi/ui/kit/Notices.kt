package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.tomoshibi.basics.Theme
import java.text.BreakIterator
import kotlin.math.abs
import kotlin.math.roundToInt

// More of the app's own controls (Kit+Notices.swift), for the panels beside
// a call and the corner of it: a volume slider, small round buttons, a
// panel's heading, a person's monogram, and the layout a notice uses to be
// no wider than its words.

/**
 * How loud, from nothing to full: a track that fills in the foreground
 * colour, and a thumb that grows under the pointer and the finger
 * (`GlassSlider`). The web's is the browser's own range input tinted in the
 * foreground colour (SoundPanel.tsx `accent-fg`), and so is this: a volume is
 * not something switched on, and the signal colour is kept for what is.
 * `step` is the web's twentieths; a press, a key and a screen reader all move
 * it to the nearest one.
 */
@Composable
fun GlassSlider(
    value: Double,
    onValueChange: (Double) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    step: Double = 0.05,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val hover = remember { MutableInteractionSource() }
    val hovering by hover.collectIsHoveredAsState()
    var dragging by remember { mutableStateOf(false) }
    val lit = (hovering || dragging) && on
    val thumb by animateDpAsState(if (lit) 17.dp else 14.dp, Theme.settle(), label = "thumb")
    val track by animateDpAsState(if (lit) 6.dp else 5.dp, Theme.settle(), label = "track")
    val shown by animateFloatAsState(value.coerceIn(0.0, 1.0).toFloat(), Motion.follow(), label = "value")
    val shadow by animateDpAsState(if (dragging) 5.dp else 3.dp, Theme.settle(), label = "shadow")
    fun set(raw: Double) {
        val snapped = (raw.coerceIn(0.0, 1.0) / step).roundToInt() * step
        if (abs(snapped - value) > 0.0001) onValueChange(snapped.coerceIn(0.0, 1.0))
    }
    BoxWithConstraints(
        modifier
            .height(22.dp)
            .alpha(if (on) 1f else 0.4f)
            .hoverable(hover, on)
            .sliderSemantics(label, value, step, on) { set(it) }
            .focusable(on)
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                when (event.key) {
                    Key.DirectionLeft -> { set(value - step); true }
                    Key.DirectionRight -> { set(value + step); true }
                    else -> false
                }
            }
            .pointerInput(on) {
                if (!on) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val half = thumb.toPx() / 2
                    val travel = maxOf(1f, size.width - thumb.toPx())
                    dragging = true
                    try {
                        set(((down.position.x - half) / travel).toDouble())
                        horizontalDrag(down.id) { change ->
                            set(((change.position.x - half) / travel).toDouble())
                            change.consume()
                        }
                    } finally {
                        dragging = false
                    }
                }
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        val travel = (maxWidth - thumb).coerceAtLeast(1.dp)
        val x = thumb / 2 + travel * shown
        Box(
            Modifier
                .fillMaxWidth()
                .height(track)
                .background(Theme.recess(0.28f), Capsule)
                .border(1.dp, Theme.wash.copy(alpha = 0.10f), Capsule),
        )
        // One flat tone, as the web's range input fills: it faded in from the
        // left until the owner asked for fewer gradients (2026-10-20).
        Box(
            Modifier
                .size(maxOf(track, x), track)
                .background(Theme.foreground.copy(alpha = 0.9f), Capsule),
        )
        Box(
            Modifier
                .offset(x = x - thumb / 2)
                .size(thumb)
                .dropShadow(CircleShape, Shadow(radius = shadow, offset = DpOffset(0.dp, 1.dp), alpha = 0.35f))
                .background(Color.White, CircleShape)
                .border(0.5.dp, Color.Black.copy(alpha = 0.12f), CircleShape),
        )
    }
}

/**
 * What a screen reader is told of a slider: its name, where it is as a
 * percentage, and the twentieths it moves in.
 */
internal fun Modifier.sliderSemantics(label: String?, value: Double, step: Double, enabled: Boolean, set: (Double) -> Unit) =
    semanticsOf {
        if (label != null) contentDescription = label
        stateDescription = "${(value * 100).roundToInt()}%"
        progressBarRangeInfo = ProgressBarRangeInfo(value.toFloat(), 0f..1f, ((1 / step).roundToInt() - 1).coerceAtLeast(0))
        if (!enabled) disabled()
        setProgress { target ->
            set(target.toDouble())
            true
        }
    }

private fun Modifier.semanticsOf(block: androidx.compose.ui.semantics.SemanticsPropertyReceiver.() -> Unit) =
    then(Modifier.clearAndSetSemantics(block))

/** What a small round button is for: most close or act, one may remove for good. */
enum class RoundRole { Normal, Destructive }

/**
 * A small round button with a symbol: a panel's close, a row's actions
 * (`RoundButton`). Lit under the pointer and the finger; a destructive one
 * reddens there, so the colour that means leaving is only on what is about
 * to be pressed. `faint` is pressable but drawn faint, for a press that will
 * only be refused with a reason (making a guest host).
 */
@Composable
fun RoundButton(
    symbol: String,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 28.dp,
    role: RoundRole = RoundRole.Normal,
    faint: Boolean = false,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val lit = press.lit && on
    val ink by animateColorAsState(
        if (!lit) Theme.muted else if (role == RoundRole.Destructive) Theme.dangerInk else Theme.foreground,
        Motion.hover(), label = "ink",
    )
    val fill by animateColorAsState(
        if (!lit) Theme.wash.copy(alpha = 0f) else if (role == RoundRole.Destructive) Theme.danger.copy(alpha = 0.16f)
        else Theme.wash.copy(alpha = 0.10f),
        Motion.hover(), label = "fill",
    )
    val edge by animateFloatAsState(if (lit) 0.12f else 0f, Motion.hover(), label = "edge")
    // Pressed over a fingertip in a narrow window, drawn at its own size (fingertip).
    Box(
        modifier
            .alpha(if (!on) DisabledAlpha else if (faint) 0.45f else 1f)
            .size(fingertip(size))
            .pressable(press, CircleShape, on, Role.Button, label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(size)
                .background(fill, CircleShape)
                .border(1.dp, Theme.wash.copy(alpha = edge), CircleShape),
            contentAlignment = Alignment.Center,
        ) { Symbol(symbol, size = size.value * 0.43f, weight = Font.Semibold, tint = ink) }
    }
}

/** What a pill is for: most answer, one says yes in the colour, one cannot be undone. */
enum class PillRole { Normal, Prominent, Destructive }

/**
 * Words in a small filled capsule, for a row's answer inside a pane that is
 * already glass (glass on glass reads as smudge): the person's colour for
 * the one that says yes, a lit plate for the other, red for the one that
 * cannot be undone (`PillButton`). `wide` stretches it across what it is
 * given, for a pair that shares a row.
 */
@Composable
fun PillButton(
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    role: PillRole = PillRole.Normal,
    busy: Boolean = false,
    height: Dp = 26.dp,
    wide: Boolean = false,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val lift = press.lit && on
    val ink = when (role) {
        PillRole.Normal -> Theme.foreground
        PillRole.Prominent -> Theme.tallyForeground
        PillRole.Destructive -> Theme.dangerForeground
    }
    val fill by animateColorAsState(
        when (role) {
            PillRole.Normal -> Theme.wash.copy(alpha = if (lift) 0.16f else 0.10f)
            PillRole.Prominent -> Theme.tally.copy(alpha = if (lift) 0.88f else 1f)
            PillRole.Destructive -> Theme.danger.copy(alpha = if (lift) 0.88f else 1f)
        },
        Motion.hover(), label = "fill",
    )
    val small = height < 28.dp
    Row(
        modifier
            .alpha(if (on) 1f else 0.4f)
            .then(if (wide) Modifier.fillMaxWidth() else Modifier)
            .heightIn(min = height)
            .background(fill, Capsule)
            .border(1.dp, if (role == PillRole.Normal) Theme.wash.copy(alpha = 0.08f) else Color.White.copy(alpha = 0.18f), Capsule)
            .pressable(press, Capsule, on, Role.Button, onClick = onClick)
            .padding(horizontal = height * 0.42f),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnimatedVisibility(
            busy,
            enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f),
            exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.6f),
        ) { MediaSpinner(Modifier.padding(end = 6.dp), size = if (small) 11f else 12f, tint = ink) }
        KitText(
            title,
            font = Font(if (small) 11.5f else 12.5f, if (role == PillRole.Normal) Font.Medium else Font.Semibold),
            color = ink,
            lines = 1,
        )
    }
}

/**
 * The top of a panel beside the call: a symbol, its name, and the close
 * (`PanelHeading`). `trailing` goes before the close, which is thirty-two
 * across, the web's panels' (2026-10-29), and pressed over a fingertip on a
 * phone (RoundButton): at twenty-six it was the smallest close in the app.
 */
@Composable
fun PanelHeading(
    title: String,
    closeLabel: String,
    close: () -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    symbolTint: Color = Theme.foreground.copy(alpha = 0.85f),
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = 14.dp, end = 10.dp, top = 10.dp, bottom = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Box(
                Modifier
                    .size(26.dp)
                    .background(Theme.wash.copy(alpha = 0.09f), rounded(8f)),
                contentAlignment = Alignment.Center,
            ) { Symbol(symbol, size = 12f, weight = Font.Semibold, tint = symbolTint) }
        }
        KitText(title, Modifier.weight(1f), font = Font(14f, Font.Semibold), lines = 1)
        trailing()
        RoundButton("xmark", closeLabel, close, size = 32.dp)
    }
}

/**
 * The hairline under a panel's heading, in the palette's border colour from
 * edge to edge (`PanelRule`); it faded out at both ends before the gradients
 * went (2026-10-20).
 */
@Composable
fun PanelRule(modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .height(1.dp)
            .background(Theme.border),
    )
}

/**
 * A group of rows inside a panel: a faint plate of its own on the panel's
 * glass, edged in `tint` where the group is about something happening (the
 * door) or about to (ending the meeting) (`.panelCard(tint:)`).
 */
@Composable
fun Modifier.panelCard(tint: Color? = null): Modifier {
    val shape = ContinuousShape(16.dp)
    val wash = Theme.wash
    return background(tint?.copy(alpha = 0.10f) ?: Theme.plate(0.05f), shape)
        .border(1.dp, tint?.copy(alpha = 0.35f) ?: wash.copy(alpha = 0.07f), shape)
}

/**
 * Somebody, small: their initials on one of the web's avatar colours
 * (live/avatar.ts `AVATAR_COLOURS`), chosen by their identity so the same
 * person is the same colour in every corner and on every platform
 * (`Monogram`).
 *
 * It was a blend of two of them, and the owner found the gradients cheap
 * (2026-10-20). The colour kept is the blend's first, the one the hash has
 * always picked first, so nobody changes colour.
 */
@Composable
fun Monogram(name: String, identity: String, modifier: Modifier = Modifier, size: Dp = 24.dp) {
    val colour = remember(identity) { Monograms.colour(identity) }
    val letters = remember(name) { Monograms.initials(name) }
    Box(
        modifier
            .size(size)
            .clearAndSetSemantics {}
            .background(colour, CircleShape)
            .border(0.75.dp, Color.White.copy(alpha = 0.14f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        val full = size.value * 0.40f
        BasicText(
            letters,
            Modifier.padding(horizontal = size * 0.12f),
            style = TextStyle(color = Color.White.copy(alpha = 0.94f), fontWeight = FontWeight.SemiBold),
            maxLines = 1,
            // `.minimumScaleFactor(0.6)`: two wide letters shrink to fit
            // rather than running out of the circle.
            autoSize = TextAutoSize.StepBased(minFontSize = (full * 0.6f).sp, maxFontSize = full.sp, stepSize = 0.25.sp),
        )
    }
}

/** The rules a monogram is drawn by, kept apart from the drawing for a test to hold. */
object Monograms {
    /** "#8a6a45", "#a1584a", "#5d6b58", "#4b5a6b", "#96814f", "#6b5566". */
    val colours = listOf(
        Color(0xFF8A6A45), Color(0xFFA1584A), Color(0xFF5D6B58),
        Color(0xFF4B5A6B), Color(0xFF96814F), Color(0xFF6B5566),
    )

    /**
     * Which of [colours]: FNV-1a over the identity's UTF-8, as the tile's
     * avatar hashes it, in 64-bit unsigned arithmetic exactly as Swift's
     * `&*` wraps it, modulo their number (TomoshibiBasics' `Monograms.index`).
     * Any other hash puts the same person in another colour on Android than
     * on the iPhone and the web. It is the first of the two the blend was
     * drawn from.
     */
    fun index(identity: String): Int {
        var hash = 1469598103934665603UL
        for (byte in identity.encodeToByteArray()) hash = (hash xor byte.toUByte().toULong()) * 1099511628211UL
        return (hash % colours.size.toULong()).toInt()
    }

    fun colour(identity: String): Color = colours[index(identity)]

    /**
     * The first letters of the first two words, in capitals; a question mark
     * for a name with none. A letter is what a reader sees as one (a
     * grapheme), so a name that starts with an emoji or an accented letter
     * keeps it whole, as Swift's `Character` does.
     */
    fun initials(name: String): String {
        val words = name.split { it.isWhitespace() }
        val letters = words.take(2).map { firstGrapheme(it) }
        return if (letters.isEmpty()) "?" else letters.joinToString("").uppercase()
    }

    private fun String.split(isSeparator: (Char) -> Boolean): List<String> {
        val out = ArrayList<String>()
        var start = -1
        for (i in indices) {
            if (isSeparator(this[i])) {
                if (start >= 0) out.add(substring(start, i))
                start = -1
            } else if (start < 0) {
                start = i
            }
        }
        if (start >= 0) out.add(substring(start))
        return out
    }

    internal fun firstGrapheme(word: String): String {
        val breaks = BreakIterator.getCharacterInstance()
        breaks.setText(word)
        val end = breaks.next()
        return if (end == BreakIterator.DONE) word else word.substring(0, end)
    }
}

/**
 * As wide as what it holds wants, within `minimum` and `maximum`, and as
 * tall as that width makes it (`FitWidth`): a notice no wider than its
 * words, and words that wrap at the maximum rather than running out of the
 * card.
 */
@Composable
fun FitWidth(minimum: Dp = 0.dp, maximum: Dp, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Layout(content, modifier) { measurables, constraints ->
        val only = measurables.firstOrNull() ?: return@Layout layout(0, 0) {}
        val ceiling = minOf(maximum.roundToPx(), constraints.maxWidth)
        val ideal = only.maxIntrinsicWidth(Constraints.Infinity)
        val width = maxOf(minOf(minimum.roundToPx(), ceiling), minOf(ideal, ceiling))
        val placed = only.measure(Constraints(minWidth = width, maxWidth = width, maxHeight = constraints.maxHeight))
        layout(width, placed.height) { placed.place(0, 0) }
    }
}
