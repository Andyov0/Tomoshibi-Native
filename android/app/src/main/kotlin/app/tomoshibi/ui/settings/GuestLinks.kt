package app.tomoshibi.ui.settings

import android.content.ClipData
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import app.tomoshibi.ui.LocalAppScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.GuestLinkRefusal
import app.tomoshibi.model.LobbyModel
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.CopyLinkButton
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassChoice
import app.tomoshibi.ui.kit.GlassChoiceOption
import app.tomoshibi.ui.kit.GlassField
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LocalEnabled
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MomentChoice
import app.tomoshibi.ui.kit.Motion
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.WrappingRow
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.Account
import uniffi.tomoshibi_protocol.MadeInvite
import java.time.LocalDateTime
import java.time.ZoneId

/**
 * Account.tsx `GuestLinks` (Settings.swift `GuestLinksCard`): a link that
 * lets somebody outside a scope into one of its rooms, for a name, between
 * two moments either of which may be left open; and the way to stop every
 * link to a room, which outlives the meeting it was made for.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun GuestLinksCard(app: AppModel, account: Account) {
    val platform = platform()
    // Kept with the settings rather than the card (SettingsState), so that
    // a link half made is still half made after the phone is folded.
    var typed by rememberDraft("guest.typed") { "" }
    var scope by rememberDraft("guest.scope") { account.scopes.firstOrNull() ?: "" }
    var name by rememberDraft("guest.name") { "" }
    var from by rememberDraft<LocalDateTime?>("guest.from") { null }
    var until by rememberDraft<LocalDateTime?>("guest.until") { null }
    var busy by rememberDraft<String?>("guest.busy") { null }
    var made by rememberDraft<MadeInvite?>("guest.made") { null }
    var shownLink by rememberDraft<MadeInvite?>("guest.shownLink") { null }
    var said by rememberDraft<String?>("guest.said") { null }
    var shownSaid by rememberDraft("guest.shownSaid") { "" }
    var saidIsGood by rememberDraft("guest.saidIsGood") { false }
    made?.let { shownLink = it }
    said?.let { shownSaid = it }
    // The app's scope: a link asked for just before the card is drawn again
    // in the other layout must still be answered, and `busy` cleared.
    val coroutines = LocalAppScope.current
    val clipboard = LocalClipboard.current

    // The room as the server will read it. An @ typed into the room half is
    // folded away rather than allowed to start a second scope: the scope is
    // chosen beside it.
    val full = platform.normaliseRoomName(typed.replace("@", "-")) + "@" + scope
    val usable = platform.validRoomName(full) && platform.validScope(scope)
    val open = busy == null && usable

    fun instant(moment: LocalDateTime?) = moment?.atZone(ZoneId.systemDefault())?.toInstant()

    val make = {
        if (busy == null && usable) {
            busy = "make"
            said = null
            coroutines.launch {
                app.makeGuestLink(full, name, instant(from), instant(until)).fold(
                    { link ->
                        made = link
                        // Copied as well as shown, as the panel in a call
                        // does: whoever made it is about to paste it somewhere.
                        clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(t("Copy link"), link.link)))
                    },
                    { refused ->
                        saidIsGood = false
                        said = (refused as? GuestLinkRefusal)?.sentence ?: t("That link could not be made. Try again.")
                    },
                )
                busy = null
            }
        }
    }
    val stop = {
        if (busy == null && usable) {
            busy = "stop"
            said = null
            coroutines.launch {
                app.stopGuestLinks(full).fold(
                    { count ->
                        made = null
                        saidIsGood = true
                        said = if (count == 1u) t("One link stopped working.")
                        else t("{count} links stopped working.", mapOf("count" to count.toString()))
                    },
                    { refused ->
                        saidIsGood = false
                        said = (refused as? GuestLinkRefusal)?.sentence ?: t("That link could not be made. Try again.")
                    },
                )
                busy = null
            }
        }
    }

    Section(
        t("Guest links"),
        footer = t("A link lets somebody outside your scope into one of its rooms as a guest. Leave the end empty for a link that works until it is stopped."),
    ) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                GlassField(
                    t("Room name"), typed, { typed = it }, Modifier.weight(1f).testTag("settings.guest.room"),
                    symbol = "number", mono = typed.isNotEmpty(), height = 40.dp,
                )
                KitText("@", font = Font(15f, mono = true), color = Theme.muted)
                ScopeControl(account, scope) { scope = it }
            }
            GlassField(t("Their name, if the link is for one person"), name, { name = it }, symbol = "person", height = 40.dp)
            // Side by side where both fit, one above the other on a phone,
            // where a chosen moment is wider than half the card.
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Moment(t("Works from"), "guest.from", from) { from = it }
                Moment(t("Works until"), "guest.until", until) { until = it }
            }
            WrappingRow(spacing = 8.dp) {
                GlassButton(make, Modifier.testTag("settings.guest.make"), shape = GlassButtonShape.Capsule(34.dp), prominent = true, enabled = open) {
                    Crossfade(busy == "make", animationSpec = Theme.settle(), label = "making") { making ->
                        if (making) MediaSpinner(size = 14f, tint = Theme.tallyForeground) else Symbol("link")
                    }
                    KitText(t("Make a link"), lines = 1)
                }
                StopButton(busy == "stop", open, stop)
            }
            Arriving(made != null) {
                shownLink?.let { link -> MadeLink(link) }
            }
            Arriving(said != null) {
                KitText(shownSaid, font = Font(12f), color = if (saidIsGood) Theme.good else Theme.danger)
            }
        }
    }
}

/**
 * Typed by an administrator, who belongs to every scope; chosen by anybody
 * else from the ones on their account, and simply named where there is only
 * one.
 */
@Composable
private fun ScopeControl(account: Account, scope: String, choose: (String) -> Unit) {
    when {
        account.admin -> GlassField(
            t("Scope"), scope, { choose(it.lowercase().trim()) }, Modifier.width(140.dp),
            mono = true, height = 40.dp,
        )
        account.scopes.size > 1 -> GlassChoice("guest.scope", account.scopes.map { GlassChoiceOption(it, it) }, scope, choose, height = 34.dp)
        else -> KitText(scope, font = Font(14f, mono = true), color = Theme.foreground, lines = 1)
    }
}

@Composable
private fun Moment(label: String, id: String, value: LocalDateTime?, change: (LocalDateTime?) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        KitText(label, font = Font(11f), color = Theme.muted, lines = 1)
        MomentChoice(id, value, change)
    }
}

/**
 * A quieter button than the one that makes, and allowed a second line: it
 * is a long sentence, and on a phone the capsule cut it off halfway through.
 */
@Composable
private fun StopButton(busy: Boolean, enabled: Boolean, stop: () -> Unit) {
    val on = enabled && LocalEnabled.current
    val press = rememberPress()
    val fill by animateColorAsState(Theme.wash.copy(alpha = if (press.lit && on) 0.10f else 0.06f), Motion.hover(), label = "fill")
    val alpha by animateFloatAsState(if (on) 1f else 0.45f, Theme.settle(), label = "enabled")
    Row(
        Modifier
            .alpha(alpha)
            .heightIn(min = 34.dp)
            .background(fill, Capsule)
            .border(1.dp, Theme.wash.copy(alpha = 0.10f), Capsule)
            .pressable(press, Capsule, on, Role.Button, onClick = stop)
            .padding(horizontal = 14.dp, vertical = 8.dp)
            .testTag("settings.guest.stop"),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val ink = Theme.foreground.copy(alpha = 0.85f)
        Crossfade(busy, animationSpec = Theme.settle(), label = "stopping") { stopping ->
            if (stopping) MediaSpinner(size = 13f, tint = ink) else Symbol("xmark.circle", size = 13f, weight = Font.Semibold, tint = ink)
        }
        KitText(t("Stop every link to this room working"), font = Font(13f, Font.Semibold), color = ink)
    }
}

@Composable
private fun MadeLink(made: MadeInvite) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Top) {
            SelectionContainer(
                Modifier
                    .weight(1f)
                    .background(Theme.recess(0.25f), rounded(11f))
                    .padding(10.dp),
            ) {
                KitText(made.link, Modifier.fillMaxWidth(), font = Font(11.5f, mono = true), color = Theme.foreground.copy(alpha = 0.85f))
            }
            CopyLinkButton(made.link)
        }
        KitText(describe(made), font = Font(11.5f), color = Theme.muted)
    }
}

/** When the link works, and for whom, in a sentence (Account.tsx `describe`). */
internal fun describe(made: MadeInvite): String {
    fun at(text: String) = AppModel.instant(text)?.let { LobbyModel.said(it) } ?: text
    val from = made.from
    val until = made.until
    var line = if (until != null) {
        if (from != null) t("This link works from {from} until {until}.", mapOf("from" to at(from), "until" to at(until)))
        else t("This link works until {until}.", mapOf("until" to at(until)))
    } else {
        if (from != null) t("This link works from {from} until somebody stops it.", mapOf("from" to at(from)))
        else t("This link works until somebody stops it.")
    }
    val name = made.name
    if (!name.isNullOrEmpty()) line += " " + t("Whoever uses it joins as {name}.", mapOf("name" to name))
    return line
}
