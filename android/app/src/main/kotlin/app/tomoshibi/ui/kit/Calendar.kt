package app.tomoshibi.ui.kit

import android.text.format.DateFormat
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.YearMonth
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.format.TextStyle
import java.time.temporal.ChronoUnit
import java.time.temporal.WeekFields
import java.util.Locale

// Days and times (Kit+Settings.swift): a month to choose a day from, an hour
// and a quarter, and a moment that may be left open; drawn rather than the
// system's pickers, in the person's colour.

/**
 * The locale dates are written in: the language the interface is in, with
 * the region the system is set to, so that a week starts on the day the
 * person's own calendar starts it on.
 */
internal fun calendarLocale(): Locale {
    val language = Localization.shared.language.locale
    val region = Locale.getDefault().country
    return if (region.isEmpty()) language else Locale.Builder().setLocale(language).setRegion(region).build()
}

/**
 * A month of days to choose one from (`MonthCalendar`): the chosen day in
 * the person's colour, today ringed, days that cannot be chosen dimmed. The
 * time of day of `day` is kept; only the day changes. `today` is for a test
 * that must not depend on the date it runs on.
 */
@Composable
fun MonthCalendar(
    day: LocalDateTime,
    onDayChange: (LocalDateTime) -> Unit,
    modifier: Modifier = Modifier,
    earliest: LocalDateTime? = null,
    latest: LocalDateTime? = null,
    today: LocalDate = LocalDate.now(),
) {
    val locale = calendarLocale()
    var month by remember { mutableStateOf(YearMonth.from(day)) }
    var forward by remember { mutableStateOf(true) }
    val weeks = WeekFields.of(locale)
    Column(modifier.fillMaxWidth().clipToBounds(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AnimatedContent(
                month,
                transitionSpec = {
                    (slideInVertically(Theme.settle()) { if (forward) it / 2 else -it / 2 } + fadeIn(Theme.settle())) togetherWith
                        (slideOutVertically(Theme.leave()) { if (forward) -it / 2 else it / 2 } + fadeOut(Theme.leave()))
                },
                label = "month",
            ) { shown ->
                val pattern = DateFormat.getBestDateTimePattern(locale, "yMMMM")
                KitText(shown.atDay(1).format(DateTimeFormatter.ofPattern(pattern, locale)), font = Font(14f, Font.Semibold), lines = 1)
            }
            Spacer(Modifier.weight(1f))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (step in listOf(-1L, 1L)) {
                    val target = month.plusMonths(step)
                    val allowed = if (step < 0) earliest?.let { !target.isBefore(YearMonth.from(it)) } ?: true
                    else latest?.let { !target.atDay(1).atStartOfDay().isAfter(it) } ?: true
                    GlassButton(
                        onClick = {
                            forward = step > 0
                            month = target
                        },
                        shape = GlassButtonShape.Circle(28.dp),
                        enabled = allowed,
                        label = if (step < 0) t("Previous month") else t("Next month"),
                    ) { Symbol(if (step < 0) "chevron.left" else "chevron.right", size = 11f, weight = Font.Bold) }
                }
            }
        }
        val first = weeks.firstDayOfWeek
        Row(Modifier.fillMaxWidth().clearAndSetSemantics {}) {
            for (offset in 0L until 7L) {
                Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                    KitText(first.plus(offset).getDisplayName(TextStyle.NARROW_STANDALONE, locale),
                        font = Font(10.5f, Font.Semibold), color = Theme.muted, lines = 1)
                }
            }
        }
        AnimatedContent(
            month,
            transitionSpec = {
                (slideInHorizontally(Theme.settle()) { if (forward) it else -it } + fadeIn(Theme.settle())) togetherWith
                    fadeOut(Theme.leave())
            },
            label = "days",
        ) { shown ->
            // The month's days with blanks before the first, so it starts
            // under its own weekday.
            val lead = Math.floorMod(shown.atDay(1).dayOfWeek.value - first.value, 7)
            val cells: List<LocalDate?> = List(lead) { null } + (1..shown.lengthOfMonth()).map { shown.atDay(it) }
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                for (week in cells.chunked(7)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                        for (index in 0 until 7) {
                            val date = week.getOrNull(index)
                            Box(Modifier.weight(1f).heightIn(min = 30.dp), contentAlignment = Alignment.Center) {
                                if (date != null) {
                                    DayCell(date, day, today, earliest, latest, locale) { chosen ->
                                        onDayChange(chosen.atTime(day.hour, day.minute))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun DayCell(
    date: LocalDate,
    day: LocalDateTime,
    today: LocalDate,
    earliest: LocalDateTime?,
    latest: LocalDateTime?,
    locale: Locale,
    choose: (LocalDate) -> Unit,
) {
    val chosen = date == day.toLocalDate()
    val isToday = date == today
    val allowed = (earliest?.let { !date.isBefore(it.toLocalDate()) } ?: true) &&
        (latest?.let { !date.atStartOfDay().isAfter(it) } ?: true)
    val press = rememberPress()
    val ink by animateColorAsState(
        when {
            chosen -> Theme.tallyForeground
            allowed -> Theme.foreground
            else -> Theme.muted.copy(alpha = Theme.muted.alpha * 0.45f)
        },
        Theme.settle(), label = "day",
    )
    Box(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 30.dp)
            .pressable(
                press, CircleShape, allowed, Role.Button,
                date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL).withLocale(locale)),
                selected = if (chosen) true else null,
            ) { choose(date) },
        contentAlignment = Alignment.Center,
    ) {
        if (isToday && !chosen) Box(Modifier.size(30.dp).border(1.dp, Theme.tally.copy(alpha = 0.7f), CircleShape))
        AnimatedVisibility(chosen, enter = fadeIn(Theme.settle()) + scaleIn(Theme.settle()), exit = fadeOut(Theme.leave()) + scaleOut(Theme.leave())) {
            Box(Modifier.size(30.dp).background(Theme.tally, CircleShape))
        }
        KitText(
            date.dayOfMonth.toString(),
            font = Font(12.5f, if (chosen || isToday) Font.Semibold else Font.Regular, digits = true),
            color = ink,
            lines = 1,
        )
    }
}

/**
 * An hour and a quarter of it, as two capsules opening a small grid each
 * in a popover of its own (`ClockChoice`).
 */
@Composable
fun ClockChoice(
    id: String,
    hour: Int,
    minute: Int,
    onHourChange: (Int) -> Unit,
    onMinuteChange: (Int) -> Unit,
    modifier: Modifier = Modifier,
    minutes: List<Int> = listOf(0, 15, 30, 45),
) {
    val popovers = LocalPopovers.current ?: rememberPopovers()
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        for (part in listOf("hour", "minute")) {
            val hours = part == "hour"
            val label = if (hours) t("Hour") else t("Minute")
            val partId = "$id.$part"
            FloatingPopover(
                partId,
                popovers,
                alignment = PopoverAlignment.Center,
                width = 236.dp,
                panel = {
                    Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        MenuHeading(label)
                        if (hours) {
                            ClockGrid((0 until 24).toList(), 6, hour) { onHourChange(it); popovers.close() }
                        } else {
                            ClockGrid(minutes, 4, minute) { onMinuteChange(it); popovers.close() }
                        }
                    }
                },
            ) { ClockCapsule(if (hours) hour else minute, label) { popovers.toggle(partId) } }
            if (hours) KitText(":", font = Font(15f, Font.Semibold), color = Theme.muted)
        }
    }
}

/** One half of a time, as the capsule that opens its grid (`ClockCapsule`). */
@Composable
fun ClockCapsule(value: Int, label: String, modifier: Modifier = Modifier, lit: Boolean = false, onClick: () -> Unit) {
    val ink by animateColorAsState(if (lit) Theme.tally else Theme.foreground, Theme.settle(), label = "ink")
    GlassButton(onClick, modifier, shape = GlassButtonShape.Capsule(32.dp), label = label) {
        KitText(
            "%02d".format(value),
            Modifier.widthIn(min = 26.dp),
            font = Font(14f, Font.Semibold, digits = true),
            color = ink,
            lines = 1,
            align = androidx.compose.ui.text.style.TextAlign.Center,
        )
    }
}

/** The hours or the quarters to choose one from (`ClockGrid`). */
@Composable
fun ClockGrid(values: List<Int>, columns: Int, chosen: Int, modifier: Modifier = Modifier, pick: (Int) -> Unit) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (row in values.chunked(columns)) {
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                for (index in 0 until columns) {
                    val value = row.getOrNull(index)
                    Box(Modifier.weight(1f)) {
                        if (value != null) {
                            val isChosen = value == chosen
                            val press = rememberPress()
                            val shape = rounded(9f)
                            val fill by animateColorAsState(
                                if (isChosen) Theme.tally else Color.White.copy(alpha = if (press.lit) 0.12f else 0.06f),
                                Theme.settle(), label = "cell",
                            )
                            Box(
                                Modifier
                                    .fillMaxWidth()
                                    .heightIn(min = 30.dp)
                                    .background(fill, shape)
                                    .pressable(press, shape, true, Role.Button, selected = if (isChosen) true else null) { pick(value) },
                                contentAlignment = Alignment.Center,
                            ) {
                                KitText(
                                    "%02d".format(value),
                                    font = Font(13f, if (isChosen) Font.Bold else Font.Regular, digits = true),
                                    color = if (isChosen) Theme.tallyForeground else Theme.foreground,
                                    lines = 1,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * How a moment is written on its capsule (LobbyModel.said): the date
 * abbreviated, the time short, and the zone, in the interface's language.
 */
fun saidMoment(moment: LocalDateTime): String {
    val locale = calendarLocale()
    val pattern = DateFormat.getBestDateTimePattern(locale, "yMMMdjmmz")
    return moment.atZone(ZoneId.systemDefault()).format(DateTimeFormatter.ofPattern(pattern, locale))
}

/**
 * A moment that may be left open: a capsule saying it, or saying that it is
 * not set, opening onto a calendar and a clock (Account.tsx's `Works from`
 * and `Works until`; `MomentChoice`). Hung from the leading edge: the capsule
 * grows from a dash to a whole date the moment a time is chosen, and a panel
 * hung from its trailing edge jumped sideways under the pointer midway
 * through choosing the time.
 */
@Composable
fun MomentChoice(
    id: String,
    moment: LocalDateTime?,
    onMomentChange: (LocalDateTime?) -> Unit,
    modifier: Modifier = Modifier,
    earliest: LocalDateTime? = LocalDateTime.now(),
    said: (LocalDateTime) -> String = ::saidMoment,
) {
    val popovers = LocalPopovers.current ?: rememberPopovers()
    FloatingPopover(
        id,
        popovers,
        modifier,
        alignment = PopoverAlignment.Leading,
        width = 290.dp,
        panel = { MomentPanel(moment, onMomentChange, earliest) },
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            GlassButton(onClick = { popovers.toggle(id) }, shape = GlassButtonShape.Capsule(30.dp)) {
                KitFont(Font(12.5f, Font.Medium)) {
                    Symbol("calendar", size = 11.5f, weight = Font.Semibold)
                    KitText(moment?.let(said) ?: "—", lines = 1)
                }
            }
            AnimatedVisibility(moment != null, enter = Theme.arrive, exit = Theme.depart) {
                GlassButton(onClick = { onMomentChange(null) }, shape = GlassButtonShape.Circle(26.dp), label = t("Remove")) {
                    Symbol("xmark", size = 10f, weight = Font.Bold)
                }
            }
        }
    }
}

private enum class ClockPart { Hour, Minute }

/**
 * The calendar, and under it the time, whose hour and quarter are chosen in
 * the panel itself, in the calendar's place while they are (`MomentPanel`).
 * Not in popovers of their own, as the clock beside a schedule is: the app
 * keeps one popover open at a time, so pressing the hour inside this one
 * would close it. Nothing is set by opening it: an end left open is a link
 * that works until it is stopped, and a moment written merely by looking
 * would be a link that stops when nobody asked it to.
 */
@Composable
private fun MomentPanel(moment: LocalDateTime?, onMomentChange: (LocalDateTime?) -> Unit, earliest: LocalDateTime?) {
    var part by remember { mutableStateOf<ClockPart?>(null) }
    val day = moment ?: nextHour()
    val hour = day.hour
    val minute = day.minute / 15 * 15
    val calendarShown by animateFloatAsState(if (part == null) 1f else 0f, Theme.settle(), label = "calendar")
    Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // The calendar keeps its room while a grid is over it, so the panel
        // does not change size under the finger.
        Box(contentAlignment = Alignment.TopStart) {
            KitEnabled(part == null) {
                MonthCalendar(day, { onMomentChange(it) }, Modifier.alpha(calendarShown), earliest = earliest)
            }
            Appear(part != null, enter = Theme.arrive, exit = Theme.depart) {
                val shown = part ?: ClockPart.Hour
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    MenuHeading(if (shown == ClockPart.Hour) t("Hour") else t("Minute"))
                    if (shown == ClockPart.Hour) {
                        ClockGrid((0 until 24).toList(), 6, hour) { picked ->
                            onMomentChange(day.withHour(picked).withMinute(minute).truncatedTo(ChronoUnit.MINUTES))
                            part = null
                        }
                    } else {
                        ClockGrid(listOf(0, 15, 30, 45), 4, minute) { picked ->
                            onMomentChange(day.withHour(hour).withMinute(picked).truncatedTo(ChronoUnit.MINUTES))
                            part = null
                        }
                    }
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            KitText(t("Time"), font = Font(12f), color = Theme.muted)
            Spacer(Modifier.weight(1f))
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                ClockCapsule(hour, t("Hour"), lit = part == ClockPart.Hour) {
                    part = if (part == ClockPart.Hour) null else ClockPart.Hour
                }
                KitText(":", font = Font(15f, Font.Semibold), color = Theme.muted)
                ClockCapsule(minute, t("Minute"), lit = part == ClockPart.Minute) {
                    part = if (part == ClockPart.Minute) null else ClockPart.Minute
                }
            }
        }
    }
}

/** The next whole hour, where a moment not yet set starts. */
private fun nextHour(): LocalDateTime = LocalDateTime.now().truncatedTo(ChronoUnit.HOURS).plusHours(1)
