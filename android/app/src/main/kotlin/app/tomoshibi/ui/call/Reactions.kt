package app.tomoshibi.ui.call

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Plan
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.host.CrateRules
import app.tomoshibi.model.host.ReactionBoard
import app.tomoshibi.ui.kit.BarAxis
import app.tomoshibi.ui.kit.BarPopover
import app.tomoshibi.ui.kit.Fingertip
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.PressLift
import app.tomoshibi.ui.kit.PressedScale
import app.tomoshibi.ui.kit.lifted
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.narrowWindow
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.keepDeparting
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.min

/**
 * Putting a hand up, and the reactions behind it (components/room/HandButton.tsx;
 * Reactions.swift `HandControl`).
 *
 * One control for two things, because they are one intention: saying
 * something without interrupting whoever is talking. A press on the hand puts
 * it up or down and never opens anything; the narrow half beside it opens the
 * six reactions above the bar, and choosing one sends it and closes them. A
 * long press on the hand opens them too, the other way of pressing it
 * (HandButton.tsx `onContextMenu`), with the small knock a long press gives.
 *
 * The six are one of the bar's popovers, a window of their own, so that they
 * close on a press anywhere else and on Back, and are never drawn under the
 * glass of something laid over the bar.
 */
@Composable
internal fun HandControl(
    call: CallModel,
    popovers: Popovers,
    axis: BarAxis,
    /**
     * Whether the reactions have their narrow half beside the hand. On a bar
     * too short for every control at a fingertip's width (the Fold's cover
     * screen) they do not, and the reactions are opened by holding the
     * hand, or from More (ControlBar).
     */
    beside: Boolean = true,
    /**
     * The bar's buttons' size, which the hand keeps to across the bar (its
     * height in a row, its width in a column): at forty-four high in the
     * compact bar's forty, the hand stood out of the row.
     */
    size: Dp = 44.dp,
    act: (suspend () -> Unit) -> Unit,
) {
    val board = ReactionBoard.of(call)
    val open = popovers.open == TRAY
    val haptics = LocalHapticFeedback.current
    val up = call.handUp
    val handFill by animateColorAsState(if (up) Theme.tally else Theme.wash.copy(alpha = 0.12f), Theme.settle(), label = "hand")
    val handInk by animateColorAsState(if (up) Theme.tallyForeground else Theme.foreground, Theme.settle(), label = "ink")
    // Held down while the six are open, as every button whose popover is
    // open is (BarButton): the finger's step of the bar's plate.
    val trayFill by animateColorAsState(Theme.wash.copy(alpha = if (open) lifted(0.12f, PressLift) else 0.12f), Theme.settle(), label = "tray")
    val turn by animateFloatAsState(if (open) 180f else 0f, Theme.settle(), label = "turn")
    val raise by rememberUpdatedState { act { call.setHand(!call.handUp) } }
    val handPress = remember { MutableInteractionSource() }
    val handScale by animateFloatAsState(if (pressing(handPress)) PressedScale else 1f, tween(120), label = "press")
    // A fingertip each on a phone and the cover screen, side by side with
    // nothing between them: the six were 36 points apart. Each drawn as a
    // forty point circle under the finger, the web's tray.
    val narrow = narrowWindow()
    // The hand and the narrow half side by side in a row, and one above the
    // other in a column: side by side down the side they were sixty points
    // wide in a bar fifty-eight wide, and stood out of it on both sides.
    val row = axis == BarAxis.Horizontal
    val round = size / 2
    BarPopover(
        TRAY, popovers, axis,
        width = if (narrow) 300.dp else 260.dp,
        panel = {
            Row(Modifier.padding(4.dp), horizontalArrangement = Arrangement.spacedBy(if (narrow) 0.dp else 2.dp)) {
                for (one in CrateRules.reactionChoices()) {
                    ReactionChoice(one, if (narrow) Fingertip else ReactionCell) {
                        board.send(one)
                        popovers.close()
                    }
                }
            }
        },
    ) {
        Stacked(row) {
            val label = if (up) t("Lower your hand") else t("Raise your hand")
            Box(
                Modifier
                    .graphicsLayer {
                        scaleX = handScale
                        scaleY = handScale
                    }
                    .size(size)
                    .background(
                        handFill,
                        when {
                            !beside -> RoundedCornerShape(50)
                            row -> RoundedCornerShape(topStart = round, bottomStart = round)
                            else -> RoundedCornerShape(topStart = round, topEnd = round)
                        },
                    )
                    .pointerInput(Unit) {
                        detectTapGestures(
                            onPress = {
                                val press = PressInteraction.Press(it)
                                handPress.emit(press)
                                tryAwaitRelease()
                                handPress.emit(PressInteraction.Release(press))
                            },
                            onLongPress = {
                                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                popovers.toggle(TRAY)
                            },
                            onTap = { raise() },
                        )
                    }
                    .semantics {
                        role = Role.Button
                        contentDescription = label
                        onClick(label) {
                            raise()
                            true
                        }
                        onLongClick(t("Reactions")) {
                            popovers.toggle(TRAY)
                            true
                        }
                    },
                contentAlignment = Alignment.Center,
            ) { Symbol("hand.raised.fill", size = 16f, weight = Font.Semibold, tint = handInk) }
            // The narrow half's press is its own length along the bar and no
            // more. Compose widens anything shorter than a fingertip to one,
            // centred on it, which took this half's reach into the button for
            // the messages beside it: the two overlapped by twenty-six pixels,
            // and a screen reader's finger over the edge of the messages'
            // button found the reactions (the health check of 2026-10-29).
            // Across the bar it is still widened to a fingertip. Thirty-two
            // along it, the web's: at twenty it was the smallest thing on
            // the bar to aim at.
            if (beside) CompositionLocalProvider(LocalViewConfiguration provides narrowTouch(LocalViewConfiguration.current, Half, row)) { Box(
                Modifier
                    .size(if (row) Half else size, if (row) size else Half)
                    .background(
                        trayFill,
                        if (row) RoundedCornerShape(topEnd = round, bottomEnd = round) else RoundedCornerShape(bottomStart = round, bottomEnd = round),
                    )
                    .pointerInput(Unit) { detectTapGestures { popovers.toggle(TRAY) } }
                    .semantics {
                        role = Role.Button
                        contentDescription = t("Reactions")
                        selected = open
                        onClick(t("Reactions")) {
                            popovers.toggle(TRAY)
                            true
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                // Towards where the tray opens: up over a row, and towards the
                // pictures from a column (Reactions.swift).
                Symbol(if (row) "chevron.up" else "chevron.left", Modifier.graphicsLayer { rotationZ = turn }, size = 11f,
                    weight = Font.Bold, tint = if (open) Theme.foreground else Theme.muted)
            } }
        }
    }
}

/** The reactions' half beside the hand, along the bar. */
internal val Half = 32.dp

/** One of the six, drawn: the web's forty, a fingertip to press on a phone. */
private val ReactionCell = 40.dp

/** [content] in a row along a bar at the foot, in a column down one at the side. */
@Composable
private fun Stacked(row: Boolean, content: @Composable () -> Unit) {
    if (row) Row { content() } else androidx.compose.foundation.layout.Column { content() }
}

/**
 * [base], but with the least a press reaches along the bar held to [length]
 * ([row]: the bar runs across, so the width): across the bar it keeps the
 * fingertip.
 */
internal fun narrowTouch(base: ViewConfiguration, length: Dp, row: Boolean = true): ViewConfiguration = object : ViewConfiguration by base {
    override val minimumTouchTargetSize: DpSize
        get() = if (row) DpSize(length, base.minimumTouchTargetSize.height) else DpSize(base.minimumTouchTargetSize.width, length)
}

/** The bar popover the six reactions are drawn in. */
internal const val TRAY = "reactions"

/** Whether a finger is down on what [source] belongs to. */
@Composable
private fun pressing(source: MutableInteractionSource): Boolean {
    var down by remember { mutableStateOf(false) }
    LaunchedEffect(source) {
        source.interactions.collect {
            when (it) {
                is PressInteraction.Press -> down = true
                is PressInteraction.Release, is PressInteraction.Cancel -> down = false
            }
        }
    }
    return down
}

/**
 * One of the six: the reaction itself, which is its own name, as the web
 * labels it. Pressed over [size], drawn as the web's forty point circle
 * lighting under the finger.
 */
@Composable
private fun ReactionChoice(what: String, size: Dp, action: () -> Unit) {
    val source = remember { MutableInteractionSource() }
    val hovering by source.collectIsHoveredAsState()
    val lit = hovering || pressing(source)
    val grow by animateFloatAsState(if (lit) 1.25f else 1f, Theme.settle(), label = "grow")
    val plate by animateColorAsState(if (lit) Theme.wash.copy(alpha = 0.12f) else Theme.wash.copy(alpha = 0f), Motion.hover(), label = "plate")
    Box(
        Modifier
            .size(size)
            .drawBehind { drawCircle(plate, radius = ReactionCell.toPx() / 2) }
            .hoverable(source)
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = {
                        val press = PressInteraction.Press(it)
                        source.emit(press)
                        tryAwaitRelease()
                        source.emit(PressInteraction.Release(press))
                    },
                    onTap = { action() },
                )
            }
            .semantics {
                role = Role.Button
                contentDescription = what
                onClick(what) {
                    action()
                    true
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        KitText(what, Modifier.graphicsLayer {
            scaleX = grow
            scaleY = grow
        }, font = Font(20f), lines = 1)
    }
}

/**
 * Whatever somebody just reacted with, rising off their picture
 * (components/room/Reacting.tsx; `FloatingReactions`): low on the picture,
 * centred, each new one beside the last rather than on top of it, rising and
 * gone in four seconds.
 */
@Composable
internal fun FloatingReactions(call: CallModel, identity: String, compact: Boolean = false) {
    val mine = ReactionBoard.of(call).on(identity)
    BoxWithConstraints(Modifier.fillMaxSize().clearAndSetSemantics {}) {
        // Above the island where the picture runs under it: the person
        // filling a call of two is drawn under the bar, and a reaction forty
        // points up was hidden behind it.
        val bottom = min(Plan.ISLAND + 12, maxHeight.value * 0.3f)
        Box(Modifier.fillMaxSize().padding(bottom = bottom.dp), contentAlignment = Alignment.BottomCenter) {
            for (entry in keepDeparting(mine) { it.id }) {
                AnimatedVisibility(
                    entry.visible,
                    // Reacting.tsx: `(at % 3) * 28 - 28` points from the
                    // middle, so two of the same at once are two things.
                    Modifier.offset(x = ((entry.place % 3) * 28 - 28).dp),
                    enter = fadeIn(tween(0)),
                    exit = fadeOut(Theme.leave()),
                ) { Floating(entry.item.what, if (compact) 22f else 30f) }
            }
        }
    }
}

/**
 * One reaction's flight: the web's `float` keyframes, in from a little below
 * at sixty per cent of its size over the first eighth, still until seven
 * tenths, then up sixty-four points and out.
 */
@Composable
private fun Floating(what: String, size: Float) {
    val arrival = remember { Animatable(0f) }
    val departure = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        val total = CrateRules.reactionShownForMs()
        launch { arrival.animateTo(1f, tween((total * 0.12).toInt(), easing = Theme.settleEasing)) }
        delay((total * 0.7).toLong())
        departure.animateTo(1f, tween((total * 0.3).toInt(), easing = EaseIn))
    }
    KitText(
        what,
        Modifier.graphicsLayer {
            val s = 0.6f + 0.4f * arrival.value
            scaleX = s
            scaleY = s
            translationY = (6f * (1 - arrival.value) - 64f * departure.value) * density
            alpha = arrival.value * (1 - departure.value)
        },
        font = Font(size),
        lines = 1,
    )
}

/** SwiftUI's `.easeIn`. */
private val EaseIn = CubicBezierEasing(0.42f, 0f, 1f, 1f)
