package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import app.tomoshibi.basics.CornerLanes
import app.tomoshibi.basics.NoticePile
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min

// The call's notices (Toasts.swift), in the corner above the controls, as
// the web draws sonner's at the bottom right (Notices.tsx). The rules they
// keep are basics/Corner.kt's, and the notices themselves the call's
// (CallModel.toasts), the one place anything in a call is said from.

/**
 * What a notice's `attachment` draws under its words, as the model keeps it
 * (`CallModel.attach`): the people at the door with their two answers. A
 * holder rather than the composable itself, so that the model, which knows
 * nothing of Compose, can keep it as any other value and the stack can find
 * it again without a cast to a composable function type.
 */
class ToastAttachment(val content: @Composable () -> Unit)

/** The widest a notice is drawn: the web's eighteen rems and a little (`ToastStack.width`). */
private val NoticeWidth = 300.dp

/**
 * The call's notices, in the corner above the controls (`ToastStack`), as
 * the call keeps them: [CallModel.toasts], taken down with its `dismiss`,
 * acted on with its `act`, their countdowns held with `holdToasts`.
 *
 * Piled as sonner piles them: the newest in front, at most two more peeking
 * above it, so that three people arriving at once is one card's worth of the
 * picture covered rather than a column of them. A tap on a card spreads the
 * pile into a column and holds every countdown for four seconds, as long as
 * it takes to read three lines (a touch screen has no pointer to rest
 * there); a finger on a card holds them too. A notice is swiped to the right
 * to be rid of it, and one that stays until it is dealt with also has its
 * close button. `saidBounds` is where the messages in the other corner are
 * drawn, in the window, for the notices to rise above them where the two
 * would meet. `spread` keeps the column open, for a picture of it.
 */
@Composable
fun ToastStack(call: CallModel, modifier: Modifier = Modifier, saidBounds: Rect? = null, spread: Boolean = false) =
    ToastStack(
        toasts = call.toasts,
        act = { call.act(it) },
        dismiss = { call.dismiss(it) },
        hold = { call.holdToasts(it) },
        modifier = modifier,
        attachment = { call.attachments[it] as? ToastAttachment },
        saidBounds = saidBounds,
        spread = spread,
    )

/**
 * The same corner for notices kept anywhere: [toasts] oldest first, as the
 * call keeps them, and what a press on one does. For a picture of the
 * corner, or the kit's gallery, with no call behind it.
 */
@Composable
fun ToastStack(
    toasts: List<CallToast>,
    act: (Long) -> Unit,
    dismiss: (Long) -> Unit,
    hold: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    attachment: (String) -> ToastAttachment? = { null },
    saidBounds: Rect? = null,
    spread: Boolean = false,
) {
    val density = LocalDensity.current
    // Newest first: the front of the pile is the last thing said.
    val shown = toasts.reversed()
    val heights = remember { mutableStateMapOf<Long, Float>() }
    var tapped by remember { mutableIntStateOf(0) }
    var reading by remember { mutableStateOf(false) }
    var leading by remember { mutableFloatStateOf(Float.POSITIVE_INFINITY) }
    val open = reading || spread
    val holding by rememberUpdatedState(hold)
    val measured = shown.map { heights[it.id] ?: 44f }
    val lifts = NoticePile.lifts(measured, open)
    val front = measured.firstOrNull() ?: 0f
    val tallness by animateDpAsState(NoticePile.height(measured, open).dp, Theme.settle(), label = "pile")
    val rise by animateDpAsState(
        with(density) { CornerLanes.lift(leading, saidBounds?.right, saidBounds?.height, CornerLanes.GAP.dp.toPx()).toDp() },
        Theme.settle(), label = "lane",
    )
    LaunchedEffect(tapped) {
        if (tapped == 0) return@LaunchedEffect
        reading = true
        holding(true)
        delay(4000)
        reading = false
        holding(spread)
    }
    LaunchedEffect(shown.isEmpty(), spread) {
        if (spread) holding(true)
        if (shown.isEmpty()) {
            reading = false
            holding(spread)
            heights.clear()
        }
    }
    Box(
        modifier
            .padding(bottom = rise)
            .widthIn(max = NoticeWidth)
            .height(tallness)
            .onGloballyPositioned { leading = it.boundsInWindow().left },
        contentAlignment = Alignment.BottomEnd,
    ) {
        for (entry in keepDeparting(shown) { it.id }) {
            val place = entry.place
            val toast = entry.item
            val scale by animateFloatAsState(NoticePile.scale(place, open), Theme.settle(), label = "scale")
            val lift by animateDpAsState((lifts.getOrNull(place) ?: 0f).dp, Theme.settle(), label = "lift")
            AnimatedVisibility(
                entry.visible,
                Modifier
                    .zIndex((shown.size - place).toFloat())
                    .wrapContentHeight(Alignment.Bottom, unbounded = true)
                    .graphicsLayer {
                        translationY = -lift.toPx()
                        scaleX = scale
                        scaleY = scale
                        transformOrigin = TransformOrigin(0.5f, 0f)
                    },
                // Faded by glassFade rather than fadeIn, which would cut the
                // card's shadow off at its edge while it arrives.
                enter = slideInVertically(Theme.settle()) { with(density) { 18.dp.roundToPx() } } +
                    scaleIn(Theme.settle(), initialScale = 0.96f, transformOrigin = TransformOrigin(0.5f, 1f)),
                exit = scaleOut(Theme.leave(), targetScale = 0.96f),
            ) {
                val fade by glassFade()
                ToastCard(
                    fade = { fade },
                    toast = toast,
                    attachment = toast.attachment?.let { attachment(it)?.content },
                    // Behind the front one, while piled, a card is the front
                    // one's height and shows nothing of its own: only its
                    // edge, peeking above.
                    pinnedHeight = if (!open && place > 0) front else null,
                    // Only the front card takes a finger while piled; a press
                    // on an edge peeking above it spreads the pile instead.
                    touchable = open || place == 0,
                    act = { act(toast.id) },
                    close = { dismiss(toast.id) },
                    // A tap on a card spreads the pile, rather than a tap
                    // anywhere in the column: the column is as wide as its
                    // widest notice and lies over the filmstrip, and a press
                    // on a picture beside a short notice went to nothing
                    // (Toasts.swift says the same of the Swift column).
                    tapped = { tapped++ },
                    interacting = { touching -> if (touching) holding(true) else if (!open) holding(false) },
                    measured = { height -> if (heights[toast.id] != height) heights[toast.id] = height },
                )
            }
        }
    }
}

@Composable
private fun ToastCard(
    fade: () -> Float,
    toast: CallToast,
    attachment: (@Composable () -> Unit)?,
    pinnedHeight: Float?,
    touchable: Boolean,
    act: () -> Unit,
    close: () -> Unit,
    tapped: () -> Unit,
    interacting: (Boolean) -> Unit,
    measured: (Float) -> Unit,
) {
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val dragged = remember { Animatable(0f) }
    val closeNow by rememberUpdatedState(close)
    val touching by rememberUpdatedState(interacting)
    val tap by rememberUpdatedState(tapped)
    val inner by animateFloatAsState(if (pinnedHeight == null) 1f else 0f, Theme.settle(), label = "pinned")
    val gone = with(density) { (NoticeWidth + 60.dp).toPx() }
    Box(
        Modifier
            .graphicsLayer {
                translationX = dragged.value
                alpha = fade() * (1f - min(0.7f, max(0f, dragged.value) / (260.dp.toPx())))
                compositingStrategy = CompositingStrategy.ModulateAlpha
            }
            // Every card, the ones peeking above the front one included: a
            // press on an edge behind spreads the pile.
            .pointerInput(Unit) { detectTapGestures { tap() } }
            .then(
                if (!touchable) Modifier else Modifier.pointerInput(Unit) {
                    val velocity = VelocityTracker()
                    var raw = 0f
                    detectHorizontalDragGestures(
                        onDragStart = {
                            raw = 0f
                            velocity.resetTracking()
                            touching(true)
                        },
                        onDragEnd = {
                            val speed = velocity.calculateVelocity().x
                            val predicted = raw + speed * 0.5f
                            if (raw > 70.dp.toPx() || predicted > 160.dp.toPx()) {
                                scope.launch {
                                    dragged.animateTo(gone, tween(180, easing = Theme.leaveEasing))
                                    closeNow()
                                    touching(false)
                                }
                            } else {
                                scope.launch { dragged.animateTo(0f, Theme.settle()) }
                                touching(false)
                            }
                        },
                        onDragCancel = {
                            scope.launch { dragged.animateTo(0f, Theme.settle()) }
                            touching(false)
                        },
                    ) { change, amount ->
                        velocity.addPosition(change.uptimeMillis, change.position)
                        raw += amount
                        // Leftwards it hardly moves, so that it feels held
                        // rather than broken.
                        scope.launch { dragged.snapTo(if (raw < 0) raw / 6 else raw) }
                        change.consume()
                    }
                },
            )
            .glassPane(14.dp)
            .semantics(mergeDescendants = true) {
                // Said as it arrives, as sonner's live region does: a notice
                // nobody can see is one nobody was given.
                liveRegion = LiveRegionMode.Polite
                customActions = listOf(CustomAccessibilityAction(t("Dismiss")) { close(); true })
            },
    ) {
        Box(
            Modifier
                .layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
                    measured(placeable.height.toDp().value)
                    val height = pinnedHeight?.dp?.roundToPx() ?: placeable.height
                    layout(placeable.width, height) { placeable.place(0, 0) }
                }
                .clipToBounds()
                .alpha(inner),
        ) {
            // A notice is no wider than its words (Notices.tsx `sm:w-fit`),
            // up to the column's width; one carrying the door's list takes
            // all of it.
            FitWidth(minimum = if (toast.attachment == null) 170.dp else NoticeWidth, maximum = NoticeWidth) {
                ToastLine(toast, attachment, act, close)
            }
        }
    }
}

@Composable
private fun ToastLine(toast: CallToast, attachment: (@Composable () -> Unit)?, act: () -> Unit, close: () -> Unit) {
    val edge = when (toast.kind) {
        CallToast.Kind.Info -> Color.White.copy(alpha = 0.22f)
        CallToast.Kind.Signal, CallToast.Kind.Warning -> Theme.tally
        CallToast.Kind.Failure -> Theme.danger
    }
    Row(
        Modifier
            .height(IntrinsicSize.Min)
            .padding(start = 9.dp, end = if (toast.closable) 7.dp else 12.dp, top = 10.dp, bottom = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // notices.ts: the edge is the whole of the severity system. A wash of
        // colour over a card this small says less than a rail beside it.
        Box(
            Modifier
                .padding(vertical = 1.dp)
                .width(3.dp)
                .fillMaxHeight()
                .then(if (toast.kind == CallToast.Kind.Info) Modifier else Modifier.dropShadow(Capsule, Shadow(4.dp, edge, alpha = 0.6f)))
                .background(edge, Capsule),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            KitText(toast.title, font = Font(13f, Font.Medium))
            if (toast.detail != null) KitText(toast.detail, font = Font(11.5f), color = Theme.muted)
            if (attachment != null) Box(Modifier.padding(top = 5.dp)) { attachment() }
        }
        if (toast.actionTitle != null) {
            Box(Modifier.fillMaxHeight(), contentAlignment = Alignment.Center) { ActionPill(toast.actionTitle, act) }
        }
        if (toast.closable) CloseDot(close)
    }
}

/**
 * A notice's one action, in the foreground colour rather than the signal one
 * (Notices.tsx `actionButton`): the notice it sits on is usually saying
 * something cannot be done, and the signal colour means something is
 * happening.
 */
@Composable
private fun ActionPill(title: String, action: () -> Unit) {
    val press = rememberPress()
    val fill by animateFloatAsState(if (press.lit) 0.88f else 1f, Motion.hover(), label = "fill")
    Box(
        Modifier
            .background(Theme.foreground.copy(alpha = fill), Capsule)
            .pressable(press, Capsule, true, Role.Button, onClick = action)
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) { KitText(title, font = Font(11.5f, Font.Semibold), color = Theme.background, lines = 1) }
}

/** The close button, for a notice about something still true. */
@Composable
private fun CloseDot(action: () -> Unit) {
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit) 0.12f else 0f, Motion.hover(), label = "light")
    Box(
        Modifier
            .size(20.dp)
            .background(Color.White.copy(alpha = light), CircleShape)
            .pressable(press, CircleShape, true, Role.Button, t("Dismiss"), onClick = action),
        contentAlignment = Alignment.Center,
    ) { Symbol("xmark", size = 8.5f, weight = Font.Bold, tint = if (press.lit) Theme.foreground else Theme.muted) }
}

/** One item of a list that arrives and leaves: where it is, and whether it is still wanted. */
internal class Departing<T>(val key: Any, var item: T, var place: Int) {
    val visible = MutableTransitionState(false).apply { targetState = true }
}

/**
 * The items, and those just taken away for as long as their departure plays,
 * each with the state its arrival and departure animate from: a list whose
 * items are simply gone on the next frame is the thing the owner does not
 * accept.
 */
@Composable
internal fun <T> keepDeparting(items: List<T>, key: (T) -> Any): List<Departing<T>> {
    val kept = remember { ArrayList<Departing<T>>() }
    val keys = items.map(key)
    for (entry in kept) {
        val index = keys.indexOf(entry.key)
        if (index >= 0) {
            entry.item = items[index]
            entry.place = index
            entry.visible.targetState = true
        } else {
            entry.visible.targetState = false
        }
    }
    items.forEachIndexed { index, item ->
        val k = keys[index]
        if (kept.none { it.key == k }) kept.add(Departing(k, item, index))
    }
    kept.removeAll { !it.visible.targetState && !it.visible.currentState && it.visible.isIdle }
    return kept.toList()
}
