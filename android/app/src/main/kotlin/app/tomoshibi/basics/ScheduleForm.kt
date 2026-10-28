package app.tomoshibi.basics

import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId

/**
 * The day and the time on the schedule's form, which of them somebody has
 * chosen, and where they start each time the form is opened
 * (ScheduleForm.swift on Apple, the same rules).
 *
 * The form's draft is kept by the lobby, which is made when somebody signs
 * in and stays for as long as the app does, and it worked out its starting
 * time once, then. Opened hours later, the form offered a quarter hour long
 * gone and said "That time has already gone." before anybody had touched
 * it. Now every opening starts what nobody has chosen at the next quarter
 * hour from that moment ([ScheduleClock.start]), and keeps what somebody
 * has: a day picked stays picked, and so does a time.
 */
data class ScheduleForm(
    val day: LocalDateTime,
    val hour: Int,
    val minute: Int,
    /** Somebody picked the day. */
    val dayChosen: Boolean = false,
    /** Somebody picked the hour or the minute. */
    val timeChosen: Boolean = false,
) {
    fun choosingDay(day: LocalDateTime) = copy(day = day, dayChosen = true)

    fun choosingHour(hour: Int) = copy(hour = hour, timeChosen = true)

    fun choosingMinute(minute: Int) = copy(minute = minute, timeChosen = true)

    /** The form opened at [now]: what nobody chose starts at the next quarter hour from now, its day and time read together. */
    fun opened(now: Instant, zone: ZoneId): ScheduleForm {
        val start = ScheduleClock.start(now, zone)
        return copy(
            day = if (dayChosen) day else start,
            hour = if (timeChosen) hour else start.hour,
            minute = if (timeChosen) minute else start.minute,
        )
    }

    /** A meeting arranged from the form: what was chosen went into it, and the next opening starts afresh, as the web's form forgets its day. */
    fun arranged() = copy(dayChosen = false, timeChosen = false)

    /** The instant the form names ([ScheduleClock.instant]). */
    fun instant(zone: ZoneId, now: Instant): Instant = ScheduleClock.instant(day.toLocalDate(), hour, minute, zone, now)

    companion object {
        /** A form nobody has touched, at the next quarter hour from [now]. */
        fun start(now: Instant, zone: ZoneId): ScheduleForm {
            val start = ScheduleClock.start(now, zone)
            return ScheduleForm(start, start.hour, start.minute)
        }
    }
}
