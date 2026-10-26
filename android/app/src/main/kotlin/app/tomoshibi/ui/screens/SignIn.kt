package app.tomoshibi.ui.screens

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.FloatingPopover
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LocalPopovers
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRule
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.putsKeyboardAway
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.lobby.Mark
import app.tomoshibi.ui.lobby.OpenFromDebug
import app.tomoshibi.ui.lobby.ProblemToast
import app.tomoshibi.ui.lobby.UpdateBanner
import app.tomoshibi.ui.lobby.pastedLink
import app.tomoshibi.ui.lobby.plainPress
import app.tomoshibi.ui.lobby.rise
import app.tomoshibi.ui.lobby.LobbyNoticeCard
import app.tomoshibi.ui.lobby.noticeShown
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.ui.platform.Clipboard
import app.tomoshibi.ui.Folded

/**
 * Say who you are (Lobby.tsx `SignIn`; `SignInView`), or open the link you
 * were sent: the app is a way in for the people who belong to the
 * deployment and for the people they send links to, and nobody gets into a
 * room by its name while signed out.
 *
 * On a phone the link is pasted with one press at the foot of the screen,
 * which reads the clipboard only when it is pressed; on a wide screen, as on
 * the Mac, there is a field for it under the sign-in.
 */
@Composable
fun SignInView(app: AppModel) {
    val scope = LocalAppScope.current
    val clipboard = LocalClipboard.current
    var name by rememberSaveable { mutableStateOf("") }
    var passphrase by rememberSaveable { mutableStateOf("") }
    var link by rememberSaveable { mutableStateOf("") }
    val popovers = rememberPopovers()
    OpenFromDebug(popovers)

    fun signIn() {
        if (name.isEmpty() || passphrase.isEmpty() || app.busy) return
        scope.launch {
            app.signIn(name, passphrase)
            // A refused attempt is something that happened, not something
            // still true: the field is cleared for the next, as on the web.
            if (app.account == null) passphrase = ""
        }
    }

    fun openLink(text: String) {
        val found = pastedLink(text) ?: return
        scope.launch { app.open(found) }
    }

    FloatingPopoverLayer(popovers) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wide = maxWidth >= 600.dp
            Backdrop(Modifier.fillMaxSize().glassSource())
            val heading: @Composable () -> Unit = {
                Column(Modifier.rise(), verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Mark(Modifier.dropShadow(rounded(19f), Shadow(radius = 18.dp, color = Color.Black, offset = DpOffset(0.dp, 8.dp), alpha = Theme.shade(0.45f))), size = 76.dp)
                    KitText(t("Sign in"), font = Font(28f, Font.Bold), lines = 1)
                    KitText(
                        t("Meetings here are started by the people who belong here. If somebody sent you a link, open that instead."),
                        font = Font(14f),
                        color = Theme.muted,
                        align = TextAlign.Center,
                    )
                }
            }
            val form: @Composable () -> Unit = {
                Box(Modifier.fillMaxWidth().rise(60).glassPane(30.dp)) {
                    ContentLayer {
                        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                            // One box holding two fields, as the web holds them: they are one answer.
                            IdentityBox(name, { name = it }, passphrase, { passphrase = it }, namePrompt = t("Name"), onSubmit = ::signIn)
                            GlassButton(
                                ::signIn,
                                Modifier.fillMaxWidth(),
                                shape = GlassButtonShape.Capsule(48.dp),
                                prominent = true,
                                enabled = !app.busy && name.isNotEmpty() && passphrase.isNotEmpty(),
                            ) {
                                Crossfade(app.busy, animationSpec = Theme.settle(), label = "busy") { busy ->
                                    if (busy) MediaSpinner(size = 16f, tint = Theme.tallyForeground)
                                    else KitText(t("Sign in"), font = Font(15f, Font.Semibold), lines = 1)
                                }
                            }
                            if (wide) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                    Box(Modifier.weight(1f).height(1.dp).background(Theme.border))
                                    KitText(t("Were you sent a link?"), font = Font(11f), color = Theme.muted, lines = 1)
                                    Box(Modifier.weight(1f).height(1.dp).background(Theme.border))
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    LinkField(link, { link = it }, Modifier.weight(1f)) { openLink(link) }
                                    GlassButton({ openLink(link) }, shape = GlassButtonShape.Capsule(38.dp), enabled = pastedLink(link) != null) {
                                        KitText(t("Join"), lines = 1)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // The deployment's notice, which is for whoever has the app,
            // signed in or not (as the deployment's own management page
            // says), and under it a newer build: at the head of the page,
            // clear of the language menu in the corner, one under the other
            // and never one over the other. The build lay over the top of
            // the sign-in, and a notice there would have lain under it.
            val says = noticeShown(app) || app.updates?.shown == true
            val head: @Composable () -> Unit = { SignInHead(app, clear = 56.dp) }
            Folded(
                flat = { FlatSignIn(wide, says, head, heading, form) { PasteLink(scope, clipboard, ::openLink) } },
                // Folded, the greeting on one side of the hinge and the
                // fields on the other: on a table both above it, where the
                // keyboard coming up over the lower half leaves them; in a
                // book, a page each.
                // The head over the greeting, whose side has the room: over
                // the fields, on a table, it pushed the sign-in itself down
                // across the hinge.
                lead = {
                    Column(
                        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        SignInHead(app, clear = 0.dp)
                        Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                            Box(Modifier.widthIn(max = 420.dp)) { heading() }
                        }
                    }
                },
                form = {
                    Column(
                        Modifier
                            .fillMaxSize()
                            .putsKeyboardAway()
                            .verticalScroll(rememberScrollState())
                            .windowInsetsPadding(WindowInsets.safeDrawing)
                            .padding(start = 16.dp, end = 16.dp, top = 64.dp, bottom = 24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) { Box(Modifier.widthIn(max = 420.dp).fillMaxWidth()) { form() } }
                },
            )
            LanguageMenu(Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp))
            ProblemToast(
                app,
                Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal)),
                clearance = if (wide) 28.dp else 100.dp,
            )
        }
    }
}

/**
 * The sign-in as a flat screen draws it: what the deployment and the
 * updates say at the head ([says]), the greeting and the fields in one
 * column, and on a phone the paste at the foot. The greeting comes up closer
 * under the head while it says something, as the head opens and closes.
 */
@Composable
private fun FlatSignIn(
    wide: Boolean,
    says: Boolean,
    head: @Composable () -> Unit,
    heading: @Composable () -> Unit,
    form: @Composable () -> Unit,
    paste: @Composable () -> Unit,
) {
    val top by animateDpAsState(if (says) 32.dp else 72.dp, Theme.settle(), label = "top")
    Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .putsKeyboardAway()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = if (wide) 140.dp else 40.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            head()
            Spacer(Modifier.height(top))
            Column(
                Modifier.widthIn(max = 420.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(22.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                heading()
                form()
            }
        }
        if (!wide) paste()
    }
}

/**
 * The deployment's notice and, under it, a newer build, at the head of the
 * sign-in (SignInView.swift's): [clear] of what is above while either is up
 * (the language menu in the corner, on a flat screen), ten points between
 * them while both are, each opening and closing its own room so that the
 * other moves up or down to make way rather than jump. With neither,
 * nothing at all.
 */
@Composable
private fun SignInHead(app: AppModel, clear: Dp) {
    val notice = noticeShown(app)
    val update = app.updates?.shown == true
    val clear by animateDpAsState(if (notice || update) clear else 0.dp, Theme.settle(), label = "clear")
    val between by animateDpAsState(if (notice && update) 10.dp else 0.dp, Theme.settle(), label = "between")
    Column(Modifier.widthIn(max = 520.dp).fillMaxWidth().padding(top = clear)) {
        LobbyNoticeCard(app, Modifier.fillMaxWidth())
        Spacer(Modifier.height(between))
        UpdateBanner(app, Modifier.fillMaxWidth(), inline = true)
    }
}

/**
 * The way in for somebody sent a link, at the foot where a thumb is: the
 * clipboard is read only when this is pressed, as the system's own paste
 * control reads it.
 */
@Composable
private fun PasteLink(scope: CoroutineScope, clipboard: Clipboard, openLink: (String) -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        KitText(t("Were you sent a link?"), font = Font(13f), color = Theme.muted, lines = 1)
        Row(
            Modifier
                .height(44.dp)
                .background(Theme.surfaceHigh, Capsule)
                .plainPress({
                    scope.launch {
                        val text = clipboard.getClipEntry()?.clipData?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text?.toString()
                        if (text != null) openLink(text)
                    }
                }, shape = Capsule)
                .padding(horizontal = 20.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Symbol("doc.on.clipboard", size = 15f, weight = Font.Medium)
            KitText(t("Paste and open"), font = Font(15f, Font.Medium), lines = 1)
        }
    }
}

/** The Mac's field for a link that was sent, a capsule in the sign-in pane. */
@Composable
private fun LinkField(text: String, onText: (String) -> Unit, modifier: Modifier = Modifier, onSubmit: () -> Unit) {
    val font = Font(13.5f)
    Row(
        modifier
            .height(38.dp)
            .background(Theme.recess(0.22f), Capsule)
            .border(1.dp, Theme.wash.copy(alpha = 0.12f), Capsule)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Symbol("link", size = 13f, tint = Theme.muted)
        BasicTextField(
            value = text,
            onValueChange = onText,
            modifier = Modifier.weight(1f).semantics { contentDescription = t("Paste the link you were sent.") },
            textStyle = font.style(Theme.foreground),
            singleLine = true,
            cursorBrush = SolidColor(Theme.tally),
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Go,
            ),
            keyboardActions = KeyboardActions(onGo = { onSubmit() }),
            decorationBox = { inner ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (text.isEmpty()) KitText(t("Paste the link you were sent."), font = font, color = Theme.muted, lines = 1)
                    inner()
                }
            },
        )
    }
}

/**
 * The language, before a call as on every web page; following the system
 * until somebody chooses (`LanguageMenu`). A popover of the screen's.
 */
@Composable
fun LanguageMenu(modifier: Modifier = Modifier) {
    val popovers = LocalPopovers.current ?: return
    val localization = Localization.shared
    FloatingPopover(
        "language",
        popovers,
        modifier,
        width = 230.dp,
        panel = {
            MenuCard {
                MenuItem(
                    t("Same as the system"),
                    { localization.followSystem(); popovers.close() },
                    symbol = "globe",
                    detail = Language.detect(Language.systemPreferences()).displayName,
                    checked = localization.followsSystem,
                )
                MenuRule()
                for (one in Language.entries) {
                    MenuItem(
                        one.displayName,
                        { localization.choose(one); popovers.close() },
                        checked = !localization.followsSystem && localization.language == one,
                    )
                }
            }
        },
    ) {
        GlassButton({ popovers.toggle("language") }, shape = GlassButtonShape.Circle(36.dp), label = t("Language")) {
            Symbol("globe", size = 14f, weight = Font.Medium)
        }
    }
}
