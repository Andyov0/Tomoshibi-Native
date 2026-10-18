package app.tomoshibi.ui.lobby

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import app.tomoshibi.basics.Theme
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContinuousShape
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

// How the phone presents what the lobby opens (a page sheet, a full-screen
// cover) and how a wide screen does (a sheet in the middle of the window),
// drawn rather than borrowed from the system's dialogs, so that the glass
// inside sees the lobby it slid over and everything moves with the app's
// two curves.

/** What a phone's sheet is: the system's page sheet (`.sheet`), or one covering everything (`.fullScreenCover`). */
enum class SheetStyle { Page, Cover }

/**
 * A sheet sliding up over the phone's lobby. A page sheet stops short of
 * the status bar with the lobby dimmed above it, wears a grabber, and goes
 * when it is pulled down from its top or from a list already at its top, or
 * tapped above; a cover takes the whole screen. Back closes either, and the
 * system's predictive back draws it going as the gesture is made.
 */
@Composable
fun SheetHost(
    visible: Boolean,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    style: SheetStyle = SheetStyle.Page,
    background: Color = Theme.background,
    content: @Composable () -> Unit,
) {
    val dismiss by rememberUpdatedState(onDismiss)
    Box(modifier.fillMaxSize()) {
        if (style == SheetStyle.Page) {
            AnimatedVisibility(visible, Modifier.fillMaxSize(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .background(Color.Black.copy(alpha = 0.45f))
                        .pointerInput(Unit) { detectTapGestures { dismiss() } },
                )
            }
        }
        AnimatedVisibility(
            visible,
            Modifier.fillMaxSize(),
            enter = slideInVertically(Theme.settle()) { it },
            exit = slideOutVertically(Theme.leave()) { it },
        ) {
            val density = LocalDensity.current
            val pulled = remember { Animatable(0f) }
            var height by remember { mutableFloatStateOf(1f) }
            val scope = rememberCoroutineScope()
            val far = with(density) { 160.dp.toPx() }
            fun letGo(velocity: Float) {
                scope.launch {
                    if (pulled.value > far || velocity > 1400f) dismiss() else pulled.animateTo(0f, Theme.settle())
                }
            }
            PredictiveBackHandler(enabled = visible) { progress ->
                try {
                    progress.collect { event -> pulled.snapTo(event.progress * far * 0.8f) }
                    dismiss()
                } catch (fault: CancellationException) {
                    pulled.animateTo(0f, Theme.settle())
                    throw fault
                }
            }
            val page = style == SheetStyle.Page
            // A list already at its top hands on what is left of a pull
            // downwards, and the sheet follows it: the system's sheet goes
            // with the list's own overscroll rather than only from its grabber.
            val following = remember {
                object : NestedScrollConnection {
                    override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                        if (!page || source != NestedScrollSource.UserInput) return Offset.Zero
                        if (available.y < 0 && pulled.value > 0f) {
                            val taken = maxOf(available.y, -pulled.value)
                            scope.launch { pulled.snapTo(pulled.value + taken) }
                            return Offset(0f, taken)
                        }
                        return Offset.Zero
                    }

                    override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                        if (!page || source != NestedScrollSource.UserInput || available.y <= 0f) return Offset.Zero
                        scope.launch { pulled.snapTo(pulled.value + available.y) }
                        return Offset(0f, available.y)
                    }

                    override suspend fun onPreFling(available: Velocity): Velocity {
                        if (!page || pulled.value <= 0f) return Velocity.Zero
                        letGo(available.y)
                        return available
                    }
                }
            }
            val dragging = rememberDraggableState { delta -> scope.launch { pulled.snapTo((pulled.value + delta).coerceAtLeast(0f)) } }
            val shape = if (page) RoundedCornerShape(topStart = 22.dp, topEnd = 22.dp) else RoundedCornerShape(0.dp)
            Column(Modifier.fillMaxSize()) {
                // Clear of the status bar by ten points, the lobby dimmed in the gap.
                if (page) {
                    Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
                    Spacer(Modifier.size(1.dp, 10.dp))
                }
                Box(
                    Modifier
                        .fillMaxSize()
                        .onSizeChanged { height = it.height.toFloat().coerceAtLeast(1f) }
                        .graphicsLayer {
                            translationY = pulled.value
                            // Pulled away, it gives a little as the system's does.
                            val shrink = (pulled.value / (height * 4f)).coerceIn(0f, 0.06f)
                            scaleX = 1f - shrink
                            scaleY = 1f - shrink
                        }
                        .then(if (page) Modifier.dropShadow(shape, Shadow(radius = 18.dp, color = Color.Black, offset = DpOffset(0.dp, -2.dp), alpha = 0.35f)) else Modifier)
                        .clip(shape)
                        .background(background)
                        // Every press on the sheet is the sheet's. Compose hands
                        // a press on a part of it that holds none to whatever is
                        // under it: the dimming, which closed the sheet, or for
                        // a cover the lobby, which pressed a button nobody could
                        // see. The pull that dismisses a page sheet holds them
                        // too; this holds them whatever the sheet is.
                        .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } }
                        .nestedScroll(following)
                        .then(if (page) Modifier.draggable(dragging, Orientation.Vertical, onDragStopped = { letGo(it) }) else Modifier),
                ) {
                    content()
                    if (page) {
                        // The grabber, as the system draws it (`.presentationDragIndicator(.visible)`).
                        Box(
                            Modifier
                                .align(Alignment.TopCenter)
                                .padding(top = 6.dp)
                                .size(36.dp, 5.dp)
                                .background(Color.White.copy(alpha = 0.32f), Capsule),
                        )
                    }
                }
            }
        }
    }
}

/**
 * A sheet in the middle of a wide window, over the window dimmed: what the
 * Mac's window-attached sheet is on a tablet or an unfolded phone. Held to
 * one half of the screen where a hinge divides it, the right half of a book
 * held open, the upper half of a phone standing on its hinge, so that none
 * of it is read across the fold. Back, and a tap outside, close it.
 */
@Composable
fun DialogHost(
    visible: Boolean,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    width: Dp = 560.dp,
    height: Dp? = null,
    cornerRadius: Dp = 26.dp,
    content: @Composable () -> Unit,
) {
    val dismiss by rememberUpdatedState(onDismiss)
    val hinge = LocalHinge.current
    val density = LocalDensity.current
    Box(modifier.fillMaxSize()) {
        AnimatedVisibility(visible, Modifier.fillMaxSize(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.45f))
                    .pointerInput(Unit) { detectTapGestures { dismiss() } },
            )
        }
        PredictiveBackHandler(enabled = visible) { progress ->
            progress.collect {}
            dismiss()
        }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            // The part of the window the sheet is centred in.
            var left = 0.dp
            var top = 0.dp
            var right = maxWidth
            var bottom = maxHeight
            if (hinge != null) {
                with(density) {
                    if (hinge.vertical) left = hinge.right.toDp() else bottom = hinge.top.toDp()
                }
            }
            val areaWidth = right - left
            val areaHeight = bottom - top
            Box(
                Modifier
                    .absoluteOffset(left, top)
                    .size(areaWidth, areaHeight)
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(24.dp),
                contentAlignment = Alignment.Center,
            ) {
                AnimatedVisibility(
                    visible,
                    enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.96f) + slideInVertically(Theme.settle()) { it / 12 },
                    exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.98f),
                ) {
                    val shape = ContinuousShape(cornerRadius)
                    Box(
                        Modifier
                            .width(min(width, areaWidth))
                            .then(if (height != null) Modifier.heightIn(max = height) else Modifier)
                            .dropShadow(shape, Shadow(radius = 30.dp, color = Color.Black, offset = DpOffset(0.dp, 10.dp), alpha = 0.45f))
                            .clip(shape)
                            .background(Theme.surface)
                            // A press inside is the sheet's, never the dimmed window's.
                            .pointerInput(Unit) { detectTapGestures { } },
                    ) { content() }
                }
            }
        }
    }
}

/**
 * What sits at the foot of the screen, inside the bars, the cutout and the
 * keyboard, and scrolls where it is taller than the room left there: the
 * phone's join pane and start sheet (Screens.swift `PaneScreen`). Measured
 * inside the insets: measured against the whole screen, the pane was as tall
 * as the screen with the bars still to come off it, and its last button sat
 * under the navigation bar until it was scrolled.
 */
@Composable
fun AtTheFoot(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    BoxWithConstraints(modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        val room = maxHeight
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            Column(Modifier.fillMaxWidth().heightIn(min = room), verticalArrangement = Arrangement.Bottom, content = content)
        }
    }
}
