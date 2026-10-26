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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
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
     * screen) they do not: the half was twenty points wide, and the
     * reactions are opened by holding the hand, or from More (ControlBar).
     */
    beside: Boolean = true,
    size: Dp = 44.dp,
    act: (suspend () -> Unit) -> Unit,
) {
    val board = ReactionBoard.of(call)
    val open = popovers.open == TRAY
    val haptics = LocalHapticFeedback.current
    val up = call.handUp
    val handFill by animateColorAsState(if (up) Theme.tally else Theme.wash.copy(alpha = 0.12f), Theme.settle(), label = "hand")
    val handInk by animateColorAsState(if (up) Theme.tallyForeground else Theme.foreground, Theme.settle(), label = "ink")
    val trayFill by animateColorAsState(Theme.wash.copy(alpha = if (open) 0.2f else 0.12f), Theme.settle(), label = "tray")
    val turn by animateFloatAsState(if (open) 180f else 0f, Theme.settle(), label = "turn")
    val raise by rememberUpdatedState { act { call.setHand(!call.handUp) } }
    val handPress = remember { MutableInteractionSource() }
    val handScale by animateFloatAsState(if (pressing(handPress)) 0.93f else 1f, tween(120), label = "press")
    // A fingertip each on a phone and the cover screen, side by side with
    // nothing between them: the six were 36 points apart.
    val narrow = narrowWindow()
    BarPopover(
        TRAY, popovers, axis,
        width = if (narrow) 300.dp else 234.dp,
        panel = {
            Row(Modifier.padding(4.dp), horizontalArrangement = Arrangement.spacedBy(if (narrow) 0.dp else 2.dp)) {
                for (one in CrateRules.reactionChoices()) {
                    ReactionChoice(one, if (narrow) DpSize(Fingertip, Fingertip) else DpSize(36.dp, 34.dp)) {
                        board.send(one)
                        popovers.close()
                    }
                }
            }
        },
    ) {
        Row {
            val label = if (up) t("Lower your hand") else t("Raise your hand")
            Box(
                Modifier
                    .graphicsLayer {
                        scaleX = handScale
                        scaleY = handScale
                    }
                    .size(if (beside) 40.dp else size, if (beside) 44.dp else size)
                    .background(
                        handFill,
                        if (beside) RoundedCornerShape(topStart = 22.dp, bottomStart = 22.dp) else RoundedCornerShape(50),
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
            if (beside) Box(
                Modifier
                    .size(20.dp, 44.dp)
                    .background(trayFill, RoundedCornerShape(topEnd = 22.dp, bottomEnd = 22.dp))
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
                Symbol("chevron.up", Modifier.graphicsLayer { rotationZ = turn }, size = 9f, weight = Font.Bold,
                    tint = if (open) Theme.foreground else Theme.muted)
            }
        }
    }
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

/** One of the six: the reaction itself, which is its own name, as the web labels it. */
@Composable
private fun ReactionChoice(what: String, size: DpSize, action: () -> Unit) {
    val source = remember { MutableInteractionSource() }
    val hovering by source.collectIsHoveredAsState()
    val lit = hovering || pressing(source)
    val grow by animateFloatAsState(if (lit) 1.25f else 1f, Theme.settle(), label = "grow")
    Box(
        Modifier
            .size(size)
            .background(if (lit) Theme.surface else Color.Transparent, RoundedCornerShape(8.dp))
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
