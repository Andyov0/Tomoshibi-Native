package app.tomoshibi.ui.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Linked
import app.tomoshibi.basics.Theme

/**
 * A message with its addresses pressable, in the person's colour and
 * underlined (Chat.swift `LinkedText`); a press opens the address. Which
 * words are an address is basics' [Linked], the web's rule.
 */
@Composable
fun LinkedText(
    text: String,
    modifier: Modifier = Modifier,
    size: Float = 13f,
    color: Color = LocalInk.current,
    lines: Int = Int.MAX_VALUE,
) {
    // Words, and so the colour's ink.
    val tally = Theme.tallyInk
    val linked: AnnotatedString = remember(text, tally) {
        val style = TextLinkStyles(SpanStyle(color = tally, textDecoration = TextDecoration.Underline))
        buildAnnotatedString {
            for (piece in Linked.pieces(text)) {
                when (piece) {
                    is Linked.Piece.Text -> append(piece.text)
                    is Linked.Piece.Link -> withLink(LinkAnnotation.Url(piece.href, style)) { append(piece.href) }
                }
            }
        }
    }
    BasicText(linked, modifier, style = Font(size).style(color), maxLines = lines, overflow = TextOverflow.Ellipsis)
}

/** One message in the corner: who said it and what (Swift's `ChatLine`, as SaidView reads it). */
data class SaidLine(val id: String, val identity: String, val name: String, val text: String)

/**
 * What somebody just said, in the bottom-leading corner above the controls,
 * for as long as the model keeps it there (Said.tsx; SaidView.swift): one
 * place, always the same, so a message is seen rather than hunted for. At
 * most three, the newest last, as a notification stack shows them; the panel
 * holds the rest.
 *
 * Always composed, drawing nothing when there is nothing to say, so that
 * each card leaves with its own animation, the last one included. Where it
 * is drawn, in the window, goes to `onBounds` (null while empty), for the
 * notices to rise above it where the two would meet ([ToastStack]).
 */
@Composable
fun SaidColumn(lines: List<SaidLine>, modifier: Modifier = Modifier, onBounds: (Rect?) -> Unit = {}) {
    val shown = lines.takeLast(3)
    val speakers = shown.map { it.identity }
    val bounds by rememberUpdatedState(onBounds)
    val density = LocalDensity.current
    DisposableEffect(Unit) { onDispose { bounds(null) } }
    Column(
        modifier
            .widthIn(max = 290.dp)
            .onGloballyPositioned { if (shown.isEmpty()) bounds(null) else bounds(it.boundsInWindow()) },
        verticalArrangement = Arrangement.spacedBy(6.dp),
        horizontalAlignment = Alignment.Start,
    ) {
        for (entry in keepDeparting(shown) { it.id }) {
            val line = entry.item
            val index = entry.place
            // Said.tsx: in with `arrive` (up eight points from 94 per cent),
            // out with `depart` (down four to 97 per cent).
            // The fade is glassFade's rather than fadeIn's, which would cut
            // the card's shadow off at its edge while it lasted.
            AnimatedVisibility(
                entry.visible,
                enter = scaleIn(Theme.settle(), 0.94f, TransformOrigin(0f, 1f)) +
                    slideInVertically(Theme.settle()) { with(density) { 8.dp.roundToPx() } } +
                    expandVertically(Theme.settle(), Alignment.Bottom, clip = false),
                exit = scaleOut(Theme.leave(), 0.97f, TransformOrigin(0f, 0.5f)) +
                    slideOutVertically(Theme.leave()) { with(density) { 4.dp.roundToPx() } } +
                    shrinkVertically(Theme.leave(), Alignment.Bottom, clip = false),
            ) {
                val fade by glassFade()
                SaidCard(line, Modifier.glassLayer({ fade }),
                    run = index > 0 && index < speakers.size && speakers[index - 1] == speakers[index])
            }
        }
    }
}

/**
 * One message: a face, a name and the words, the first two left out for a
 * run from the same person, the same rule the panel uses so that both read
 * as one conversation (SaidView.swift `SaidCard`).
 */
@Composable
fun SaidCard(line: SaidLine, modifier: Modifier = Modifier, run: Boolean = false) {
    Row(
        modifier
            .glassPane(16.dp)
            .semantics(mergeDescendants = true) {}
            .padding(start = 8.dp, end = 12.dp, top = if (run) 7.dp else 8.dp, bottom = if (run) 7.dp else 8.dp),
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Monogram(line.name, line.identity, Modifier.alpha(if (run) 0f else 1f), size = 24.dp)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            if (!run) KitText(line.name, font = Font(11f, Font.Semibold), color = Theme.muted, lines = 1)
            // Links work here as in the panel: the commonest thing typed
            // during a call is an address, and it is wanted now.
            LinkedText(line.text, size = 13f, color = Theme.foreground, lines = 5)
        }
    }
}
