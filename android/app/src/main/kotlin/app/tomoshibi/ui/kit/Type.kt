package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.tomoshibi.basics.Theme

// The words and the colour they are drawn in, the way SwiftUI hands
// `.font(...)` and `.foregroundStyle(...)` down to whatever Text and Image
// are inside a view. A button sets its ink once and the label inside it,
// words or a symbol, is drawn in it without being told.

/**
 * The colour words and symbols are drawn in where nothing says otherwise:
 * SwiftUI's foreground style, inherited.
 */
val LocalInk = compositionLocalOf { Theme.foreground }

/**
 * Whether what is inside may be pressed: SwiftUI's `isEnabled`, which
 * `.disabled(true)` on any view turns off for everything in it. Every
 * pressable piece of the kit reads it as well as its own `enabled`.
 */
val LocalEnabled = compositionLocalOf { true }

/** `.disabled(!enabled)` on a whole group of views. */
@Composable
fun KitEnabled(enabled: Boolean, content: @Composable () -> Unit) {
    val outer = LocalEnabled.current
    CompositionLocalProvider(LocalEnabled provides (outer && enabled), content = content)
}

/** `.foregroundStyle(ink)` on a group of views. */
@Composable
fun KitInk(ink: Color, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalInk provides ink, content = content)
}

/**
 * The size and weight words inside are drawn at: SwiftUI's `.font(...)`,
 * inherited, which a button sets for whatever label it is given.
 */
val LocalFont = staticCompositionLocalOf { Font(13.5f) }

/**
 * A system font at a point size, as `.system(size:weight:design:)` names it.
 * The size is in points and is drawn in sp: on Android a point and a dp are
 * the same length, and sp follows the person's text size the way Android
 * apps are expected to.
 */
data class Font(
    val size: Float,
    val weight: FontWeight = FontWeight.Normal,
    val mono: Boolean = false,
    /** `.monospacedDigit()`: figures that do not jostle as they change. */
    val digits: Boolean = false,
    /** `.tracking(...)`, in points. */
    val tracking: Float = 0f,
) {
    fun style(color: Color): TextStyle = TextStyle(
        color = color,
        fontSize = size.sp,
        fontWeight = weight,
        fontFamily = if (mono) FontFamily.Monospace else FontFamily.SansSerif,
        fontFeatureSettings = if (digits) "tnum" else null,
        letterSpacing = if (tracking == 0f) 0.sp else (tracking / size).em,
        // A line as tall as SF Pro's at the same size (about 1.2 of it), so
        // a stack of words takes the room it takes on the iPhone; Roboto's
        // own is taller.
        lineHeight = (size * 1.22f).sp,
        lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
    )

    companion object {
        val Regular = FontWeight.Normal
        val Medium = FontWeight.Medium
        val Semibold = FontWeight.SemiBold
        val Bold = FontWeight.Bold
    }
}

/** `.font(font)` on a group of views. */
@Composable
fun KitFont(font: Font, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalFont provides font, content = content)
}

/**
 * Words, in the inherited font and ink unless told: SwiftUI's `Text`.
 * `lines` is `.lineLimit`; with nothing given the words wrap as they need,
 * which is what `.fixedSize(horizontal: false, vertical: true)` asks for.
 */
@Composable
fun KitText(
    text: String,
    modifier: Modifier = Modifier,
    font: Font = LocalFont.current,
    color: Color = LocalInk.current,
    lines: Int = Int.MAX_VALUE,
    align: TextAlign? = null,
    overflow: TextOverflow = TextOverflow.Ellipsis,
    fit: Boolean = false,
) {
    val style = font.style(color).let { if (align != null) it.copy(textAlign = align) else it }
    BasicText(
        text = text, modifier = modifier, style = style, maxLines = lines, overflow = overflow,
        // SwiftUI's `.minimumScaleFactor(0.75)`: words that are a little too
        // long for their place, a segment's or a field's prompt on the
        // Fold's cover screen, made a little smaller rather than cut short.
        autoSize = if (fit) TextAutoSize.StepBased(minFontSize = (font.size * 0.75f).sp, maxFontSize = font.size.sp, stepSize = 0.25.sp) else null,
    )
}

/**
 * A view that arrives and leaves rather than popping in and out, wherever it
 * is: `AnimatedVisibility` with no row or column to belong to, which inside a
 * box inside a row Compose would otherwise refuse to choose.
 */
@Composable
fun Appear(
    visible: Boolean,
    modifier: Modifier = Modifier,
    enter: EnterTransition = Theme.arrive,
    exit: ExitTransition = Theme.depart,
    content: @Composable AnimatedVisibilityScope.() -> Unit,
) {
    AnimatedVisibility(visible, modifier, enter, exit, content = content)
}

/** The curves the kit moves with beyond Theme's two. */
object Motion {
    /** SwiftUI's `.easeOut(duration:)`, the curve of a light under the pointer. */
    val easeOut = CubicBezierEasing(0f, 0f, 0.58f, 1f)

    /** `.easeOut(duration: 0.12)`: a row or button lighting under a finger. */
    fun <T> hover(): FiniteAnimationSpec<T> = tween(120, easing = easeOut)

    /** `.easeOut(duration: 0.08)`: a level or a slider following a value. */
    fun <T> follow(): FiniteAnimationSpec<T> = tween(80, easing = easeOut)
}
