package app.tomoshibi.basics

import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId

/**
 * The schedule's form holds a day, an hour and a minute; this is where they
 * start and what instant they name (ScheduleClock.swift on Apple,
 * ScheduleClock.cs on Windows, the same rules).
 *
 * The form starts at the next quarter hour, its day read from the same
 * instant as its hour and minute, so a quarter after midnight is on the next
 * day, month or year. The Apple form took the day from today and offered
 * midnight on it from 23:45 on, already gone; this one did not, and the
 * tests here hold it to that.
 */
object ScheduleClock {
    /**
     * The next quarter hour strictly after [now] (Schedule.tsx
     * `nextQuarter`), on the clock rather than by setting fields, which once
     * landed on a minute the minute list does not offer and left it blank.
     * Every zone in use is a whole number of quarter hours from UTC, so a
     * quarter here is a quarter on the wall.
     */
    fun nextQuarter(now: Instant): Instant = Instant.ofEpochSecond(Math.floorDiv(now.epochSecond, 900L) * 900 + 900)

    /** Where the form starts: the next quarter hour as the wall clock in [zone] says it, day and time together. */
    fun start(now: Instant, zone: ZoneId): LocalDateTime = LocalDateTime.ofInstant(nextQuarter(now), zone)

    /**
     * The instant the form's [day], [hour] and [minute] name in [zone].
     *
     * A time the clocks pass twice, in the hour they are put back, is the
     * first of the two, as java.time takes it, unless that one has gone by
     * [now] and the second has not. The form starting at 01:50 on that night
     * offered 01:00 (the second one, ten minutes ahead) and read it as the
     * first, an hour before, and said it had gone. A time the clocks skip is
     * moved on by the length of the gap, as java.time moves it.
     */
    fun instant(day: LocalDate, hour: Int, minute: Int, zone: ZoneId, now: Instant): Instant {
        val first = day.atTime(hour, minute).atZone(zone).withEarlierOffsetAtOverlap()
        val last = first.withLaterOffsetAtOverlap()
        return if (first.toInstant().isBefore(now) && !last.toInstant().isBefore(now)) last.toInstant() else first.toInstant()
    }
}
