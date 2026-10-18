package app.tomoshibi.ui.call

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import app.tomoshibi.model.Person
import app.tomoshibi.model.host.HostFeature
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRole
import app.tomoshibi.ui.kit.Monogram
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.PanelHeading
import app.tomoshibi.ui.kit.PanelRule
import app.tomoshibi.ui.kit.PillButton
import app.tomoshibi.ui.kit.PillRole
import app.tomoshibi.ui.kit.RoundButton
import app.tomoshibi.ui.kit.RoundRole
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.panelCard
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.room.Pasteboard
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.MediaSource

/**
 * The small console for whoever is running the meeting
 * (components/room/HostPanel.tsx; HostPanel.swift `HostPanelView`).
 *
 * A link somebody can be sent, who is at the door, where the meeting is held,
 * and the people in it, each of whom can be quietened, made host or removed;
 * then ending it for everybody, asked twice, in the panel itself rather than
 * in a dialog over the call. Deliberately less than the management pages can
 * do: this reaches nobody outside this room.
 *
 * The room can change hands while the panel is open, by a handover from here
 * or by the server after the host left and came back, and then the panel says
 * that somebody else runs the room rather than keeping controls that would
 * all answer "not yours".
 */
@Composable
fun HostPanel(feature: HostFeature, close: () -> Unit, modifier: Modifier = Modifier) {
    DisposableEffect(feature) {
        feature.panelOpen = true
        onDispose { feature.panelOpen = false }
    }
    LaunchedEffect(feature) { feature.loadRelays() }
    Column(modifier.fillMaxWidth().glassPane(22.dp)) {
        PanelHeading(t("Running this room"), t("Close"), close, symbol = "crown.fill", symbolTint = Theme.tally)
        PanelRule()
        AnimatedContent(
            feature.standing.yours,
            transitionSpec = { Theme.arrive togetherWith Theme.depart },
            label = "standing",
        ) { yours ->
            if (yours) Running(feature) else NotYours()
        }
    }
}

@Composable
private fun NotYours() {
    Row(
        Modifier.fillMaxWidth().padding(12.dp).padding(vertical = 18.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Symbol("crown", size = 13f, tint = Theme.muted)
        KitText(t("Somebody else runs this room."), font = Font(12.5f), color = Theme.muted)
    }
}

@Composable
private fun Running(feature: HostFeature) {
    Column(Modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Inviting(feature)
        Section(feature.waiting.isNotEmpty()) { Door(feature) }
        Section(feature.usableRelays.size >= 2) { Moving(feature) }
        People(feature)
        Ending(feature)
        KitText(
            if (feature.standing.admin) {
                t("You can do this because you administer this server. Removing somebody ends their call; it does not stop them rejoining.")
            } else {
                t("Removing somebody ends their call. It does not stop them rejoining with the same name.")
            },
            Modifier.padding(horizontal = 6.dp),
            font = Font(11f), color = Theme.muted.copy(alpha = Theme.muted.alpha * 0.85f),
        )
    }
}

/** A part of the panel that comes and goes with what it is about, the panel growing and shrinking round it. */
@Composable
private fun Section(visible: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(visible, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
        content()
    }
}

// A link, and the door

@Composable
private fun Inviting(feature: HostFeature) {
    val context = LocalContext.current
    Column(Modifier.fillMaxWidth().panelCard().padding(4.dp)) {
        MenuItem(
            if (feature.copied) t("Link copied") else t("Invite somebody"),
            { feature.call.scope.launch { feature.invite { Pasteboard.copy(context, it) } } },
            symbol = if (feature.copied) "checkmark" else "link",
            role = MenuRole.Prominent,
            busy = feature.busy == "invite",
            enabled = feature.busy != "invite",
        )
        Section(feature.link != null) {
            val link = feature.link ?: ""
            Column(Modifier.padding(start = 8.dp, end = 8.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SelectionContainer(
                    Modifier
                        .fillMaxWidth()
                        .background(Color.Black.copy(alpha = 0.25f), rounded(10f))
                        .border(1.dp, Color.White.copy(alpha = 0.07f), rounded(10f))
                        .padding(horizontal = 10.dp, vertical = 8.dp),
                ) {
                    KitText(link, font = Font(10.5f, mono = true), color = Theme.foreground.copy(alpha = 0.8f))
                }
                KitText(
                    t("Anybody with this link can join, until you end the meeting. No passphrase needed.") + " " +
                        t("It stops working after a day either way."),
                    font = Font(11f), color = Theme.muted,
                )
                // A link goes out and cannot be taken back; this stops it
                // without ending the meeting. Under a scope it also stops the
                // standing links members made, so it says every link.
                QuietButton(
                    if (feature.scoped) t("Stop every link to this room working") else t("Stop this link working"),
                    busy = feature.busy == "revoke",
                    enabled = feature.busy != "revoke",
                ) { feature.call.scope.launch { feature.revoke() } }
            }
        }
    }
}

/** Who is outside, under the control for letting people in: the two are the same thought (HostPanel.tsx). */
@Composable
private fun Door(feature: HostFeature) {
    Column(Modifier.fillMaxWidth().panelCard(tint = Theme.tally), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        MenuHeading(t("Waiting to be let in"))
        DoorList(feature, Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp))
    }
}

// Where it is held

/**
 * The move under way takes the chevron's place with the app's spinner, each
 * fading into the other (HostPanel.tsx `Elsewhere`, whose button swaps its
 * icon for the spinner the same way and is not pressable meanwhile).
 */
@Composable
private fun Moving(feature: HostFeature) {
    val busy = feature.busy == "move"
    val turn by animateFloatAsState(if (feature.choosingRelay) 180f else 0f, Theme.settle(), label = "chevron")
    Column(Modifier.fillMaxWidth().panelCard().padding(4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Box {
            MenuItem(t("Hold this call somewhere else"), { feature.choosingRelay = !feature.choosingRelay },
                symbol = "server.rack", enabled = !busy)
            AnimatedContent(
                busy,
                Modifier.align(Alignment.CenterEnd).padding(end = 12.dp),
                transitionSpec = {
                    (fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f)) togetherWith
                        (fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.6f))
                },
                label = "moving",
            ) { moving ->
                if (moving) {
                    MediaSpinner(size = 14f, tint = Theme.muted)
                } else {
                    Symbol("chevron.down", Modifier.graphicsLayer { rotationZ = turn }, size = 10f, weight = Font.Bold, tint = Theme.muted)
                }
            }
        }
        Section(feature.choosingRelay) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(
                    t("Everybody is reconnected. Nobody has to do anything."),
                    Modifier.padding(start = 10.dp, end = 10.dp, top = 2.dp, bottom = 4.dp),
                    font = Font(11f), color = Theme.muted,
                )
                for (relay in feature.usableRelays) {
                    MenuItem(say(relay.label?.takeIf { it.isNotEmpty() } ?: relay.name), {
                        feature.choosingRelay = false
                        feature.call.scope.launch { feature.move(relay) }
                    }, symbol = "arrow.right")
                }
            }
        }
    }
}

// The people

@Composable
private fun People(feature: HostFeature) {
    val others = feature.call.people
    Column(Modifier.fillMaxWidth().panelCard().padding(4.dp)) {
        Section(others.isEmpty()) {
            KitText(
                t("Nobody else is here."),
                Modifier.fillMaxWidth().padding(vertical = 14.dp),
                font = Font(12f), color = Theme.muted, align = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
        for (entry in keepDeparting(others) { it.id }) {
            AnimatedVisibility(entry.visible, enter = Theme.arrive + expandVertically(Theme.settle()), exit = Theme.depart + shrinkVertically(Theme.leave())) {
                PersonRow(feature, entry.item)
            }
        }
    }
}

/** One person, and what can be done about them (HostPanel.tsx's list). */
@Composable
private fun PersonRow(feature: HostFeature, person: Person) {
    val busy = feature.busy != null
    val name = person.name.ifEmpty { person.id }
    // `isMicrophoneEnabled`: a microphone there and not already muted.
    val microphoneOn = person.track(MediaSource.MICROPHONE)?.let { !it.muted } ?: false
    // The Swift row lights under the pointer; a finger has nothing to rest
    // on a row that is not itself a button, so it is not lit here.
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 8.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Monogram(name, person.id, size = 26.dp)
        KitText(name, Modifier.weight(1f), font = Font(13f, Font.Medium), lines = 1)
        AnimatedVisibility(feature.busy == person.id, enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
            MediaSpinner(size = 14f, tint = Theme.muted)
        }
        RoundButton("mic.slash.fill", t("Mute"), { feature.call.scope.launch { feature.silence(person) } }, enabled = !busy && microphoneOn)
        // A room held under a scope has no host to hand over, and the server
        // refuses it outright; the button goes rather than answering with a
        // refusal.
        if (!feature.scoped) {
            // Faint beside a guest rather than taken away, and pressable, so
            // the reason can be said on a phone, where nobody hovers.
            val guest = feature.isGuest(person)
            RoundButton("crown", t("Make host"), {
                if (guest) feature.refuseHandOver() else feature.call.scope.launch { feature.handOver(person) }
            }, faint = guest, enabled = !busy)
        }
        RoundButton("person.badge.minus", t("Remove from room"), { feature.call.scope.launch { feature.turnOut(person) } },
            role = RoundRole.Destructive, enabled = !busy)
    }
}

// Ending it

@Composable
private fun Ending(feature: HostFeature) {
    AnimatedContent(
        feature.ending,
        transitionSpec = {
            if (targetState) {
                (fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.96f, transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0.5f, 0f))) togetherWith
                    fadeOut(Theme.leave())
            } else {
                fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave())
            }
        },
        label = "ending",
    ) { asked ->
        if (asked) {
            // The question names what will happen rather than asking whether
            // they are sure, which nobody has ever answered no to by reading
            // it (HostPanel.tsx `ending`).
            Column(Modifier.fillMaxWidth().panelCard(tint = Theme.danger).padding(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                        Symbol("door.left.hand.closed", size = 13f, weight = Font.Semibold, tint = Theme.danger)
                    }
                    KitText(
                        if (feature.scoped) {
                            t("Everybody will be disconnected and told the room has closed. Links made in the call stop working; links made on an account page do not.")
                        } else {
                            t("Everybody will be disconnected and told the room has closed. Links to it stop working.")
                        },
                        font = Font(12f),
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PillButton(t("Cancel"), { feature.ending = false }, height = 30.dp)
                    // Red: ending it for everybody is leaving, for everybody.
                    Box(Modifier.weight(1f)) {
                        PillButton(t("Close the room"), { feature.call.scope.launch { feature.dissolve() } },
                            role = PillRole.Destructive, busy = feature.busy == "close", height = 30.dp, wide = true,
                            enabled = feature.busy != "close")
                    }
                }
            }
        } else {
            Box(Modifier.fillMaxWidth().panelCard().padding(4.dp)) {
                MenuItem(t("End this meeting"), { feature.ending = true }, symbol = "door.left.hand.closed")
            }
        }
    }
}

/**
 * A small, quiet press for something that undoes rather than does: stopping
 * the link (`QuietButton`). It warms to red under the finger, where it is
 * about to happen.
 */
@Composable
private fun QuietButton(title: String, busy: Boolean, enabled: Boolean, action: () -> Unit) {
    val press = rememberPress()
    val lit = press.lit
    val ink by animateColorAsState(if (lit) Theme.foreground else Theme.muted, Motion.hover(), label = "ink")
    val fill by animateColorAsState(if (lit) Theme.danger.copy(alpha = 0.14f) else Color.White.copy(alpha = 0.05f), Motion.hover(), label = "fill")
    KitEnabled(enabled) {
        Row(
            Modifier
                .background(fill, Capsule)
                .pressable(press, Capsule, enabled, Role.Button, onClick = action)
                .padding(horizontal = 10.dp, vertical = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AnimatedContent(
                busy,
                transitionSpec = {
                    (fadeIn(Theme.settle()) + scaleIn(Theme.settle(), initialScale = 0.6f)) togetherWith
                        (fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.6f))
                },
                label = "revoking",
            ) { working ->
                if (working) {
                    MediaSpinner(size = 10f, tint = ink)
                } else {
                    // A link struck through; there is no such symbol.
                    Symbol(
                        "link",
                        Modifier.drawWithContent {
                            drawContent()
                            rotate(-45f) {
                                drawLine(ink, Offset(center.x, center.y - 7.dp.toPx()), Offset(center.x, center.y + 7.dp.toPx()), 1.4.dp.toPx())
                            }
                        },
                        size = 10f, weight = Font.Semibold, tint = ink,
                    )
                }
            }
            KitText(title, font = Font(11.5f, Font.Medium), color = ink, lines = 1)
        }
    }
}
