package app.tomoshibi.ui

import android.app.Activity
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateValueAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import app.tomoshibi.basics.Theme

// A foldable half open (Galaxy Z Fold, Pixel Fold, a Flip): the window
// library's folding feature, read once at the app's root (App.kt) and handed
// down as LocalHinge, so that each screen asks where the hinge is rather than
// asking the window again. Nothing in the Swift or the web knows about a
// hinge; the rule here is the one every foldable's guidance gives and the
// owner asked for on his Fold6: nothing that would be read bent (a line of
// text, a button, a sheet, a face) lies across the fold. Standing on a table
// (the hinge across) the upper half is what is looked at and the lower half
// what is touched and typed into, the keyboard included; held open as a book
// (the hinge down) the two halves are two pages.

/**
 * A foldable's hinge where it divides the window: half open, as a book or
 * on a table. In the window's own pixels, which are the screen's own pixels
 * here, since the app draws from edge to edge. Null where there is none, or
 * where the screen is flat and the hinge divides nothing (an unfolded Fold
 * lying flat reports its fold as not separating).
 */
@Immutable
data class Hinge(val left: Float, val top: Float, val right: Float, val bottom: Float, val vertical: Boolean)

/**
 * Where the hinge is, for everything drawn under the app's root. A test
 * provides one around the app to stand a phone on its hinge, since
 * Robolectric folds nothing: the root uses the window's own while the window
 * reports one, and whatever is provided above it otherwise.
 */
val LocalHinge = compositionLocalOf<Hinge?> { null }

/** How the phone stands, as far as a layout cares. */
enum class Posture {
    /** Flat, closed, or not a foldable at all. */
    Flat,

    /** The hinge across the window: standing on a table, the upper half up (Samsung's Flex mode). */
    Tabletop,

    /** The hinge down the window: held open as a book. */
    Book,
}

val Hinge?.posture: Posture
    get() = when {
        this == null -> Posture.Flat
        vertical -> Posture.Book
        else -> Posture.Tabletop
    }

/** The hinge in points from the window's top left. */
fun Hinge.inPoints(density: Density): Rect = with(density) {
    Rect(left.toDp().value, top.toDp().value, right.toDp().value, bottom.toDp().value)
}

/**
 * The least a half has to be, along the axis the hinge divides, to be laid
 * out as a screen of its own. Less than this (a window in split screen that
 * only just reaches over the fold) and the window is treated as flat: a
 * strip of a pane is somewhere nothing fits, which is worse than a fold.
 */
const val LEAST_HALF = 200f

/** Which half a part of a screen keeps to while the hinge divides the window. */
enum class Pane {
    /**
     * What is read and typed into: the upper half on a table, where the
     * keyboard does not come, and the page at the reading end of a book
     * (the right, where the lobby's sidebar has the left).
     */
    Reading,

    /** What sits at the foot of a screen, a notice: the lower half on a table, the reading page of a book. */
    Foot,

    /** The upper half on a table, the starting page of a book. */
    First,

    /** The lower half on a table, the ending page of a book. */
    Second,
}

/**
 * The part of a window [width] by [height] points that [pane] is while
 * [this] hinge divides it, in points from the window's top left; the whole
 * window where there is no hinge, or where one of its halves would be too
 * small to be a screen.
 */
fun Hinge?.pane(width: Float, height: Float, density: Density, direction: LayoutDirection, pane: Pane): Rect {
    val whole = Rect(0f, 0f, width, height)
    val hinge = this?.inPoints(density) ?: return whole
    if (this.vertical) {
        if (hinge.left < LEAST_HALF || width - hinge.right < LEAST_HALF) return whole
        val left = Rect(0f, 0f, hinge.left, height)
        val right = Rect(hinge.right, 0f, width, height)
        val start = if (direction == LayoutDirection.Ltr) left else right
        val end = if (direction == LayoutDirection.Ltr) right else left
        return when (pane) {
            Pane.Reading, Pane.Foot, Pane.Second -> end
            Pane.First -> start
        }
    }
    if (hinge.top < LEAST_HALF || height - hinge.bottom < LEAST_HALF) return whole
    val upper = Rect(0f, 0f, width, hinge.top)
    val lower = Rect(0f, hinge.bottom, width, height)
    return when (pane) {
        Pane.Reading, Pane.First -> upper
        Pane.Foot, Pane.Second -> lower
    }
}

/**
 * The window's own posture with the halves too small to be screens left out,
 * which is the posture the layouts below lay out for.
 */
fun Hinge?.effectivePosture(width: Float, height: Float, density: Density): Posture {
    val hinge = this?.inPoints(density) ?: return Posture.Flat
    return if (this.vertical) {
        if (hinge.left < LEAST_HALF || width - hinge.right < LEAST_HALF) Posture.Flat else Posture.Book
    } else {
        if (hinge.top < LEAST_HALF || height - hinge.bottom < LEAST_HALF) Posture.Flat else Posture.Tabletop
    }
}

/**
 * A screen, or the part of one, that keeps to one half of the window while
 * the hinge divides it, moving there and back with the arrival curve as the
 * phone is opened out or stood up, so that a card does not jump from one
 * half to the other. Laid out over the whole window, which is where every
 * screen's root is (the app draws from edge to edge from the window's top
 * left).
 *
 * The system's bars, the cutout and the keyboard are left to the half they
 * fall in: the half is told that the window's edges beyond it are already
 * taken care of, so that a card on the upper half of a phone on a table is
 * not pushed up by a keyboard that came up on the lower half, which would
 * otherwise leave it a strip above the hinge.
 */
@Composable
fun KeptToOnePane(
    modifier: Modifier = Modifier,
    pane: Pane = Pane.Reading,
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val hinge = LocalHinge.current
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    BoxWithConstraints(modifier.fillMaxSize()) {
        val width = maxWidth.value
        val height = maxHeight.value
        val target = hinge.pane(width, height, density, direction, pane)
        val at by animateValueAsState(target, Rect.VectorConverter, Theme.settle(), label = "pane")
        CompositionLocalProvider(LocalInHalf provides (target.width < width || target.height < height)) {
            Box(Modifier.inPane(at, width, height), contentAlignment = contentAlignment, content = content)
        }
    }
}

/** A part of a screen moving to where the posture puts it, with the arrival curve. */
@Composable
fun Rect.settled(label: String): Rect {
    val now by animateValueAsState(this, Rect.VectorConverter, Theme.settle(), label = label)
    return now
}

/**
 * Whether what is drawn here keeps to one half of a folded window: a card
 * there is given less room around it, since the room it had was a
 * screen's and a half is half of one.
 */
val LocalInHalf = compositionLocalOf { false }

/**
 * Placed at [at], points from the top left of a window [width] by [height]
 * points, with the system's insets beyond it taken as already kept clear of
 * (KeptToOnePane).
 */
fun Modifier.inPane(at: Rect, width: Float, height: Float): Modifier = this
    .absoluteOffset(at.left.dp, at.top.dp)
    .size(at.width.coerceAtLeast(0f).dp, at.height.coerceAtLeast(0f).dp)
    .consumeWindowInsets(
        PaddingValues.Absolute(
            left = at.left.coerceAtLeast(0f).dp,
            top = at.top.coerceAtLeast(0f).dp,
            right = (width - at.right).coerceAtLeast(0f).dp,
            bottom = (height - at.bottom).coerceAtLeast(0f).dp,
        ),
    )

/**
 * Two parts of a screen that a flat screen stacks and a folded one lays on
 * either side of the hinge (the picture of yourself and the form that takes
 * you in, a heading and the fields under it): on a table, side by side on
 * the upper half, where the keyboard leaves them in sight; in a book, one
 * on each page. [flat] draws the screen as it is drawn unfolded; [lead] and
 * [form] are the two parts, given the half or the part of the half each
 * has. Cross-fades between the two as the phone is folded and opened.
 */
@Composable
fun Folded(
    modifier: Modifier = Modifier,
    leadShare: Float = 0.5f,
    flat: @Composable BoxScope.() -> Unit,
    lead: @Composable BoxScope.() -> Unit,
    form: @Composable BoxScope.() -> Unit,
) {
    val hinge = LocalHinge.current
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    BoxWithConstraints(modifier.fillMaxSize()) {
        val width = maxWidth.value
        val height = maxHeight.value
        val posture = hinge.effectivePosture(width, height, density)
        androidx.compose.animation.AnimatedContent(
            posture,
            transitionSpec = {
                androidx.compose.animation.fadeIn(Theme.settle()) togetherWith androidx.compose.animation.fadeOut(Theme.leave())
            },
            label = "folded",
        ) { now ->
            when (now) {
                Posture.Flat -> Box(Modifier.fillMaxSize(), content = flat)
                Posture.Book -> {
                    val first = hinge.pane(width, height, density, direction, Pane.First)
                    val second = hinge.pane(width, height, density, direction, Pane.Second)
                    CompositionLocalProvider(LocalInHalf provides true) {
                        Box(Modifier.inPane(first, width, height), content = lead)
                        Box(Modifier.inPane(second, width, height), content = form)
                    }
                }
                Posture.Tabletop -> {
                    val upper = hinge.pane(width, height, density, direction, Pane.First)
                    val split = upper.width * leadShare
                    val leading = Rect(upper.left, upper.top, upper.left + split, upper.bottom)
                    val trailing = Rect(upper.left + split, upper.top, upper.right, upper.bottom)
                    val (left, right) = if (direction == LayoutDirection.Ltr) leading to trailing else
                        Rect(upper.right - split, upper.top, upper.right, upper.bottom) to Rect(upper.left, upper.top, upper.right - split, upper.bottom)
                    CompositionLocalProvider(LocalInHalf provides true) {
                        Box(Modifier.inPane(left, width, height), content = lead)
                        Box(Modifier.inPane(right, width, height), content = form)
                    }
                }
            }
        }
    }
}

/**
 * Where the hinge is while it divides the window, from the window library's
 * folding feature: half open (a book held open, a phone on a table), the
 * two halves are two screens to lay out for, and a sheet or a column of the
 * wide lobby lying across the fold is read bent.
 */
@Composable
internal fun rememberHinge(activity: Activity): Hinge? {
    val flow = remember(activity) { WindowInfoTracker.getOrCreate(activity).windowLayoutInfo(activity) }
    val layout by flow.collectAsStateWithLifecycle(initialValue = null)
    val fold = layout?.displayFeatures?.filterIsInstance<FoldingFeature>()?.firstOrNull { it.isSeparating } ?: return null
    val bounds = fold.bounds
    return Hinge(
        bounds.left.toFloat(), bounds.top.toFloat(), bounds.right.toFloat(), bounds.bottom.toFloat(),
        vertical = fold.orientation == FoldingFeature.Orientation.VERTICAL,
    )
}
