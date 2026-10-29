package app.tomoshibi.ui.screens

import app.tomoshibi.ui.lobby.RoomNames
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.EaseInOut
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.LobbyModel
import app.tomoshibi.ui.Folded
import app.tomoshibi.ui.KeptToOnePane
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.LocalInHalf
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.CopyLinkButton
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.pictureShade
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.putsKeyboardAway
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.lobby.AtTheFoot
import app.tomoshibi.ui.lobby.CameraStage
import app.tomoshibi.ui.lobby.DeviceSwitches
import app.tomoshibi.ui.lobby.Mark
import app.tomoshibi.ui.lobby.OpenFromDebug
import app.tomoshibi.ui.lobby.OptionsCard
import app.tomoshibi.ui.lobby.ProblemToast
import app.tomoshibi.ui.lobby.rise
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.Joining
import uniffi.tomoshibi_protocol.Opening
import java.time.Instant

// The screens before and after a call that are not the lobby (Screens.swift).

/**
 * A screen that is a single card of glass over the backdrop
 * (`CardScreen`), with the language in its corner.
 */
@Composable
internal fun CardScreen(width: Dp = 420.dp, content: @Composable ColumnScope.() -> Unit) {
    val popovers = rememberPopovers()
    OpenFromDebug(popovers)
    FloatingPopoverLayer(popovers) {
        Box(Modifier.fillMaxSize()) {
            Backdrop(Modifier.fillMaxSize().glassSource())
            // On one half of a folded screen (ui/Fold.kt): the upper one on a
            // table, the reading page of a book, rather than across the fold.
            KeptToOnePane {
                // Less room above and below on the upper half of a phone on
                // a table, which is half a screen tall; a book's page is a
                // screen tall and keeps it, clear of the language's button.
                val half = LocalInHalf.current && LocalHinge.current?.vertical == false
                Column(
                    Modifier
                        .fillMaxSize()
                        .putsKeyboardAway()
                        .verticalScroll(rememberScrollState())
                        .windowInsetsPadding(WindowInsets.safeDrawing)
                        .padding(horizontal = 16.dp, vertical = if (half) 12.dp else 60.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(Modifier.widthIn(max = width).fillMaxWidth().rise().glassPane(30.dp)) {
                        ContentLayer {
                            Column(
                                Modifier.fillMaxWidth().padding(22.dp).animateContentSize(Theme.settle()),
                                verticalArrangement = Arrangement.spacedBy(18.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                                content = content,
                            )
                        }
                    }
                }
            }
            LanguageMenu(Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp))
        }
    }
}

/**
 * The join screens on a phone (`PaneScreen`): the picture of yourself
 * filling the screen, and a pane of glass at its foot that scrolls where it
 * is taller than the room left above the keyboard. On a wide screen the
 * Mac's card, with the picture in it.
 */
@Composable
internal fun PaneScreen(app: AppModel, content: @Composable ColumnScope.(wide: Boolean) -> Unit) {
    val popovers = rememberPopovers()
    OpenFromDebug(popovers)
    FloatingPopoverLayer(popovers) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            if (maxWidth >= 600.dp) {
                Box(Modifier.fillMaxSize()) {
                    Backdrop(Modifier.fillMaxSize().glassSource())
                    Folded(
                        // The Mac's join screen: a card of 560 points with the
                        // picture inside it.
                        flat = {
                            Column(
                                Modifier
                                    .fillMaxSize()
                                    .putsKeyboardAway()
                                    .verticalScroll(rememberScrollState())
                                    .windowInsetsPadding(WindowInsets.safeDrawing)
                                    .padding(horizontal = 16.dp, vertical = 60.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                            ) { JoinCard(content) }
                            LanguageMenu(Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp))
                        },
                        // Folded, the picture of yourself on one side of the
                        // hinge and the card on the other: on a table both
                        // on the upper half, above the keyboard; in a book a
                        // page each. A picture of a face is the one thing
                        // that may not lie across a fold.
                        lead = {
                            Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(12.dp).rise()) {
                                CameraStage(app, Modifier.matchParentSize(), cornerRadius = 24.dp)
                                DeviceSwitches(
                                    app,
                                    Modifier.align(Alignment.BottomCenter).padding(bottom = 14.dp)
                                        .glassSurface(Capsule, shade = pictureShade(app.startWithCamera)).padding(6.dp),
                                    size = 44.dp,
                                )
                                LanguageMenu(Modifier.align(Alignment.TopEnd).padding(12.dp))
                            }
                        },
                        form = {
                            CompositionLocalProvider(LocalPictureBeside provides true) {
                                Column(
                                    Modifier
                                        .fillMaxSize()
                                        .putsKeyboardAway()
                                        .verticalScroll(rememberScrollState())
                                        .windowInsetsPadding(WindowInsets.safeDrawing)
                                        .padding(12.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                ) { JoinCard(content) }
                            }
                        },
                    )
                }
            } else {
                CameraStage(app, Modifier.fillMaxSize().glassSource())
                AtTheFoot(Modifier.putsKeyboardAway()) {
                        Box(Modifier.padding(10.dp).fillMaxWidth().rise().glassPane(34.dp).background(pictureShade(app.startWithCamera))) {
                            ContentLayer {
                                Column(
                                    Modifier.fillMaxWidth().padding(18.dp).animateContentSize(Theme.settle()),
                                    verticalArrangement = Arrangement.spacedBy(16.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                ) { content(false) }
                            }
                        }
                }
            }
        }
    }
}

/** The wide join screen's card of glass, 560 points at most, with the screen's own content in it. */
@Composable
private fun JoinCard(content: @Composable ColumnScope.(wide: Boolean) -> Unit) {
    Box(Modifier.widthIn(max = 560.dp).fillMaxWidth().rise().glassPane(30.dp)) {
        ContentLayer {
            Column(
                Modifier.fillMaxWidth().padding(22.dp).animateContentSize(Theme.settle()),
                verticalArrangement = Arrangement.spacedBy(18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) { content(true) }
        }
    }
}

/** Whether the picture of yourself is drawn beside the card, on the other side of a hinge, rather than in it. */
private val LocalPictureBeside = compositionLocalOf { false }

/** The plainer screens' heading (`Heading`). */
@Composable
private fun Heading(title: String, detail: String? = null, mono: Boolean = false) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        KitText(title, font = if (mono) Font(20f, mono = true) else Font(24f, Font.Bold), lines = 2, align = TextAlign.Center)
        if (detail != null) KitText(detail, font = Font(13.5f), color = Theme.muted, align = TextAlign.Center)
    }
}

/** A quiet way back (`BackButton`). */
@Composable
private fun BackButton(action: () -> Unit) {
    GlassButton(action, shape = GlassButtonShape.Capsule(32.dp)) {
        Symbol("chevron.left", size = 13f)
        KitText(t("Back"), font = Font(13f), lines = 1)
    }
}

/** A line under the room saying something about it, in the muted voice (`Note`). */
@Composable
private fun Note(text: String, danger: Boolean = false) {
    KitText(text, Modifier.fillMaxWidth(), font = Font(12.5f), color = if (danger) Theme.dangerInk else Theme.muted, align = TextAlign.Center)
}

/** The main button's words, or a spinner while what it asked is out. */
@Composable
private fun Pressing(busy: Boolean, words: String) {
    Crossfade(busy, animationSpec = Theme.settle(), label = "busy") { now ->
        if (now) MediaSpinner(size = 16f, tint = Theme.tallyForeground) else KitText(words, font = Font(15f, Font.Semibold), lines = 1)
    }
}

// MARK: - The deployment not answering

/**
 * What opening on a deployment that will not answer shows: the one way on,
 * which is to ask again (`UnreachableScreen`). Without the address: which
 * server this is is not something the app says anywhere (AppModel `home`).
 */
@Composable
fun UnreachableScreen(app: AppModel) {
    val scope = LocalAppScope.current
    Box(Modifier.fillMaxSize()) {
        CardScreen {
            Mark(size = 56.dp)
            Heading(t("Could not reach the server."))
            GlassButton(
                { scope.launch { app.connect() } },
                Modifier.fillMaxWidth(),
                shape = GlassButtonShape.Capsule(48.dp),
                prominent = true,
                enabled = !app.busy,
            ) { Pressing(app.busy, t("Try again")) }
        }
        ProblemToast(app, Modifier.windowInsetsPadding(WindowInsets.safeDrawing))
    }
}

// MARK: - Arriving through a link

/**
 * The join screen for somebody who came through a link (PreJoin.tsx;
 * `PreJoinScreen`): the picture of yourself, what you are invited to, your
 * name, and in. The line saying why the last press did not take, and the
 * knock it may offer, come and go with the curves they declare.
 */
@Composable
fun PreJoinScreen(app: AppModel) {
    val scope = LocalAppScope.current
    // A word already typed for this sitting is shown, and the switch on.
    var sealing by rememberSaveable { mutableStateOf(app.sealingWord.isNotEmpty()) }
    fun join() {
        if (app.busy || app.joiningName.isEmpty()) return
        if (!sealing) app.sealingWord = ""
        scope.launch { app.join() }
    }
    PaneScreen(app) { wide ->
        RoomHeading(app, app.room, door(app))
        JoinPicture(app, wide)
        if (app.account == null) {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                // Somebody on an invitation has a name to choose and nothing
                // else to answer: the link is what lets them in, and a
                // passphrase field beside it reads as something they forgot.
                // A name the link was made for is shown and cannot be changed,
                // because the server signs that name in whatever is typed.
                IdentityBox(
                    name = app.invitedName ?: app.name,
                    onName = { app.name = it },
                    passphrase = app.passphrase,
                    onPassphrase = { app.passphrase = it },
                    nameOnly = app.arrivedInvited,
                    fixed = app.invitedName != null,
                    onSubmit = ::join,
                )
                Crossfade(identityLine(app), animationSpec = Theme.settle(), label = "identity") { line -> Note(line) }
                linkWindow(app)?.let { Note(it) }
            }
        }
        // Where the call is held, for somebody who did not come through the
        // lobby, whose own picker they never saw: an invitation's link,
        // signed in or not, or nobody signed in at all; and only where there
        // is a choice (AppModel `offersRelayChoiceAtTheDoor`). It was offered
        // only to somebody signed out, so a member opening an invitation
        // could not choose.
        OptionsCard(app, sealing, { sealing = it }, serverId = "prejoin.server", offerServer = app.offersRelayChoiceAtTheDoor)
        ProblemSlot(app)
        GlassButton(
            ::join,
            Modifier.fillMaxWidth(),
            shape = GlassButtonShape.Capsule(52.dp),
            prominent = true,
            enabled = !app.busy && app.joiningName.isNotEmpty(),
        ) { Pressing(app.busy, t("Join")) }
        AnimatedVisibility(app.problem != null && app.canKnock, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            GlassButton(
                { scope.launch { app.knock() } },
                Modifier.fillMaxWidth(),
                shape = GlassButtonShape.Capsule(44.dp),
                enabled = !app.busy && app.joiningName.isNotEmpty(),
            ) { KitText(t("Ask to be let in"), lines = 1) }
        }
        // Calls off a join that is out (AppModel `cancelJoin`).
        BackButton { app.backToLobby() }
    }
}

/**
 * The picture of yourself in the join screen: on a phone it is the whole
 * screen behind the pane, and the pane holds the switches alone; on a wide
 * screen it is a picture in the card, the switches over its foot.
 */
@Composable
private fun JoinPicture(app: AppModel, wide: Boolean) {
    // Drawn beside the card, on the other side of the hinge (PaneScreen).
    if (wide && LocalPictureBeside.current) return
    if (wide) {
        Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f)) {
            CameraStage(app, Modifier.matchParentSize(), cornerRadius = 20.dp)
            DeviceSwitches(
                app,
                Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp)
                    .glassSurface(Capsule, shade = pictureShade(app.startWithCamera)).padding(6.dp),
                size = 44.dp,
            )
        }
    } else {
        DeviceSwitches(app, size = 52.dp)
    }
}

/**
 * PreJoin.tsx's one line under the identity, which changes rather than
 * doubling: what a passphrase is for until there is one, and what it does
 * once there is.
 */
private fun identityLine(app: AppModel): String {
    app.invitedName?.let { return t("You were invited to this room as {name}.", mapOf("name" to it)) }
    if (app.arrivedInvited) return t("You were invited to this room. Choose a name and go in.")
    if (app.passphrase.isNotEmpty()) return t("Only you can join as {name}", mapOf("name" to app.joiningName.ifEmpty { "?" }))
    return t("Add a passphrase to keep your name, and to run the room you open.")
}

/**
 * When the link stops working, where it does, in the reader's own clock: a
 * guest who arrives early for a call at four should know the link will
 * still be good at four. A link with no end says nothing.
 */
private fun linkWindow(app: AppModel): String? {
    if (!app.arrivedInvited) return null
    val until = app.invitation?.until?.let(AppModel::instant) ?: return null
    val from = app.invitation?.from?.let(AppModel::instant)
    if (from != null) return t("This link works from {from} until {until}.", mapOf("from" to LobbyModel.said(from), "until" to LobbyModel.said(until)))
    return t("This link works until {until}.", mapOf("until" to LobbyModel.said(until)))
}

/**
 * RoomTitle.tsx: what this name is worth at this deployment's door, said
 * before anybody presses Join. Not to somebody holding an invitation, whom
 * it would tell to go and do what they just did, nor to somebody signed in,
 * whose door the session already answers.
 */
private fun door(app: AppModel): String? {
    if (app.account != null || app.arrivedInvited) return null
    val info = app.info ?: return null
    val scope = RoomNames.scope(app.room)
    if (scope.isNotEmpty()) return t("Only members of {scope} can join this room.", mapOf("scope" to scope))
    when (info.joinedBy) {
        Joining.ACCOUNTS -> return t("Sign in to join a room here.")
        Joining.INVITED -> return t("Rooms here are by invitation. Open the link you were sent, or sign in.")
        Joining.ANYONE -> {}
    }
    return when (info.openedBy) {
        Opening.ADMINS -> t("Only administrators can start new rooms. Enter the name you were given.")
        Opening.SIGNED -> t("Set a passphrase to start a new room. Anybody can join one that already exists.")
        else -> if (RoomNames.generated(app.room)) null else t("Anyone who guesses this name can join.")
    }
}

/**
 * The room, as the heading of the screen, in the machine's monospace, with
 * the way to copy its address where a plain address is a way in
 * (`RoomHeading`). Only where anybody may come in by the name, and never for
 * a room held under a scope (RoomTitle.tsx `useLinkWorks`): where the door
 * asks for an invitation the copied address opens for nobody, and whoever
 * sends it has invited a person to be turned away.
 */
@Composable
private fun RoomHeading(app: AppModel, room: String, door: String?) {
    val link = run {
        if (app.info?.joinedBy != Joining.ANYONE || room.isEmpty() || RoomNames.scope(room).isNotEmpty()) return@run null
        val base = app.deployment?.baseUrl() ?: return@run null
        (if (base.endsWith("/")) base else "$base/") + "#/" + room
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            KitText(room, Modifier.weight(1f, fill = false), font = Font(20f, mono = true), lines = 2, align = TextAlign.Center)
            if (link != null) CopyLinkButton(link, size = 30.dp)
        }
        var said by remember { mutableStateOf(door) }
        if (door != null) said = door
        AnimatedVisibility(door != null, enter = Theme.arrive, exit = Theme.depart) {
            KitText(said ?: "", font = Font(12.5f), color = Theme.muted, align = TextAlign.Center)
        }
    }
}

/**
 * Who somebody is, as one control (Identity.tsx; `IdentityBox`): a name and
 * the passphrase that proves it are one answer, so they are one box with a
 * hairline through it. The key lights once there is a passphrase, and an
 * eye offers to show it once there is something to show.
 */
@Composable
internal fun IdentityBox(
    name: String,
    onName: (String) -> Unit,
    passphrase: String,
    onPassphrase: (String) -> Unit,
    nameOnly: Boolean = false,
    fixed: Boolean = false,
    namePrompt: String = t("Your name"),
    onSubmit: () -> Unit = {},
) {
    val shape = rounded(18f)
    Column(
        Modifier
            .fillMaxWidth()
            .background(Theme.recess(0.22f), shape)
            .border(1.dp, Theme.wash.copy(alpha = 0.10f), shape)
            .animateContentSize(Theme.settle()),
    ) {
        IdentityRow(if (fixed) "person.fill.checkmark" else "person", if (fixed) Theme.tallyInk else Theme.muted) {
            if (fixed) {
                SelectionContainer { KitText(name, font = Font(15f), color = Theme.muted, lines = 1) }
            } else {
                IdentityField(name, onName, namePrompt, secret = false, onSubmit = onSubmit)
            }
        }
        AnimatedVisibility(!nameOnly, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Column {
                Box(Modifier.fillMaxWidth().height(1.dp).background(Theme.wash.copy(alpha = 0.07f)))
                PassphraseRow(passphrase, onPassphrase, onSubmit)
            }
        }
    }
}

@Composable
internal fun PassphraseRow(passphrase: String, onPassphrase: (String) -> Unit, onSubmit: () -> Unit) {
    var shown by rememberSaveable { mutableStateOf(false) }
    val key by animateColorAsState(if (passphrase.isEmpty()) Theme.muted else Theme.tallyInk, Theme.settle(), label = "key")
    IdentityRow("key", key) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f)) { IdentityField(passphrase, onPassphrase, t("Passphrase"), secret = !shown, password = true, onSubmit = onSubmit) }
            AnimatedVisibility(passphrase.isNotEmpty(), enter = Theme.arrive, exit = Theme.depart) {
                val press = rememberPress()
                Box(
                    Modifier
                        .size(24.dp)
                        .pressable(press, CircleShape, true, Role.Button, if (shown) t("Hide passphrase") else t("Show passphrase")) { shown = !shown },
                    contentAlignment = Alignment.Center,
                ) { SwappingSymbol(if (shown) "eye.slash" else "eye", size = 13f, tint = Theme.muted) }
            }
        }
    }
}

@Composable
internal fun IdentityRow(symbol: String, tint: Color, field: @Composable () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(50.dp).padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) { Symbol(symbol, size = 15f, tint = tint) }
        Box(Modifier.weight(1f)) { field() }
    }
}

/**
 * One of the identity's fields: no capital, no correction and no
 * suggestion, as the Swift ones ask of the keyboard, since what is typed is
 * a name or a passphrase, and a keyboard that "corrects" a name signs in as
 * somebody else.
 */
@Composable
internal fun IdentityField(
    text: String,
    onText: (String) -> Unit,
    prompt: String,
    secret: Boolean,
    password: Boolean = false,
    onSubmit: () -> Unit,
) {
    val font = Font(15f)
    BasicTextField(
        value = text,
        onValueChange = onText,
        modifier = Modifier.fillMaxWidth().semantics { contentDescription = prompt },
        textStyle = font.style(Theme.foreground),
        singleLine = true,
        cursorBrush = SolidColor(Theme.tallyInk),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.None,
            autoCorrectEnabled = false,
            keyboardType = if (password && secret) KeyboardType.Password else KeyboardType.Text,
            imeAction = ImeAction.Go,
        ),
        keyboardActions = KeyboardActions(onGo = { onSubmit() }),
        decorationBox = { inner ->
            Box(contentAlignment = Alignment.CenterStart) {
                if (text.isEmpty()) KitText(prompt, font = font, color = Theme.muted, lines = 1)
                inner()
            }
        },
    )
}

/**
 * Why the last attempt did not take, in red under the form (`ProblemLine`);
 * one that is about why they are not in the meeting can be closed rather
 * than fading. Arrives and leaves with its curves, rather than being gone
 * between one frame and the next with the form jumping up under it.
 */
@Composable
private fun ProblemSlot(app: AppModel) {
    var said by remember { mutableStateOf(app.problem) }
    app.problem?.let { said = it }
    AnimatedVisibility(app.problem != null, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(Theme.danger.copy(alpha = 0.10f), rounded(13f))
                .padding(horizontal = 12.dp, vertical = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Symbol("exclamationmark.circle.fill", Modifier.padding(top = 1.dp), size = 12.5f, tint = Theme.dangerInk)
            KitText(said ?: "", Modifier.weight(1f), font = Font(12.5f), color = Theme.dangerInk)
            if (app.problemStays) {
                val press = rememberPress()
                Box(
                    Modifier.size(18.dp).pressable(press, CircleShape, true, Role.Button, t("Close")) { app.problem = null },
                    contentAlignment = Alignment.Center,
                ) { Symbol("xmark", size = 9.5f, weight = Font.Bold, tint = Theme.muted) }
            }
        }
    }
}

// MARK: - At the door

/** Waiting for somebody inside to let you in (`KnockingScreen`). */
@Composable
fun KnockingScreen(app: AppModel) {
    CardScreen {
        Heading(app.room, mono = true)
        Row(Modifier.padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            MediaSpinner(size = 16f, tint = Theme.muted)
            KitText(t("Waiting for somebody to let you in."), font = Font(14f))
        }
        BackButton { app.stopKnocking() }
    }
}

// MARK: - An arranged meeting

/**
 * Waiting.tsx inside PreJoin.tsx (`WaitingScreen`): the ordinary join
 * screen, because the point of arriving early is to have the camera, the
 * microphone and the name settled before the meeting, with a button that
 * waits. Nothing is asked of the door until it is pressed, so nobody is
 * joined with a name they were still typing.
 */
@Composable
fun WaitingScreen(app: AppModel) {
    val scope = LocalAppScope.current
    var sealing by rememberSaveable { mutableStateOf(app.sealingWord.isNotEmpty()) }
    // The host's button comes alive when the window opens, without anybody
    // reloading anything: a clock, every thirty seconds.
    var now by remember { mutableStateOf(Instant.now()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = Instant.now()
        }
    }
    PaneScreen(app) { wide ->
        Heading(app.room, mono = true)
        JoinPicture(app, wide)
        if (app.account == null) {
            // A guest with no saved name gives one here; the button waits for
            // it (Waiting.tsx: "Nothing is asked until there is one").
            IdentityBox(app.name, { app.name = it }, app.passphrase, { app.passphrase = it }, nameOnly = true)
        }
        // A meeting's link is never the lobby's join, so the picker is here
        // as on the join screen; it had none, and whoever opened one could
        // not choose where the meeting they were early for would be held.
        OptionsCard(app, sealing, { sealing = it }, serverId = "waiting.server", offerServer = app.offersRelayChoiceAtTheDoor)
        WaitingState(app, now) {
            if (!sealing) app.sealingWord = ""
            scope.launch { app.goArranged() }
        }
        ProblemSlot(app)
        BackButton { app.stopWaiting() }
    }
}

@Composable
private fun WaitingState(app: AppModel, now: Instant, go: () -> Unit) {
    val waiting = app.waiting
    Column(Modifier.fillMaxWidth().animateContentSize(Theme.settle()), verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        when (waiting.gone) {
            AppModel.Waiting.Gone.Cancelled -> Note(t("That invitation is no longer good. Ask for another."))
            AppModel.Waiting.Gone.Ended -> Note(t("That meeting has ended."))
            AppModel.Waiting.Gone.NotComing -> Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Note(t("The host has not started this meeting."))
                GlassButton({ app.askAgain() }, Modifier.fillMaxWidth(), shape = GlassButtonShape.Capsule(44.dp)) { KitText(t("Ask again"), lines = 1) }
            }
            null -> {
                val arrangement = app.arrangement
                val opens = app.arrangedOpens
                val tooEarly = arrangement?.mine == true && !app.arrangedReady && opens != null && now.isBefore(opens)
                if (arrangement != null && !app.arrangedReady) {
                    val whenSaid = AppModel.instant(arrangement.at)?.let { LobbyModel.said(it) } ?: arrangement.at
                    Note(
                        if (arrangement.mine) {
                            if (tooEarly) t("Your meeting can start from {when}.", mapOf("when" to LobbyModel.said(opens)))
                            else t("Your meeting is arranged for {when}. Joining starts it.", mapOf("when" to whenSaid))
                        } else {
                            t("This meeting is arranged for {when}.", mapOf("when" to whenSaid))
                        },
                    )
                } else if (arrangement == null) {
                    if (waiting.unreachable) Note(t("Could not reach the meeting. Trying again.")) else MediaSpinner(size = 16f, tint = Theme.muted)
                }
                val asking = waiting.asking || app.busy
                GlassButton(
                    go,
                    Modifier.fillMaxWidth(),
                    shape = GlassButtonShape.Capsule(52.dp),
                    prominent = true,
                    spacing = 8.dp,
                    enabled = app.joiningName.isNotEmpty() && !waiting.asking && !app.busy && arrangement != null && !tooEarly,
                ) {
                    Appear(asking) { MediaSpinner(size = 16f, tint = Theme.tallyForeground) }
                    KitText(
                        when {
                            waiting.asking -> t("Waiting for the host to start the meeting.")
                            app.arrangedReady -> t("Join")
                            arrangement?.mine == true -> t("Start the meeting")
                            else -> t("Wait for the host")
                        },
                        font = Font(15f, Font.Semibold),
                        lines = 1,
                    )
                }
            }
        }
    }
}

// MARK: - After

/** App.tsx's three endings, and the one somebody chose (`EndedScreen`). */
@Composable
fun EndedScreen(app: AppModel) {
    val scope = LocalAppScope.current
    val ending = app.ending
    Box(Modifier.fillMaxSize()) {
        CardScreen {
            Box(Modifier.size(64.dp).glassSurface(CircleShape), contentAlignment = Alignment.Center) {
                Symbol(
                    when (ending) {
                        AppModel.Ending.Left -> "phone.down.fill"
                        AppModel.Ending.Closed -> "door.left.hand.closed"
                        AppModel.Ending.Removed -> "person.crop.circle.badge.xmark"
                        AppModel.Ending.Lost -> "wifi.exclamationmark"
                    },
                    size = 26f, weight = Font.Medium,
                    tint = if (ending == AppModel.Ending.Left) Theme.muted else Theme.foreground,
                )
            }
            Heading(
                when (ending) {
                    AppModel.Ending.Left -> t("You left the call.")
                    AppModel.Ending.Closed -> t("The host closed this room.")
                    AppModel.Ending.Removed -> t("You were removed from this room.")
                    AppModel.Ending.Lost -> t("The connection to this room was lost.")
                },
                detail = app.room.ifEmpty { null },
            )
            // Back in where that is something to do: after leaving, and after
            // a connection that dropped, which is one press back with the
            // devices already chosen (App.tsx). Not into a room the host
            // closed, or one somebody was put out of. Once: pressed twice it
            // ran two joins as the same participant, and the server put one
            // of them out.
            if (ending == AppModel.Ending.Left || ending == AppModel.Ending.Lost) {
                GlassButton(
                    { scope.launch { app.join() } },
                    Modifier.fillMaxWidth(),
                    shape = GlassButtonShape.Capsule(48.dp),
                    prominent = true,
                    enabled = !app.busy,
                ) { Pressing(app.busy, t("Join again")) }
            }
            ProblemSlot(app)
            BackButton { app.backToLobby() }
        }
    }
}

// MARK: - While the deployment is first asked about

/** The ground and the mark, rather than a form asking for an address the app already has (`StartingScreen`). */
@Composable
fun StartingScreen() {
    val breathing = rememberInfiniteTransition(label = "breathing")
    val alpha by breathing.animateFloat(0.55f, 1f, infiniteRepeatable(tween(1100, easing = EaseInOut), RepeatMode.Reverse), label = "mark")
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Backdrop(Modifier.fillMaxSize())
        // In the middle of the half that is looked at, not of the screen,
        // which on a phone standing on a table is the hinge.
        KeptToOnePane(contentAlignment = Alignment.Center) {
            Column(verticalArrangement = Arrangement.spacedBy(18.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Mark(Modifier.alpha(alpha), size = 56.dp)
                MediaSpinner(size = 16f, tint = Theme.muted)
            }
        }
    }
}

/**
 * Where a move is taking the meeting while there is no call to show it over
 * (CallOverlay.swift `MovingOverlay`): between leaving the old room and
 * entering the new one the room view is gone, and the screen was blank.
 */
@Composable
fun MovingOverlay(to: String) {
    val named = say(to.trim())
    Box(Modifier.fillMaxSize().background(Theme.background.copy(alpha = 0.8f)).clearAndSetSemantics {
        contentDescription = if (named.isEmpty()) t("Moving this meeting") else t("Moving this meeting to {name}", mapOf("name" to named))
    }, contentAlignment = Alignment.Center) {
        KeptToOnePane(contentAlignment = Alignment.Center) {
            Column(Modifier.padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                MediaSpinner(size = 18f, tint = Theme.muted)
                KitText(
                    if (named.isEmpty()) t("Moving this meeting") else t("Moving this meeting to {name}", mapOf("name" to named)),
                    font = Font(14f, Font.Medium),
                    align = TextAlign.Center,
                )
                // The one thing worth saying: nobody has to do anything.
                KitText(t("You will be back in a moment."), font = Font(12f), color = Theme.muted, align = TextAlign.Center)
            }
        }
    }
}
