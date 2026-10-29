package app.tomoshibi.ui.lobby

import android.icu.text.RelativeDateTimeFormatter
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.KeptToOnePane
import app.tomoshibi.ui.Pane
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassGroup
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.glassFade
import app.tomoshibi.ui.kit.glassLayer
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.net.URI
import java.time.Duration
import java.time.Instant
import kotlin.math.roundToInt
import uniffi.tomoshibi_protocol.generateRoomName
import uniffi.tomoshibi_protocol.looksGenerated
import uniffi.tomoshibi_protocol.normaliseRoomName
import uniffi.tomoshibi_protocol.scopeOf
import uniffi.tomoshibi_protocol.validRoomName

// The small pieces the screens before a call share (Lobby.swift, "The mark
// and small pieces").

/**
 * A debug build's way to open one part of these screens without a finger,
 * as TOMOSHIBI_DEBUG_LOBBY, TOMOSHIBI_DEBUG_ROOM and TOMOSHIBI_DEBUG_POPOVER
 * do on Apple (Lobby.swift `LobbyDebug`): the launch's `lobby` (`scheduled`,
 * `servers`, `start`, `join`, `again` or `settings`), `room` (typed into the
 * start and join sheet's field) and `popover` extras. Set by MainActivity from
 * a debug launch only; the part of the lobby is opened by the first lobby
 * drawn and then forgotten (LobbyState).
 */
object LobbyDebug {
    var open: String? = null
    var room: String? = null
    var popover: String? = null

    /** The section the settings open at when `open` is `settings` (`general`, `media`, `watching`, `music`, `account`, `about`). */
    var section: String? = null

    fun sheet(open: String?): MeetingMode? =
        when (open) {
            "start" -> MeetingMode.Start
            "join" -> MeetingMode.Join("")
            "again" -> MeetingMode.Again(room ?: "")
            else -> null
        }
}

/**
 * Opens the popover a debug launch named, a moment after the screen arrives
 * (`Popovers.openFromDebug`), for a test to see it without a pointer.
 */
@Composable
internal fun OpenFromDebug(popovers: Popovers) {
    LaunchedEffect(popovers) {
        val id = LobbyDebug.popover ?: return@LaunchedEffect
        delay(1200)
        popovers.show(id)
    }
}

/** The product's own mark, as the favicon and the app icon draw it (`Mark`). */
@Composable
fun Mark(modifier: Modifier = Modifier, size: Dp = 20.dp) {
    Canvas(modifier.size(size).clearAndSetSemantics {}) {
        val unit = this.size.width / 32
        drawRoundRect(MarkGround, cornerRadius = CornerRadius(8 * unit))
        val inner = unit * 0.86f
        val offset = (this.size.width - 32 * inner) / 2
        for ((x, y, live) in listOf(Triple(6f, 8f, false), Triple(17f, 8f, false), Triple(6f, 17f, false), Triple(17f, 17f, true))) {
            drawRoundRect(
                if (live) MarkLive else MarkIdle,
                topLeft = Offset(offset + x * inner, offset + y * inner),
                size = Size(9 * inner, 7 * inner),
                cornerRadius = CornerRadius(2 * inner),
            )
        }
    }
}

private val MarkGround = Color(0xFF0F0E0D)
private val MarkIdle = Color(0xFF55524E)
private val MarkLive = Color(0xFFFFA23B)

/** How much of the fleet is answering, in a light and two numbers (Lobby.tsx `Fleet`; `FleetLine`). */
@Composable
fun FleetLine(app: AppModel, modifier: Modifier = Modifier) {
    val relays = app.relays ?: return
    if (relays.total == 0u) return
    val all = relays.online == relays.total
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier.size(6.dp).background(
                if (relays.online == 0u) Theme.danger else if (all) Theme.good else Theme.tally,
                CircleShape,
            ),
        )
        KitText("${relays.online}/${relays.total}", font = Font(11f, digits = true), color = Theme.muted, lines = 1)
        KitText(t("Relays answering"), font = Font(11f), color = Theme.muted, lines = 1)
    }
}

/**
 * Something to tell the person, at the foot of the screen (Notices.tsx;
 * `ToastModifier`). Two kinds, as the web has them: something that happened
 * and is over fades on its own after ten seconds, and something about why
 * they are not in a meeting stays, with a way to close it, until they do: an
 * invitation that opens tomorrow said when and was gone in five seconds,
 * while the person was still reading it. `clearance` keeps it above what sits
 * at the foot of the screen, the phone's join bar.
 */
@Composable
fun ProblemToast(app: AppModel, modifier: Modifier = Modifier, clearance: Dp = 28.dp) {
    val text = app.problem
    var said by remember { mutableStateOf(text) }
    if (text != null) said = text
    val stays = app.problemStays
    LaunchedEffect(text, stays) {
        if (text == null || stays) return@LaunchedEffect
        delay(10_000)
        if (app.problem == text && !app.problemStays) app.problem = null
    }
    val density = LocalDensity.current
    val below = with(density) { clearance.roundToPx() }
    // At the foot of one page of a book rather than across the two (ui/Fold.kt).
    KeptToOnePane(pane = Pane.Foot) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        // Glass slid in from below the screen and faded on its own layer, so
        // that its shadow is not cut off square while it moves (Glass.kt).
        AnimatedVisibility(
            text != null,
            enter = slideInVertically(Theme.settle()) { it + below },
            exit = slideOutVertically(Theme.leave()) { it + below },
        ) {
            val fade by glassFade()
            val press = rememberPress()
            Row(
                Modifier
                    .padding(start = 20.dp, end = 20.dp, bottom = clearance)
                    .widthIn(max = 560.dp)
                    .glassLayer({ fade })
                    .glassSurface(Capsule)
                    .pressable(press, Capsule, !stays, role = null) { if (!app.problemStays) app.problem = null }
                    .padding(start = 14.dp, end = if (stays) 7.dp else 16.dp, top = 9.dp, bottom = 9.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Symbol(
                    if (stays) "exclamationmark.circle.fill" else "info.circle",
                    size = 14f, weight = Font.Semibold, tint = if (stays) Theme.tallyInk else Theme.muted,
                )
                KitText(said ?: "", Modifier.weight(1f, fill = false), font = Font(13f))
                if (stays) {
                    // Drawn plain on the toast's glass, never glass on glass.
                    ContentLayer {
                        GlassButton({ app.problem = null }, shape = GlassButtonShape.Circle(24.dp), label = t("Close")) {
                            Symbol("xmark", size = 10f, weight = Font.Bold)
                        }
                    }
                }
            }
        }
    }
    }
}

/**
 * The microphone and camera as they will be on joining, as switches
 * (`DeviceSwitches`). Off is the solid plate of the foreground the call's own
 * bar draws (BarButton), white by night and near black by day: this is the
 * last moment to notice before everybody else does, and the one switch drawn
 * red here and grey in the call looked like two different things a screen
 * apart (the owner, 2026-10-29). Forty-four across over a picture, the
 * call's own button, where it was 38 or 40 by the screen, and the large 52
 * where it stands alone on the card; the symbol four tenths of either.
 */
@Composable
fun DeviceSwitches(app: AppModel, modifier: Modifier = Modifier, size: Dp = 44.dp, shade: Color = Color.Transparent) {
    GlassGroup(8.dp) {
        Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val mic = app.startWithMicrophone
            GlassButton(
                { app.startWithMicrophone = !app.startWithMicrophone },
                shape = GlassButtonShape.Circle(size),
                off = !mic,
                label = if (mic) t("Mute microphone") else t("Unmute microphone"),
                shade = shade,
            ) { SwappingSymbol(if (mic) "mic.fill" else "mic.slash.fill", size = size.value * 0.4f, weight = Font.Semibold) }
            val camera = app.startWithCamera
            GlassButton(
                { app.startWithCamera = !app.startWithCamera },
                shape = GlassButtonShape.Circle(size),
                off = !camera,
                label = if (camera) t("Turn camera off") else t("Turn camera on"),
                shade = shade,
            ) { SwappingSymbol(if (camera) "video.fill" else "video.slash.fill", size = size.value * 0.4f, weight = Font.Semibold) }
        }
    }
}

/**
 * What was typed, if it is a link: an invitation or a room's address pasted
 * where a room's name goes (`pastedLink`). The app opens it as it opens one
 * handed over by the system, and refuses it there if it is another server's.
 */
fun pastedLink(text: String): String? {
    val trimmed = text.trim()
    if (trimmed.isEmpty()) return null
    val uri = try {
        URI(trimmed)
    } catch (_: Exception) {
        return null
    }
    val scheme = uri.scheme?.lowercase() ?: return null
    if (scheme !in setOf("https", "http", "tomoshibi") || uri.host == null) return null
    return trimmed
}

/**
 * The core's rules for room names, as the screens ask them: how a typed
 * name is written, whether it is one, a fresh one nobody has used, the scope
 * it is held under, and whether it looks made up. Held here so that the JVM
 * tests, where the core's library (built for the phone) cannot be loaded,
 * can hand in the same rules written plainly; the app never changes them.
 */
object RoomNames {
    var normalise: (String) -> String = { normaliseRoomName(it) }
    var valid: (String) -> Boolean = { validRoomName(it) }
    var generate: () -> String = { generateRoomName() }
    var scope: (String) -> String = { scopeOf(it) }
    var generated: (String) -> Boolean = { looksGenerated(it) }
}

/** A name typed where a room goes is one only once it is something; nothing typed is no room, and asks the core nothing. */
fun validTyped(text: String): Boolean = text.isNotBlank() && RoomNames.valid(RoomNames.normalise(text))

/**
 * ServerPicker.tsx `Latency`: green to 150 ms, the signal colour to 400, red
 * beyond (`latencyColour`). A figure, so each colour is its ink.
 */
@Composable
fun latencyColour(ms: Double): Color = if (ms <= 150) Theme.goodInk else if (ms <= 400) Theme.tallyInk else Theme.dangerInk

/**
 * What the top bar of a page lays over the page where it runs up under the
 * status bar: the page's own ground, flat and nearly opaque, in either
 * appearance. The dark had a fade into black from top to bottom; the owner's
 * rule is no gradients anywhere (2026-10-29), which the light had already
 * kept to since 2026-10-26, and the web took its own dark glow out of the top
 * of the lobby for the same rule.
 */
@Composable
internal fun Modifier.barShade(): Modifier = background(Theme.ground.copy(alpha = 0.94f))

/**
 * Everything on these screens arrives in an order: in from fourteen points
 * below, over 420 ms on the arriving curve, after `delay` (`rise`). Faded on
 * each thing drawn rather than as a layer, so a pane's shadow is not cut off
 * square while it rises.
 */
@Composable
fun Modifier.rise(delay: Int = 0): Modifier {
    val shown = remember { Animatable(0f) }
    LaunchedEffect(Unit) { shown.animateTo(1f, tween(420, delayMillis = delay, easing = Theme.settleEasing)) }
    val lift = with(LocalDensity.current) { 14.dp.toPx() }
    return graphicsLayer {
        alpha = shown.value
        translationY = (1f - shown.value) * lift
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }
}

/**
 * A press on something drawn as it is, a row or a card (SwiftUI's plain
 * button style): it dims a little under the finger and nothing else.
 */
@Composable
fun Modifier.plainPress(
    onClick: () -> Unit,
    enabled: Boolean = true,
    label: String? = null,
    shape: Shape = RectangleShape,
): Modifier {
    val press = rememberPress()
    val dim by animateFloatAsState(if (press.pressed && enabled) 0.6f else 1f, Motion.hover(), label = "press")
    return this
        .graphicsLayer {
            alpha = dim
            compositingStrategy = CompositingStrategy.ModulateAlpha
        }
        .pressable(press, shape, enabled, Role.Button, label, onClick = onClick)
}

/**
 * When something happened, said the way the system's named relative date
 * says it (`.relative(presentation: .named)`): "now", "5 minutes ago",
 * "yesterday", "last week", in the language the interface is in rather
 * than the system's.
 */
fun relativeNamed(at: Instant, now: Instant = Instant.now()): String {
    val formatter = RelativeDateTimeFormatter.getInstance(readingLocale())
    val seconds = Duration.between(at, now).seconds
    val ago = seconds >= 0
    val span = kotlin.math.abs(seconds)
    val direction = if (ago) RelativeDateTimeFormatter.Direction.LAST else RelativeDateTimeFormatter.Direction.NEXT
    fun count(value: Long, unit: RelativeDateTimeFormatter.RelativeUnit) =
        formatter.format(value.toDouble(), if (ago) RelativeDateTimeFormatter.Direction.LAST else RelativeDateTimeFormatter.Direction.NEXT, unit)
    fun named(unit: RelativeDateTimeFormatter.AbsoluteUnit) = formatter.format(direction, unit)
    return when {
        span < 60 -> formatter.format(RelativeDateTimeFormatter.Direction.PLAIN, RelativeDateTimeFormatter.AbsoluteUnit.NOW)
        span < 3600 -> count(span / 60, RelativeDateTimeFormatter.RelativeUnit.MINUTES)
        span < 86_400 -> count(span / 3600, RelativeDateTimeFormatter.RelativeUnit.HOURS)
        span < 2 * 86_400 -> named(RelativeDateTimeFormatter.AbsoluteUnit.DAY)
        span < 7 * 86_400 -> count(span / 86_400, RelativeDateTimeFormatter.RelativeUnit.DAYS)
        span < 14 * 86_400 -> named(RelativeDateTimeFormatter.AbsoluteUnit.WEEK)
        span < 30 * 86_400 -> count(span / (7 * 86_400), RelativeDateTimeFormatter.RelativeUnit.WEEKS)
        span < 60 * 86_400 -> named(RelativeDateTimeFormatter.AbsoluteUnit.MONTH)
        span < 365 * 86_400 -> count(span / (30 * 86_400), RelativeDateTimeFormatter.RelativeUnit.MONTHS)
        span < 2 * 365 * 86_400 -> named(RelativeDateTimeFormatter.AbsoluteUnit.YEAR)
        else -> count(span / (365 * 86_400), RelativeDateTimeFormatter.RelativeUnit.YEARS)
    }
}

/**
 * A row with one action out of sight until it is swiped to the left, as
 * the system's list rows keep theirs (`.swipeActions(edge: .trailing)`):
 * Remove on a recent room, Cancel on an arranged meeting. Let go past half
 * the action's width it stays open; tapped while open it closes rather than
 * opening the row; with `fullSwipe`, swiped most of the way across the action
 * is taken. A screen reader is offered the action as one of the row's own.
 */
@Composable
fun SwipeRow(
    action: String,
    symbol: String,
    onAction: () -> Unit,
    modifier: Modifier = Modifier,
    fullSwipe: Boolean = false,
    actionLabel: String = action,
    content: @Composable (closeFirst: () -> Boolean) -> Unit,
) {
    val density = LocalDensity.current
    val reveal = with(density) { 88.dp.toPx() }
    val offset = remember { Animatable(0f) }
    var width by remember { mutableFloatStateOf(0f) }
    val scope = rememberCoroutineScope()
    fun settle(velocity: Float) {
        scope.launch {
            val pulled = -offset.value
            when {
                fullSwipe && width > 0f && pulled > width * 0.55f -> {
                    offset.animateTo(-width, Theme.settle())
                    onAction()
                }
                pulled > reveal / 2 || velocity < -800f -> offset.animateTo(-reveal, Theme.settle())
                else -> offset.animateTo(0f, Theme.settle())
            }
        }
    }
    val drag = rememberDraggableState { delta ->
        scope.launch { offset.snapTo((offset.value + delta).coerceIn(-(if (fullSwipe) width else reveal * 1.4f), 0f)) }
    }
    Box(
        modifier
            .clipToBounds()
            .onSizeChanged { width = it.width.toFloat() }
            .semantics { customActions = listOf(CustomAccessibilityAction(actionLabel) { onAction(); true }) },
    ) {
        // The action, behind the row, filling what the row has uncovered.
        Box(Modifier.matchParentSize(), contentAlignment = Alignment.CenterEnd) {
            val shown = with(density) { (-offset.value).coerceAtLeast(0f).toDp() }
            Column(
                Modifier
                    .width(shown)
                    .fillMaxHeight()
                    .background(Theme.danger)
                    .pressable(rememberPress(), RectangleShape, shown > 0.dp, Role.Button, actionLabel) {
                        scope.launch {
                            offset.animateTo(-width, Theme.settle())
                            onAction()
                        }
                    },
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                if (shown > 40.dp) {
                    Symbol(symbol, size = 15f, weight = Font.Semibold, tint = Theme.dangerForeground)
                    KitText(action, Modifier.padding(top = 4.dp, start = 4.dp, end = 4.dp), font = Font(12f, Font.Medium), color = Theme.dangerForeground, lines = 1)
                }
            }
        }
        Box(
            Modifier
                .offset { IntOffset(offset.value.roundToInt(), 0) }
                .draggable(drag, Orientation.Horizontal, onDragStopped = { velocity -> settle(velocity) }),
        ) {
            content {
                // A tap on a row that is open closes it and goes no further.
                if (offset.value != 0f) {
                    scope.launch { offset.animateTo(0f, Theme.settle()) }
                    true
                } else {
                    false
                }
            }
        }
    }
}

/** A hairline between the rows of a card, from `leading` on (`Divider().padding(.leading:)`). */
@Composable
fun RowRule(modifier: Modifier = Modifier, leading: Dp = 0.dp) {
    Box(
        modifier
            .fillMaxWidth()
            .padding(start = leading)
            .height(0.5.dp)
            .background(Theme.wash.copy(alpha = 0.12f)),
    )
}
