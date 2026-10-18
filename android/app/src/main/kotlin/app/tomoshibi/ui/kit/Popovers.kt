package app.tomoshibi.ui.kit

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberTransition
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import app.tomoshibi.basics.Theme

// The app's own popovers (Kit.swift `Popovers`, `glassPopover`;
// Kit+Controls.swift `barPopover`; Kit+Settings.swift `floatingPopover`).
//
// Drawn rather than borrowed, as the owner asked (2026-10-17): a pane of
// glass hung off its control, arriving and leaving with the app's two
// curves. Each is drawn in a popup window of its own, above everything on
// the screen, which is what Kit+Settings.swift had to build a layer at the
// page's root for: a scroll view clips what it holds, so a choice near the
// foot of a settings card opened into nothing, and a later card drew over an
// earlier card's panel. The window takes the press outside it and closes the
// popover with it, as the Swift catcher does, so the press that closes one
// does not also press whatever was under it; Back and Escape close it too.

/**
 * Which of the screen's popovers is open: at most one, as with the system's
 * own, so opening one closes another.
 */
@Stable
class Popovers {
    var open: String? by mutableStateOf(null)
        private set

    fun toggle(id: String) {
        open = if (open == id) null else id
    }

    fun show(id: String) {
        open = id
    }

    fun close() {
        if (open != null) open = null
    }
}

@Composable
fun rememberPopovers(): Popovers = remember { Popovers() }

/** The screen's popovers, for the pieces that open one (`@Environment(\.popovers)`). */
val LocalPopovers = compositionLocalOf<Popovers?> { null }

/**
 * A page that keeps popovers (`.floatingPopoverLayer(_:)`): hands them to
 * what is on it, and closes the open one when the page goes, so that none is
 * left over the next.
 */
@Composable
fun FloatingPopoverLayer(popovers: Popovers, content: @Composable () -> Unit) {
    DisposableEffect(popovers) { onDispose { popovers.close() } }
    CompositionLocalProvider(LocalPopovers provides popovers, content = content)
}

/** Which side of its control a popover opens on (`VerticalEdge`). */
enum class PopoverEdge { Top, Bottom }

/** How a popover lines up with its control (`HorizontalAlignment`). */
enum class PopoverAlignment { Leading, Center, Trailing }

/** Which way the call's bar runs: along the foot, or down the side. */
enum class BarAxis { Horizontal, Vertical }

/**
 * Where along its control a bar's popover sits: centred on it, or flush with
 * its end (the trailing edge along the foot, the bottom down the side), so
 * that the last buttons' popovers do not hang off the window.
 */
enum class BarPopoverPlace { Centre, End }

/**
 * A pane of glass anchored to a control, above or below it, twelve points
 * clear (`.glassPopover(_:in:edge:alignment:width:)`). The control is
 * [anchor]; the popover shows while `popovers.open == id`.
 */
@Composable
fun GlassPopover(
    id: String,
    popovers: Popovers,
    modifier: Modifier = Modifier,
    edge: PopoverEdge = PopoverEdge.Top,
    alignment: PopoverAlignment = PopoverAlignment.Center,
    width: Dp = 280.dp,
    panel: @Composable () -> Unit,
    anchor: @Composable () -> Unit,
) {
    Box(modifier) {
        anchor()
        PopoverPanel(id, popovers, Placement.Hang(edge == PopoverEdge.Top, alignment), width, panel)
    }
}

/**
 * One of the call bar's popovers: above its control while the bar runs along
 * the foot of the call, to its side while the bar runs down the side ("At
 * the side", controls.ts), where above a button is another button
 * (`.barPopover(_:in:axis:place:width:)`). Clear of the bar's own edge as well
 * as the control's: the bar's padding is seven points.
 */
@Composable
fun BarPopover(
    id: String,
    popovers: Popovers,
    axis: BarAxis,
    modifier: Modifier = Modifier,
    place: BarPopoverPlace = BarPopoverPlace.Centre,
    width: Dp = 280.dp,
    panel: @Composable () -> Unit,
    anchor: @Composable () -> Unit,
) {
    Box(modifier) {
        anchor()
        PopoverPanel(id, popovers, Placement.Bar(axis == BarAxis.Vertical, place == BarPopoverPlace.End), width, panel)
    }
}

/**
 * A popover over a page that scrolls (`.floatingPopover(_:in:edge:alignment:width:)`):
 * on its preferred side of the control, unless it would run off the screen
 * and the other side has the room, and kept inside the screen.
 */
@Composable
fun FloatingPopover(
    id: String,
    popovers: Popovers,
    modifier: Modifier = Modifier,
    edge: PopoverEdge = PopoverEdge.Bottom,
    alignment: PopoverAlignment = PopoverAlignment.Trailing,
    width: Dp = 260.dp,
    panel: @Composable () -> Unit,
    anchor: @Composable () -> Unit,
) {
    Box(modifier) {
        anchor()
        PopoverPanel(id, popovers, Placement.Floating(edge == PopoverEdge.Bottom, alignment), width, panel)
    }
}

/** Where a popover goes relative to its control, and how it grows in. */
internal sealed interface Placement {
    /** The scale it arrives from and leaves to. */
    val arrivesFrom: Float
    val leavesTo: Float

    /** Kit.swift's: above (`above`) or below, aligned, twelve points clear. */
    data class Hang(val above: Boolean, val alignment: PopoverAlignment) : Placement {
        override val arrivesFrom get() = 0.92f
        override val leavesTo get() = 0.96f
    }

    /** Kit+Controls.swift's: above, or beside (`beside`) when the bar is upright. */
    data class Bar(val beside: Boolean, val end: Boolean) : Placement {
        override val arrivesFrom get() = 0.92f
        override val leavesTo get() = 0.96f
    }

    /** Kit+Settings.swift's: below (`below`) or above, turning where there is no room. */
    data class Floating(val below: Boolean, val alignment: PopoverAlignment) : Placement {
        override val arrivesFrom get() = 0.94f
        override val leavesTo get() = 0.97f
    }
}

/** The glass's shadow reaches 14 points soft and 6 down (Glass.kt). */
private val ShadowRoom = 24.dp

/** Where the panel ended up, for its arrival to grow from the control's side. */
private class Grows {
    var origin by mutableStateOf(TransformOrigin(0.5f, 1f))
}

@Composable
private fun PopoverPanel(id: String, popovers: Popovers, placement: Placement, width: Dp, panel: @Composable () -> Unit) {
    val open = popovers.open == id
    // Kept in the composition after it closes, until its departure has
    // played: a popover that is simply gone on the next frame is the one
    // thing the owner does not accept.
    val shown = remember { MutableTransitionState(false) }
    shown.targetState = open
    if (!shown.currentState && !shown.targetState && shown.isIdle) return

    val density = LocalDensity.current
    val window = LocalWindowInfo.current.containerSize
    val room = with(density) { window.width.toDp() } - 20.dp
    val grows = remember { Grows() }
    val position = remember(placement, density) { PopoverPosition(placement, density, grows) }
    Popup(
        popupPositionProvider = position,
        onDismissRequest = { if (popovers.open == id) popovers.close() },
        properties = PopupProperties(focusable = true),
    ) {
        val transition = rememberTransition(shown, label = "popover")
        val progress by transition.animateFloat(
            transitionSpec = { if (targetState) Theme.settle() else Theme.leave() },
            label = "arrival",
        ) { if (it) 1f else 0f }
        val focus = remember { FocusRequester() }
        // Focus on the panel, so that a keyboard's Escape reaches it.
        LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
        val shadowRoom = with(density) { ShadowRoom.toPx() }
        // Room around the panel inside its window for the glass's shadow,
        // which the window's edge otherwise cut off square under its rounded
        // corners. A press in that room is a press outside the popover.
        Box(
            Modifier
                .pointerInput(Unit) {
                    detectTapGestures { at ->
                        val inside = at.x >= shadowRoom && at.y >= shadowRoom &&
                            at.x <= size.width - shadowRoom && at.y <= size.height - shadowRoom
                        if (!inside) popovers.close()
                    }
                }
                .padding(ShadowRoom),
        ) {
            GlassShell(
                Modifier
                    .width(if (room > 0.dp) min(width, room) else width)
                    .glassLayer(
                        alpha = { progress },
                        scale = {
                            val from = if (transition.targetState) placement.arrivesFrom else placement.leavesTo
                            from + (1f - from) * progress
                        },
                        origin = { grows.origin },
                    )
                    .focusRequester(focus)
                    .focusable()
                    .onKeyEvent { event ->
                        if (event.key == Key.Escape && event.type == KeyEventType.KeyUp) {
                            popovers.close()
                            true
                        } else {
                            false
                        }
                    },
                cornerRadius = 18.dp,
            ) { panel() }
        }
    }
}

private class PopoverPosition(
    private val placement: Placement,
    private val density: Density,
    private val grows: Grows,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        // Placed by the panel itself, not the room for its shadow around it.
        val room = with(density) { ShadowRoom.roundToPx() }
        val w = popupContentSize.width - room * 2
        val h = popupContentSize.height - room * 2
        val margin = with(density) { 10.dp.roundToPx() }
        val a = anchorBounds
        fun aligned(alignment: PopoverAlignment): Int {
            val leading = if (layoutDirection == LayoutDirection.Ltr) a.left else a.right - w
            val trailing = if (layoutDirection == LayoutDirection.Ltr) a.right - w else a.left
            return when (alignment) {
                PopoverAlignment.Leading -> leading
                PopoverAlignment.Center -> a.center.x - w / 2
                PopoverAlignment.Trailing -> trailing
            }
        }
        var x: Int
        var y: Int
        when (placement) {
            is Placement.Hang -> {
                val gap = with(density) { 12.dp.roundToPx() }
                x = aligned(placement.alignment)
                y = if (placement.above) a.top - gap - h else a.bottom + gap
            }
            is Placement.Bar -> {
                val gap = with(density) { 19.dp.roundToPx() }
                if (!placement.beside) {
                    x = if (placement.end) a.right - w else a.center.x - w / 2
                    y = a.top - gap - h
                } else {
                    x = a.left - gap - w
                    y = if (placement.end) a.bottom - h else a.center.y - h / 2
                }
            }
            is Placement.Floating -> {
                val gap = with(density) { 8.dp.roundToPx() }
                x = aligned(placement.alignment)
                val below = a.bottom + gap
                val above = a.top - gap - h
                val fitsBelow = below + h <= windowSize.height - margin
                val fitsAbove = above >= margin
                y = if (placement.below) {
                    if (fitsBelow || !fitsAbove) below else above
                } else {
                    if (fitsAbove || !fitsBelow) above else below
                }
            }
        }
        x = x.coerceIn(margin, maxOf(margin, windowSize.width - w - margin))
        y = y.coerceIn(margin, maxOf(margin, windowSize.height - h - margin))
        // Grown from the control's side of the panel, and from where along
        // that side the control is, so that it seems to come out of it.
        val besides = placement is Placement.Bar && placement.beside
        grows.origin = if (besides) {
            TransformOrigin(1f, fraction(a.center.y - y, h))
        } else {
            TransformOrigin(fraction(a.center.x - x, w), if (y < a.top) 1f else 0f)
        }
        return IntOffset(x - room, y - room)
    }

    private fun fraction(offset: Int, length: Int) = if (length <= 0) 0.5f else (offset.toFloat() / length).coerceIn(0f, 1f)
}
