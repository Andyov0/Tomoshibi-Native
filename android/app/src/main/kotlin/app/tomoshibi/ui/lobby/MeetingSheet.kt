package app.tomoshibi.ui.lobby

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.say
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.FloatingPopover
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.LocalPopovers
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuHeading
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.SealControl
import app.tomoshibi.ui.kit.SegmentedChoice
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.putsKeyboardAway
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.rememberPress
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.draw.alpha
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What the start and join sheet was opened for (`MeetingSheet.Mode`). */
sealed interface MeetingMode {
    data object Start : MeetingMode
    data class Join(val room: String) : MeetingMode

    /**
     * A room from the recent list: joined if a meeting is live in it, started
     * again under the same name if not. Opened as a join, it was refused
     * whenever the meeting had ended ("No meeting is happening under that
     * name"), which for a room somebody has been in before is most of the time.
     */
    data class Again(val room: String) : MeetingMode
}

/**
 * What the sheet holds while it is open: kept with the lobby rather than in
 * the sheet, so that a phone unfolded with the sheet open, which redraws it
 * as the wide window's, keeps what was typed and the join under way.
 */
@Stable
class MeetingDraft(mode: MeetingMode, sealingWord: String) {
    var mode by mutableStateOf(mode)
    var room by mutableStateOf(
        LobbyDebug.room ?: when (mode) {
            is MeetingMode.Join -> mode.room
            is MeetingMode.Again -> mode.room
            MeetingMode.Start -> ""
        },
    )

    /** A word already typed for this sitting is shown, and the switch on. */
    var sealing by mutableStateOf(sealingWord.isNotEmpty())
    var working by mutableStateOf(false)

    /** The press of Start or Join while it asks whether the meeting is live: Cancel calls it off. */
    var going: Job? = null

    val starting: Boolean get() = mode == MeetingMode.Start
    val again: Boolean get() = mode is MeetingMode.Again
}

/**
 * Start a meeting or join one: the preview, the room, where it is held and
 * whether it is sealed, and then straight into the call (`MeetingSheet`). On
 * a phone the whole screen, the picture behind a pane of glass at the foot;
 * on a wide screen a sheet over the window, the picture in it.
 */
@Composable
fun MeetingSheet(app: AppModel, draft: MeetingDraft, close: () -> Unit, wide: Boolean, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    val popovers = rememberPopovers()
    OpenFromDebug(popovers)

    // Settles a room from the recent list into the choice the sheet shows:
    // Join while a meeting is live in it, Start while none is. Not over a
    // choice made meanwhile, and left as it is when the deployment does not
    // say, which `go` then takes either way.
    LaunchedEffect(draft) {
        val again = draft.mode as? MeetingMode.Again ?: return@LaunchedEffect
        val deployment = app.deployment ?: return@LaunchedEffect
        val live = runCatching { deployment.roomLive(again.room) }.getOrNull()
        if (!draft.again || live == null) return@LaunchedEffect
        draft.mode = if (live) MeetingMode.Join(draft.room) else MeetingMode.Start
    }

    // Closes the sheet and calls off whatever its press set going: Cancel and
    // the cross stayed live while a join was out and did nothing to it, so
    // somebody who pressed Cancel was put in the call a moment later with the
    // microphone on.
    fun cancel() {
        draft.going?.cancel()
        draft.going = null
        app.cancelJoin()
        close()
    }

    // Asks whether a meeting is live under the name first, as the web does:
    // one that exists cannot be started and one that does not cannot be
    // joined. An unknown answer goes through; the join is the authority.
    fun go() {
        if (draft.working || app.busy) return
        val deployment = app.deployment ?: return
        // An invitation pasted in where a name goes is opened as the link it
        // is: a room's name taken out of it would leave the invitation behind.
        if (!draft.starting) {
            pastedLink(draft.room)?.let { link ->
                if (!draft.sealing) app.sealingWord = ""
                close()
                scope.launch { app.open(link) }
                return
            }
        }
        val typed = if (draft.room.isBlank()) "" else RoomNames.normalise(draft.room)
        val target = if (draft.starting) (if (typed.isNotEmpty() && RoomNames.valid(typed)) typed else RoomNames.generate()) else typed
        if (target.isEmpty() || !RoomNames.valid(target)) return
        if (!draft.sealing) app.sealingWord = ""
        draft.working = true
        app.problem = null
        val starting = draft.starting
        val again = draft.again
        draft.going = scope.launch {
            try {
                val live = runCatching { deployment.roomLive(target) }.getOrNull()
                // Called off while the question was out: nothing more is asked.
                if (!isActive) return@launch
                if (starting && live == true) {
                    app.problem = t("A meeting is already happening under that name.")
                    return@launch
                }
                // A room from the recent list goes in whether or not it is
                // live: pressed before the sheet had settled which it is, it is
                // that room somebody means, and entering it starts it again.
                if (!starting && !again && live == false) {
                    app.problem = t("No meeting is happening under that name.")
                    return@launch
                }
                // Past this point a Cancel is the model's to honour
                // (AppModel `cancelJoin`): cancelling the join's coroutine
                // halfway through entering the room would leave a room
                // entered and nobody to leave it.
                withContext(NonCancellable) { app.enter(target) }
            } finally {
                draft.working = false
            }
        }
    }

    FloatingPopoverLayer(popovers) {
        if (wide) {
            ContentLayer {
                Box(modifier) {
                    Backdrop(Modifier.matchParentSize().alpha(0.55f))
                    Column(
                        Modifier
                            .verticalScroll(rememberScrollState())
                            .padding(20.dp)
                            .animateContentSize(Theme.settle()),
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        Header(draft, ::cancel, wide = true)
                        Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f)) {
                            CameraStage(app, Modifier.matchParentSize(), cornerRadius = 20.dp)
                            DeviceSwitches(
                                app,
                                Modifier
                                    .align(Alignment.BottomCenter)
                                    .padding(bottom = 12.dp)
                                    .glassSurface(Capsule)
                                    .padding(6.dp),
                                size = 38.dp,
                            )
                        }
                        Form(app, draft, ::go)
                        Footer(app, draft, ::go, ::cancel, close, wide = true)
                    }
                }
            }
        } else {
            Box(modifier.fillMaxSize().putsKeyboardAway()) {
                CameraStage(app, Modifier.fillMaxSize().glassSource())
                AtTheFoot {
                        GlassShell(Modifier.padding(10.dp).fillMaxWidth(), cornerRadius = 34.dp) {
                            Column(
                                Modifier.padding(18.dp).animateContentSize(Theme.settle()),
                                verticalArrangement = Arrangement.spacedBy(16.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                            ) {
                                Header(draft, ::cancel, wide = false)
                                DeviceSwitches(app, size = 52.dp)
                                Form(app, draft, ::go)
                                Footer(app, draft, ::go, ::cancel, close, wide = false)
                            }
                        }
                }
            }
        }
    }
}

@Composable
private fun Header(draft: MeetingDraft, cancel: () -> Unit, wide: Boolean) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        GlassButton(cancel, shape = GlassButtonShape.Circle(32.dp), label = t("Cancel")) {
            Symbol("xmark", size = 13f, weight = Font.Bold)
        }
        val choice = @Composable { choiceModifier: Modifier ->
            SegmentedChoice(
                options = listOf(true to t("Start new meeting"), false to t("Join a meeting")),
                selection = draft.starting,
                onSelect = { start -> draft.mode = if (start) MeetingMode.Start else MeetingMode.Join(draft.room) },
                modifier = choiceModifier,
                height = 32.dp,
            )
        }
        if (wide) {
            Spacer(Modifier.weight(1f))
            choice(Modifier.widthIn(max = 300.dp))
            Spacer(Modifier.weight(1f))
            Spacer(Modifier.size(32.dp))
        } else {
            // The whole width beside the close button: centred between two
            // buttons' worth of space it cut "Start new meeting" short on a
            // phone held upright.
            choice(Modifier.weight(1f))
        }
    }
}

@Composable
private fun Form(app: AppModel, draft: MeetingDraft, go: () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        // The scope half of a name completes from the account's own
        // (ScopedRoomInput.tsx), in both directions as on the web.
        ScopedRoomField(
            prompt = if (draft.starting) t("Name it yourself, or leave it blank for one nobody has used.") else t("Room name or invite link"),
            text = draft.room,
            onTextChange = { draft.room = it },
            scopes = app.account?.scopes ?: emptyList(),
            admin = app.account?.admin ?: false,
            focusOnAppear = LobbyDebug.room != null,
            onSubmit = go,
        )
        OptionsCard(app, draft.sealing, { draft.sealing = it }, serverId = "sheet.server")
    }
}

/**
 * Where the call is held, where there is a choice, and whether it is
 * sealed, as one card: what the start sheet and the join screen both ask
 * before a call.
 */
@Composable
internal fun OptionsCard(app: AppModel, sealing: Boolean, onSealing: (Boolean) -> Unit, serverId: String, offerServer: Boolean = true) {
    val shape = rounded(18f)
    Column(
        Modifier
            .fillMaxWidth()
            .background(Color.Black.copy(alpha = 0.22f), shape)
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape)
            .animateContentSize(Theme.settle()),
    ) {
        AnimatedVisibility(
            offerServer && app.offersRelayChoice,
            enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()),
            exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave()),
        ) {
            Column {
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 14.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                        Symbol("server.rack", size = 13f, weight = Font.Semibold, tint = Theme.muted)
                    }
                    KitText(t("Server"), Modifier.weight(1f), font = Font(14f), lines = 1)
                    ServerMenu(app, compact = true, id = serverId)
                }
                Box(Modifier.fillMaxWidth().height(1.dp).background(Color.White.copy(alpha = 0.06f)))
            }
        }
        SealControl(sealing, onSealing, app.sealingWord, { app.sealingWord = it }, Modifier.padding(horizontal = 14.dp, vertical = 12.dp))
    }
}

@Composable
private fun Footer(app: AppModel, draft: MeetingDraft, go: () -> Unit, cancel: () -> Unit, close: () -> Unit, wide: Boolean) {
    val scope = LocalAppScope.current
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        var said by remember { mutableStateOf(app.problem) }
        app.problem?.let { said = it }
        Appear(app.problem != null) {
            KitText(said ?: "", font = Font(12.5f), color = Theme.danger, align = TextAlign.Center)
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            val knock = app.problem != null && app.canKnock
            Appear(knock) {
                GlassButton(
                    { scope.launch { app.knock(); close() } },
                    shape = GlassButtonShape.Capsule(40.dp),
                    enabled = !app.busy,
                ) { KitText(t("Ask to be let in"), lines = 1) }
            }
            if (wide) {
                Spacer(Modifier.weight(1f))
                GlassButton(cancel, shape = GlassButtonShape.Capsule(36.dp)) { KitText(t("Cancel"), lines = 1) }
            }
            val ready = draft.starting || validTyped(draft.room) || pastedLink(draft.room) != null
            GlassButton(
                go,
                if (wide) Modifier else Modifier.weight(1f),
                shape = GlassButtonShape.Capsule(if (wide) 36.dp else 56.dp),
                prominent = true,
                // And while any join is out, a called-off one included: a
                // second beside it would go in as the same participant.
                enabled = !draft.working && !app.busy && ready,
            ) {
                Crossfade(draft.working || app.busy, animationSpec = Theme.settle(), label = "go") { busy ->
                    if (busy) MediaSpinner(size = 16f, tint = Theme.tallyForeground)
                    else KitText(if (draft.starting) t("Start") else t("Join"), font = Font(if (wide) 13.5f else 16f, Font.Semibold), lines = 1)
                }
            }
        }
    }
}

/**
 * A room name, with the part after `@` filled in from the account's own
 * scopes (ScopedRoomInput.tsx; `ScopedRoomField`). The scope is the half
 * nobody should have to type, and a slip in it is not another room but a
 * refusal at the door, so typing `@` offers the list, a single scope is
 * simply written in, and a scope the account is not in is said under the
 * field before anything is pressed. Only completed, never rewritten; an
 * administrator belongs to every scope and is never warned. `capsule` is the
 * phone's join bar, a capsule of glass rather than a field in a card.
 */
@Composable
fun ScopedRoomField(
    prompt: String,
    text: String,
    onTextChange: (String) -> Unit,
    scopes: List<String>,
    modifier: Modifier = Modifier,
    admin: Boolean = false,
    capsule: Boolean = false,
    focusOnAppear: Boolean = false,
    onSubmit: () -> Unit = {},
) {
    var dismissed by remember { mutableStateOf(false) }
    var active by remember { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val at = text.lastIndexOf('@').takeIf { it >= 0 }
    val typedScope = at?.let { text.substring(it + 1).lowercase() } ?: ""
    val matches = if (at == null) emptyList() else scopes.filter { it.startsWith(typedScope) }
    val open = focused && !dismissed && matches.isNotEmpty() && !(matches.size == 1 && matches[0] == typedScope)
    val strange = at != null && typedScope.isNotEmpty() && !admin && typedScope !in scopes && !open
    val chosen = active.coerceIn(0, maxOf(0, matches.size - 1))

    fun accept(scope: String) {
        val cut = at ?: return
        onTextChange(text.substring(0, cut + 1) + scope)
        active = 0
        dismissed = true
    }

    LaunchedEffect(focusOnAppear) {
        if (!focusOnAppear) return@LaunchedEffect
        delay(800)
        runCatching { focus.requestFocus() }
    }

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (capsule) {
            AnimatedVisibility(open, enter = Theme.slideUp, exit = Theme.slideDown) {
                Suggestions(matches, chosen, ::accept) { active = it }
            }
        }
        val font = Font(if (capsule) 16f else 14.5f, mono = text.isNotEmpty())
        Row(
            Modifier
                .fillMaxWidth()
                .height(if (capsule) 54.dp else 46.dp)
                .then(
                    if (capsule) Modifier.glassSurface(Capsule)
                    else Modifier.background(Color.Black.copy(alpha = 0.24f), rounded(16f)).border(1.dp, Color.White.copy(alpha = 0.10f), rounded(16f)),
                )
                .padding(horizontal = if (capsule) 18.dp else 14.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                Symbol(if (capsule) "link" else "number", size = if (capsule) 16f else 14f, weight = Font.Medium, tint = Theme.muted)
            }
            BasicTextField(
                value = text,
                onValueChange = { new ->
                    dismissed = false
                    active = 0
                    // The @ just typed, at the end, by somebody with one
                    // scope: there is nothing to choose, so it is written in.
                    if (new == "$text@" && scopes.size == 1 && !text.contains('@')) {
                        onTextChange(new + scopes[0])
                        dismissed = true
                    } else {
                        onTextChange(new)
                    }
                },
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(focus)
                    .onFocusChanged { focused = it.isFocused }
                    .semantics { contentDescription = prompt }
                    .onPreviewKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown || !open) return@onPreviewKeyEvent false
                        when (event.key) {
                            Key.DirectionDown -> { active = (chosen + 1) % matches.size; true }
                            Key.DirectionUp -> { active = (chosen - 1 + matches.size) % matches.size; true }
                            Key.Tab -> { accept(matches[chosen]); true }
                            Key.Escape -> { dismissed = true; true }
                            else -> false
                        }
                    },
                textStyle = font.style(Theme.foreground),
                singleLine = true,
                cursorBrush = SolidColor(Theme.tally),
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { if (open) accept(matches[chosen]) else onSubmit() }),
                decorationBox = { field ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (text.isEmpty()) KitText(prompt, font = Font(if (capsule) 16f else 14.5f), color = Theme.muted, lines = 1)
                        field()
                    }
                },
            )
        }
        if (!capsule) {
            AnimatedVisibility(open, enter = Theme.arrive, exit = Theme.depart) {
                Suggestions(matches, chosen, ::accept) { active = it }
            }
        }
        AnimatedVisibility(strange, enter = Theme.arrive, exit = Theme.depart) {
            KitText(
                t("You are not in {scope}, so this room will not let you in.", mapOf("scope" to typedScope)),
                Modifier.padding(horizontal = 6.dp),
                font = Font(12f),
                color = Theme.danger,
            )
        }
    }
}

@Composable
private fun Suggestions(matches: List<String>, chosen: Int, accept: (String) -> Unit, hover: (Int) -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .glassPane(14.dp)
            .padding(5.dp)
            .semantics { contentDescription = t("Your scopes") },
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        matches.forEachIndexed { index, scope ->
            val press = rememberPress()
            if (press.hovered) hover(index)
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 32.dp)
                    .background(Color.White.copy(alpha = if (index == chosen) 0.10f else 0f), rounded(9f))
                    .pressable(press, rounded(9f), true, Role.Button) { accept(scope) }
                    .padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                KitText("@", font = Font(13.5f, mono = true), color = Theme.muted, lines = 1)
                KitText(scope, Modifier.padding(start = 2.dp).weight(1f), font = Font(13.5f, mono = true), lines = 1)
                if (index == chosen) Symbol("return", size = 10f, weight = Font.Semibold, tint = Theme.muted)
            }
        }
    }
}

/**
 * Where the call is held, as the app's own popover: automatic or one relay,
 * each with how quickly it answered from here (`ServerMenu`). Drawn only
 * where there is a choice to make (`AppModel.offersRelayChoice`).
 */
@Composable
fun ServerMenu(app: AppModel, modifier: Modifier = Modifier, compact: Boolean = false, id: String = "server") {
    val popovers = LocalPopovers.current ?: rememberPopovers()
    LaunchedEffect(Unit) { if (app.lobby.measured == null) app.lobby.measure() }
    val current: Double? = app.relayChoice?.let { app.lobby.measured?.get(it) } ?: if (app.relayChoice == null) app.lobby.fastest?.millis else null
    FloatingPopover(
        id,
        popovers,
        modifier,
        width = 290.dp,
        panel = {
            Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
                MenuCard {
                    MenuItem(
                        t("Automatic"),
                        { app.relayChoice = null; popovers.close() },
                        symbol = "wand.and.stars",
                        detail = app.lobby.fastest?.let { "${t("Whichever answers fastest")} · ${app.lobby.label(it.name)}" } ?: t("Whichever answers fastest"),
                        checked = app.relayChoice == null,
                    )
                    for (group in app.lobby.grouped()) {
                        if (group.region.isNotEmpty()) MenuHeading(group.region)
                        for (relay in group.relays) {
                            val named = say(relay.label ?: relay.name)
                            MenuItem(
                                if (relay.adminOnly) "$named [Admin]" else named,
                                { app.relayChoice = relay.name; popovers.close() },
                                detail = if (relay.maintenance) t("Not taking calls") else millis(app, relay.name),
                                checked = app.relayChoice == relay.name,
                                enabled = !relay.maintenance,
                            )
                        }
                    }
                }
            }
        },
    ) {
        GlassButton({ popovers.toggle(id) }, shape = GlassButtonShape.Capsule(if (compact) 30.dp else 34.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                if (!compact) Symbol("server.rack", size = 12f)
                KitText(app.relayChoice?.let { app.lobby.label(it) } ?: t("Automatic"), font = Font(12.5f, Font.Medium), lines = 1)
                if (current != null) KitText("${current.toInt()} ms", font = Font(12.5f, Font.Medium, digits = true), color = latencyColour(current), lines = 1)
                Symbol("chevron.up.chevron.down", size = 9f, weight = Font.Semibold, tint = Theme.muted)
            }
        }
    }
}

private fun millis(app: AppModel, name: String): String? {
    val measured = app.lobby.measured ?: return null
    val value = measured[name] ?: return t("Timeout")
    return "${value.toInt()} ms"
}
