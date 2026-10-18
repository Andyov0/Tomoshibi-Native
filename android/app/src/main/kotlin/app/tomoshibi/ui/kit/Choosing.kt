package app.tomoshibi.ui.kit

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme

// Choosing (Kit.swift's switch and segmented choice, Kit+Settings.swift's
// GlassChoice, Kit+Controls.swift's ChoiceRow): the app's own, each moving
// to its new state with Theme.settle.

/**
 * The app's switch: a capsule that fills with the chosen colour when on
 * (`GlassSwitchStyle`). `label` is what a screen reader calls it, for a switch
 * with no words beside it (`Toggle("", isOn:).labelsHidden()`).
 */
@Composable
fun GlassSwitch(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    label: String? = null,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    SwitchTrack(checked, on, modifier.switchable(press, Capsule, checked, on, label, onCheckedChange))
}

/** The switch drawn, for a row that is itself the switch. */
@Composable
internal fun SwitchTrack(checked: Boolean, enabled: Boolean, modifier: Modifier = Modifier) {
    val fill by animateColorAsState(if (checked) Theme.tally else Color.White.copy(alpha = 0.14f), Theme.settle(), label = "fill")
    val edge by animateFloatAsState(if (checked) 0f else 0.1f, Theme.settle(), label = "edge")
    val knob by animateDpAsState(if (checked) 15.dp else 0.dp, Theme.settle(), label = "knob")
    Box(
        modifier
            .alpha(if (enabled) 1f else 0.4f)
            .size(38.dp, 23.dp)
            .background(fill, Capsule)
            .border(1.dp, Color.White.copy(alpha = edge), Capsule)
            .padding(2.5.dp),
    ) {
        Box(
            Modifier
                .offset(x = knob)
                .size(18.dp)
                .dropShadow(CircleShape, Shadow(radius = 2.dp, offset = DpOffset(0.dp, 1.dp), alpha = 0.25f))
                .background(Color.White, CircleShape),
        )
    }
}

/**
 * A few choices side by side, the chosen one under a pill that slides to it
 * (`SegmentedChoice`).
 */
@Composable
fun <T> SegmentedChoice(
    options: List<Pair<T, String>>,
    selection: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    height: Dp = 30.dp,
) {
    val chosen = options.indexOfFirst { it.first == selection }
    val place by animateFloatAsState(chosen.coerceAtLeast(0).toFloat(), Theme.settle(), label = "pill")
    val gap = 2.dp
    Row(
        modifier
            .fillMaxWidth()
            .background(Color.Black.copy(alpha = 0.22f), Capsule)
            .border(1.dp, Color.White.copy(alpha = 0.07f), Capsule)
            .padding(2.dp)
            .drawBehind {
                if (chosen < 0 || options.isEmpty()) return@drawBehind
                val spacing = gap.toPx()
                val width = (size.width - spacing * (options.size - 1)) / options.size
                val left = place * (width + spacing)
                val radius = CornerRadius(size.height / 2)
                drawRoundRect(Color.White.copy(alpha = 0.16f), Offset(left, 0f), Size(width, size.height), radius)
                drawRoundRect(Color.White.copy(alpha = 0.10f), Offset(left + 0.5f, 0.5f), Size(width - 1f, size.height - 1f),
                    radius, style = Stroke(1.dp.toPx()))
            }
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(gap),
    ) {
        options.forEachIndexed { index, (value, label) ->
            val isChosen = index == chosen
            val ink by animateColorAsState(if (isChosen) Theme.foreground else Theme.muted, Theme.settle(), label = "ink")
            val press = rememberPress()
            Box(
                Modifier
                    .weight(1f)
                    .heightIn(min = height - 4.dp)
                    .selectable(isChosen, press.source, FocusRing(Capsule), role = Role.RadioButton) { onSelect(value) }
                    .padding(horizontal = 10.dp),
                contentAlignment = Alignment.Center,
            ) {
                KitText(label, font = Font(12.5f, if (isChosen) Font.Semibold else Font.Regular), color = ink, lines = 1, fit = true)
            }
        }
    }
}

/** One of a [GlassChoice]'s choices (`GlassChoice.Option`). */
data class GlassChoiceOption<T>(
    val value: T,
    val label: String,
    val detail: String? = null,
    val symbol: String? = null,
    val disabled: Boolean = false,
)

/**
 * One choice of several, as a capsule saying the current one; pressed, a
 * popover of the others (`GlassChoice`): what a pull-down menu was for. Uses
 * the page's [LocalPopovers], or popovers of its own on a page with none.
 */
@Composable
fun <T> GlassChoice(
    id: String,
    options: List<GlassChoiceOption<T>>,
    selection: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    width: Dp = 260.dp,
    height: Dp = 30.dp,
    heading: String? = null,
) {
    val popovers = LocalPopovers.current ?: rememberPopovers()
    val current = options.firstOrNull { it.value == selection }
    FloatingPopover(
        id,
        popovers,
        modifier,
        width = width,
        panel = {
            MenuCard {
                if (heading != null) MenuHeading(heading)
                for (option in options) {
                    MenuItem(
                        option.label,
                        onClick = {
                            onSelect(option.value)
                            popovers.close()
                        },
                        symbol = option.symbol,
                        detail = option.detail,
                        checked = option.value == selection,
                        enabled = !option.disabled,
                    )
                }
            }
        },
    ) {
        GlassButton(onClick = { popovers.toggle(id) }, shape = GlassButtonShape.Capsule(height)) {
            KitFont(Font(12.5f, Font.Medium)) {
                if (symbol != null) Symbol(symbol, size = 11.5f, weight = Font.Semibold)
                KitText(current?.label ?: "—", lines = 1)
                Symbol("chevron.up.chevron.down", size = 9f, weight = Font.Bold, tint = Theme.muted)
            }
        }
    }
}

/**
 * One of a list to pick from, inside a settings card: a device, a language
 * (`ChoiceRow`). The chosen one is ticked in the signal colour; the row
 * lights under the pointer and the finger, as a menu's rows do.
 */
@Composable
fun ChoiceRow(
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    checked: Boolean = false,
    last: Boolean = false,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit && on) 0.06f else 0f, Motion.hover(), label = "light")
    val tint by animateColorAsState(if (checked) Theme.tally else Theme.foreground.copy(alpha = 0.75f), Theme.settle(), label = "tint")
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .alpha(if (on) 1f else 0.45f)
                .background(Color.White.copy(alpha = light))
                .pressable(press, androidx.compose.ui.graphics.RectangleShape, on, Role.Button,
                    selected = if (checked) true else null, onClick = onClick)
                // A fingertip tall on a phone and the cover screen, where the
                // rows of a list touch (Press.kt `fingertip`).
                .heightIn(min = fingertip(40.dp))
                .padding(horizontal = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (symbol != null) {
                // As wide as a settings row's symbol tile, so the words of
                // both kinds of row start in one line.
                Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) {
                    Symbol(symbol, size = 12.5f, weight = Font.Semibold, tint = tint)
                }
            }
            KitText(
                title,
                Modifier.weight(1f),
                font = Font(13.5f, if (checked) Font.Medium else Font.Regular),
                lines = 1,
                overflow = TextOverflow.MiddleEllipsis,
            )
            // The Swift row's spacer (eight points), and the row's spacing
            // after it while there is a tick to space.
            Box(Modifier.padding(start = 8.dp)) {
                Appear(
                    visible = checked,
                    enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.5f),
                    exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.5f),
                ) {
                    Symbol("checkmark", Modifier.padding(start = 12.dp), size = 11.5f, weight = Font.Bold, tint = Theme.tally)
                }
            }
        }
        if (!last) Hairline(if (symbol == null) 14.dp else 52.dp)
    }
}

/** The line between two rows of a settings card, from where the words start. */
@Composable
internal fun Hairline(leading: Dp) {
    Box(
        Modifier
            .padding(start = leading)
            .fillMaxWidth()
            .height(1.dp)
            .background(Color.White.copy(alpha = 0.06f)),
    )
}
