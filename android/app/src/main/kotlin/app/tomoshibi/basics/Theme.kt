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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * The apps' palette and motion, the same numbers as Theme.swift.
 *
 * Dark in every system appearance, on purpose: a call is looked at for an
 * hour at a stretch and the pictures are the only thing in it that should be
 * bright. The neutrals are Apple's own dark greys, so that the same app looks
 * the same on a phone of either kind. One signal colour, the "tally" a studio
 * camera lights when it is live, chosen by the person (Appearance); red
 * appears on leaving and on errors and nowhere else; green only on a
 * connection light.
 */
object Theme {
    val background = Color(0.043f, 0.043f, 0.051f)
    val surface = Color(0.110f, 0.110f, 0.118f)
    val surfaceHigh = Color(0.173f, 0.173f, 0.180f)
    val border = Color.White.copy(alpha = 0.10f)
    val foreground = Color(0.961f, 0.961f, 0.969f)
    val muted = Color(0.922f, 0.922f, 0.961f).copy(alpha = 0.6f)
    val good = Color(0.188f, 0.820f, 0.345f)
    val danger = Color(1.0f, 0.271f, 0.227f)
    val dangerForeground = Color.White

    /** The colour the person chose, read where it is drawn. */
    val tally: Color get() = Appearance.shared.accent.color
    val tallyForeground: Color get() = Appearance.shared.accent.onColor

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
