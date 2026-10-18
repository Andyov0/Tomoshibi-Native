package app.tomoshibi.ui.settings

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.kit.AccountAvatar
import app.tomoshibi.ui.kit.Chip
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassField
import app.tomoshibi.ui.kit.KitEnabled
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.WrappingRow
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.Account
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

// The Account page (Settings.swift `AccountSettings`; Account.tsx): who the
// person is here and their picture, what they may use, their passphrase,
// and the guest links into their scope's rooms.

/**
 * Where the signed-in account's picture is, resolved against the deployment:
 * the server says `/api/avatar/<trip>`, a path on itself (Kit+Settings.swift
 * `avatarAddress`). Carries a counter that a new picture moves on, so the
 * image already held under that address is not shown again (Account.tsx
 * appends the time for the same reason).
 */
val AppModel.avatarAddress: String?
    get() {
        val avatar = account?.avatar?.takeIf { it.isNotEmpty() } ?: return null
        if (avatar.startsWith("http://") || avatar.startsWith("https://")) return avatar
        val base = deployment?.baseUrl() ?: return null
        val root = base.trimEnd('/')
        val path = if (avatar.startsWith("/")) avatar else "/$avatar"
        return root + path + if (pictureVersion > 0) "?$pictureVersion" else ""
    }

@Composable
internal fun AccountSettings(app: AppModel) {
    var said by remember { mutableStateOf<String?>(null) }
    var kept by remember { mutableStateOf("") }
    said?.let { kept = it }
    Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
        // Signing out takes the page's cards away, and they leave rather
        // than vanish between two frames.
        AnimatedContent(
            app.account,
            transitionSpec = { Theme.arrive.togetherWith(Theme.depart) },
            contentKey = { it?.trip },
            label = "account",
        ) { account ->
            if (account != null) {
                Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
                    Profile(app, account) { said = it }
                    Abilities(app, account)
                    Passphrase(app) { said = it }
                    if (app.holds("invite") && (account.admin || account.scopes.isNotEmpty())) {
                        GuestLinksCard(app, account)
                    }
                }
            } else {
                KitText(t("Sign in to change your passphrase or picture."), font = Font(12.5f), color = Theme.muted)
            }
        }
        AnimatedVisibility(said != null, enter = Theme.arrive, exit = Theme.depart) {
            KitText(
                kept,
                Modifier.clickable(interactionSource = null, indication = null) { said = null }.testTag("settings.account.said"),
                font = Font(12.5f),
                color = Theme.danger,
            )
        }
    }
}

// MARK: - Who

@Composable
private fun Profile(app: AppModel, account: Account, say: (String?) -> Unit) {
    var picturing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val platform = platform()
    // The system's photo picker, which needs no permission and shows only
    // the pictures, as the iPhone's PhotosPicker does.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            picturing = true
            val picture = platform.picture(uri)
            if (picture == null) {
                say(t("That picture could not be used."))
            } else {
                say(app.setPicture(picture))
            }
            picturing = false
        }
    }
    val choose = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
    val shape = rounded(20f)
    KitEnabled(!picturing) {
        Column(
            Modifier
                .fillMaxWidth()
                .background(Color.White.copy(alpha = 0.05f), shape)
                .border(1.dp, Color.White.copy(alpha = 0.07f), shape),
        ) {
            Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                Box {
                    AccountAvatar(
                        account.name,
                        app.avatarAddress,
                        Modifier.dropShadow(CircleShape, Shadow(radius = 10.dp, color = Color.Black, offset = DpOffset(0.dp, 4.dp), alpha = 0.35f)),
                        size = 72.dp,
                    )
                    GlassButton(
                        choose,
                        Modifier.align(Alignment.BottomEnd).offset(x = 4.dp, y = 4.dp).testTag("settings.picture.badge"),
                        shape = GlassButtonShape.Circle(28.dp),
                        prominent = true,
                        label = t("Choose a picture"),
                    ) { SwappingSymbol(if (picturing) "hourglass" else "camera.fill", size = 11f, weight = Font.Bold) }
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    KitText(account.name, font = Font(21f, Font.Bold), lines = 1)
                    // The signature: the public half of the account, and
                    // what proves the name in a room; this is the one page
                    // where somebody has a reason to read their own.
                    SelectionContainer { KitText(account.trip, font = Font(12f, mono = true), color = Theme.muted) }
                }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.06f)))
            Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                KitText(t("Shown beside your name in a call. Scaled down before it is sent."), font = Font(11.5f), color = Theme.muted)
                // Wrapping rather than in one row, which on a phone held
                // upright was wider than the screen.
                WrappingRow(spacing = 8.dp) {
                    GlassButton(choose, Modifier.testTag("settings.picture.choose"), shape = GlassButtonShape.Capsule(32.dp)) {
                        ProfileButtonLabel(t("Choose a picture"), "photo")
                    }
                    // Opening its room as it comes, so that the button after it
                    // moves over rather than jumping.
                    AnimatedVisibility(
                        account.avatar != null,
                        enter = Theme.arrive + expandHorizontally(Theme.settle()),
                        exit = Theme.depart + shrinkHorizontally(Theme.leave()),
                    ) {
                        GlassButton(
                            {
                                scope.launch {
                                    picturing = true
                                    say(app.removePicture())
                                    picturing = false
                                }
                            },
                            shape = GlassButtonShape.Capsule(32.dp),
                        ) { ProfileButtonLabel(t("Remove"), "trash") }
                    }
                    GlassButton({ scope.launch { app.signOut() } }, Modifier.testTag("settings.signout"), shape = GlassButtonShape.Capsule(32.dp)) {
                        ProfileButtonLabel(t("Sign out"), "rectangle.portrait.and.arrow.right")
                    }
                }
            }
        }
    }
}

/**
 * A symbol and words, measured whole (Settings.swift `ProfileButtonLabel`):
 * the system's label, drawn in the glass capsule, was measured short and cut
 * "Remove" to "Remo…".
 */
@Composable
private fun ProfileButtonLabel(title: String, symbol: String) {
    Symbol(symbol, size = 12.5f, weight = Font.Semibold)
    KitText(title, font = Font(12.5f, Font.Semibold), lines = 1)
}

// MARK: - What they may use

@Composable
private fun Abilities(app: AppModel, account: Account) {
    val permissions = listOf("watch", "emby", "music", "arrange", "invite", "open")
    val held = permissions.filter { app.holds(it) }
    // Emby is left off what was withheld: it is given one at a time and
    // spends the deployment's money, and a list of it to everybody without it
    // would be an advertisement for asking (Account.tsx `Abilities`).
    val withheld = permissions.filter { it != "emby" && !app.holds(it) }
    val created = account.created?.let(AppModel::instant)
    Section(t("What you can use"), footer = t("Set by whoever runs this deployment.")) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (held.isNotEmpty()) {
                WrappingRow(spacing = 6.dp) {
                    for (one in held) Chip(permissionName(one), symbol = permissionSymbol(one), lit = one == "emby")
                }
            }
            if (withheld.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    KitText(t("Not opened to this account"), font = Font(11f), color = Theme.muted)
                    WrappingRow(spacing = 6.dp) {
                        for (one in withheld) Chip(permissionName(one), symbol = permissionSymbol(one), withheld = true)
                    }
                }
            }
            if (account.scopes.isNotEmpty() || created != null) {
                Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.06f)))
                val rows = buildList<Pair<String, @Composable () -> Unit>> {
                    if (account.scopes.isNotEmpty()) {
                        add(t("Your scopes") to {
                            WrappingRow(spacing = 5.dp) { for (scope in account.scopes) Chip("@$scope", mono = true) }
                        })
                    }
                    if (created != null) {
                        add(t("Account created") to {
                            val day = DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG).withLocale(readingLocale())
                                .format(created.atZone(ZoneId.systemDefault()))
                            KitText(day, font = Font(12.5f))
                        })
                    }
                }
                LabelledRows(rows)
            }
        }
    }
}

/**
 * Two columns, the words on the left as wide as the widest of them, each
 * row on its first line of text (the Swift's `Grid(alignment:
 * .leadingFirstTextBaseline)`).
 */
@Composable
private fun LabelledRows(rows: List<Pair<String, @Composable () -> Unit>>) {
    Layout(
        content = {
            for ((label, value) in rows) {
                KitText(label, font = Font(12f), color = Theme.muted, lines = 1)
                Box { value() }
            }
        },
    ) { measurables, constraints ->
        val gap = 14.dp.roundToPx()
        val spacing = 9.dp.roundToPx()
        val labels = measurables.filterIndexed { index, _ -> index % 2 == 0 }.map { it.measure(Constraints()) }
        val column = labels.maxOfOrNull { it.width } ?: 0
        val values = measurables.filterIndexed { index, _ -> index % 2 == 1 }
            .map { it.measure(Constraints(maxWidth = (constraints.maxWidth - column - gap).coerceAtLeast(0))) }
        // Each row on its first line of text: the label's baseline on the
        // value's where the value has one; chips have none of their own, and
        // the label sits on the middle of their first line instead.
        val chipLine = 23.dp.roundToPx()
        var y = 0
        val placed = labels.indices.map { index ->
            val label = labels[index]
            val value = values[index]
            val labelLine = label[FirstBaseline]
            val valueLine = value[FirstBaseline]
            val (labelTop, valueTop) = if (valueLine != AlignmentLine.Unspecified && labelLine != AlignmentLine.Unspecified) {
                maxOf(0, valueLine - labelLine) to maxOf(0, labelLine - valueLine)
            } else {
                maxOf(0, (minOf(value.height, chipLine) - label.height) / 2) to 0
            }
            val top = y
            y += maxOf(labelTop + label.height, valueTop + value.height) + spacing
            Triple(top, labelTop, valueTop)
        }
        layout(constraints.maxWidth, (y - spacing).coerceAtLeast(0)) {
            labels.indices.forEach { index ->
                val (top, labelTop, valueTop) = placed[index]
                labels[index].place(0, top + labelTop)
                values[index].place(column + gap, top + valueTop)
            }
        }
    }
}

/**
 * manage/grants.ts `PERMISSION_NAMES`: the words the management pages tick,
 * so "an administrator gave me Music" and this say the same word.
 */
internal fun permissionName(one: String): String = when (one) {
    "watch" -> t("Watch together")
    "emby" -> t("Emby library")
    "music" -> t("Music")
    "arrange" -> t("Arrange meetings")
    "invite" -> t("Guest links")
    else -> t("Open new rooms")
}

/** The symbol of the button each opens in a call (Account.tsx `PERMISSION_ICONS`). */
internal fun permissionSymbol(one: String): String = when (one) {
    "watch" -> "play.rectangle"
    "emby" -> "film"
    "music" -> "music.note"
    "arrange" -> "calendar.badge.clock"
    "invite" -> "link"
    else -> "door.left.hand.open"
}

// MARK: - The passphrase

/**
 * Account.tsx `Passphrase`. The new one is held to eight UTF-16 units, as the
 * web counts `next.length`, which is what a Kotlin string's length counts too.
 * Neither field is kept in the saved state of the screen: a passphrase
 * written into a bundle would outlive the page on the device's storage.
 */
@Composable
private fun Passphrase(app: AppModel, say: (String?) -> Unit) {
    var current by remember { mutableStateOf("") }
    var next by remember { mutableStateOf("") }
    var changing by remember { mutableStateOf(false) }
    var changed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val ready = passphraseReady(current, next, changing)
    val change = {
        if (passphraseReady(current, next, changing)) {
            changing = true
            changed = false
            scope.launch {
                val refused = app.changePassphrase(current, next)
                changing = false
                if (refused != null) {
                    say(refused)
                } else {
                    current = ""
                    next = ""
                    say(null)
                    changed = true
                }
            }
        }
    }
    Section(
        t("Passphrase"),
        footer = t("Your passphrase is not stored here and cannot be sent to you. If you lose it, an administrator sets a new one."),
    ) {
        Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            KitText(t("This is how you are recognised, in a call and here. At least eight characters."), font = Font(11.5f), color = Theme.muted)
            GlassField(
                t("Current passphrase"), current, { current = it }, Modifier.testTag("settings.passphrase.current"),
                symbol = "key", lights = true, secret = true, height = 40.dp,
            )
            GlassField(
                t("New passphrase"), next, { next = it }, Modifier.testTag("settings.passphrase.next"),
                symbol = "key.fill", lights = true, secret = true, height = 40.dp, onSubmit = change,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                GlassButton(
                    change,
                    Modifier.testTag("settings.passphrase.change"),
                    shape = GlassButtonShape.Capsule(32.dp),
                    prominent = true,
                    enabled = ready,
                ) {
                    Crossfade(changing, animationSpec = Theme.settle(), label = "changing") { busy ->
                        if (busy) MediaSpinner(size = 14f, tint = Theme.tallyForeground) else KitText(t("Change it"), lines = 1)
                    }
                }
                AnimatedVisibility(changed, enter = Theme.arrive, exit = Theme.depart) {
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Symbol("checkmark.circle.fill", size = 12f, tint = Theme.good)
                        KitText(t("Changed."), font = Font(12f), color = Theme.good, lines = 1)
                    }
                }
            }
        }
    }
}

/** Whether the passphrase can be changed with what is typed: something now, and eight or more next. */
internal fun passphraseReady(current: String, next: String, changing: Boolean): Boolean =
    !changing && current.isNotEmpty() && next.length >= 8
