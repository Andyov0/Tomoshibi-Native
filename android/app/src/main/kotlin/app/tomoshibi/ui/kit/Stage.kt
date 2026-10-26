package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme

// The small controls drawn over pictures (Kit+Stage.swift, Glass over a
// picture). The window's full screen and the Mac's window reader are the
// Mac's alone and have no part here.

/**
 * A round control laid over a picture, or a capsule of words with `words`
 * (`PictureButtonStyle`, `.overPicture`): dark, so it reads on a bright slide
 * and on a black one alike, lit a little under the pointer and the finger,
 * giving a little when pressed.
 *
 * The Swift one is the system's thin material under a black shade. Here it
 * is the shade alone, a little deeper: a call's pictures are drawn on
 * surfaces a blur cannot sample, and there is one of these on every tile,
 * which is the many live blurs Apple's guidance and the hot phone both warn
 * against; over a moving picture the two read the same.
 */
@Composable
fun PictureButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 32.dp,
    words: Boolean = false,
    label: String? = null,
    enabled: Boolean = true,
    content: @Composable RowScope.() -> Unit,
) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val shade by animateFloatAsState(if (press.lit) 0.64f else 0.52f, Motion.hover(), label = "shade")
    val scale by animateFloatAsState(if (press.pressed) 0.94f else 1f, Theme.settle(), label = "press")
    val shape = if (words) Capsule else CircleShape
    Row(
        modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .alpha(if (on) 1f else 0.45f)
            .height(size)
            .widthIn(min = size)
            .background(Color.Black.copy(alpha = shade), shape)
            .border(1.dp, Color.White.copy(alpha = 0.14f), shape)
            .pressable(press, shape, on, Role.Button, label, onClick = onClick)
            .padding(horizontal = if (words) 12.dp else 0.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Light words whatever the appearance: what is under them is a picture.
        KitInk(Theme.onPicture) { KitFont(Font(12.5f, Font.Semibold)) { content() } }
    }
}

/**
 * One of the paging arrows either side of the pictures (Plane.tsx
 * `PageButton`; `PageArrow`): quiet until the pointer is on it, as the web's
 * are at sixty per cent until hovered, and looping, so the page indicator is
 * what says where somebody is. A finger on it lights it and presses it in.
 */
@Composable
fun PageArrow(symbol: String, label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val press = rememberPress()
    val shown by animateFloatAsState(if (press.lit) 1f else 0.6f, Theme.settle(), label = "shown")
    val scale by animateFloatAsState(
        if (press.pressed) 0.94f else if (press.hovered) 1.06f else 1f,
        Theme.settle(), label = "scale",
    )
    Box(
        modifier
            .glassLayer(alpha = { shown }, scale = { scale })
            .size(38.dp)
            .glassSurface(CircleShape, interactive = true)
            .pressable(press, CircleShape, true, Role.Button, label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Symbol(symbol, size = 14f, weight = Font.Bold, tint = Theme.foreground) }
}

/**
 * Which page is showing, "2 / 3" (`PageIndicator`). At the top rather than
 * the bottom, as the web learned: centred at the bottom it was under the
 * controls on every size. The figure rolls to the next as the page turns.
 */
@Composable
fun PageIndicator(page: Int, pages: Int, modifier: Modifier = Modifier) {
    val ink = Theme.foreground.copy(alpha = 0.85f)
    Row(
        modifier
            .height(24.dp)
            .glassSurface(Capsule)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnimatedContent(
            page,
            transitionSpec = {
                val up = targetState > initialState
                (slideInVertically(Theme.settle()) { if (up) it else -it } + fadeIn(Theme.settle())) togetherWith
                    (slideOutVertically(Theme.leave()) { if (up) -it else it } + fadeOut(Theme.leave()))
            },
            label = "page",
        ) { shown ->
            KitText("${shown + 1} / $pages", font = Font(11.5f, Font.Semibold, digits = true), color = ink, lines = 1)
        }
    }
}
