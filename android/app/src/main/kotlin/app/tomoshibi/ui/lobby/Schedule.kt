package app.tomoshibi.ui.lobby

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.provider.CalendarContract
import android.text.format.DateFormat
import android.util.Log
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.Clipboard
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.ScheduleForm
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.readingLocale
import app.tomoshibi.basics.t
import app.tomoshibi.model.AppModel
import app.tomoshibi.model.LobbyModel
import app.tomoshibi.ui.LocalAppScope
import app.tomoshibi.ui.kit.ClockChoice
import app.tomoshibi.ui.kit.ContentLayer
import app.tomoshibi.ui.kit.FloatingPopoverLayer
import app.tomoshibi.ui.kit.Font
import app.tomoshibi.ui.kit.GlassButton
import app.tomoshibi.ui.kit.GlassButtonShape
import app.tomoshibi.ui.kit.GlassField
import app.tomoshibi.ui.kit.KitText
import app.tomoshibi.ui.kit.MediaSpinner
import app.tomoshibi.ui.kit.MonthCalendar
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.Capsule
import app.tomoshibi.ui.kit.Symbol
import app.tomoshibi.ui.kit.SwappingSymbol
import app.tomoshibi.ui.kit.glassPane
import app.tomoshibi.ui.kit.keepDeparting
import app.tomoshibi.ui.kit.pressable
import app.tomoshibi.ui.kit.putsKeyboardAway
import app.tomoshibi.ui.kit.rememberPopovers
import app.tomoshibi.ui.kit.rememberPress
import app.tomoshibi.ui.kit.fingertip
import app.tomoshibi.ui.kit.narrowWindow
import app.tomoshibi.ui.kit.rounded
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.Account
import uniffi.tomoshibi_protocol.Arrangement as Meeting
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.TimeZone

/** One arranged meeting as a row (`MeetingRow`). */
@Composable
fun MeetingRow(one: Meeting, app: AppModel, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier
                .size(34.dp)
                .background(if (one.started) Theme.tally.copy(alpha = 0.18f) else Theme.wash.copy(alpha = 0.08f), rounded(10f)),
            contentAlignment = Alignment.Center,
        ) { Symbol(if (one.started) "video.fill" else "calendar", size = 15f, tint = if (one.started) Theme.tallyInk else Theme.muted) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            KitText(one.room, font = Font(14f, Font.Semibold), lines = 1)
            // When, and where, on two lines where a phone or the cover screen
            // leaves it less than a line beside the row's buttons.
            KitText(meetingWhen(one, app), font = Font(12f), color = Theme.muted, lines = if (narrowWindow()) 2 else 1)
        }
        if (one.started) {
            KitText(
                t("Under way"),
                Modifier.background(Theme.tally, Capsule).padding(horizontal = 8.dp, vertical = 3.dp),
                font = Font(11f, Font.Semibold),
                color = Theme.tallyForeground,
                lines = 1,
            )
        }
    }
}

private fun meetingWhen(one: Meeting, app: AppModel): String {
    val date = AppModel.instant(one.at)?.let(::meetingSaid) ?: one.at
    val relay = one.relay
    return if (!relay.isNullOrEmpty()) "$date · ${app.lobby.label(relay)}" else date
}

/**
 * A meeting's time as a row has room for it (`MeetingRow.said`): this
 * year's go without the year, which with the zone cut the time short on a
 * phone ("Oct 18, 2026 at 10:00 AM G…").
 */
fun meetingSaid(at: Instant, zone: ZoneId = ZoneId.systemDefault()): String {
    val locale = readingLocale()
    if (at.atZone(zone).year != ZonedDateTime.now(zone).year) return LobbyModel.said(at, locale, zone)
    val pattern = DateFormat.getBestDateTimePattern(locale, "MMMdjmm")
    val timeZone = TimeZone.getTimeZone(zone)
    val daylight = timeZone.inDaylightTime(java.util.Date.from(at))
    return DateTimeFormatter.ofPattern(pattern, locale).withZone(zone).format(at) + " " + timeZone.getDisplayName(daylight, TimeZone.SHORT, locale)
}

/**
 * What the schedule holds while it is open: kept with the lobby, so that a
 * phone unfolded with its schedule open, which redraws it as a section of
 * the wide window, keeps the name, the day and the time.
 */
@Stable
class ScheduleDraft(now: Instant = Instant.now()) {
    var planning by mutableStateOf("")

    /**
     * The day and the time, and which of them somebody chose
     * ([ScheduleForm]): at the next quarter hour, its own day, and again at
     * every opening for what nobody chose ([open]).
     */
    var form by mutableStateOf(ScheduleForm.start(now, ZoneId.systemDefault()))
        private set

    /** What the calendar and the clock set: chosen by somebody. */
    var day: LocalDateTime
        get() = form.day
        set(value) { form = form.choosingDay(value) }
    var hour: Int
        get() = form.hour
        set(value) { form = form.choosingHour(value) }
    var minute: Int
        get() = form.minute
        set(value) { form = form.choosingMinute(value) }
    var arranging by mutableStateOf(false)
    var copied by mutableStateOf<String?>(null)
    var now by mutableStateOf(now)

    /** The schedule opened at [at] (LobbyState `place`). */
    fun open(at: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()) {
        form = form.opened(at, zone)
        now = at
    }

    /** A meeting arranged from it. */
    fun arranged() {
        form = form.arranged()
    }

    companion object {
        /** Sixty days, as far ahead as Schedule.tsx offers. */
        const val AHEAD_SECONDS = 60L * 24 * 60 * 60
    }
}

/**
 * Schedule.tsx: which name, which day, what time, which machine, and the
 * meetings already arranged with their links (`ScheduleView`). On a phone a
 * sheet of its own with a close; on a wide screen a section of the window,
 * the form beside the list where both fit.
 *
 * Drawn as the start and join sheet is, on the room's evening ground; the
 * time sits at the end of its row and the verdict has the card's width
 * above a button that has it too; the list is a card at the height of the
 * form's, empty or not; and what comes and goes does so with the app's own
 * arrival and leaving (the owner, 2026-10-17, on a page that had none of it).
 */
@Composable
fun ScheduleView(app: AppModel, account: Account, draft: ScheduleDraft, wide: Boolean, close: () -> Unit, modifier: Modifier = Modifier) {
    val popovers = rememberPopovers()
    OpenFromDebug(popovers)
    LaunchedEffect(Unit) {
        draft.now = Instant.now()
        app.lobby.refreshArrangements()
    }
    FloatingPopoverLayer(popovers) {
        ContentLayer {
            Box(modifier.fillMaxSize()) {
                // In a wide window it stands on the window's own backdrop,
                // which the sidebar floats over too: a second one drawn over it
                // began with a seam at the sidebar's edge.
                if (!wide) Backdrop(Modifier.matchParentSize().alpha(0.6f))
                if (wide) {
                    BoxWithConstraints(Modifier.fillMaxSize()) {
                        val sideBySide = maxWidth >= (360 + 20 + 420 + 56).dp
                        Column(
                            Modifier
                                .fillMaxSize()
                                .verticalScroll(rememberScrollState())
                                .windowInsetsPadding(WindowInsets.safeDrawing)
                                .padding(28.dp),
                            verticalArrangement = Arrangement.spacedBy(20.dp),
                        ) {
                            Heading(26f)
                            // Side by side where the list has room for a
                            // meeting's name, its time and its three buttons;
                            // narrower, it was squeezed to "…" beside the form,
                            // so it goes under it.
                            if (sideBySide) {
                                Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                                    Form(app, account, draft, Modifier.width(360.dp))
                                    MeetingList(app, draft, wide = true, Modifier.weight(1f).widthIn(min = 420.dp))
                                }
                            } else {
                                Form(app, account, draft, Modifier.widthIn(max = 560.dp))
                                MeetingList(app, draft, wide = true, Modifier.widthIn(max = 560.dp))
                            }
                        }
                    }
                } else {
                    Column(
                        Modifier
                            .fillMaxSize()
                            .putsKeyboardAway()
                            .verticalScroll(rememberScrollState())
                            .windowInsetsPadding(WindowInsets.safeDrawing)
                            .padding(horizontal = 16.dp)
                            .padding(bottom = 24.dp),
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        // Clear of the sheet's grabber, which it sat against.
                        Row(Modifier.padding(top = 22.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Heading(22f, Modifier.weight(1f))
                            GlassButton(close, shape = GlassButtonShape.Circle(32.dp), label = t("Close")) {
                                Symbol("xmark", size = 13f, weight = Font.Bold)
                            }
                        }
                        Form(app, account, draft, Modifier.fillMaxWidth())
                        MeetingList(app, draft, wide = false, Modifier.fillMaxWidth())
                    }
                }
            }
        }
    }
}

@Composable
private fun Heading(size: Float, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        KitText(t("Scheduled meetings"), font = Font(size, Font.Bold))
        KitText(t("Pick a time and hand out the link before it starts."), font = Font(if (size > 24) 14f else 13f), color = Theme.muted)
    }
}

/** A label at the start of one of the form's rows. */
@Composable
private fun RowLabel(text: String, symbol: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Symbol(symbol, size = 12f, weight = Font.Semibold, tint = Theme.muted)
        KitText(text, font = Font(13f), color = Theme.muted, lines = 1)
    }
}

private fun ScheduleDraft.at(): Instant = form.instant(ZoneId.systemDefault(), now)

private fun ScheduleDraft.problem(): Boolean {
    val at = at()
    return at.isBefore(now) || at.isAfter(now.plusSeconds(ScheduleDraft.AHEAD_SECONDS))
}

private fun ScheduleDraft.verdict(): String {
    val at = at()
    if (at.isBefore(now)) return t("That time has already gone.")
    if (at.isAfter(now.plusSeconds(ScheduleDraft.AHEAD_SECONDS))) return t("That is more than sixty days away.")
    return t("Arranged for {when}", mapOf("when" to LobbyModel.said(at)))
}

@Composable
private fun Form(app: AppModel, account: Account, draft: ScheduleDraft, modifier: Modifier) {
    val scope = LocalAppScope.current
    val clipboard = LocalClipboard.current
    val canArrange = account.can?.contains("arrange") ?: true
    Column(modifier.glassPane(24.dp).padding(18.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        if (!canArrange) {
            KitText(t("Arranging meetings has not been opened to you."), font = Font(13f), color = Theme.muted)
            return@Column
        }
        GlassField(t("Room name"), draft.planning, { draft.planning = it }, symbol = "number", mono = draft.planning.isNotEmpty(), height = 40.dp)
        KitText(t("Name it yourself, or leave it blank for one nobody has used."), font = Font(11.5f), color = Theme.muted)
        // The app's own month rather than the system's calendar: the chosen
        // day in the person's colour, the days past and beyond sixty dimmed
        // (Schedule.tsx offers no further ahead).
        val today = LocalDateTime.now()
        MonthCalendar(
            draft.day,
            { draft.day = it },
            Modifier.background(Theme.recess(0.18f), rounded(16f)).padding(12.dp),
            earliest = today,
            latest = today.plusDays(60),
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            RowLabel(t("Time"), "clock")
            Spacer(Modifier.weight(1f).widthIn(min = 8.dp))
            ClockChoice("schedule.clock", draft.hour, draft.minute, { draft.hour = it }, { draft.minute = it })
        }
        if (app.offersRelayChoice) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                RowLabel(t("Server"), "server.rack")
                Spacer(Modifier.weight(1f).widthIn(min = 8.dp))
                ServerMenu(app, compact = true, id = "schedule.server")
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Theme.wash.copy(alpha = 0.06f)))
        val problem = draft.problem()
        val ink by animateColorAsState(if (problem) Theme.dangerInk else Theme.muted, Theme.settle(), label = "verdict")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            SwappingSymbol(if (problem) "exclamationmark.circle" else "calendar.badge.clock", Modifier.padding(top = 1.dp), size = 12f, weight = Font.Semibold, tint = ink)
            Crossfade(draft.verdict(), Modifier.weight(1f), animationSpec = Theme.settle(), label = "verdict") { said ->
                KitText(said, font = Font(12.5f), color = ink)
            }
        }
        GlassButton(
            { plan(app, draft, scope, clipboard) },
            Modifier.fillMaxWidth(),
            shape = GlassButtonShape.Capsule(40.dp),
            prominent = true,
            enabled = !draft.arranging && !problem,
        ) {
            Crossfade(draft.arranging, animationSpec = Theme.settle(), label = "arrange") { busy ->
                if (busy) MediaSpinner(size = 15f, tint = Theme.tallyForeground) else KitText(t("Arrange"), lines = 1)
            }
        }
    }
}

private fun plan(app: AppModel, draft: ScheduleDraft, scope: kotlinx.coroutines.CoroutineScope, clipboard: Clipboard) {
    if (draft.arranging) return
    draft.now = Instant.now()
    if (draft.problem()) return
    val wanted = draft.planning
    val room = if (validTyped(wanted)) RoomNames.normalise(wanted) else RoomNames.generate()
    val at = draft.at()
    draft.arranging = true
    scope.launch {
        try {
            app.lobby.arrange(room, at, app.relayChoice).fold(
                { made ->
                    draft.planning = ""
                    draft.arranged()
                    copy(app, made, draft, clipboard)
                },
                { refused -> app.problem = refused.message },
            )
        } finally {
            draft.arranging = false
        }
    }
}

private suspend fun copy(app: AppModel, one: Meeting, draft: ScheduleDraft, clipboard: Clipboard) {
    val link = app.lobby.link(one) ?: return
    clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(t("Copy link"), link)))
    draft.copied = one.id
    delay(1600)
    if (draft.copied == one.id) draft.copied = null
}

/** What the list has to say, which its card changes between. */
private enum class Listing { SignedOut, Failed, Loading, Empty, Meetings }

private fun listing(app: AppModel): Listing = when (app.lobby.status) {
    LobbyModel.Status.SignedOut -> Listing.SignedOut
    LobbyModel.Status.Failed -> Listing.Failed
    LobbyModel.Status.Loading -> if (app.lobby.arrangements.isEmpty()) Listing.Loading else Listing.Meetings
    LobbyModel.Status.Ready -> if (app.lobby.arrangements.isEmpty()) Listing.Empty else Listing.Meetings
}

@Composable
private fun MeetingList(app: AppModel, draft: ScheduleDraft, wide: Boolean, modifier: Modifier) {
    val scope = LocalAppScope.current
    val now = listing(app)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        AnimatedVisibility(now == Listing.SignedOut, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Notice(t("Your session has expired. Sign in again."), "person.crop.circle.badge.exclamationmark")
        }
        AnimatedVisibility(now == Listing.Failed, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Notice(t("Could not load your meetings."), "exclamationmark.triangle") {
                GlassButton({ scope.launch { app.lobby.refreshArrangements() } }, shape = GlassButtonShape.Capsule(30.dp)) {
                    KitText(t("Try again"), lines = 1)
                }
            }
        }
        AnimatedVisibility(now == Listing.Loading, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Box(Modifier.fillMaxWidth().heightIn(min = 140.dp).glassPane(22.dp), contentAlignment = Alignment.Center) {
                MediaSpinner(size = 16f, tint = Theme.muted)
            }
        }
        AnimatedVisibility(now == Listing.Empty, enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Column(
                Modifier.fillMaxWidth().heightIn(min = 140.dp).glassPane(22.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Symbol("calendar.badge.clock", size = 26f, tint = Theme.muted)
                KitText(t("No meetings arranged yet."), font = Font(14f, Font.Semibold))
            }
        }
        val arrangements = app.lobby.arrangements
        AnimatedVisibility(arrangements.isNotEmpty(), enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()), exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave())) {
            Column(Modifier.fillMaxWidth().glassPane(22.dp)) {
                val kept = keepDeparting(arrangements) { it.id }
                for (entry in kept) {
                    val one = entry.item
                    AnimatedVisibility(
                        entry.visible,
                        enter = expandVertically(Theme.settle()) + fadeIn(Theme.settle()),
                        exit = shrinkVertically(Theme.leave()) + fadeOut(Theme.leave()) + scaleOut(Theme.leave(), targetScale = 0.96f),
                    ) {
                        Column {
                            if (wide) {
                                MeetingLine(app, one, draft, cancel = true)
                            } else {
                                // On a phone Cancel is out of sight until a
                                // meeting is swiped to the left, as Remove is
                                // on the recent rooms (the owner asked for it
                                // there), which leaves the row room for its
                                // time beside the two buttons kept.
                                SwipeRow(t("Cancel meeting"), "xmark", { scope.launch { app.lobby.cancel(one)?.let { app.problem = it } } }) { closeFirst ->
                                    MeetingLine(app, one, draft, cancel = false, Modifier.heightIn(min = 60.dp), closeFirst)
                                }
                            }
                            if (one.id != arrangements.lastOrNull()?.id) RowRule(leading = if (wide) 0.dp else 60.dp)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Notice(text: String, symbol: String, trailing: @Composable () -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().glassPane(18.dp).padding(14.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Symbol(symbol, size = 14f, tint = Theme.muted)
        KitText(text, Modifier.weight(1f), font = Font(13f), color = Theme.muted)
        trailing()
    }
}

@Composable
private fun MeetingLine(
    app: AppModel,
    one: Meeting,
    draft: ScheduleDraft,
    cancel: Boolean,
    modifier: Modifier = Modifier,
    closeFirst: () -> Boolean = { false },
) {
    val scope = LocalAppScope.current
    val clipboard = LocalClipboard.current
    val context = LocalContext.current
    Row(
        modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
        // The row's buttons carry their own fingertip's margin in a narrow
        // window (RowCircle), which is the spacing there.
        horizontalArrangement = Arrangement.spacedBy(if (narrowWindow()) 0.dp else 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        MeetingRow(
            one, app,
            Modifier
                .weight(1f)
                .alpha(if (one.ended) 0.5f else 1f)
                .plainPress({
                    if (!closeFirst() && !one.ended) app.lobby.link(one)?.let { link -> scope.launch { app.open(link) } }
                }, enabled = !one.ended),
        )
        val copied = draft.copied == one.id
        RowCircle(if (copied) "checkmark" else "doc.on.doc", if (copied) t("Link copied") else t("Copy link")) {
            if (!closeFirst()) scope.launch { copy(app, one, draft, clipboard) }
        }
        if (app.lobby.link(one) != null) {
            RowCircle("calendar.badge.plus", t("Add to calendar")) { if (!closeFirst()) addToCalendar(context, app, one) }
        }
        if (cancel) {
            RowCircle("xmark", t("Cancel meeting"), destructive = true) {
                scope.launch { app.lobby.cancel(one)?.let { app.problem = it } }
            }
        }
    }
}

/**
 * A row's small round button, drawn as the app's other buttons in the
 * content layer are, a light fill and a hairline (`ScheduleView.rowCircle`),
 * and pressed by itself rather than by the row it is in.
 */
@Composable
private fun RowCircle(symbol: String, label: String, destructive: Boolean = false, action: () -> Unit) {
    val press = rememberPress()
    // Pressed over a fingertip on a phone and the Fold's cover screen, where
    // three of them stood six points apart (kit `fingertip`).
    Box(
        Modifier
            .size(fingertip(32.dp))
            .pressable(press, CircleShape, true, Role.Button, label, onClick = action),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(32.dp)
                .background(Theme.wash.copy(alpha = if (press.pressed) 0.14f else 0.07f), CircleShape)
                .border(1.dp, Theme.wash.copy(alpha = 0.09f), CircleShape),
            contentAlignment = Alignment.Center,
        ) { SwappingSymbol(symbol, size = 12f, weight = Font.Semibold, tint = if (destructive) Theme.dangerInk else Theme.foreground) }
    }
}

/**
 * The meeting into the person's calendar. Apple hands over an .ics file
 * (calendar.ts `icsFor`) through the share sheet; Android's own way is the
 * calendar's new-event screen filled in, which needs no file and no provider
 * to serve one, and leaves the person to save it where they keep their
 * days. The link goes in the description, as the .ics carries it.
 */
private fun addToCalendar(context: Context, app: AppModel, one: Meeting) {
    val link = app.lobby.link(one) ?: return
    val start = AppModel.instant(one.at) ?: return
    val insert = Intent(Intent.ACTION_INSERT)
        .setData(CalendarContract.Events.CONTENT_URI)
        .putExtra(CalendarContract.Events.TITLE, one.room)
        .putExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, start.toEpochMilli())
        .putExtra(CalendarContract.Events.DESCRIPTION, link)
        .putExtra(CalendarContract.Events.EVENT_LOCATION, link)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    try {
        context.startActivity(insert)
    } catch (_: ActivityNotFoundException) {
        // No calendar to take it: the link goes wherever the person sends
        // things, as the share sheet would have sent the file.
        val share = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link).putExtra(Intent.EXTRA_SUBJECT, one.room)
        try {
            context.startActivity(Intent.createChooser(share, t("Add to calendar")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (fault: ActivityNotFoundException) {
            Log.w("Tomoshibi", "nothing can take a calendar entry")
        }
    }
}
