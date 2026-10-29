package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme

// What the call's controls add to the kit (Kit+Controls.swift): a button
// for the bar, the press it gives, the dot that says something is happening
// out of sight. The bar's popovers are BarPopover (Popovers.kt); the row to
// pick one of a list from is ChoiceRow (Choosing.kt).

/**
 * One round button of the control bar (`BarButton`). Inside the bar's glass
 * the buttons are plates rather than glass of their own: glass on glass
 * reads as smudge.
 *
 * Four looks, each meaning one thing (the owner, 2026-10-29):
 * - plain: a faint plate of the foreground, a step brighter under the
 *   pointer and another under the finger;
 * - [pressed], a popover or a panel it opened is open: held at the finger's
 *   step, the neutral grey of a button held down, so the button that closes
 *   it is the one that looks pressed. It was amber, and a bar with the
 *   messages open beside a raised hand had two amber buttons meaning two
 *   different things; amber is kept for what is happening in the call;
 * - [lit]: something is happening (a share going out), in the signal colour;
 * - [off]: the microphone or the camera is off, a solid plate of the
 *   foreground with the symbol in the page's colour, as the phone's own call
 *   screen draws a muted microphone: the one state on the bar somebody has to
 *   read from across the room. It was the plain plate with a dimmer symbol,
 *   which at arm's length was the same button; not red, which is for leaving
 *   and for faults, and a muted microphone is a choice.
 *
 * `dot` is [BarDot] in its corner. The symbol is four tenths of the button,
 * as the web's island draws it: at a third it read as a button with little
 * on it.
 */
@Composable
fun BarButton(
    symbol: String,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    lit: Boolean = false,
    off: Boolean = false,
    pressed: Boolean = false,
    dot: Boolean = false,
    size: Dp = 44.dp,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val held = pressed || (press.pressed && on)
    val plate by animateColorAsState(
        when {
            lit -> Theme.tally
            off -> Theme.foreground.copy(alpha = if (held) 0.78f else if (press.hovered && on) 0.88f else 1f)
            held -> Theme.wash.copy(alpha = lifted(PLATE, PressLift))
            press.hovered && on -> Theme.wash.copy(alpha = lifted(PLATE, HoverLift))
            else -> Theme.wash.copy(alpha = PLATE)
        },
        if (lit || pressed) Theme.settle() else Motion.hover(),
        label = "plate",
    )
    val edge by animateFloatAsState(if (lit) 0.3f else if (off) 0f else if (pressed) 0.2f else 0.08f, Theme.settle(), label = "edge")
    val ink by animateColorAsState(
        if (lit) Theme.tallyForeground else if (off) Theme.background else Theme.foreground,
        Theme.settle(),
        label = "ink",
    )
    Box(modifier) {
        Box(
            Modifier
                .barPress(press)
                .size(size)
                .background(plate, CircleShape)
                .border(1.dp, Theme.wash.copy(alpha = edge), CircleShape)
                .pressable(press, CircleShape, on, Role.Button, label, selected = if (lit || pressed) true else null,
                    onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            SwappingSymbol(symbol, size = size.value * 0.40f, weight = Font.Semibold, tint = ink)
        }
        BarDot(dot, Modifier.align(Alignment.TopEnd))
    }
}

/** The plain bar button's plate: the foreground at this strength, over the bar's glass. */
private const val PLATE = 0.12f

/** A press that answers: the button gives a little under the finger (`BarPress`). */
@Composable
internal fun Modifier.barPress(press: Press): Modifier {
    val scale by animateFloatAsState(if (press.pressed) PressedScale else 1f, Motion.hover(), label = "press")
    return graphicsLayer {
        scaleX = scale
        scaleY = scale
    }
}

/**
 * The dot a bar button wears when something it stands for is happening out
 * of sight (ControlBar.tsx ~383, `BarDot`): in the signal colour, ringed in
 * the bar's own grey, arriving and leaving as everything does.
 */
@Composable
fun BarDot(visible: Boolean, modifier: Modifier = Modifier) {
    AnimatedVisibility(visible, modifier.offset(x = (-2).dp, y = 2.dp), enter = Theme.arrive, exit = Theme.depart) {
        Box(
            Modifier
                .size(9.dp)
                .background(Theme.tally, CircleShape)
                .border(2.dp, Theme.surfaceHigh, CircleShape),
        )
    }
}
