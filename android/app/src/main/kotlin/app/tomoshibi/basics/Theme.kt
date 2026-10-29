package app.tomoshibi.basics

import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.dp

/**
 * The colours of one appearance, the same numbers as Theme.swift and the
 * daylight palette every platform shares (2026-10-26).
 *
 * The dark is the app as it always was: a call is looked at for an hour at a
 * stretch and the pictures are the only thing in it that should be bright.
 * The owner asked for a day mode beside it, the same screens in the light,
 * day and night following the system. The neutrals are Apple's own greys of
 * either appearance, so that the same app looks the same on a phone of
 * either kind. One signal colour, the "tally" a studio camera lights when it
 * is live, chosen by the person (Appearance); red appears on leaving and on
 * errors and nowhere else; green only on a connection light.
 *
 * Every dark value is exactly what it was before there was a light one: the
 * dark screens are held to be pixel for pixel what they were.
 */
@Immutable
class Palette private constructor(
    /** 0 in the dark, 1 in the light, and between the two while the system turns from one to the other. */
    val daylight: Float,
    val background: Color,
    val surface: Color,
    val surfaceHigh: Color,
    val border: Color,
    val foreground: Color,
    val muted: Color,
    val good: Color,
    val danger: Color,
    val dangerForeground: Color,
    /**
     * `good` and `danger` as words or a thin symbol on the page (a latency
     * figure, an error line): themselves in the dark, a step darker in the
     * light, where the connection green was about 2.2 to one on the page
     * and the red about 3.3 (the palette's addendum). The light, the dot,
     * the red button keep `good` and `danger`.
     */
    val goodInk: Color,
    val dangerInk: Color,
    /**
     * What a screen stands on under its cards: black in the dark, where the
     * phone's lobby and its sheets have always stood on black, and the
     * page's own grey in the light, the cards white on it.
     */
    val ground: Color,
    /**
     * What the translucent fills and hairlines of the interface are made of:
     * white over the dark greys, black over the light ones, at the same
     * strength. A plate at seven per cent of it is as faint in either.
     */
    val wash: Color,
) {
    /** Whether this is (mostly) the dark appearance, for a choice that cannot be blended. */
    val dark: Boolean get() = daylight < 0.5f

    /** The person's colour in this appearance, as a fill: Apple's light variant of the same colour in the light. */
    fun tally(accent: Accent): Color = mix(accent.color, accent.light, daylight)

    /**
     * The person's colour as words or a thin symbol: the fill itself in the
     * dark, the darker ink in the light (Accent.ink), so that a link, a
     * tick or a figure in it reads on the page.
     */
    fun tallyInk(accent: Accent): Color = mix(accent.color, accent.ink, daylight)

    /**
     * A recess in a surface (a field, a track, the ground of a segmented
     * choice) at the strength it has in the dark: black at that alpha there,
     * and a quarter as much in the light, where black at a fifth of the way
     * is a grey slab rather than a well.
     */
    fun recess(alpha: Float): Color = Color.Black.copy(alpha = alpha * (1f - 0.75f * daylight))

    /**
     * A plate raised off what it sits on (a card of settings, a pane's
     * content, a group of rows in a panel) at the strength it has in the
     * dark: white at that alpha there, lighter than the grey under it; in the
     * light, white at nearly three quarters, lighter than the page as the
     * light's cards are. [wash] would have made it darker than the page, a
     * grey card on a grey page, which reads as sunk rather than raised. On a
     * white card it is all but nothing, and its edge says where it is.
     */
    fun plate(alpha: Float): Color = when {
        daylight <= 0f -> Color.White.copy(alpha = alpha)
        else -> Color.White.copy(alpha = alpha + (0.72f - alpha) * daylight)
    }

    /**
     * How dark a shadow or a veil is, given what it is in the dark: half as
     * much in the light, where the dark's shadows read as smudges and its
     * veils as night falling over the page.
     */
    fun shade(alpha: Float): Float = alpha * (1f - 0.5f * daylight)

    companion object {
        /** The app as it always was. */
        val Dark = Palette(
            daylight = 0f,
            background = Color(0.043f, 0.043f, 0.051f),
            surface = Color(0.110f, 0.110f, 0.118f),
            surfaceHigh = Color(0.173f, 0.173f, 0.180f),
            border = Color.White.copy(alpha = 0.10f),
            foreground = Color(0.961f, 0.961f, 0.969f),
            muted = Color(0.922f, 0.922f, 0.961f).copy(alpha = 0.6f),
            good = Color(0.188f, 0.820f, 0.345f),
            danger = Color(1.0f, 0.271f, 0.227f),
            dangerForeground = Color.White,
            goodInk = Color(0.188f, 0.820f, 0.345f),
            dangerInk = Color(1.0f, 0.271f, 0.227f),
            ground = Color.Black,
            wash = Color.White,
        )

        /** The daylight palette's light column (#F2F2F7 and its kin). */
        val Light = Palette(
            daylight = 1f,
            background = Color(0xFFF2F2F7),
            surface = Color(0xFFFFFFFF),
            surfaceHigh = Color(0xFFE5E5EA),
            border = Color.Black.copy(alpha = 0.10f),
            foreground = Color(0xFF1C1C1E),
            // Secondary words at three quarters rather than Apple's sixty per
            // cent: at sixty they were about 3.2 to one on the page and on a
            // white card, under the 4.5 words are read at (the web's daylight
            // check, 2026-10-29, which moved its own to the same value).
            muted = Color(60, 60, 67).copy(alpha = 0.75f),
            good = Color(0xFF34C759),
            danger = Color(0xFFFF3B30),
            dangerForeground = Color.White,
            goodInk = Color(0xFF217E38),
            dangerInk = Color(0xFFD70015),
            ground = Color(0xFFF2F2F7),
            wash = Color.Black,
        )

        /**
         * The two blended `daylight` of the way from the dark to the light,
         * for the moment the system turns over. Either end is that palette
         * itself, not a colour worked out again on the way: a dark value
         * taken through the blend's colour space came back a shade off.
         */
        fun between(daylight: Float): Palette = when {
            daylight <= 0f -> Dark
            daylight >= 1f -> Light
            else -> Palette(
                daylight,
                lerp(Dark.background, Light.background, daylight),
                lerp(Dark.surface, Light.surface, daylight),
                lerp(Dark.surfaceHigh, Light.surfaceHigh, daylight),
                lerp(Dark.border, Light.border, daylight),
                lerp(Dark.foreground, Light.foreground, daylight),
                lerp(Dark.muted, Light.muted, daylight),
                lerp(Dark.good, Light.good, daylight),
                lerp(Dark.danger, Light.danger, daylight),
                lerp(Dark.dangerForeground, Light.dangerForeground, daylight),
                lerp(Dark.goodInk, Light.goodInk, daylight),
                lerp(Dark.dangerInk, Light.dangerInk, daylight),
                lerp(Dark.ground, Light.ground, daylight),
                lerp(Dark.wash, Light.wash, daylight),
            )
        }

        private fun mix(dark: Color, light: Color, daylight: Float): Color = when {
            daylight <= 0f -> dark
            daylight >= 1f -> light
            else -> lerp(dark, light, daylight)
        }
    }
}

/**
 * The palette what is drawn here is drawn in. The dark where nothing has
 * said otherwise: a view drawn outside the app's root (the call's bubble
 * over other apps, a test, a preview) is drawn as the app always was.
 */
val LocalPalette = compositionLocalOf { Palette.Dark }

/**
 * The apps' palette and motion, the same numbers as Theme.swift: the colours
 * of the palette the view is drawn in (LocalPalette), and the curves.
 */
object Theme {
    /**
     * Whether a meeting stays dark in the daylight: the room and its stage,
     * its control bar, everything opened from inside it, and the small
     * window it shrinks to, as FaceTime and Meet keep theirs. The one place
     * to turn it, for the owner, who may want the call in the light one day
     * (ui/App.kt reads it for the call's screen, and nothing else does).
     */
    const val MEETING_STAYS_DARK = true

    val background: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.background
    val surface: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surface
    val surfaceHigh: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.surfaceHigh
    val border: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.border
    val foreground: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.foreground
    val muted: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.muted
    val good: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.good
    val danger: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.danger
    val dangerForeground: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.dangerForeground
    val goodInk: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.goodInk
    val dangerInk: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.dangerInk
    val ground: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.ground
    val wash: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.wash

    /**
     * The palette a meeting is drawn in, given the one around it: the dark
     * while [MEETING_STAYS_DARK], the one around it otherwise. For the call
     * and for the small pictures of one the settings draw.
     */
    fun meeting(around: Palette): Palette = if (MEETING_STAYS_DARK) Palette.Dark else around

    /** Whether the view is drawn in the dark, for a choice that cannot be blended. */
    val dark: Boolean @Composable @ReadOnlyComposable get() = LocalPalette.current.dark

    /** The colour the person chose, read where it is drawn, in the appearance it is drawn in: as a fill. */
    val tally: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.tally(Appearance.shared.accent)

    /** The colour the person chose as words or a thin symbol on the page ([Palette.tallyInk]). */
    val tallyInk: Color @Composable @ReadOnlyComposable get() = LocalPalette.current.tallyInk(Appearance.shared.accent)
    val tallyForeground: Color get() = Appearance.shared.accent.onColor

    /** A recess at the strength it has in the dark ([Palette.recess]). */
    @Composable @ReadOnlyComposable
    fun recess(alpha: Float): Color = LocalPalette.current.recess(alpha)

    /** A raised plate at the strength it has in the dark ([Palette.plate]). */
    @Composable @ReadOnlyComposable
    fun plate(alpha: Float): Color = LocalPalette.current.plate(alpha)

    /** A shadow's or a veil's alpha, given the dark's ([Palette.shade]). */
    @Composable @ReadOnlyComposable
    fun shade(alpha: Float): Float = LocalPalette.current.shade(alpha)

    /**
     * Words and symbols laid over a picture (a name on a tile, a control on
     * a video): the dark's own light ink whatever the appearance, since what
     * is under them is a picture and not the page.
     */
    val onPicture: Color get() = Palette.Dark.foreground

    /** The tile's corner, 0.5rem. */
    val tileRadius = 8.dp

    /** Arrivals decelerate into place (--ease-settle, 320 ms). */
    val settleEasing = CubicBezierEasing(0.2f, 0.9f, 0.3f, 1f)

    /** Departures need no such courtesy (--ease-leave, 200 ms). */
    val leaveEasing = CubicBezierEasing(0.4f, 0f, 1f, 1f)

    fun <T> settle(): FiniteAnimationSpec<T> = tween(320, easing = settleEasing)
    fun <T> leave(): FiniteAnimationSpec<T> = tween(200, easing = leaveEasing)

    /**
     * Every appearance and disappearance uses these: the person who owns
     * this product does not accept a view that simply pops in or out.
     */
    val arrive: EnterTransition get() = fadeIn(settle()) + scaleIn(settle(), initialScale = 0.97f)
    val depart: ExitTransition get() = fadeOut(leave())

    val slideUp: EnterTransition get() = slideInVertically(settle()) { it } + fadeIn(settle())
    val slideDown: ExitTransition get() = slideOutVertically(leave()) { it } + fadeOut(leave())

    val slideIn: EnterTransition get() = slideInHorizontally(settle()) { it } + fadeIn(settle())
    val slideOut: ExitTransition get() = slideOutHorizontally(leave()) { it } + fadeOut(leave())
}
