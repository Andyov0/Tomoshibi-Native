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
 * One round button of the control bar: a lit plate in the chosen colour
 * while what it stands for is on, a plain one otherwise, a little brighter
 * under the pointer and while its popover is open (`BarButton`). Inside the
 * bar's glass the buttons are plates rather than glass of their own: glass
 * on glass reads as smudge.
 *
 * `quiet` is off and drawn quieter for it: a muted microphone is a choice,
 * not a fault, so it is unlit rather than red (ControlBar.tsx `Toggle`).
 * `pressed` is its popover being open. `dot` is [BarDot] in its corner.
 */
@Composable
fun BarButton(
    symbol: String,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    lit: Boolean = false,
    quiet: Boolean = false,
    pressed: Boolean = false,
    dot: Boolean = false,
    size: Dp = 44.dp,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val plate by animateColorAsState(
        when {
            lit -> Theme.tally
            pressed -> Theme.wash.copy(alpha = 0.24f)
            press.lit && on -> Theme.wash.copy(alpha = 0.18f)
            else -> Theme.wash.copy(alpha = 0.12f)
        },
        if (lit || pressed) Theme.settle() else Motion.hover(),
        label = "plate",
    )
    val edge by animateFloatAsState(if (lit) 0.3f else if (pressed) 0.2f else 0.08f, Theme.settle(), label = "edge")
    val ink by animateColorAsState(
        if (lit) Theme.tallyForeground else if (quiet) Theme.muted else Theme.foreground,
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
            SwappingSymbol(symbol, size = size.value * 0.36f, weight = Font.Semibold, tint = ink)
        }
        BarDot(dot, Modifier.align(Alignment.TopEnd))
    }
}

/** A press that answers: the button gives a little under the finger (`BarPress`). */
@Composable
internal fun Modifier.barPress(press: Press): Modifier {
    val scale by animateFloatAsState(if (press.pressed) 0.93f else 1f, Motion.hover(), label = "press")
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
