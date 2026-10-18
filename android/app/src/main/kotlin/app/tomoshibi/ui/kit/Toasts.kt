package app.tomoshibi.ui.kit

import android.os.SystemClock
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
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
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
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

// The call's notices (Toasts.swift) and the rules they keep (Corner.swift),
// in the corner above the controls, as the web draws sonner's at the bottom
// right (Notices.tsx).

/** How long the call's notices stay (web/src/live/notices.ts; Corner.swift `NoticeTimes`). */
object NoticeTimes {
    /** An ordinary notice: long enough to read four words (`BRIEFLY`). */
    const val briefly = 2.2

    /** A share, because the picture is about to change under you (`A_MOMENT`). */
    const val aMoment = 3.0

    /** The longest any notice stays (`AT_MOST`). Ten seconds has been seen; for ever has been ignored. */
    const val atMost = 10.0

    /** What fits above the controls without becoming a column of its own (Notices.tsx `visibleToasts`). */
    const val shown = 3

    /**
     * The least a held notice is given once let go. Sonner resumes with
     * whatever was left, so a notice read to its last tenth of a second
     * vanished the instant the pointer moved off it, which looked like the
     * pointer had knocked it away.
     */
    const val afterHolding = 1.0

    /**
     * Which notices to drop so that no more than `limit` remain, oldest
     * first: only ones that go by themselves (`lasting` false). One that
     * stays until it is dealt with, somebody at the door, is never pushed
     * out by somebody arriving.
     */
    fun overflow(lasting: List<Boolean>, limit: Int = shown): List<Int> {
        var excess = lasting.size - limit
        val drop = ArrayList<Int>()
        for ((index, stays) in lasting.withIndex()) {
            if (excess <= 0) break
            if (!stays) {
                drop.add(index)
                excess--
            }
        }
        return drop
    }
}

/**
 * A notice's countdown, which waits while the corner is being read (sonner
 * pauses every timer while its stack is expanded under the pointer;
 * Corner.swift `NoticeClock`). Times are seconds on whatever clock the
 * caller keeps.
 */
class NoticeClock(lasts: Double, now: Double, running: Boolean) {
    /** Seconds still to go, as of [since] when running. */
    var left = lasts
        private set

    /** When it last started running. */
    var since: Double? = if (running) now else null
        private set

    val running: Boolean get() = since != null

    /** Stops it, keeping what was left. */
    fun hold(now: Double) {
        val started = since ?: return
        left = max(0.0, left - (now - started))
        since = null
    }

    /** Starts it again, with at least [NoticeTimes.afterHolding] to go. */
    fun run(now: Double) {
        if (since != null) return
        left = max(left, NoticeTimes.afterHolding)
        since = now
    }

    /** Seconds from `now` until it is due; null while it is held. */
    fun due(now: Double): Double? {
        val started = since ?: return null
        return max(0.0, left - (now - started))
    }
}

/**
 * Where each notice in the corner sits (Corner.swift `NoticePile`): piled,
 * the newest in front and the others peeking above it, until the corner is
 * read, when they spread into a column. In points.
 */
object NoticePile {
    /** Between notices spread into a column (Notices.tsx `gap`). */
    const val gap = 8f

    /** How much of each notice behind the front one shows above it. */
    const val peek = 9f

    /** How much smaller each one behind is drawn, per place. */
    const val shrink = 0.05f

    /**
     * How far up from the bottom of the corner each notice's bottom edge
     * sits, newest first, given each one's height in the same order. Piled,
     * each sits `peek` above the one in front, all drawn the front one's
     * height; spread, each sits on the one below.
     */
    fun lifts(heights: List<Float>, spread: Boolean): List<Float> {
        val lifts = ArrayList<Float>(heights.size)
        var below = 0f
        for ((place, height) in heights.withIndex()) {
            if (spread) {
                lifts.add(below)
                below += height + gap
            } else {
                lifts.add(place * peek)
            }
        }
        return lifts
    }

    /** The height the whole corner takes. */
    fun height(heights: List<Float>, spread: Boolean): Float {
        val front = heights.firstOrNull() ?: return 0f
        if (spread) return heights.sum() + gap * (heights.size - 1)
        return front + peek * (heights.size - 1)
    }

    /** The scale a notice is drawn at in its place. */
    fun scale(place: Int, spread: Boolean): Float = if (spread) 1f else max(0.5f, 1f - shrink * place)
}

/**
 * Keeping the notices and what people said out of each other's way
 * (Corner.swift `CornerLanes`). Messages sit in the bottom-leading corner and
 * notices in the bottom-trailing one; on a phone both columns cover the
 * width, and there the notices rise above the messages, and not the other
 * way about: a message stays six seconds and a notice two, so the one that
 * comes and goes is the one that moves.
 */
object CornerLanes {
    /** Space kept between the two, in points. */
    const val gap = 8f

    /**
     * How far the notices must rise, given where the messages are drawn and
     * the leading edge of the notices' own column, both in one coordinate
     * space and unit; nothing while the two do not share any width.
     */
    fun lift(noticesLeading: Float, said: Rect?, gap: Float = CornerLanes.gap): Float {
        if (said == null || said.isEmpty || said.right + gap <= noticesLeading) return 0f
        return said.height + gap
    }
}

/**
 * What a notice is about, said by its edge (Notices.tsx: "the edge is the
 * whole of the severity system"): plain, the signal colour for somebody else
 * taking the stage or anything else happening now, red for a failure.
 */
enum class ToastKind { Info, Signal, Warning, Failure }

/**
 * One thing said in the corner of a call (web/src/live/notices.ts; Swift's
 * `CallToast`). The web's rule is about lifetime rather than severity:
 * something that happened is said here and fades; a notice about something
 * over carries no way to close it, because a target that lives two seconds is
 * one nobody hits; one somebody may want to deal with now, or not at all, has
 * the close button. `lasts` is seconds, or null to stay until dismissed; a
 * second notice under the same `key` replaces the first, in place;
 * `attachment` names what [Toasts.attach] draws under its words.
 */
data class Toast(
    val title: String,
    val detail: String? = null,
    val kind: ToastKind = ToastKind.Info,
    val lasts: Double? = NoticeTimes.briefly,
    val actionTitle: String? = null,
    val key: String? = null,
    val closable: Boolean = lasts == null,
    val attachment: String? = null,
    val id: Long = nextId(),
) {
    private companion object {
        private var last = 0L

        @Synchronized
        fun nextId(): Long = ++last
    }
}

/**
 * The call's notices and their countdowns, as CallModel keeps them on Apple:
 * a model holds one and says things with [announce]; [ToastStack] draws it.
 * Countdowns run on `scope`, by `clock` (seconds since boot), and all wait
 * while the corner is being read.
 */
@Stable
class Toasts(
    private val scope: CoroutineScope,
    private val clock: () -> Double = { SystemClock.uptimeMillis() / 1000.0 },
) {
    private val said = mutableStateListOf<Toast>()

    /** What is being said, oldest first. */
    val toasts: List<Toast> get() = said

    /** Whether every countdown is waiting, because the corner is being read. */
    var held by mutableStateOf(false)
        private set

    private val actions = HashMap<Long, () -> Unit>()
    private val clocks = HashMap<Long, NoticeClock>()
    private val timers = HashMap<Long, Job>()
    internal val attachments = mutableStateMapOf<String, @Composable () -> Unit>()

    /** Says `toast`; one with an action calls it when its button is pressed. */
    fun announce(toast: Toast, action: (() -> Unit)? = null) {
        var shown = toast
        val same = toast.key?.let { key -> said.indexOfFirst { it.key == key } } ?: -1
        if (same >= 0) {
            shown = toast.copy(id = said[same].id)
            stopClock(shown.id)
            said[same] = shown
        } else {
            said.add(shown)
            // The fourth pushes out the oldest one that goes by itself; one
            // somebody has to act on is never what is lost.
            for (index in NoticeTimes.overflow(said.map { it.lasts == null }).reversed()) {
                forget(said[index].id)
                said.removeAt(index)
            }
        }
        if (action != null) actions[shown.id] = action else actions.remove(shown.id)
        shown.lasts?.let { lasts ->
            clocks[shown.id] = NoticeClock(lasts, clock(), running = !held)
            runClock(shown.id)
        }
    }

    fun dismiss(id: Long) {
        forget(id)
        said.removeAll { it.id == id }
    }

    /** Takes down whatever was said under `key`, if anything still is. */
    fun dismiss(key: String) {
        said.firstOrNull { it.key == key }?.let { dismiss(it.id) }
    }

    /** The notice's button: the notice goes and its action runs. */
    fun act(id: Long) {
        val action = actions[id]
        dismiss(id)
        action?.invoke()
    }

    /**
     * Holds every countdown while the corner is read, and lets them run on
     * once it is not, each with what it had left.
     */
    fun hold(held: Boolean) {
        if (held == this.held) return
        this.held = held
        val now = clock()
        for (id in clocks.keys.toList()) {
            if (held) {
                clocks[id]?.hold(now)
                timers.remove(id)?.cancel()
            } else {
                clocks[id]?.run(now)
                runClock(id)
            }
        }
    }

    /**
     * What a notice's `attachment` draws: the people at the door with their
     * two answers. Drawn each time the card is, so whatever it draws stays live.
     */
    fun attach(name: String, draw: @Composable () -> Unit) {
        attachments[name] = draw
    }

    private fun runClock(id: Long) {
        timers.remove(id)?.cancel()
        val due = clocks[id]?.due(clock()) ?: return
        timers[id] = scope.launch {
            delay((due * 1000).roundToInt().toLong())
            dismiss(id)
        }
    }

    private fun stopClock(id: Long) {
        timers.remove(id)?.cancel()
        clocks.remove(id)
    }

    private fun forget(id: Long) {
        stopClock(id)
        actions.remove(id)
    }
}

/** The widest a notice is drawn: the web's eighteen rems and a little (`ToastStack.width`). */
private val NoticeWidth = 300.dp

/**
 * The call's notices, in the corner above the controls (`ToastStack`).
 *
 * Piled as sonner piles them: the newest in front, at most two more peeking
 * above it, so that three people arriving at once is one card's worth of the
 * picture covered rather than a column of them. A tap on the corner spreads
 * the pile into a column and holds every countdown for four seconds, as long
 * as it takes to read three lines (a touch screen has no pointer to rest
 * there); a finger on a card holds them too. A notice is swiped to the right
 * to be rid of it, and one that stays until it is dealt with also has its
 * close button. `saidBounds` is where the messages in the other corner are
 * drawn, in the window, for the notices to rise above them where the two
 * would meet. `spread` keeps the column open, for a picture of it.
 */
@Composable
fun ToastStack(toasts: Toasts, modifier: Modifier = Modifier, saidBounds: Rect? = null, spread: Boolean = false) {
    val density = LocalDensity.current
    // Newest first: the front of the pile is the last thing said.
    val shown = toasts.toasts.reversed()
    val heights = remember { mutableStateMapOf<Long, Float>() }
    var tapped by remember { mutableIntStateOf(0) }
    var reading by remember { mutableStateOf(false) }
    var leading by remember { mutableFloatStateOf(Float.POSITIVE_INFINITY) }
    val open = reading || spread
    val measured = shown.map { heights[it.id] ?: 44f }
    val lifts = NoticePile.lifts(measured, open)
    val front = measured.firstOrNull() ?: 0f
    val tallness by animateDpAsState(NoticePile.height(measured, open).dp, Theme.settle(), label = "pile")
    val rise by animateDpAsState(
        with(density) { CornerLanes.lift(leading, saidBounds, CornerLanes.gap.dp.toPx()).toDp() },
        Theme.settle(), label = "lane",
    )
    LaunchedEffect(tapped) {
        if (tapped == 0) return@LaunchedEffect
        reading = true
        toasts.hold(true)
        delay(4000)
        reading = false
        toasts.hold(spread)
    }
    LaunchedEffect(shown.isEmpty(), spread) {
        if (spread) toasts.hold(true)
        if (shown.isEmpty()) {
            reading = false
            toasts.hold(spread)
            heights.clear()
        }
    }
    Box(
        modifier
            .padding(bottom = rise)
            .widthIn(max = NoticeWidth)
            .height(tallness)
            .onGloballyPositioned { leading = it.boundsInWindow().left }
            .pointerInput(Unit) { detectTapGestures { if (toasts.toasts.isNotEmpty()) tapped++ } },
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
                    attachment = toast.attachment?.let { toasts.attachments[it] },
                    // Behind the front one, while piled, a card is the front
                    // one's height and shows nothing of its own: only its
                    // edge, peeking above.
                    pinnedHeight = if (!open && place > 0) front else null,
                    // Only the front card takes a finger while piled; a press
                    // on an edge peeking above it spreads the pile instead.
                    touchable = open || place == 0,
                    act = { toasts.act(toast.id) },
                    close = { toasts.dismiss(toast.id) },
                    interacting = { touching -> if (touching) toasts.hold(true) else if (!open) toasts.hold(false) },
                    measured = { height -> if (heights[toast.id] != height) heights[toast.id] = height },
                )
            }
        }
    }
}

@Composable
private fun ToastCard(
    fade: () -> Float,
    toast: Toast,
    attachment: (@Composable () -> Unit)?,
    pinnedHeight: Float?,
    touchable: Boolean,
    act: () -> Unit,
    close: () -> Unit,
    interacting: (Boolean) -> Unit,
    measured: (Float) -> Unit,
) {
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val dragged = remember { Animatable(0f) }
    val closeNow by rememberUpdatedState(close)
    val touching by rememberUpdatedState(interacting)
    val inner by animateFloatAsState(if (pinnedHeight == null) 1f else 0f, Theme.settle(), label = "pinned")
    val gone = with(density) { (NoticeWidth + 60.dp).toPx() }
    Box(
        Modifier
            .graphicsLayer {
                translationX = dragged.value
                alpha = fade() * (1f - min(0.7f, max(0f, dragged.value) / (260.dp.toPx())))
                compositingStrategy = CompositingStrategy.ModulateAlpha
            }
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
private fun ToastLine(toast: Toast, attachment: (@Composable () -> Unit)?, act: () -> Unit, close: () -> Unit) {
    val edge = when (toast.kind) {
        ToastKind.Info -> Color.White.copy(alpha = 0.22f)
        ToastKind.Signal, ToastKind.Warning -> Theme.tally
        ToastKind.Failure -> Theme.danger
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
                .then(if (toast.kind == ToastKind.Info) Modifier else Modifier.dropShadow(Capsule, Shadow(4.dp, edge, alpha = 0.6f)))
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
