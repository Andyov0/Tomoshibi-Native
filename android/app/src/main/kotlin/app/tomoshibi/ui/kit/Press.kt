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
import androidx.compose.ui.semantics.testTag
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
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

/** The least a control should be pressed by, Android's own: 48 points, a fingertip. */
internal val Fingertip = 48.dp

/** Narrower than this, the window is a phone's or the Fold's cover screen, held in one hand and pressed with a thumb. */
private val NarrowWindow = 600.dp

/** Whether the app's window is a phone's: a phone's, or the cover screen of a Fold. */
@Composable
internal fun narrowWindow(): Boolean =
    with(LocalDensity.current) { LocalWindowInfo.current.containerSize.width.toDp() } < NarrowWindow

/**
 * How large a control drawn [drawn] across is pressed at: a fingertip in a
 * narrow window, and the size it is drawn at in a wide one, which keeps the
 * iPad's and the Mac's spacing. The control is still drawn at its own size
 * inside that, as the iPhone draws it. Compose's own reach beyond a small
 * control (the view configuration's minimum touch target) is not enough
 * here: it gives way to any neighbour nearer than a fingertip, and in a row
 * of the small round buttons a panel lines up beside a name every neighbour
 * was nearer.
 */
@Composable
internal fun fingertip(drawn: Dp): Dp = if (narrowWindow()) maxOf(drawn, Fingertip) else drawn

/**
 * Every press made on this is this's, never what lies under it: a bar of
 * controls or a panel floating over the call's pictures, which took a press
 * that landed between two buttons, or on a panel's heading, and put the
 * picture under it on the stage. Holding the press here also lets Compose
 * give it to whichever control in here is within a fingertip of it, which a
 * picture under a gap between two buttons, pressed directly, always beat.
 */
internal fun Modifier.holdsPresses(): Modifier = this
    .semantics { testTag = HOLDS_PRESSES }
    .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } }

/** Says, to a test looking for controls a fingertip apart, where presses are held. */
internal const val HOLDS_PRESSES = "holds.presses"

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
