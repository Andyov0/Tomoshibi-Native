package app.tomoshibi.ui.room

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateValueAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.core.app.PictureInPictureModeChangedInfo
import androidx.core.util.Consumer
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.ConnectionBadge
import app.tomoshibi.basics.ControlsPlacement
import app.tomoshibi.basics.Plan
import app.tomoshibi.basics.StagePin
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.Person
import app.tomoshibi.model.Sound
import app.tomoshibi.model.Surface
import app.tomoshibi.model.host.CrateRules
import app.tomoshibi.model.host.HostFeature
import app.tomoshibi.ui.call.CallOverlay
import app.tomoshibi.ui.call.HostPanel
import app.tomoshibi.ui.call.MovingOverlay
import app.tomoshibi.ui.call.SoundPanel
import app.tomoshibi.ui.kit.BarAxis
import app.tomoshibi.ui.kit.FloatingPopover
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.GlassAppear
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.MenuCard
import app.tomoshibi.ui.kit.MenuItem
import app.tomoshibi.ui.kit.MenuRule
import app.tomoshibi.ui.kit.PageArrow
import app.tomoshibi.ui.kit.PageIndicator
import app.tomoshibi.ui.kit.PopoverAlignment
import app.tomoshibi.ui.kit.PopoverEdge
import app.tomoshibi.ui.kit.Popovers
import app.tomoshibi.ui.kit.SaidColumn
import app.tomoshibi.ui.kit.SaidLine
import app.tomoshibi.ui.kit.SignalBadge
import app.tomoshibi.ui.kit.ToastStack
import app.tomoshibi.ui.kit.glassFade
import app.tomoshibi.ui.kit.glassLayer
import app.tomoshibi.ui.kit.glassSource
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.rounded
import app.tomoshibi.ui.music.MusicPanel
import app.tomoshibi.ui.settings.DevicesPanel
import app.tomoshibi.ui.watch.WatchPanel
import app.tomoshibi.ui.watch.WatchStage
import app.tomoshibi.ui.watch.watchShowing
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallState
import uniffi.tomoshibi_core.LinkQuality
import uniffi.tomoshibi_protocol.ConnectionGrade
import uniffi.tomoshibi_protocol.ConnectionReading
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.NamedIdentity
import kotlin.math.max
import kotlin.math.min

/**
 * Work that must outlive the room that asked for it: leaving the call, and
 * ending it for everybody, which take the room away as they finish and would
 * be cancelled with it half done.
 */
internal val RoomWork = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

private const val ME = "me/camera"
private const val MY_SHARE = "me/screen"
/** The shared video, when a show is running: it takes the stage the way a shared screen does (Room.tsx). */
private const val WATCH_STAGE = "watch/stage"
/** The popover a long press on a picture, or on the room between them, opens. */
internal const val MENU = "picture"

/** One picture in the room, and what it is of. */
private data class Picture(val id: String, val role: Role) {
    sealed interface Role {
        data object Watch : Role
        data object Me : Role
        data object MyShare : Role
        data class Remote(val surface: Surface) : Role
    }
}

/** Where a long press opened the menu: on a picture, or on the room between the pictures (`picture` null). */
internal data class MenuAt(val picture: String?, val at: Offset)

/**
 * What the room remembers of itself between redraws: the open panel and
 * popover, what is on the stage and whether it fills the screen, the page,
 * and the alone-in-the-room capsule once closed (RoomView.swift's @State).
 * Kept with the call, so that the activity made again (the phone turned)
 * comes back to the room as it was rather than to a closed panel and a
 * stage somebody had chosen gone.
 */
@Stable
internal class RoomState {
    var panel by mutableStateOf<RoomPanel?>(null)
    /** The control bar's popovers and the pictures' menus: one open at a time. */
    val popovers = Popovers()
    /** The alone-in-the-room capsule, once closed, stays closed for the call. */
    var aloneDismissed by mutableStateOf(false)
    /** Which picture is on the stage (usePin.ts). */
    var pin by mutableStateOf(StagePin())
    /** Which page of the grid or the strip is showing (usePagination.ts): pictures on other pages are not drawn at all, so they cost nothing. */
    var page by mutableIntStateOf(0)
    /** The stage given the whole screen (the web's "Fill the screen"). */
    var filled by mutableStateOf(false)
    val idle = IdleWatch()
    val presses = Presses()
    /** Where the corner's messages are drawn, in the window, while there are any: the notices rise above them where the two would meet. */
    var saidBounds by mutableStateOf<Rect?>(null)
    /** The system's bars and cut-outs as they were before the stage filled the screen, which the filled stage keeps its marks clear of. */
    var deviceInsets by mutableStateOf(Insets())
    var menu by mutableStateOf<MenuAt?>(null)

    companion object {
        private const val PART = "room"

        fun of(call: CallModel): RoomState = call.parts.getOrPut(PART) { RoomState() } as RoomState
    }
}

/** Insets in points, the same on either side whatever the reading direction, as SwiftUI's EdgeInsets are. */
internal data class Insets(val top: Float = 0f, val leading: Float = 0f, val bottom: Float = 0f, val trailing: Float = 0f) {
    val padding: PaddingValues get() = PaddingValues(start = leading.dp, top = top.dp, end = trailing.dp, bottom = bottom.dp)
}

/**
 * The call (RoomView.swift).
 *
 * Every picture is drawn once, keyed by its id, and only its frame changes as
 * the arrangement does (basics' Plan): putting somebody on the stage moves
 * their picture there rather than building a new one, so nothing resubscribes
 * and nothing goes black. The arrangement follows the web's Room.tsx:
 * whatever is pinned takes the stage with everybody else in a strip beneath
 * it, a shared screen pinning itself; two people are one filling the screen
 * and one in the corner; three or more are a grid of up to nine, your own
 * picture on every page.
 *
 * The room is drawn edge to edge and keeps itself inside the system's bars
 * and cut-outs, and inside the keyboard while one is up, as the iPhone's
 * safe area does; filled, the stage takes the whole glass, under the bars,
 * which are hidden while it does. Back closes what is open, then leaves the
 * filled stage, then opens the question of leaving, never leaving the call
 * by itself: a call left by a swipe meant for something else is a call
 * somebody has to find their way back into.
 */
@Composable
fun RoomView(app: AppModel, call: CallModel, modifier: Modifier = Modifier) {
    val state = remember(call) { RoomState.of(call) }
    val host = remember(call) { HostFeature.of(call) }
    val record = remember(call) { StageRecord.of(call) }
    GlassBackdrop {
        FloatingPopoverLayer(state.popovers) {
            if (inPictureInPicture()) {
                SmallWindow(call, state, modifier)
            } else {
                Room(app, call, state, host, record, modifier)
            }
        }
    }
}

@Composable
private fun Room(app: AppModel, call: CallModel, state: RoomState, host: HostFeature, record: StageRecord, modifier: Modifier) {
    val density = LocalDensity.current
    val direction = LocalLayoutDirection.current
    val safe = WindowInsets.safeDrawing.asPaddingValues(density)
    val now = Insets(
        top = safe.calculateTopPadding().value,
        leading = safe.calculateLeftPadding(direction).value.let { if (direction == LayoutDirection.Ltr) it else safe.calculateRightPadding(direction).value },
        bottom = safe.calculateBottomPadding().value,
        trailing = safe.calculateRightPadding(direction).value.let { if (direction == LayoutDirection.Ltr) it else safe.calculateLeftPadding(direction).value },
    )
    val filled = state.filled
    SideEffect { if (!filled && state.deviceInsets != now) state.deviceInsets = now }
    // Inside the safe area, except filled: the insets as they were before it
    // reached past them, which the stage's marks and the bar keep clear of.
    val frame = if (filled) Insets() else now
    val inset = if (filled) state.deviceInsets else Insets()
    val popovers = state.popovers

    HidesSystemBars(filled)
    BackHandler {
        when {
            // A popover's own window takes Back first where the system
            // hands Back to the window in front; where it hands it to the
            // activity, the popover is still what Back closes.
            popovers.open != null -> popovers.close()
            state.panel != null -> state.panel = null
            filled -> state.filled = false
            else -> popovers.show(LEAVE)
        }
    }

    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(Theme.background)
            // Anybody there brings the controls back: a touch anywhere, seen
            // before anything under it takes it, as the web listens on the
            // window with capture.
            .pointerInput(state) {
                awaitPointerEventScope {
                    while (true) {
                        awaitPointerEvent(PointerEventPass.Initial)
                        state.idle.poke()
                    }
                }
            },
    ) {
        val whole = Size(max(0f, maxWidth.value - frame.leading - frame.trailing), max(0f, maxHeight.value - frame.top - frame.bottom))
        val placement = Appearance.shared.controlsPlacement
        // Down the side only where the screen is tall enough to hold a
        // column (useRoomFor.ts); a shape that does not fit is not the thing
        // somebody chose.
        val side = placement == ControlsPlacement.Side && whole.height >= Plan.SIDE_NEEDS
        // A lane for the controls, and the reason nothing else has to know
        // about them: the room is narrower by it, so the pictures, the panels
        // and the cards on the right all keep out of it (Room.tsx `aside`).
        // Not while the stage fills the screen, which is the whole screen.
        val lane = if (side && !filled) Plan.SIDE_LANE else 0f
        val size = Size(max(0f, whole.width - lane), whole.height)
        val island = if (side) Plan.GAP else Plan.ISLAND
        val pictures = pictures(call, record)
        val room = Plan.room(size, pictures.map { it.id }, ME, state.pin.pinned, filled, state.page, island)
        // The controls may step aside when the setting says so, and whenever
        // the stage fills the screen, and come back on any sign of somebody:
        // the same rule for both, because fullscreen used to hide them
        // outright and nothing brought them back (ControlBar.tsx `hiding`).
        val hides = placement == ControlsPlacement.Idle || filled
        val shown = !hides || !state.idle.idle || popovers.open != null

        val shares = (if (watchShowing(call)) listOf(WATCH_STAGE) else emptyList()) +
            call.surfaces.filter { it.kind == Surface.Kind.Screen }.map { it.id }
        val ids = pictures.map { it.id }
        LaunchedEffect(ids, shares) { state.pin = state.pin.update(ids, shares) }
        // Taking the last picture off the stage leaves nothing to fill the
        // screen with, and a screen still filled by nothing is one somebody
        // has to find the way out of: with the controls hidden, there was
        // none (Room.tsx `exit`).
        LaunchedEffect(state.pin.pinned) { if (state.pin.pinned == null && state.filled) state.filled = false }
        // The sharer whose screen is on this participant's stage, or "" (the
        // web's `watched`): written to the `watching` attribute so a sharer
        // can see who is looking at their screen. Only a share actually on
        // the stage counts; one sitting in the strip as a card is a thing
        // nobody has opened.
        val stageOwner = call.surfaces.firstOrNull { it.kind == Surface.Kind.Screen && it.id == state.pin.pinned }?.person?.id ?: ""
        LaunchedEffect(stageOwner) { runCatching { call.setWatching(stageOwner) } }
        LaunchedEffect(hides) { if (hides) state.idle.run() }
        LaunchedEffect(state.panel) { call.chatOpen = state.panel == RoomPanel.Messages }

        Box(Modifier.padding(frame.padding).size(whole.width.dp, whole.height.dp)) {
            Box(Modifier.size(size.width.dp, size.height.dp)) {
                Content(app, call, state, host, record, pictures, room, size, island, shown, inset, barAtFoot = !side)
            }
            // Along the foot, or down the side for "At the side", sliding
            // from its edge as the placement changes.
            val shift by animateDpAsState(if (shown) 0.dp else 110.dp, if (shown) Theme.settle() else Theme.leave(), label = "controls")
            val seen by animateFloatAsState(if (shown) 1f else 0f, if (shown) Theme.settle() else Theme.leave(), label = "seen")
            val offers = offers(call, host)
            AnimatedVisibility(
                !side,
                Modifier.align(Alignment.BottomCenter),
                enter = slideInVertically(Theme.settle()) { it },
                exit = slideOutVertically(Theme.leave()) { it },
            ) {
                ControlBar(
                    app, call, state.panel, { state.panel = it }, offers, popovers, BarAxis.Horizontal, whole.width.dp,
                    Modifier
                        .padding(bottom = (20 + inset.bottom).dp)
                        .offset(y = shift)
                        .glassLayer({ seen })
                        .then(if (shown) Modifier else Modifier.clearAndSetSemantics {}),
                )
            }
            AnimatedVisibility(
                side,
                Modifier.align(Alignment.CenterEnd),
                enter = slideInHorizontally(Theme.settle()) { it },
                exit = slideOutHorizontally(Theme.leave()) { it },
            ) {
                ControlBar(
                    app, call, state.panel, { state.panel = it }, offers, popovers, BarAxis.Vertical, whole.height.dp,
                    Modifier
                        .padding(end = (12 + inset.trailing).dp)
                        .offset(x = shift)
                        .glassLayer({ seen })
                        .then(if (shown) Modifier else Modifier.clearAndSetSemantics {}),
                )
            }
        }
        // Over everything, the controls included: drawn under them, the bar
        // stayed pressable through "you will be back in a moment".
        val movingTo = host.movingTo
        AnimatedVisibility(movingTo != null, enter = Theme.arrive, exit = Theme.depart) {
            MovingOverlay(movingTo ?: "")
        }
    }
}

/** What the features offer the bar (ControlBar.swift `Offers`), read here so the bar does not hold the features. */
private fun offers(call: CallModel, host: HostFeature): Offers {
    val watch = call.parts[BarOffer.WATCH] as? BarOffer
    val music = call.parts[BarOffer.MUSIC] as? BarOffer
    val sendingMusic = call.people.any { it.musicTrack != null }
    return Offers(
        watch = watch?.offered ?: watchShowing(call),
        watchPlaying = watch?.playing ?: false,
        music = music?.offered ?: sendingMusic,
        musicPlaying = music?.playing ?: sendingMusic,
        host = host.standing.yours,
        knocking = host.waiting.size,
    )
}

/**
 * Every picture, in the order the web lists them (surface.ts `surfaces`):
 * the video watched together first, then each person's camera and screen
 * together, this participant's own among them.
 *
 * Once there are more people than a page holds, whoever spoke last comes
 * forward (useSpeakingOrder.ts): somebody talking from the second page is
 * somebody nobody can see. Below that the order holds still, because a grid
 * that rearranges itself whenever somebody speaks is worse than one that
 * does not.
 */
private fun pictures(call: CallModel, record: StageRecord): List<Picture> {
    val out = ArrayList<Picture>()
    if (watchShowing(call)) out += Picture(WATCH_STAGE, Picture.Role.Watch)
    out += Picture(ME, Picture.Role.Me)
    if (mySharePicture(call)) out += Picture(MY_SHARE, Picture.Role.MyShare)
    val crowded = call.people.size + 1 > Plan.TILES_PER_PAGE
    val order = if (crowded) record.speaking.order(call.identities) else call.identities
    val rank = HashMap<String, Int>()
    order.forEachIndexed { index, id -> rank.putIfAbsent(id, index) }
    out += call.surfaces.withIndex()
        .sortedWith(compareBy<IndexedValue<Surface>> { rank[it.value.person.id] ?: 0 }.thenBy { it.index })
        .map { Picture(it.value.id, Picture.Role.Remote(it.value)) }
    return out
}

/**
 * This participant is sharing a screen: the web lists a screen surface
 * whenever the screen's track is there (surface.ts ~91), whatever else is
 * being shared beside it. `active` is the screen alone and `listening` sound
 * shared on its own, and the two run at once.
 */
private fun mySharePicture(call: CallModel): Boolean = call.sharing.active

/** Before the first sample, the server's own verdict is all there is. */
private fun unmeasured(quality: LinkQuality): ConnectionReading = ConnectionReading(
    grade = when (quality) {
        LinkQuality.EXCELLENT, LinkQuality.GOOD -> ConnectionGrade.GOOD
        LinkQuality.POOR -> ConnectionGrade.POOR
        LinkQuality.LOST -> ConnectionGrade.LOST
    },
    rttMs = null, lossPercent = null, upKbps = null, downKbps = null, jitterMs = null,
    share = null, ownAddress = null, holding = null, measured = false,
)

/**
 * Beside the call where the screen has room for it, and on a screen that has
 * none (a phone held upright) the width of the screen inside the same twelve
 * points on either side (RoomView.swift `panelWidth`).
 */
internal fun panelWidth(size: Size): Float {
    val beside = min(360f, size.width - 32)
    return if (size.width < beside + 24 + 172 + 22) max(0f, size.width - 24) else beside
}

// The room

@Composable
private fun Content(
    app: AppModel,
    call: CallModel,
    state: RoomState,
    host: HostFeature,
    record: StageRecord,
    pictures: List<Picture>,
    room: Plan.Room,
    size: Size,
    island: Float,
    shown: Boolean,
    inset: Insets,
    barAtFoot: Boolean,
) {
    val filled = state.filled
    val panel = state.panel
    val myShare = mySharePicture(call)
    val narrow = size.width < panelWidth(size) + 24 + 172 + 22
    // The list of who is watching moves beside an open panel rather than
    // going behind it, and on a screen with no room beside one it shrinks to
    // the eye and the count in the row at the top, which the panel then
    // starts below.
    val watchersInRow = myShare && panel != null && narrow
    val suspects = suspects(call)

    fun fill() {
        if (state.pin.pinned == null) return
        state.filled = true
        state.idle.poke()
    }
    fun unfill() {
        state.filled = false
    }
    fun select(id: String) {
        // While the stage fills the screen a press only brings the controls
        // back. On the web it took the picture off the stage, and so out of
        // the full screen with it, which no video player does with a tap.
        if (state.filled) {
            state.idle.poke()
            return
        }
        state.pin = state.pin.toggle(id)
    }
    fun expand(id: String) {
        if (state.filled) {
            unfill()
            return
        }
        state.pin = state.pin.pin(id)
        fill()
    }
    // A press on a picture, or on the space between them (null). One press
    // puts the picture on the stage or takes it off, at once; a second within
    // the double-tap interval fills the screen with it (Tile.tsx `onSelect`,
    // `onExpand`). The second press counts for the picture the first one was
    // on wherever it lands, because the first one moved that picture: from
    // the stage back to the grid, the finger is over somebody else by the
    // time the second arrives, and it pinned them.
    fun pressed(id: String?) {
        val now = SystemClock.uptimeMillis()
        val last = state.presses.last
        if (last != null && now - last.second < Presses.INTERVAL_MILLIS) {
            state.presses.last = null
            expand(last.first)
            return
        }
        if (id == null) {
            state.presses.last = null
            return
        }
        state.presses.last = id to now
        select(id)
    }
    fun openMenu(picture: String?, at: Offset) {
        state.menu = MenuAt(picture, at)
        state.popovers.show(MENU)
    }

    Box(Modifier.fillMaxSize()) {
        // The space between the pictures: a press there is the second half
        // of a double press that moved its picture away from under the
        // finger, and its long press carries the room's link (RoomMenu.tsx).
        // The ground is what the glass over the room refracts; the pictures
        // are drawn on surfaces of their own, which a blur cannot sample.
        Box(
            Modifier
                .zIndex(-3f)
                .fillMaxSize()
                .background(Theme.background)
                .glassSource(0f)
                .pointerInput(record) {
                    detectTapGestures(
                        onTap = { pressed(null) },
                        onLongPress = { at -> if (record.linkWorks) openMenu(null, at) },
                    )
                },
        )

        Tiles(app, call, state, host, record, pictures, room, size, shown, inset, barAtFoot, suspects, ::pressed, ::openMenu, ::fill, ::unfill)

        CallOverlay(host)

        Pager(state, room, size, island)

        GlassFade(!filled || shown) {
            Row(
                Modifier.padding(start = (14 + inset.leading).dp, top = (12 + inset.top).dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.Top,
            ) {
                RoomTitle(call)
                // Only once connected, as the web draws it: a reading of a
                // connection that is not there yet is a reading of nothing.
                val badge = Appearance.shared.connectionBadge
                GlassAppear(badge != ConnectionBadge.Hidden && call.state == CallState.CONNECTED) {
                    SignalBadge(
                        call.connection ?: unmeasured(call.localQuality),
                        detailed = badge == ConnectionBadge.Detailed,
                        open = call.connectionOpen,
                        onOpenChange = { call.connectionOpen = it },
                        relay = call.relay,
                        holding = call.holding,
                    )
                }
                GlassAppear(watchersInRow) { WatchersCard(call, compact = true) }
            }
        }

        val alone = call.people.isEmpty() && state.pin.pinned == null && !state.aloneDismissed && !filled &&
            call.state == CallState.CONNECTED && record.joining != null
        Box(
            Modifier
                .size(size.width.dp, max(0f, size.height - island - 2).dp)
                .padding(horizontal = 16.dp),
            contentAlignment = Alignment.BottomCenter,
        ) {
            GlassAppear(alone) {
                EmptyRoomCard(
                    call, record.linkWorks, host.standing.yours, record.link,
                    recheck = { host.refreshStanding() },
                    dismiss = { state.aloneDismissed = true },
                )
            }
        }

        val notice = call.notice
        Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.TopCenter) {
            GlassAppear(notice != null) { Notice(notice ?: "") }
        }
        Box(Modifier.fillMaxWidth().padding(top = 12.dp), contentAlignment = Alignment.TopCenter) {
            GlassAppear(call.state != CallState.CONNECTED) {
                Notice(if (call.state == CallState.RECONNECTING) t("Reconnecting…") else t("Connecting…"))
            }
        }

        // Who has our share open, to the right of the pictures and below the
        // stage's corner (Room.tsx: top-16 right-3), beside a panel when one
        // is open.
        val watchersTop by animateDpAsState(if (panel == null) 58.dp else 12.dp, Theme.settle(), label = "watchers")
        val watchersEnd by animateDpAsState(if (panel == null) 14.dp else (panelWidth(size) + 24 + 8).dp, Theme.settle(), label = "watchers")
        Box(Modifier.align(Alignment.TopEnd).padding(top = watchersTop, end = watchersEnd)) {
            GlassAppear(myShare && !watchersInRow && (!filled || shown)) { WatchersCard(call) }
        }

        Panel(app, call, state, host, size, narrow, inset)

        // Always there, so each card leaves with its own animation.
        SaidColumn(
            call.arriving.map { SaidLine(it.id, it.identity, it.name, it.text) },
            Modifier
                .align(Alignment.BottomStart)
                .widthIn(max = min(320f, size.width - 32).dp)
                .padding(start = 12.dp, bottom = (Plan.ISLAND + 8).dp),
            onBounds = { state.saidBounds = it },
        )

        // The corner for what the call says, drawn after the panels so a tall
        // panel never covers them: beside an open panel where the screen
        // leaves room for both, over it where it does not (a phone), since a
        // toast is brief and a panel is not.
        val beside = panel != null && size.width - panelWidth(size) - 48 >= 280
        val toastsEnd by animateDpAsState(if (beside) (panelWidth(size) + 24 + 8).dp else 16.dp, Theme.settle(), label = "toasts")
        ToastStack(
            call,
            Modifier.align(Alignment.BottomEnd).padding(end = toastsEnd, bottom = (Plan.ISLAND + 8).dp),
            saidBounds = state.saidBounds,
        )

        Menu(call, state, host, record, pictures, ::fill, ::unfill)
    }
}

/** Whoever is wearing a name somebody else in the room proved, worked out here rather than in a tile, because a tile can only see itself (Room.tsx ~402-406). */
private fun suspects(call: CallModel): Set<String> {
    val roster = listOf(NamedIdentity(call.localName, call.localIdentity)) + call.people.map { NamedIdentity(it.name, it.id) }
    return roster.filter { CrateRules.impersonating(it, roster) }.map { it.identity }.toSet()
}

/** Where the room's name and the signal sit over the pictures, with the signal's figures when they are open. */
private fun furniture(call: CallModel): Rect {
    val open = Appearance.shared.connectionBadge == ConnectionBadge.Detailed && call.connectionOpen
    return Rect(0f, 0f, if (open) 380f else 340f, if (open) 330f else 50f)
}

// Pictures

@Composable
private fun Tiles(
    app: AppModel,
    call: CallModel,
    state: RoomState,
    host: HostFeature,
    record: StageRecord,
    pictures: List<Picture>,
    room: Plan.Room,
    size: Size,
    shown: Boolean,
    inset: Insets,
    barAtFoot: Boolean,
    suspects: Set<String>,
    pressed: (String?) -> Unit,
    openMenu: (String?, Offset) -> Unit,
    fill: () -> Unit,
    unfill: () -> Unit,
) {
    // Where each picture was last placed, so that one leaving fades where it
    // was rather than at the corner.
    val placedAt = remember { HashMap<String, Rect>() }
    val placed = pictures.filter { it.id in room.spots }
    for ((id, spot) in room.spots) placedAt[id] = spot
    for (entry in keepDeparting(placed) { it.id }) {
        val picture = entry.item
        key(picture.id) {
            val target = room.spots[picture.id] ?: placedAt[picture.id] ?: Rect.Zero
            val spot by animateValueAsState(target, Rect.VectorConverter, Theme.settle(), label = "spot")
            AnimatedVisibility(
                entry.visible,
                Modifier
                    .offset(spot.left.dp, spot.top.dp)
                    .size(spot.width.dp, spot.height.dp)
                    // Above the room and under everything laid over it, and
                    // the larger picture under the smaller: in a call of two
                    // the card in the corner sits over the person filling
                    // the screen.
                    .zIndex(-1f - spot.width * spot.height / 1e8f),
                enter = Theme.arrive,
                exit = Theme.depart,
            ) {
                Tile(app, call, state, host, record, picture, spot, room, size, shown, inset, barAtFoot, suspects, pressed, openMenu, fill, unfill)
            }
        }
    }
}

@Composable
private fun Tile(
    app: AppModel,
    call: CallModel,
    state: RoomState,
    host: HostFeature,
    record: StageRecord,
    picture: Picture,
    spot: Rect,
    room: Plan.Room,
    size: Size,
    shown: Boolean,
    inset: Insets,
    barAtFoot: Boolean,
    suspects: Set<String>,
    pressed: (String?) -> Unit,
    openMenu: (String?, Offset) -> Unit,
    fill: () -> Unit,
    unfill: () -> Unit,
) {
    val filled = state.filled
    val onStage = picture.id == state.pin.pinned
    val compact = spot.height < 140
    // The stage's own controls: on a touch screen whenever the bar is shown,
    // since there is no pointer to hover.
    val revealed = onStage && shown
    val radius: Dp = if (filled && onStage) 0.dp else Theme.tileRadius
    val ground = if (filled && onStage) Color.Black else Theme.surface
    // And above the controls while they are out, where the bar is wide
    // enough to reach the name at the foot of a picture that runs under it,
    // as it is on a phone: the name slides up with the bar and back down
    // after it. Plan.swift's room lifts it only for a filled stage; the
    // person filling a call of two runs under the bar as well, and their
    // name was drawn half behind it, on the iPhone as here.
    val underBar = barAtFoot && spot.bottom > size.height - Plan.ISLAND + 0.5f
    val lift by animateFloatAsState(
        if (underBar && shown && spot.width < 900) Plan.ISLAND - Plan.GAP else 0f,
        if (shown) Theme.settle() else Theme.leave(), label = "lift",
    )
    val edge = if (filled && onStage) {
        PaddingValues(start = inset.leading.dp, top = inset.top.dp, end = inset.trailing.dp, bottom = (inset.bottom + lift).dp)
    } else {
        PaddingValues(bottom = lift.dp)
    }
    // The card in the corner of a call of two.
    val floating = state.pin.pinned == null && room.spots.size == Plan.ALONE_TOGETHER && picture.id == ME
    // The top left of a picture there is under the room's name and the
    // signal, so a hand raised there was drawn behind them; it goes beside
    // the name instead.
    val cornered = furniture(call).overlaps(Rect(spot.left, spot.top, spot.left + 44, spot.top + 44))
    val press = { pressed(picture.id) }
    // The finger's place in the room, in pixels, for the menu to open
    // beside it: the tile says where in itself it was pressed.
    val density = LocalDensity.current.density
    val menu = { at: Offset -> openMenu(picture.id, Offset(spot.left * density + at.x, spot.top * density + at.y)) }
    val controls: @Composable androidx.compose.foundation.layout.BoxScope.(Boolean) -> Unit = {
        StageControlsOver(call, state, picture, revealed, edge, fill, unfill)
    }
    when (val role = picture.role) {
        Picture.Role.Watch -> {
            TileFrame(TileMarks("", unlabelled = true), press, Modifier.fillMaxSize(), compact = compact, selected = onStage,
                expandable = false, radius = radius, ground = Color.Black, edge = edge,
                content = { WatchStage(app, call, Modifier.fillMaxSize()) }, overlay = controls)
        }
        Picture.Role.Me -> {
            val speaking = steady(record.meSpeaking && call.microphoneOn)
            TileFrame(
                TileMarks(
                    t("{name} (you)", mapOf("name" to call.localName)),
                    proven = CrateRules.proven(call.localIdentity),
                    unverified = call.localIdentity in suspects,
                    speaking = speaking, muted = !call.microphoneOn, hand = call.handUp, handBesideName = cornered,
                ),
                press, Modifier.fillMaxSize(), menu = menu, compact = compact, selected = onStage,
                expandable = !onStage, radius = radius, floating = floating, ground = ground, edge = edge,
                content = { SelfPicture(call, compact) }, overlay = controls,
            )
        }
        Picture.Role.MyShare -> {
            TileFrame(
                TileMarks(t("{name} (you)", mapOf("name" to t("{name} (screen)", mapOf("name" to call.localName))))),
                press, Modifier.fillMaxSize(), menu = menu, compact = compact, selected = onStage,
                expandable = !onStage, radius = radius, ground = ground, edge = edge,
                content = { MyShareFace(compact) { call.scope.launch { runCatching { call.sharing.stopScreen() } } } },
                overlay = controls,
            )
        }
        is Picture.Role.Remote -> {
            val surface = role.surface
            val person = surface.person
            val name = person.name.ifEmpty { person.id }
            val screen = surface.kind == Surface.Kind.Screen
            val label = if (screen) t("{name} (screen)", mapOf("name" to name)) else name
            val quiet = host.setting(person, if (screen) Sound.Screen else Sound.Voice).silenced
            if (screen && !onStage) {
                TileFrame(
                    TileMarks(name, silenced = quiet), press, Modifier.fillMaxSize(), menu = menu,
                    compact = compact, selected = false, radius = radius,
                    content = { ShareCardFace(name, compact) },
                )
            } else {
                val camera = surface.kind == Surface.Kind.Camera
                val speaking = steady(camera && person.speaking)
                TileFrame(
                    TileMarks(
                        label,
                        proven = camera && CrateRules.proven(person.id),
                        unverified = camera && person.id in suspects,
                        speaking = speaking,
                        muted = camera && (person.track(MediaSource.MICROPHONE)?.muted ?: true),
                        silenced = quiet,
                        hand = person.handUp,
                        handBesideName = cornered,
                    ),
                    press, Modifier.fillMaxSize(), menu = menu, compact = compact, selected = onStage,
                    expandable = !onStage, radius = radius, ground = ground, edge = edge,
                    content = { SurfacePicture(surface, call, compact) }, overlay = controls,
                )
            }
        }
    }
}

/** The stage's own controls over the picture on it, arriving with the bar and leaving with it. */
@Composable
private fun androidx.compose.foundation.layout.BoxScope.StageControlsOver(
    call: CallModel,
    state: RoomState,
    picture: Picture,
    revealed: Boolean,
    edge: PaddingValues,
    fill: () -> Unit,
    unfill: () -> Unit,
) {
    val other = counterpart(call, picture)
    AnimatedVisibility(
        revealed,
        Modifier.align(Alignment.TopEnd).padding(edge).padding(12.dp),
        enter = Theme.arrive, exit = Theme.depart,
    ) {
        StageControls(
            other?.second, state.filled,
            switchTo = { other?.let { state.pin = state.pin.pin(it.first) } },
            fill = { if (state.filled) unfill() else fill() },
        )
    }
}

/**
 * The same person's other picture, which the switch on the stage offers
 * (Room.tsx `counterpart`): never the video watched together, which belongs
 * to whoever runs it but is not another picture of them.
 */
private fun counterpart(call: CallModel, picture: Picture): Pair<String, Surface.Kind>? = when (val role = picture.role) {
    Picture.Role.Watch -> null
    Picture.Role.Me -> if (mySharePicture(call)) MY_SHARE to Surface.Kind.Screen else null
    Picture.Role.MyShare -> ME to Surface.Kind.Camera
    is Picture.Role.Remote -> call.surfaces.firstOrNull { it.person.id == role.surface.person.id && it.id != role.surface.id }?.let { it.id to it.kind }
}

// Pages

/**
 * The paging arrows, beside the strip when there is a stage, where the
 * pictures being paged are, either side of the grid otherwise; and which page
 * is showing, at the top, below the row there where the two would meet.
 */
@Composable
private fun androidx.compose.foundation.layout.BoxScope.Pager(state: RoomState, room: Plan.Room, size: Size, island: Float) {
    val paging = room.pages > 1 && !state.filled
    val strip = if (state.pin.pinned != null) room.shown.firstNotNullOfOrNull { room.spots[it]?.center?.y } else null
    val middle = strip ?: ((size.height - island) / 2)
    Row(
        Modifier
            .width(size.width.dp)
            .offset(y = (middle - 19).dp)
            .padding(horizontal = 10.dp),
    ) {
        GlassAppear(paging) {
            PageArrow("chevron.left", t("Previous page"), { state.page = (room.page - 1 + room.pages) % room.pages })
        }
        Spacer(Modifier.weight(1f))
        GlassAppear(paging) {
            PageArrow("chevron.right", t("Next page"), { state.page = (room.page + 1) % room.pages })
        }
    }
    Box(Modifier.align(Alignment.TopCenter).padding(top = if (size.width < 560) 52.dp else 15.dp)) {
        GlassAppear(paging) { PageIndicator(room.page, room.pages) }
    }
}

// Panels

@Composable
private fun androidx.compose.foundation.layout.BoxScope.Panel(
    app: AppModel,
    call: CallModel,
    state: RoomState,
    host: HostFeature,
    size: Size,
    narrow: Boolean,
    inset: Insets,
) {
    val width = panelWidth(size)
    // Never taller than the space above the controls: a panel that grows
    // past it covers the buttons that close it.
    val tallest = max(160f, size.height - Plan.ISLAND - 24 - (if (narrow) 38 else 0))
    AnimatedContent(
        state.panel,
        Modifier
            .align(Alignment.TopEnd)
            .padding(12.dp)
            // Below the row at the top wherever it cannot stand beside it:
            // over it, the panel hid the room's name and its signal.
            .padding(top = ((if (narrow) 38 else 0) + inset.top).dp),
        transitionSpec = { Theme.slideIn togetherWith Theme.slideOut },
        contentAlignment = Alignment.TopEnd,
        label = "panel",
    ) { open ->
        if (open == null) {
            Box(Modifier.size(0.dp))
            return@AnimatedContent
        }
        val close = { state.panel = null }
        Column(
            Modifier
                .width(width.dp)
                .heightIn(max = tallest.dp)
                // Where it covers the call rather than standing beside it,
                // dark under its glass: through the glass alone the edges of
                // the pictures beneath ran across the panel.
                .clip(rounded(22f))
                .background(Theme.background.copy(alpha = if (narrow) 0.72f else 0f), rounded(22f))
                .verticalScroll(rememberScrollState()),
        ) {
            when (open) {
                RoomPanel.Messages -> ChatPanel(call, close)
                RoomPanel.Sound -> SoundPanel(host, close)
                RoomPanel.Watch -> WatchPanel(app, call, close)
                RoomPanel.Music -> MusicPanel(app, call, close)
                RoomPanel.Host -> HostPanel(host, close)
                RoomPanel.Devices -> DevicesPanel(app, call, close)
            }
        }
    }
}

// The menus a long press opens

/**
 * What can be done to one picture, on the gesture that has always meant that
 * (PictureMenu.tsx; RoomView.swift `pictureMenu`), or, on the space between
 * the pictures, the room's own: its link (RoomMenu.tsx). Everything here is
 * reachable another way, and that is the point: a double press nobody finds
 * is a feature nobody has, and a menu says the names out loud. Each item
 * names the picture, since a menu opened on a small tile in a busy grid has
 * to say which one it caught. It opens beside the finger.
 */
@Composable
private fun Menu(
    call: CallModel,
    state: RoomState,
    host: HostFeature,
    record: StageRecord,
    pictures: List<Picture>,
    fill: () -> Unit,
    unfill: () -> Unit,
) {
    val context = LocalContext.current
    // The last place a menu was opened, kept after it closes so that the
    // menu leaves with its own departure rather than with the place.
    val at = state.menu ?: return
    val popovers = state.popovers
    FloatingPopover(
        MENU, popovers,
        Modifier.offset(at.at.x.toDp(), at.at.y.toDp()),
        edge = PopoverEdge.Bottom,
        alignment = PopoverAlignment.Leading,
        width = 260.dp,
        panel = {
            MenuCard {
                val picture = pictures.firstOrNull { it.id == at.picture }
                fun done() = popovers.close()
                if (picture != null) {
                    val onStage = picture.id == state.pin.pinned
                    val person: Person? = (picture.role as? Picture.Role.Remote)?.surface?.person
                    val screen = (picture.role as? Picture.Role.Remote)?.surface?.kind == Surface.Kind.Screen || picture.role == Picture.Role.MyShare
                    val label = when (val role = picture.role) {
                        Picture.Role.Watch -> t("Watch together")
                        Picture.Role.Me -> call.localName
                        Picture.Role.MyShare -> t("{name} (screen)", mapOf("name" to call.localName))
                        is Picture.Role.Remote -> role.surface.person.name.ifEmpty { role.surface.person.id }.let {
                            if (screen) t("{name} (screen)", mapOf("name" to it)) else it
                        }
                    }
                    MenuItem(
                        if (onStage) t("Show everybody") else t("Show {name} larger", mapOf("name" to label)),
                        {
                            done()
                            state.pin = state.pin.toggle(picture.id)
                        },
                        symbol = if (onStage) "arrow.down.right.and.arrow.up.left" else "arrow.up.left.and.arrow.down.right",
                    )
                    // Only for the picture already on the stage, which is
                    // what fills the screen: anywhere else it would have to
                    // be put there first, and the press would do two things
                    // under one name.
                    if (onStage) {
                        MenuItem(
                            if (state.filled) t("Leave fullscreen") else t("Fill the screen"),
                            {
                                done()
                                if (state.filled) unfill() else fill()
                            },
                            symbol = if (state.filled) "arrow.down.right.and.arrow.up.left.rectangle" else "arrow.up.left.and.arrow.down.right.rectangle",
                        )
                    }
                    // Nobody hears themselves, so nothing here for our own pictures.
                    if (person != null) {
                        MenuRule()
                        val sound = if (screen) Sound.Screen else Sound.Voice
                        val blocked = host.setting(person, sound).blocked
                        MenuItem(
                            if (blocked) t("Unmute {name}", mapOf("name" to label)) else t("Mute {name}", mapOf("name" to label)),
                            {
                                done()
                                host.setBlocked(person, sound, !blocked)
                            },
                            symbol = if (blocked) "speaker.wave.2.fill" else "speaker.slash.fill",
                        )
                        MenuItem(t("Sound settings"), {
                            done()
                            state.panel = RoomPanel.Sound
                        }, symbol = "slider.horizontal.3")
                    }
                    // Only a signature somebody earned: an issued one is fresh
                    // each visit, and a copy of it is true for this call and
                    // misleading afterwards.
                    val identity = person?.id ?: call.localIdentity
                    val signature = if (picture.role == Picture.Role.Watch) null else CrateRules.signature(identity)
                    if (signature != null && signature.proven) {
                        MenuRule()
                        MenuItem(t("Copy signature"), {
                            done()
                            Pasteboard.copy(context, signature.trip)
                        }, symbol = "signature")
                    }
                }
                // The room's own, at the foot of every picture's: two people
                // are one picture filling the frame and another in its
                // corner, so the space around the pictures, where the room's
                // menu lives, is not there in the commonest call anybody holds.
                if (record.linkWorks) {
                    if (picture != null) MenuRule()
                    MenuItem(t("Copy link"), {
                        done()
                        Pasteboard.copy(context, record.link)
                    }, symbol = "link")
                }
            }
        },
    ) {
        Box(Modifier.size(1.dp))
    }
}

/**
 * Glass faded in and out, and nothing else, as the row at the top of the
 * room comes and goes with the controls (`.transition(.opacity)`): faded the
 * way GlassAppear fades, on each thing drawn, since a fade of the whole row
 * cut every shadow in it off square while it lasted.
 */
@Composable
private fun GlassFade(visible: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(visible, enter = EnterTransition.None, exit = ExitTransition.None) {
        val fade by glassFade()
        Box(Modifier.glassLayer({ fade })) { content() }
    }
}

@Composable
private fun Float.toDp(): Dp = with(LocalDensity.current) { this@toDp.toDp() }

// The system around the room

/**
 * Filled on a phone means the glass edge to edge, with the clock and the
 * navigation hidden while it is, as the iPhone hides its status bar and its
 * home indicator; a swipe from the edge brings them back for a moment.
 */
@Composable
private fun HidesSystemBars(filled: Boolean) {
    val view = LocalView.current
    DisposableEffect(filled, view) {
        val window = view.context.activity()?.window
        val controller = window?.let { WindowCompat.getInsetsController(it, view) }
        if (filled) {
            controller?.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller?.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            controller?.show(WindowInsetsCompat.Type.systemBars())
        }
        onDispose { if (filled) controller?.show(WindowInsetsCompat.Type.systemBars()) }
    }
}

private fun Context.activity(): Activity? {
    var context: Context? = this
    while (context is ContextWrapper) {
        if (context is Activity) return context
        context = context.baseContext
    }
    return null
}

/**
 * Whether the activity is in the small window.
 *
 * Hook for the media engineer's PictureInPicture (media/PictureInPicture.kt),
 * which says only whether the window can be opened and which picture it
 * would show, not whether it is open: until it does, the room asks the
 * activity itself, which says so whichever part of the app opened it.
 */
@Composable
private fun inPictureInPicture(): Boolean {
    val activity = LocalContext.current.activity() as? ComponentActivity ?: return false
    var inside by remember(activity) { mutableStateOf(activity.isInPictureInPictureMode) }
    DisposableEffect(activity) {
        val listener = Consumer<PictureInPictureModeChangedInfo> { inside = it.isInPictureInPictureMode }
        activity.addOnPictureInPictureModeChangedListener(listener)
        onDispose { activity.removeOnPictureInPictureModeChangedListener(listener) }
    }
    return inside
}

/**
 * The room in the small window: the stage alone, whole, with nothing to press
 * (PictureInPictureHost): whatever is on the stage, or the call's own choice
 * of picture (CallModel.pictureInPictureSid), a shared screen first.
 */
@Composable
private fun SmallWindow(call: CallModel, state: RoomState, modifier: Modifier) {
    val pictures = LocalRoomPictures.current
    val staged = call.surfaces.firstOrNull { it.id == state.pin.pinned && it.sid != null }
    val sid = staged?.sid ?: call.pictureInPictureSid
    val fills = (staged ?: call.surfaces.firstOrNull { it.sid == sid })?.kind == Surface.Kind.Camera
    Box(modifier.fillMaxSize().background(Color.Black), contentAlignment = Alignment.Center) {
        AnimatedContent(sid, transitionSpec = { fadeIn(Theme.settle()) togetherWith fadeOut(Theme.leave()) }, label = "small") { shown ->
            if (shown != null) {
                pictures.remote(call, shown, fills, Modifier.fillMaxSize())
            } else {
                val other = call.people.firstOrNull()
                Avatar(other?.name ?: call.localName, other?.id ?: call.localIdentity)
            }
        }
    }
}
