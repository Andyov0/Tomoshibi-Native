package app.tomoshibi.ui.kit

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme

// The pieces a popover menu is made of (Kit.swift, Menus): rows of items,
// small headings, rules. Drawn rather than the system's pull-down menu,
// which looked like a preferences pane bolted onto a call (the owner,
// 2026-10-17).

/** A popover menu's rows, two points apart inside six of margin (`MenuCard`). */
@Composable
fun MenuCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(modifier.fillMaxWidth().padding(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp), content = content)
}

/** A small heading over some of a menu's rows, in capitals (`MenuHeading`). */
@Composable
fun MenuHeading(title: String, modifier: Modifier = Modifier) {
    KitText(
        title.uppercase(),
        modifier.padding(start = 10.dp, end = 10.dp, top = 8.dp, bottom = 2.dp),
        font = Font(10.5f, Font.Semibold, tracking = 0.6f),
        color = Theme.muted,
        lines = 1,
    )
}

/** A hairline between parts of a menu (`MenuRule`). */
@Composable
fun MenuRule(modifier: Modifier = Modifier) {
    Box(
        modifier
            .padding(horizontal = 8.dp, vertical = 4.dp)
            .fillMaxWidth()
            .height(1.dp)
            .background(Theme.wash.copy(alpha = 0.08f)),
    )
}

/** What a menu row is for: most are plain, leaving is red, the main choice wears the colour. */
enum class MenuRole { Normal, Destructive, Prominent }

/**
 * One row of a menu: a symbol, words, perhaps a detail, a check or a spinner
 * (`MenuItem`). Lit under the pointer, and under the finger while it is
 * pressed, which is what a touch screen has for a pointer resting on it.
 */
@Composable
fun MenuItem(
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    detail: String? = null,
    role: MenuRole = MenuRole.Normal,
    checked: Boolean = false,
    busy: Boolean = false,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val light by animateFloatAsState(if (press.lit && on) 0.09f else 0f, Motion.hover(), label = "light")
    val shape = rounded(10f)
    Row(
        modifier
            .fillMaxWidth()
            .alpha(if (on) 1f else 0.4f)
            .background(Theme.wash.copy(alpha = light), shape)
            .pressable(press, shape, on, Role.Button, selected = if (checked) true else null, onClick = onClick)
            // A fingertip tall on a phone and the Fold's cover screen, where a
            // menu is pressed with a thumb and its rows touch; the Mac's 34
            // points in a wide window.
            .heightIn(min = if (narrowWindow()) Fingertip else 34.dp)
            .padding(horizontal = 10.dp, vertical = if (detail == null) 7.dp else 6.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                Symbol(
                    symbol,
                    size = 13f,
                    weight = Font.Medium,
                    tint = when (role) {
                        MenuRole.Destructive -> Theme.dangerInk
                        MenuRole.Prominent -> Theme.tallyInk
                        MenuRole.Normal -> Theme.foreground.copy(alpha = 0.85f)
                    },
                )
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            KitText(
                title,
                font = Font(13.5f, if (role == MenuRole.Normal) Font.Regular else Font.Semibold),
                color = if (role == MenuRole.Destructive) Theme.dangerInk else Theme.foreground,
            )
            if (detail != null) KitText(detail, font = Font(11.5f), color = Theme.muted)
        }
        // The Swift row's spacer (eight points), and the row's spacing after
        // it while there is a tick or a spinner to space.
        Box(Modifier.padding(start = 8.dp), contentAlignment = Alignment.Center) {
            Appear(
                visible = busy,
                enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f),
                exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.6f),
            ) { MediaSpinner(Modifier.padding(start = 10.dp), size = 14f) }
            Appear(
                visible = checked && !busy,
                enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle()),
                exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave()),
            ) { Symbol("checkmark", Modifier.padding(start = 10.dp), size = 11.5f, weight = Font.Bold, tint = Theme.tallyInk) }
        }
    }
}

/**
 * A menu row that is a switch (`MenuSwitch`). The whole row turns it, so a
 * finger need not find the switch itself, and a screen reader says it once,
 * as a switch with its words.
 */
@Composable
fun MenuSwitch(
    title: String,
    isOn: Boolean,
    onChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    detail: String? = null,
    enabled: Boolean = true,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val shape = rounded(10f)
    Row(
        modifier
            .fillMaxWidth()
            .switchable(press, shape, isOn, on, onChange = onChange)
            .heightIn(min = if (narrowWindow()) Fingertip else 36.dp)
            .padding(horizontal = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                Symbol(symbol, size = 13f, weight = Font.Medium, tint = Theme.foreground.copy(alpha = 0.85f))
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            KitText(title, font = Font(13.5f))
            if (detail != null) KitText(detail, font = Font(11.5f), color = Theme.muted)
        }
        // After the Swift row's spacer (eight points) and its spacing.
        SwitchTrack(isOn, on, Modifier.padding(start = 18.dp))
    }
}
