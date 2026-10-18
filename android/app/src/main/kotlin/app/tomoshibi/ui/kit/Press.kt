package app.tomoshibi.ui.kit

import androidx.compose.foundation.IndicationNodeFactory
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.FocusInteraction
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.node.DelegatableNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.Modifier.Node
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import kotlinx.coroutines.launch

// How the kit's controls answer a finger, a pointer and a keyboard.
//
// No ripple: the Apple apps answer a press by the control itself giving a
// little (a scale, a light), and Material's ink spreading from the finger is
// the one thing that would say "Android app" at once. What the system's
// focus does for a keyboard on the iPad, a ring in the person's colour does
// here, drawn only while a keyboard or a remote has moved focus onto the
// control: Compose gives a clickable focus only outside touch mode.

/** The state a control is drawn from: pressed, and under a pointer. */
internal class Press(val source: MutableInteractionSource, pressedState: State<Boolean>, hoveredState: State<Boolean>) {
    val pressed by pressedState
    val hovered by hoveredState

    /** What SwiftUI's `onHover` lights: on Android a finger resting on it too. */
    val lit by derivedStateOf { pressed || hovered }
}

@Composable
internal fun rememberPress(): Press {
    val source = remember { MutableInteractionSource() }
    val pressed = source.collectIsPressedAsState()
    val hovered = source.collectIsHoveredAsState()
    return remember(source) { Press(source, pressed, hovered) }
}

/**
 * A press on the control, with its focus ring, its role and the words a
 * screen reader says for it when it has none of its own (an icon button's
 * `.accessibilityLabel`). `selected` is `.accessibilityAddTraits(.isSelected)`.
 */
internal fun Modifier.pressable(
    press: Press,
    shape: Shape,
    enabled: Boolean,
    role: Role? = Role.Button,
    label: String? = null,
    selected: Boolean? = null,
    onClick: () -> Unit,
): Modifier = this
    .clickable(interactionSource = press.source, indication = FocusRing(shape), enabled = enabled, role = role,
        onClick = onClick)
    .described(label, selected)

/** The same for a switch: on or off, said as a switch is. */
internal fun Modifier.switchable(
    press: Press,
    shape: Shape,
    on: Boolean,
    enabled: Boolean,
    label: String? = null,
    onChange: (Boolean) -> Unit,
): Modifier = this
    .toggleable(value = on, interactionSource = press.source, indication = FocusRing(shape), enabled = enabled,
        role = Role.Switch, onValueChange = onChange)
    .described(label, null)

internal fun Modifier.described(label: String?, selected: Boolean?): Modifier =
    if (label == null && selected == null) this else semantics {
        if (label != null) contentDescription = label
        if (selected != null) this.selected = selected
    }

/**
 * SwiftUI's `.brightness(amount)` within `shape`: the light a plain surface
 * gives when it is pressed, added to whatever is drawn there.
 */
internal fun Modifier.brightened(shape: Shape, amount: () -> Float): Modifier = drawWithContent {
    drawContent()
    val light = amount()
    if (light > 0f) {
        drawOutline(shape.createOutline(size, layoutDirection, this), Color.White.copy(alpha = light),
            blendMode = BlendMode.Plus)
    }
}

/** The ring a control wears while a keyboard has it. */
internal class FocusRing(private val shape: Shape) : IndicationNodeFactory {
    override fun create(interactionSource: InteractionSource): DelegatableNode = FocusRingNode(interactionSource, shape)
    override fun equals(other: Any?) = other is FocusRing && other.shape == shape
    override fun hashCode() = shape.hashCode()
}

private class FocusRingNode(private val source: InteractionSource, private val shape: Shape) : Node(), DrawModifierNode {
    private var focused = false

    override fun onAttach() {
        coroutineScope.launch {
            var held = 0
            source.interactions.collect { interaction ->
                when (interaction) {
                    is FocusInteraction.Focus -> held++
                    is FocusInteraction.Unfocus -> held = maxOf(0, held - 1)
                }
                val now = held > 0
                if (now != focused) {
                    focused = now
                    invalidateDraw()
                }
            }
        }
    }

    override fun ContentDrawScope.draw() {
        drawContent()
        if (!focused) return
        val gap = 3.dp.toPx()
        val ring = Size(size.width + gap * 2, size.height + gap * 2)
        translate(-gap, -gap) {
            drawOutline(shape.createOutline(ring, layoutDirection, this), Theme.tally.copy(alpha = 0.9f),
                style = Stroke(width = 2.dp.toPx()))
        }
    }
}
