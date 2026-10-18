package app.tomoshibi.ui.room

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.host.HostFeature
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.ProtocolException

/**
 * The room's name, at the head of the call (RoomTitle.swift).
 *
 * It carried a connection light as well, beside the signal badge that says
 * the same thing better: two lights for one connection, which could disagree
 * for the seconds between the server's grade and the first measurement. The
 * badge is the web's (Signal.tsx), so the name stands alone.
 */
@Composable
internal fun RoomTitle(call: CallModel, modifier: Modifier = Modifier) {
    Box(
        modifier
            .height(30.dp)
            .glassSurface(Capsule)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) { KitText(call.room, font = Font(13f, Font.Semibold), lines = 1, overflow = TextOverflow.MiddleEllipsis) }
}

/**
 * Alone in the room: the link that brings somebody else in, or, for somebody
 * who does not run the room, who can (components/room/EmptyRoom.tsx;
 * `EmptyRoomCard`).
 *
 * The address is only a way in where anybody may use a room that exists
 * ([linkWorks]). Where the door asks for an invitation, copying it and
 * sending it invites somebody to be turned away, so there it mints an
 * invitation instead, and only for whoever runs the room, since the server
 * refuses anybody else; somebody who does not is told who can.
 *
 * An invitation that fails says why in the corner and leaves the button
 * there to press again; only the server's "not yours" says the standing is
 * out of date, and then the standing is asked again, which is what turns the
 * button into the sentence. Any failure at all used to turn it into the
 * sentence for the rest of the call, a dropped connection included.
 *
 * A small capsule above the controls that can be put away, as the web's is a
 * single quiet button. It used to be a pane of glass with a heading laid
 * over the middle of your own picture, with no way to close it, and the
 * owner found it jarring (2026-10-16).
 */
@Composable
internal fun EmptyRoomCard(
    call: CallModel,
    linkWorks: Boolean,
    host: Boolean,
    link: String,
    recheck: suspend () -> Unit,
    dismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    var copied by remember { mutableStateOf(false) }
    var copies by remember { mutableIntStateOf(0) }
    var minting by remember { mutableStateOf(false) }
    val notHost = !linkWorks && !host
    LaunchedEffect(copies) {
        if (copies == 0) return@LaunchedEffect
        copied = true
        delay(1600)
        copied = false
    }
    fun copy() {
        Pasteboard.copy(context, link)
        copies++
    }
    fun invite() {
        if (minting) return
        minting = true
        call.scope.launch {
            try {
                val made = call.deployment.createInvite(call.room, call.call.token())
                Pasteboard.copy(context, call.deployment.inviteLink(made.token, call.room))
                minting = false
                copies++
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                minting = false
                call.actionFailed(HostFeature.explain(fault))
                if ((fault as? ProtocolException.Refused)?.code == "not_yours") recheck()
            }
        }
    }
    Row(
        modifier
            .widthIn(max = 420.dp)
            .glassSurface(Capsule),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AnimatedContent(
            notHost,
            Modifier.weight(1f, fill = false),
            transitionSpec = { Theme.arrive togetherWith Theme.depart },
            label = "invite",
        ) { told ->
            if (told) {
                KitText(
                    t("Only whoever runs this room can let somebody else in."),
                    Modifier.padding(start = 16.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
                    font = Font(12.5f), color = Theme.muted, lines = 2,
                )
            } else {
                val press = rememberPress()
                val ink by animateColorAsState(if (copied) Theme.tally else Theme.foreground, Theme.settle(), label = "copied")
                Row(
                    Modifier
                        .height(34.dp)
                        .pressable(press, Capsule, !minting, Role.Button) { if (linkWorks) copy() else invite() }
                        .padding(start = 14.dp, end = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.width(14.dp), contentAlignment = Alignment.Center) {
                        AnimatedContent(
                            when {
                                minting -> "spinner"
                                copied -> "checkmark"
                                linkWorks -> "doc.on.doc"
                                else -> "link"
                            },
                            transitionSpec = {
                                (scaleIn(Theme.settle()) + fadeIn(Theme.settle())) togetherWith (scaleOut(Theme.leave()) + fadeOut(Theme.leave()))
                            },
                            label = "symbol",
                        ) { which ->
                            if (which == "spinner") MediaSpinner(size = 12f) else Symbol(which, size = 13f, weight = Font.Medium, tint = ink)
                        }
                    }
                    AnimatedContent(
                        if (copied) t("Link copied") else if (linkWorks) t("Copy link") else t("Invite somebody"),
                        transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                        label = "words",
                    ) { words -> KitText(words, font = Font(13f, Font.Medium), lines = 1) }
                }
            }
        }
        Box(Modifier.size(1.dp, 16.dp).background(Color.White.copy(alpha = 0.12f)))
        val press = rememberPress()
        Box(
            Modifier
                .size(34.dp)
                .pressable(press, Capsule, true, Role.Button, t("Close"), onClick = dismiss),
            contentAlignment = Alignment.Center,
        ) { Symbol("xmark", size = 10.5f, weight = Font.Bold, tint = Theme.muted) }
    }
}

/** A line across the top that says something is happening or went wrong (RoomView.swift `Notice`). */
@Composable
internal fun Notice(text: String, modifier: Modifier = Modifier, danger: Boolean = false) {
    KitText(
        text,
        modifier
            .glassSurface(Capsule, tint = if (danger) Theme.danger else null)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        font = Font(13f, Font.Medium),
        color = if (danger) Theme.dangerForeground else Theme.foreground,
    )
}
