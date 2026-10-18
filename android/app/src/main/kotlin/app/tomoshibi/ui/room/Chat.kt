package app.tomoshibi.ui.room

import android.icu.text.DateTimePatternGenerator
import android.icu.text.SimpleDateFormat
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.ChatRules
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.ChatLine
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LinkedText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.Monogram
import app.tomoshibi.ui.kit.PanelHeading
import app.tomoshibi.ui.kit.PanelRule
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallState
import java.time.Instant
import java.util.Date

/**
 * Everything said this call (components/room/ChatPanel.tsx; Chat.swift
 * `ChatPanel`): kept only for the call, which the panel says while nothing
 * has been said yet.
 *
 * A run of messages from one person names them once, with the time it
 * started; addresses in a message are links; the box takes several lines,
 * up to the web's two thousand characters. While the call is reconnecting
 * nothing can leave, and the box says so rather than taking words it cannot
 * send; a message that did not leave is put back in the box, with a line
 * saying it was not sent.
 */
@Composable
internal fun ChatPanel(call: CallModel, close: () -> Unit, modifier: Modifier = Modifier) {
    GlassShell(modifier.fillMaxWidth(), cornerRadius = 22.dp) {
        Column {
            PanelHeading(t("Messages"), t("Close messages"), close, symbol = "bubble.left.and.bubble.right.fill")
            PanelRule()
            AnimatedContent(
                call.messages.isEmpty(),
                transitionSpec = { Theme.arrive togetherWith Theme.depart },
                label = "said",
            ) { empty -> if (empty) Empty() else Log(call.messages) }
            Composer(call, Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp))
        }
    }
}

/** The tallest the conversation grows before it scrolls: the web's panel stops at twenty-six rems with its heading and its box. */
private val Tallest = 330.dp

@Composable
private fun Log(lines: List<ChatLine>) {
    val state = rememberLazyListState()
    val speakers = lines.map { it.identity }
    // The newest is where a conversation is read from.
    LaunchedEffect(Unit) { if (lines.isNotEmpty()) state.scrollToItem(lines.lastIndex) }
    LaunchedEffect(lines.lastOrNull()?.id) { if (lines.isNotEmpty()) state.animateScrollToItem(lines.lastIndex) }
    LazyColumn(
        Modifier.fillMaxWidth().heightIn(max = Tallest),
        state = state,
        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp, vertical = 12.dp),
    ) {
        itemsIndexed(lines, key = { _, line -> line.id }) { index, line ->
            val run = ChatRules.continues(speakers, index)
            Said(
                line, run,
                Modifier
                    .padding(top = if (index == 0) 0.dp else if (run) 3.dp else 12.dp)
                    .animateItem(fadeInSpec = Theme.settle(), placementSpec = Theme.settle(), fadeOutSpec = Theme.leave()),
            )
        }
    }
}

/** One message, and above the first of a run, who said it and when (`Said`). */
@Composable
private fun Said(line: ChatLine, run: Boolean, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (!run) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Monogram(line.name, line.identity, size = 20.dp)
                // Through the same phrase the tile uses.
                KitText(
                    if (line.mine) t("{name} (you)", mapOf("name" to line.name)) else line.name,
                    Modifier.weight(1f, fill = false),
                    font = Font(12f, Font.Semibold), color = Theme.foreground.copy(alpha = 0.92f), lines = 1,
                )
                KitText(clock(line.at), font = Font(10.5f, digits = true), color = Theme.muted, lines = 1)
            }
        }
        // Somebody's own words, so they are theirs to take away.
        SelectionContainer(Modifier.padding(start = 28.dp)) {
            LinkedText(line.text, size = 13f, color = Theme.foreground)
        }
    }
}

/**
 * Wall-clock time to the minute, in the language in use rather than the
 * system's, with no opinion about the twelve-hour clock: whether a time
 * reads 14:30 or 02:30 PM is the language's habit (ChatPanel.tsx `clock`,
 * `hour: "2-digit", minute: "2-digit"`, which the skeleton asks for).
 */
internal fun clock(at: Instant): String {
    val locale = readingLocale()
    // The hour kept at two figures where the language's clock has room for
    // them, as the web's "2-digit" asks: the best pattern alone shortens it
    // to one in English, 4:30 PM where a browser says 04:30 PM.
    val pattern = DateTimePatternGenerator.getInstance(locale).getBestPattern("jjmm", DateTimePatternGenerator.MATCH_HOUR_FIELD_LENGTH)
    return SimpleDateFormat(pattern, locale).format(Date.from(at))
}

/**
 * Nothing has been said yet (`Empty`). Carries the one fact somebody needs
 * before they type: this is not written down anywhere.
 */
@Composable
private fun Empty() {
    Column(
        Modifier.fillMaxWidth().padding(vertical = 26.dp).semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier
                .size(80.dp)
                .background(Brush.radialGradient(listOf(Theme.tally.copy(alpha = 0.16f), Theme.tally.copy(alpha = 0f))), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Symbol("bubble.left.and.text.bubble.right", size = 26f, weight = Font.Regular, tint = Theme.foreground.copy(alpha = 0.5f))
        }
        KitText(
            t("Messages disappear when the call ends."),
            Modifier.widthIn(max = 200.dp),
            font = Font(12f), color = Theme.muted, align = TextAlign.Center,
        )
    }
}

/**
 * The box: as many lines as a message needs, up to five, then it scrolls
 * (`Composer`). The screen's own keyboard makes a new line and the arrow
 * sends; a hardware keyboard sends on Enter, Shift-Enter making the new line,
 * as on the Mac. It takes the keyboard once the panel has arrived, which it
 * was opened for (ChatPanel.tsx focuses its box on mount).
 */
@Composable
private fun Composer(call: CallModel, modifier: Modifier = Modifier) {
    val offline = call.state != CallState.CONNECTED
    val draft = call.chatDraft
    val sendable = !offline && !call.sending && draft.isNotBlank()
    var focused by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val nearLimit = draft.length > ChatRules.LONGEST - 200
    fun send() {
        if (!sendable) return
        call.scope.launch { call.sendDraft() }
    }
    LaunchedEffect(Unit) {
        // After the panel's own arrival, or the keyboard rises through it.
        delay(350)
        runCatching { focus.requestFocus() }
    }
    val edge by animateColorAsState(
        if (focused && !offline) Theme.tally.copy(alpha = 0.55f) else Color.White.copy(alpha = 0.09f),
        Theme.settle(), label = "edge",
    )
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .alpha(if (offline) 0.6f else 1f)
                .background(Color.Black.copy(alpha = 0.22f), rounded(18f))
                .border(1.dp, edge, rounded(18f))
                .padding(start = 12.dp, end = 5.dp, top = 5.dp, bottom = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            val style = Font(15f).style(Theme.foreground)
            BasicTextField(
                value = draft,
                onValueChange = { call.chatDraft = ChatRules.clamp(it) },
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 4.dp)
                    .focusRequester(focus)
                    .onFocusChanged { focused = it.isFocused }
                    .onPreviewKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown || (event.key != Key.Enter && event.key != Key.NumPadEnter)) {
                            false
                        } else if (event.isShiftPressed) {
                            false
                        } else {
                            send()
                            true
                        }
                    }
                    .semantics { contentDescription = t("Say something") },
                enabled = !offline,
                textStyle = style,
                cursorBrush = SolidColor(Theme.tally),
                minLines = 1,
                maxLines = 5,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                decorationBox = { field ->
                    Box {
                        if (draft.isEmpty()) {
                            KitText(if (offline) t("Reconnecting…") else t("Say something"), font = Font(15f), color = Theme.muted, lines = 1)
                        }
                        field()
                    }
                },
            )
            SendButton(sendable, call.sending, ::send, Modifier.padding(bottom = 1.dp))
        }
        AnimatedVisibility(
            call.chatFailed || nearLimit,
            enter = Theme.arrive + expandVertically(Theme.settle()),
            exit = Theme.depart + shrinkVertically(Theme.leave()),
        ) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                AnimatedVisibility(call.chatFailed, enter = Theme.arrive, exit = Theme.depart) {
                    Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                        Symbol("exclamationmark.circle.fill", size = 11f, tint = Theme.danger)
                        KitText(t("That could not be sent."), font = Font(11f), color = Theme.danger, lines = 1)
                    }
                }
                Spacer(Modifier.weight(1f))
                // How much is left, once there is little: the box otherwise
                // stops taking characters without a word.
                AnimatedVisibility(nearLimit, enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                    KitText(
                        "${draft.length}/${ChatRules.LONGEST}",
                        font = Font(10.5f, digits = true),
                        color = if (draft.length >= ChatRules.LONGEST) Theme.tally else Theme.muted,
                        lines = 1,
                    )
                }
            }
        }
    }
}

@Composable
private fun SendButton(sendable: Boolean, sending: Boolean, send: () -> Unit, modifier: Modifier = Modifier) {
    val press = rememberPress()
    val fill by animateColorAsState(if (sendable || sending) Theme.tally else Color.White.copy(alpha = 0.08f), Theme.settle(), label = "send")
    Box(
        modifier
            .size(28.dp)
            .background(fill, CircleShape)
            .pressable(press, CircleShape, sendable, Role.Button, t("Send"), onClick = send),
        contentAlignment = Alignment.Center,
    ) {
        AnimatedContent(sending, transitionSpec = { fadeIn(tween(160)) togetherWith fadeOut(tween(160)) }, label = "sending") { busy ->
            if (busy) {
                MediaSpinner(size = 14f, tint = Theme.tallyForeground)
            } else {
                Symbol("arrow.up", size = 12.5f, weight = Font.Bold, tint = if (sendable) Theme.tallyForeground else Theme.muted)
            }
        }
    }
}
