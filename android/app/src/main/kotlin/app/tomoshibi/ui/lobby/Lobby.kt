package app.tomoshibi.ui.lobby

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.LobbyModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.LocalHinge
import app.tomoshibi.ui.Pane
import app.tomoshibi.ui.Posture
import app.tomoshibi.ui.effectivePosture
import app.tomoshibi.ui.inPane
import app.tomoshibi.ui.pane
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.ui.platform.LocalLayoutDirection
import app.tomoshibi.ui.kit.AccountAvatar
import app.tomoshibi.ui.kit.Appear
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.FloatingPopover
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassGroup
import app.tomoshibi.ui.kit.GlassShell
import app.tomoshibi.ui.kit.Initial
import app.tomoshibi.ui.kit.KitFont
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRole
import app.tomoshibi.ui.kit.MenuRule
import app.tomoshibi.ui.kit.PopoverAlignment
import app.tomoshibi.ui.kit.PopoverEdge
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.fingertip
import app.tomoshibi.ui.kit.holdsPresses
import app.tomoshibi.ui.kit.glassSurface
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.putsKeyboardAway
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.settings.SettingsState
import app.tomoshibi.ui.settings.SettingsTopic
import app.tomoshibi.ui.settings.SettingsView
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.Account

// The screens in front of a call (Lobby.swift), drawn as the platforms draw
// their own (the owner's design canvas, 2026-10-15): on a phone a
// large-titled home with the picture of yourself in a card and a field at the
// foot to join by name; on a wide screen, an unfolded phone or a tablet, the
// Mac's window with a sidebar of glass floating over the picture. What they
// offer is the web's lobby (Lobby.tsx, Schedule.tsx, ServerPicker.tsx), and
// only somebody signed in, or holding a link they were sent, gets past the
// first of them.

/** Where in the lobby somebody is, beside its home: the arranged meetings, or the servers. */
enum class Place { Home, Scheduled, Servers }

/**
 * Everything the lobby has open, kept above the choice between the phone's
 * layout and the wide one, so that a Fold opened or closed in the middle of
 * choosing a time or typing a room keeps it: the phone's schedule sheet is
 * the wide window's schedule section, its pushed server page the server
 * section, and the start sheet keeps its draft and its join.
 */
@Stable
class LobbyState(sealingWord: String) {
    // A debug launch's part of the lobby opens the first lobby only: read
    // again by the lobby a call came back to, it opened the start sheet over
    // it once more after the test had left the call.
    private val opened = LobbyDebug.open.also { LobbyDebug.open = null }

    var place by mutableStateOf(
        when (opened) {
            "scheduled" -> Place.Scheduled
            "servers" -> Place.Servers
            else -> Place.Home
        },
    )
    var sheet by mutableStateOf(LobbyDebug.sheet(opened)?.let { MeetingDraft(it, sealingWord) })
    private var settingsOpen by mutableStateOf(opened == "settings")

    /**
     * Whether the settings are open. Opening them starts them afresh, as
     * the Swift sheet starts its @State afresh; what they hold while open
     * is [settingsPage], which the phone's sheet and the wide window's
     * dialog both draw, so that a Fold folded or opened out with them
     * showing keeps the section and whatever was half typed in it.
     */
    var settings: Boolean
        get() = settingsOpen
        set(open) {
            if (open && !settingsOpen) settingsPage = SettingsState()
            settingsOpen = open
        }
    var settingsPage by mutableStateOf(
        SettingsState(
            if (opened == "settings") LobbyDebug.section?.let { name -> SettingsTopic.entries.firstOrNull { it.name.equals(name, ignoreCase = true) } } else null,
        ).also { LobbyDebug.section = null },
    )
        private set
    var typed by mutableStateOf("")
    val schedule = ScheduleDraft()
    val popovers = Popovers()

    /** Whether a sheet, a dialog or a pushed page is over the lobby. */
    val covered: Boolean get() = sheet != null || settings || place != Place.Home

    fun open(mode: MeetingMode, app: AppModel) {
        sheet = MeetingDraft(mode, app.sealingWord)
    }
}

/** The lobby of somebody signed in (`LobbyView`): a phone's under 600 points wide, a window's with a sidebar from there. */
@Composable
fun LobbyView(app: AppModel, account: Account, modifier: Modifier = Modifier) {
    val state = remember { LobbyState(app.sealingWord) }
    LaunchedEffect(account.name) { app.lobby.refreshArrangements() }
    // The server section is for choosing a machine; where there is none to
    // choose it has nothing to offer, and it goes.
    LaunchedEffect(state) {
        snapshotFlow { app.offersRelayChoice }.drop(1).collect { offered ->
            if (!offered && state.place == Place.Servers) state.place = Place.Home
        }
    }
    LaunchedEffect(state) { snapshotFlow { state.place to state.settings }.drop(1).collect { state.popovers.close() } }
    BoxWithConstraints(modifier.fillMaxSize()) {
        val wide = maxWidth >= 600.dp
        AnimatedContent(wide, transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) }, label = "layout") { isWide ->
            if (isWide) WideLobby(app, account, state) else PhoneLobby(app, account, state)
        }
    }
}

/** Where the signed-in account's picture is, resolved against the deployment (Kit+Settings.swift `avatarAddress`). */
val AppModel.avatarAddress: String?
    get() {
        val avatar = account?.avatar?.takeIf { it.isNotEmpty() } ?: return null
        if (avatar.startsWith("http://") || avatar.startsWith("https://")) return avatar
        val base = deployment?.baseUrl() ?: return null
        val root = if (base.endsWith("/")) base.dropLast(1) else base
        val path = if (avatar.startsWith("/")) avatar else "/$avatar"
        // A counter a new picture moves on, so the image already held under
        // the address is not shown again (Account.tsx appends the time).
        return root + path + (if (pictureVersion > 0) "?$pictureVersion" else "")
    }

/** The sheet's draft while the sheet is open, and for as long as it is leaving afterwards. */
@Composable
private fun rememberShownDraft(state: LobbyState): MeetingDraft? {
    var shown by remember { mutableStateOf(state.sheet) }
    state.sheet?.let { shown = it }
    return shown
}

// MARK: - The phone

/**
 * A large-titled home (`PhoneLobby`): the picture of yourself in a card
 * with the two things to do on it, what is coming, the rooms lately, the
 * server, and a field at the foot to join by the name you were given.
 */
@Composable
private fun PhoneLobby(app: AppModel, account: Account, state: LobbyState) {
    val scroll = rememberScrollState()
    val pushed by animateFloatAsState(if (state.place == Place.Servers) 1f else 0f, Theme.settle(), label = "pushed")
    FloatingPopoverLayer(state.popovers) {
        OpenFromDebug(state.popovers)
        // Back closes the page pushed over the lobby; the sheets close
        // themselves. With nothing open the system's Back leaves the app.
        BackHandler(state.place == Place.Servers) { state.place = Place.Home }
        BoxWithConstraints(Modifier.fillMaxSize().background(Color.Black)) {
            val width = constraints.maxWidth.toFloat()
            Box(
                Modifier
                    .fillMaxSize()
                    // Under a pushed page the lobby moves a third of the way
                    // aside and darkens, as a navigation stack's does.
                    .graphicsLayer { translationX = -width * 0.3f * pushed }
                    .alpha(1f - 0.35f * pushed)
                    // Out of a screen reader's reach while something is over
                    // it, as what is under a sheet on the iPhone is.
                    .then(if (state.covered) Modifier.clearAndSetSemantics {} else Modifier),
            ) {
                ContentLayer {
                    Column(
                        Modifier
                            .fillMaxSize()
                            // The page is content; the join bar floats over it
                            // as a toolbar does, and stays glass.
                            .glassSource()
                            .putsKeyboardAway()
                            .verticalScroll(scroll)
                            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal)),
                    ) {
                        Spacer(Modifier.windowInsetsTopHeight(WindowInsets.safeDrawing))
                        Spacer(Modifier.height(BarHeight))
                        KitText("Tomoshibi", Modifier.padding(start = 16.dp, end = 16.dp, bottom = 10.dp), font = Font(34f, Font.Bold), lines = 1)
                        // In the page under its title, rather than over it: laid
                        // over the top it covered the title, and what the
                        // navigation bar leaves of it.
                        UpdateBanner(app, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), inline = true)
                        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
                            Hero(app, state, Modifier.rise())
                            Upcoming(app, state, Modifier.rise(60))
                            Recent(app, state, Modifier.rise(100))
                            ServerCard(app, state, Modifier.rise(140))
                        }
                        Spacer(Modifier.height(24.dp + 60.dp + 6.dp))
                        Spacer(Modifier.windowInsetsBottomHeight(WindowInsets.safeDrawing))
                    }
                }
                NavigationBar(app, account, state, scroll)
                JoinBar(app, account, state, Modifier.align(Alignment.BottomCenter))
            }
            AnimatedVisibility(
                state.place == Place.Servers,
                enter = slideInHorizontally(Theme.settle()) { it },
                exit = slideOutHorizontally(Theme.leave()) { it },
            ) { ServersView(app, wide = false, back = { state.place = Place.Home }) }
            SheetHost(state.place == Place.Scheduled, { state.place = Place.Home }) {
                ScheduleView(app, account, state.schedule, wide = false, close = { state.place = Place.Home })
            }
            SheetHost(state.settings, { state.settings = false }) {
                SettingsView(app, close = { state.settings = false }, state = state.settingsPage)
            }
            // Over the sheets, so that a meeting that could not be arranged
            // says so where the schedule is open: behind the sheet, as on the
            // iPhone, it said nothing at all.
            Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                ProblemToast(app, clearance = if (state.place == Place.Home && !state.settings) 92.dp else 20.dp)
            }
            val draft = rememberShownDraft(state)
            SheetHost(state.sheet != null, {
                state.sheet?.let { open ->
                    open.going?.cancel()
                    app.cancelJoin()
                }
                state.sheet = null
            }, style = SheetStyle.Cover, background = Color.Black) {
                draft?.let { MeetingSheet(app, it, close = { state.sheet = null }, wide = false) }
            }
        }
    }
}

/** The navigation bar's row, the large title sitting under it until it is scrolled away. */
private val BarHeight = 52.dp

/**
 * The top of the phone's lobby as the system's navigation bar draws it over
 * a large title: the account in a circle of glass at the trailing end, and
 * the page's name in the middle once the large one has gone up under it,
 * over the page fading into the status bar.
 */
@Composable
private fun NavigationBar(app: AppModel, account: Account, state: LobbyState, scroll: ScrollState) {
    val density = LocalDensity.current
    val past by remember { derivedStateOf { scroll.value > with(density) { 40.dp.toPx() } } }
    val title by animateFloatAsState(if (past) 1f else 0f, Theme.settle(), label = "title")
    Box(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .matchParentSize()
                .alpha(title)
                .background(Brush.verticalGradient(listOf(Color.Black.copy(alpha = 0.92f), Color.Black.copy(alpha = 0.6f), Color.Transparent))),
        )
        Column(Modifier.fillMaxWidth()) {
            Spacer(Modifier.windowInsetsTopHeight(WindowInsets.safeDrawing))
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(BarHeight)
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))
                    .padding(horizontal = 16.dp),
            ) {
                KitText("Tomoshibi", Modifier.align(Alignment.Center).alpha(title), font = Font(17f, Font.Semibold), lines = 1)
                GlassButton(
                    { state.settings = true },
                    Modifier.align(Alignment.CenterEnd),
                    shape = GlassButtonShape.Circle(40.dp),
                    label = t("Settings"),
                ) { AccountAvatar(account.name, app.avatarAddress, size = 30.dp) }
            }
        }
    }
}

/** The picture of yourself in a card, with the switches for how you will arrive and the two things to do. */
@Composable
private fun Hero(app: AppModel, state: LobbyState, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(292.dp)) {
        CameraStage(app, Modifier.matchParentSize(), cornerRadius = 30.dp)
        DeviceSwitches(app, Modifier.align(Alignment.TopEnd).padding(12.dp), size = 40.dp)
        GlassGroup(8.dp) {
            Row(
                Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                GlassButton(
                    { state.open(MeetingMode.Start, app) },
                    Modifier.weight(1f),
                    shape = GlassButtonShape.Capsule(52.dp),
                    prominent = true,
                    spacing = 8.dp,
                ) {
                    KitFont(Font(16f, Font.Semibold)) {
                        Symbol("video.fill")
                        KitText(t("Start a meeting"), lines = 1)
                    }
                }
                GlassButton(
                    { state.place = Place.Scheduled },
                    shape = GlassButtonShape.Circle(52.dp),
                    label = t("Schedule a meeting"),
                ) { Symbol("calendar", size = 16f, weight = Font.Semibold) }
            }
        }
    }
}

private val CardShape = rounded(24f)

@Composable
private fun Section(title: String, modifier: Modifier = Modifier, trailing: @Composable () -> Unit = {}, rows: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            KitText(title, Modifier.weight(1f), font = Font(20f, Font.Bold), lines = 1)
            trailing()
        }
        Column(Modifier.fillMaxWidth().clip(CardShape).background(Theme.surface)) { rows() }
    }
}

/** What is coming, the four soonest, with the way to all of them. */
@Composable
private fun Upcoming(app: AppModel, state: LobbyState, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    val upcoming = app.lobby.upcoming.take(4)
    AnimatedVisibility(
        upcoming.isNotEmpty(),
        modifier,
        enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()),
        exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave()),
    ) {
        Section(t("Upcoming"), trailing = {
            // A fingertip tall, the words in its middle: drawn as a line of
            // words it was pressed over twenty points, with the first meeting
            // eight below.
            Box(Modifier.heightIn(min = fingertip(0.dp)).plainPress({ state.place = Place.Scheduled }), contentAlignment = Alignment.CenterEnd) {
                KitText(t("All of them"), font = Font(16f), color = Theme.tally, lines = 1)
            }
        }) {
            for (one in upcoming) {
                MeetingRow(
                    one, app,
                    Modifier
                        .fillMaxWidth()
                        .plainPress({ app.lobby.link(one)?.let { link -> scope.launch { app.open(link) } } })
                        .padding(horizontal = 14.dp, vertical = 11.dp),
                )
                if (one.id != upcoming.last().id) RowRule(leading = 62.dp)
            }
        }
    }
}

/**
 * The rooms joined lately, four of them, each opened again in one press
 * and taken off the list by a swipe to the left: Remove is out of sight until
 * the row is swiped, as the system's list rows keep theirs.
 */
@Composable
private fun Recent(app: AppModel, state: LobbyState, modifier: Modifier = Modifier) {
    val shown = app.lobby.recentHere.take(4)
    AnimatedVisibility(
        shown.isNotEmpty(),
        modifier,
        enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()),
        exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave()),
    ) {
        Section(t("Recent")) {
            val kept = keepDeparting(shown) { it.id }
            for (entry in kept) {
                val recent = entry.item
                AnimatedVisibility(
                    entry.visible,
                    enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()),
                    exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave()),
                ) {
                    Column {
                        SwipeRow(
                            t("Remove"),
                            "trash",
                            { app.lobby.forget(recent) },
                            fullSwipe = true,
                            actionLabel = t("Remove {room} from recent", mapOf("room" to recent.room)),
                        ) { closeFirst ->
                            RecentLine(recent, Modifier.plainPress({ if (!closeFirst()) state.open(MeetingMode.Again(recent.room), app) }))
                        }
                        if (recent.id != shown.lastOrNull()?.id) RowRule(leading = 62.dp)
                    }
                }
            }
        }
    }
}

@Composable
private fun RecentLine(recent: LobbyModel.Recent, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .height(58.dp)
            .background(Theme.surface)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Initial(recent.room, size = 36.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            KitText(recent.room, font = Font(16f, Font.Semibold), lines = 1)
            KitText(relativeNamed(recent.at), font = Font(13f), color = Theme.muted, lines = 1)
        }
        Symbol("chevron.right", size = 13f, weight = Font.Semibold, tint = Theme.muted.copy(alpha = Theme.muted.alpha * 0.6f))
    }
}

/** The server, where there is a machine to choose, opening the page of them. */
@Composable
private fun ServerCard(app: AppModel, state: LobbyState, modifier: Modifier = Modifier) {
    AnimatedVisibility(
        app.offersRelayChoice,
        modifier,
        enter = Theme.arrive,
        exit = Theme.depart,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .plainPress({ state.place = Place.Servers }, shape = CardShape)
                .background(Theme.surface, CardShape)
                .padding(14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(36.dp).background(Color.White.copy(alpha = 0.08f), rounded(11f)), contentAlignment = Alignment.Center) {
                Symbol("server.rack", size = 16f, tint = Theme.muted)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                KitText(t("Server"), font = Font(16f, Font.Semibold), lines = 1)
                FleetLine(app)
            }
            KitText(app.relayChoice?.let { app.lobby.label(it) } ?: t("Automatic"), font = Font(14f), color = Theme.muted, lines = 1)
            Symbol("chevron.right", size = 13f, weight = Font.Semibold, tint = Theme.muted.copy(alpha = Theme.muted.alpha * 0.6f))
        }
    }
}

/**
 * Joining by the name you were given, from the foot of the screen where a
 * thumb already is; an invitation pasted in is opened as the link it is.
 */
@Composable
private fun JoinBar(app: AppModel, account: Account, state: LobbyState, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    fun joinTyped() {
        pastedLink(state.typed)?.let { link ->
            state.typed = ""
            keyboard?.hide()
            focus.clearFocus()
            scope.launch { app.open(link) }
            return
        }
        if (state.typed.isBlank()) return
        val room = RoomNames.normalise(state.typed)
        if (!validTyped(room)) return
        state.open(MeetingMode.Join(room), app)
        state.typed = ""
    }
    Row(
        modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
            .padding(start = 16.dp, end = 16.dp, bottom = 6.dp)
            // A toolbar over the page: a press beside the field was a press
            // on the recent room scrolled under it.
            .holdsPresses(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        ScopedRoomField(
            t("Room name or invite link"),
            state.typed,
            { state.typed = it },
            account.scopes,
            Modifier.weight(1f),
            admin = account.admin,
            capsule = true,
            onSubmit = ::joinTyped,
        )
        GlassButton(
            ::joinTyped,
            shape = GlassButtonShape.Circle(54.dp),
            prominent = true,
            enabled = validTyped(state.typed) || pastedLink(state.typed) != null,
            label = t("Join"),
        ) { Symbol("arrow.right", size = 20f, weight = Font.Bold) }
    }
}

// MARK: - A wide screen

/**
 * A window with a sidebar, as the Mac's own apps are (`MacLobby`): what to
 * do and what is coming in a sidebar of glass, and the picture of yourself
 * filling the rest. Where a hinge divides the screen (ui/Fold.kt), held as a
 * book the sidebar keeps to the first page and the rest, the picture with
 * it, to the second; standing on a table the window turns into the desk
 * lobby, the picture and the sections above the hinge and the sidebar's
 * contents laid out along the lower half.
 */
@Composable
private fun WideLobby(app: AppModel, account: Account, state: LobbyState) {
    val hinge = LocalHinge.current
    val density = LocalDensity.current
    FloatingPopoverLayer(state.popovers) {
        OpenFromDebug(state.popovers)
        BoxWithConstraints(Modifier.fillMaxSize().background(Theme.background)) {
            val standing = hinge.effectivePosture(maxWidth.value, maxHeight.value, density) == Posture.Tabletop
            // The ground the sidebar floats over: the picture at home, the
            // backdrop in the other sections.
            Backdrop(Modifier.fillMaxSize().glassSource())
            AnimatedContent(
                standing,
                transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) },
                label = "standing",
            ) { onTable ->
                Box(Modifier.fillMaxSize().then(if (state.sheet != null || state.settings) Modifier.clearAndSetSemantics {} else Modifier)) {
                    if (onTable) DeskLobby(app, account, state) else SidebarLobby(app, account, state)
                }
            }
            // Over the whole window only on the desk, where there is no
            // section of its own to sit in; beside the sidebar otherwise.
            if (standing) UpdateBanner(app, Modifier.windowInsetsPadding(WindowInsets.safeDrawing))
            ProblemToast(app, Modifier.windowInsetsPadding(WindowInsets.safeDrawing))
            DialogHost(state.settings, { state.settings = false }, width = 900.dp, height = 700.dp) {
                SettingsView(app, close = { state.settings = false }, Modifier.heightIn(max = 700.dp), state.settingsPage)
            }
            val draft = rememberShownDraft(state)
            DialogHost(state.sheet != null, {
                state.sheet?.let { open ->
                    open.going?.cancel()
                    app.cancelJoin()
                }
                state.sheet = null
            }, width = if (standing) 860.dp else 560.dp) {
                draft?.let { MeetingSheet(app, it, close = { state.sheet = null }, wide = true) }
            }
        }
    }
}

/** The wide lobby flat or held as a book: the sidebar, and the section beside it. */
@Composable
private fun SidebarLobby(app: AppModel, account: Account, state: LobbyState) {
    val hinge = LocalHinge.current?.takeIf { it.vertical }
    val density = LocalDensity.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val home = state.place == Place.Home
        val folded = hinge?.let { with(density) { it.left.toDp() to it.right.toDp() } }
            ?.takeIf { (left, right) -> left >= 240.dp && maxWidth - right >= 320.dp }
        // The picture of yourself on the second page of a book rather than
        // under the sidebar and across the hinge, where a face is folded.
        val beside = folded?.second ?: 0.dp
        Appear(home, Modifier.absoluteOffset(x = beside).width(maxWidth - beside).fillMaxHeight(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
            CameraStage(app, Modifier.fillMaxSize().glassSource(1f))
        }
        val sidebar = folded?.first ?: if (maxWidth < 760.dp) 250.dp else 284.dp
        Row(Modifier.fillMaxSize()) {
            Box(Modifier.width(sidebar).fillMaxHeight()) {
                Sidebar(
                    app, account, state,
                    Modifier
                        .width(min(sidebar, 340.dp))
                        .fillMaxHeight()
                        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Start + WindowInsetsSides.Vertical))
                        .padding(10.dp),
                )
            }
            if (folded != null) Spacer(Modifier.width(folded.second - folded.first))
            Box(Modifier.weight(1f).fillMaxHeight()) {
                ChosenSection(app, account, state)
                // In the section beside the sidebar, whose buttons it covered
                // when it was centred on the window, and below the server menu
                // in the section's corner.
                UpdateBanner(app, Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.End)), top = 52.dp)
            }
        }
    }
}

/** The section the sidebar has chosen: home's picture and greeting, the arranged meetings, or the servers. */
@Composable
private fun ChosenSection(app: AppModel, account: Account, state: LobbyState, onTable: Boolean = false) {
    val density = LocalDensity.current
    // One identity for each section, so a section arrives with its own
    // transition rather than as one view changing what is in it, which
    // arrived as a cut.
    AnimatedContent(
        state.place,
        transitionSpec = {
            (fadeIn(Theme.settle()) + slideInVertically(Theme.settle()) { with(density) { 10.dp.roundToPx() } }) togetherWith
                fadeOut(Theme.leave())
        },
        label = "section",
    ) { place ->
        when (place) {
            Place.Home -> HomeStage(app, account, onTable)
            Place.Scheduled -> ScheduleView(app, account, state.schedule, wide = true, close = { state.place = Place.Home })
            Place.Servers -> ServersView(app, wide = true, back = { state.place = Place.Home })
        }
    }
}

/**
 * The wide lobby on a phone standing on a table (Samsung's Flex mode): the
 * upper half is what is looked at, the picture of yourself at home and the
 * section chosen otherwise, and the lower half, which lies on the table
 * under the hand, is the sidebar laid out along it: what to do and where to
 * go on the left, what is coming and the rooms lately beside them. Nothing
 * of either lies across the hinge; a sheet opened from here opens above it
 * (DialogHost), where the keyboard coming up over the lower half leaves it.
 */
@Composable
private fun DeskLobby(app: AppModel, account: Account, state: LobbyState) {
    val hinge = LocalHinge.current
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = maxWidth.value
        val height = maxHeight.value
        val upper = hinge.pane(width, height, density, direction, Pane.First)
        val lower = hinge.pane(width, height, density, direction, Pane.Second)
        Box(Modifier.inPane(upper, width, height)) {
            Appear(state.place == Place.Home, Modifier.fillMaxSize(), enter = fadeIn(Theme.settle()), exit = fadeOut(Theme.leave())) {
                CameraStage(app, Modifier.fillMaxSize().glassSource(1f))
            }
            ChosenSection(app, account, state, onTable = true)
        }
        Box(Modifier.inPane(lower, width, height)) {
            Desk(
                app, account, state,
                Modifier
                    .fillMaxSize()
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(10.dp),
            )
        }
    }
}

/** The sidebar's contents along the lower half of a phone on a table (DeskLobby). */
@Composable
private fun Desk(app: AppModel, account: Account, state: LobbyState, modifier: Modifier) {
    GlassShell(modifier, cornerRadius = 22.dp) {
        Row(Modifier.fillMaxSize()) {
            Column(Modifier.width(268.dp).fillMaxHeight()) {
                SidebarActions(app, state)
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 8.dp, vertical = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) { SidebarSections(app, state) }
                SidebarFoot(app, account, state)
            }
            Box(Modifier.padding(vertical = 14.dp).width(1.dp).fillMaxHeight().background(Theme.border))
            Row(Modifier.weight(1f).fillMaxHeight().padding(horizontal = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Column(
                    Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(vertical = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) { SidebarUpcoming(app, Modifier.fillMaxWidth()) }
                Column(
                    Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(vertical = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) { SidebarRecent(app, state, Modifier.fillMaxWidth()) }
            }
        }
    }
}

@Composable
private fun Sidebar(app: AppModel, account: Account, state: LobbyState, modifier: Modifier = Modifier) {
    GlassShell(modifier, cornerRadius = 22.dp) {
        Column(Modifier.fillMaxSize()) {
            SidebarActions(app, state)
            Column(
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                SidebarSections(app, state)
                SidebarUpcoming(app)
                SidebarRecent(app, state)
            }
            SidebarFoot(app, account, state)
        }
    }
}

/** The sidebar's two things to do, at its head. */
@Composable
private fun SidebarActions(app: AppModel, state: LobbyState) {
    Row(
        Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        GlassButton({ state.open(MeetingMode.Start, app) }, Modifier.weight(1f), shape = GlassButtonShape.Capsule(36.dp), prominent = true, label = t("Start a meeting")) {
            Symbol("video.fill")
            KitText(t("Start"), lines = 1)
        }
        GlassButton({ state.open(MeetingMode.Join(""), app) }, Modifier.weight(1f), shape = GlassButtonShape.Capsule(36.dp), label = t("Join a meeting")) {
            Symbol("link")
            KitText(t("Join"), lines = 1)
        }
    }
}

/** The sidebar's sections: home, the arranged meetings, and the servers where there is a machine to choose. */
@Composable
private fun SidebarSections(app: AppModel, state: LobbyState) {
    SidebarItem(t("Home"), "house", state.place == Place.Home) { state.place = Place.Home }
    SidebarItem(t("Scheduled meetings"), "calendar", state.place == Place.Scheduled, badge = app.lobby.upcoming.size) {
        state.place = Place.Scheduled
    }
    AnimatedVisibility(app.offersRelayChoice, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
        SidebarItem(t("Server"), "server.rack", state.place == Place.Servers) { state.place = Place.Servers }
    }
}

/** What is coming, the five soonest, each opened in one press. */
@Composable
private fun SidebarUpcoming(app: AppModel, modifier: Modifier = Modifier) {
    val scope = LocalAppScope.current
    val upcoming = app.lobby.upcoming.take(5)
    AnimatedVisibility(upcoming.isNotEmpty(), modifier, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            SidebarHeading(t("Upcoming"))
            for (one in upcoming) {
                MeetingRow(
                    one, app,
                    Modifier
                        .fillMaxWidth()
                        .plainPress({ app.lobby.link(one)?.let { link -> scope.launch { app.open(link) } } }, shape = rounded(10f))
                        .padding(horizontal = 8.dp, vertical = 6.dp),
                )
            }
        }
    }
}

/** The rooms lately, six of them, each opened again in one press or taken off the list. */
@Composable
private fun SidebarRecent(app: AppModel, state: LobbyState, modifier: Modifier = Modifier) {
    val recent = app.lobby.recentHere.take(6)
    AnimatedVisibility(recent.isNotEmpty(), modifier, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            SidebarHeading(t("Recent"))
            for (entry in keepDeparting(recent) { it.id }) {
                AnimatedVisibility(entry.visible, enter = Theme.arrive, exit = Theme.depart) {
                    RecentSidebarRow(
                        entry.item,
                        open = { state.open(MeetingMode.Again(entry.item.room), app) },
                        forget = { app.lobby.forget(entry.item) },
                    )
                }
            }
        }
    }
}

/** The sidebar's foot: the servers answering, and the account with its menu. */
@Composable
private fun SidebarFoot(app: AppModel, account: Account, state: LobbyState) {
    Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FleetLine(app, Modifier.padding(horizontal = 6.dp))
        AccountMenu(app, account, state)
    }
}

/** A sidebar entry with its symbol in the person's colour, lit while it is the section shown. */
@Composable
private fun SidebarItem(title: String, symbol: String, selected: Boolean, badge: Int = 0, select: () -> Unit) {
    val lit by animateFloatAsState(if (selected) 1f else 0f, Theme.settle(), label = "selected")
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 34.dp)
            .background(Theme.tally.copy(alpha = 0.20f * lit), rounded(10f))
            .plainPress(select, shape = rounded(10f))
            .padding(horizontal = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(20.dp), contentAlignment = Alignment.Center) { Symbol(symbol, size = 14f, tint = Theme.tally) }
        KitText(title, Modifier.weight(1f), font = Font(13.5f, if (selected) Font.Semibold else Font.Regular), lines = 1)
        if (badge > 0) KitText("$badge", font = Font(12f, Font.Medium, digits = true), color = Theme.muted, lines = 1)
    }
}

@Composable
private fun SidebarHeading(title: String) {
    KitText(title, Modifier.padding(start = 10.dp, top = 14.dp, bottom = 4.dp), font = Font(11f, Font.Semibold), color = Theme.muted, lines = 1)
}

/**
 * A recent room in the sidebar: the row opens it, and the cross beside it
 * takes it off the list (`RecentSidebarRow`). The Mac shows the cross under
 * the pointer; a finger has no pointer to rest, so here it is always there,
 * faint until pressed.
 */
@Composable
private fun RecentSidebarRow(recent: LobbyModel.Recent, open: () -> Unit, forget: () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(
            Modifier
                .weight(1f)
                .plainPress(open, shape = rounded(10f))
                .padding(horizontal = 8.dp, vertical = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Initial(recent.room, size = 26.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                KitText(recent.room, font = Font(13f, Font.Medium), lines = 1)
                KitText(relativeNamed(recent.at), font = Font(11f), color = Theme.muted, lines = 1)
            }
        }
        RecentForget(recent.room, forget, Modifier.padding(end = 6.dp))
    }
}

/** The cross that takes a room off the recent list (`RecentForget`). */
@Composable
private fun RecentForget(room: String, action: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(20.dp)
            .background(Color.White.copy(alpha = 0.07f), Capsule)
            .plainPress(action, label = t("Remove {room} from recent", mapOf("room" to room)), shape = Capsule),
        contentAlignment = Alignment.Center,
    ) { Symbol("xmark", size = 7.2f, weight = Font.Bold, tint = Theme.muted) }
}

/**
 * The account and everything that is not a meeting, as the app's own
 * popover over the sidebar's foot (`AccountMenu`). No pages of the web's
 * here: the account's own settings are in Settings.
 */
@Composable
private fun AccountMenu(app: AppModel, account: Account, state: LobbyState) {
    val popovers = state.popovers
    FloatingPopover(
        "account",
        popovers,
        edge = PopoverEdge.Top,
        alignment = PopoverAlignment.Leading,
        width = 262.dp,
        panel = {
            MenuCard {
                MenuItem(t("Settings"), { popovers.close(); state.settings = true }, symbol = "gearshape")
                MenuRule()
                MenuItem(
                    t("Sign out"),
                    {
                        popovers.close()
                        // Asked first (SignOutPrompt), over the whole window.
                        app.signOutAsked = true
                    },
                    symbol = "rectangle.portrait.and.arrow.right",
                    role = MenuRole.Destructive,
                )
            }
        },
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .glassSurface(rounded(16f))
                .plainPress({ popovers.toggle("account") }, shape = rounded(16f))
                .padding(horizontal = 10.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AccountAvatar(account.name, app.avatarAddress, size = 30.dp)
            KitText(account.name, Modifier.weight(1f), font = Font(13f, Font.Semibold), lines = 1)
            Symbol("chevron.up.chevron.down", size = 10f, weight = Font.Semibold, tint = Theme.muted)
        }
    }
}

/**
 * Home on a wide screen (`HomeStage`): over the picture of yourself, which
 * fills the window under the sidebar, a greeting, the switches for how you
 * will arrive, and the server where there is a choice.
 */
@Composable
private fun HomeStage(app: AppModel, account: Account, onTable: Boolean) {
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        Column(Modifier.padding(start = 36.dp, top = 56.dp, end = 24.dp).rise(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            ShadowedText(t("Hello, {name}", mapOf("name" to account.name)), Font(34f, Font.Bold), Theme.foreground)
            // On a table the sidebar's contents are below the hinge (DeskLobby).
            val start = if (onTable) t("Start one from below when you are ready.") else t("Start one from the sidebar when you are ready.")
            ShadowedText(start, Font(15f), Color.White.copy(alpha = 0.78f))
        }
        // The capsule is the glass, and the switches inside it are drawn
        // plain, so that nothing is glass on glass.
        Box(
            Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 28.dp)
                .rise(80)
                .glassSurface(Capsule)
                .padding(8.dp),
        ) { ContentLayer { DeviceSwitches(app) } }
        // In the stage's corner, only where there is a machine to choose.
        Appear(app.offersRelayChoice, Modifier.align(Alignment.TopEnd).padding(top = 12.dp, end = 16.dp)) {
            ServerMenu(app)
        }
    }
}

/** Words over the picture, with the soft shadow that keeps them read over a bright one. */
@Composable
private fun ShadowedText(text: String, font: Font, color: Color) {
    BasicText(text, style = font.style(color).copy(shadow = Shadow(Color.Black.copy(alpha = 0.45f), Offset(0f, 4f), 32f)))
}
