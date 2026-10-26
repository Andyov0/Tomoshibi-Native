package app.tomoshibi.basics

import java.time.Instant
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * Where the schedule's form starts and what it names; the Apple apps'
 * ScheduleClockTests, case for case. What these guard: the form starting on
 * today's date with a time after midnight, which had gone before anybody
 * touched it (from 23:45 on, at the end of a month and of a year as well),
 * and a time the clocks pass twice read as the one that has gone, in zones
 * that put their clocks back and forward, one of them at midnight.
 */
class ScheduleClockTest {
    private fun at(text: String): Instant = OffsetDateTime.parse(text).toInstant()

    private class Form(val day: LocalDate, val hour: Int, val minute: Int, val at: Instant)

    /** The form as it starts at [now], and the instant it names then. */
    private fun starting(now: Instant, zone: String): Form {
        val id = ZoneId.of(zone)
        val start = ScheduleClock.start(now, id)
        return Form(start.toLocalDate(), start.hour, start.minute,
            ScheduleClock.instant(start.toLocalDate(), start.hour, start.minute, id, now))
    }

    @Test
    fun middayStartsAtTheNextQuarterOfTheSameDay() {
        val form = starting(at("2026-10-27T10:07:30+08:00"), "Asia/Hong_Kong")
        assertEquals(LocalDate.of(2026, 10, 27), form.day)
        assertEquals(listOf(10, 15), listOf(form.hour, form.minute))
        assertEquals(at("2026-10-27T10:15:00+08:00"), form.at)
    }

    @Test
    fun tenToMidnightStartsAtMidnightOnTheNextDay() {
        val form = starting(at("2026-10-27T23:50:00+08:00"), "Asia/Hong_Kong")
        assertEquals("the next day, not today", LocalDate.of(2026, 10, 28), form.day)
        assertEquals(listOf(0, 0), listOf(form.hour, form.minute))
        assertEquals("ten minutes ahead, not a day before", at("2026-10-28T00:00:00+08:00"), form.at)
    }

    @Test
    fun aMinuteToMidnightStartsAtMidnightOnTheNextDay() {
        val form = starting(at("2026-10-27T23:59:59+08:00"), "Asia/Hong_Kong")
        assertEquals(LocalDate.of(2026, 10, 28), form.day)
        assertEquals(listOf(0, 0), listOf(form.hour, form.minute))
        assertEquals(at("2026-10-28T00:00:00+08:00"), form.at)
    }

    @Test
    fun theLastNightOfAMonthStartsOnTheFirstOfTheNext() {
        val form = starting(at("2026-10-31T23:50:00+08:00"), "Asia/Hong_Kong")
        assertEquals(LocalDate.of(2026, 11, 1), form.day)
        assertEquals(at("2026-11-01T00:00:00+08:00"), form.at)
    }

    @Test
    fun theLastNightOfAYearStartsOnNewYearsDay() {
        val form = starting(at("2026-12-31T23:55:00+08:00"), "Asia/Hong_Kong")
        assertEquals(LocalDate.of(2027, 1, 1), form.day)
        assertEquals(listOf(0, 0), listOf(form.hour, form.minute))
        assertEquals(at("2027-01-01T00:00:00+08:00"), form.at)
    }

    /** New York puts its clocks forward at 02:00 to 03:00: ten minutes before, the next quarter is 03:00. */
    @Test
    fun clocksPutForwardStartAfterTheSkippedHour() {
        val form = starting(at("2026-03-08T01:50:00-05:00"), "America/New_York")
        assertEquals(LocalDate.of(2026, 3, 8), form.day)
        assertEquals(listOf(3, 0), listOf(form.hour, form.minute))
        assertEquals(at("2026-03-08T03:00:00-04:00"), form.at)
    }

    /**
     * New York puts its clocks back at 02:00 to 01:00: ten minutes before,
     * the next quarter is the second 01:00, which java.time reads as the
     * first, an hour before now.
     */
    @Test
    fun clocksPutBackStartAtTheSecondPassOfTheHour() {
        val form = starting(at("2026-11-01T01:50:00-04:00"), "America/New_York")
        assertEquals(LocalDate.of(2026, 11, 1), form.day)
        assertEquals(listOf(1, 0), listOf(form.hour, form.minute))
        assertEquals("the second 01:00, ten minutes ahead", at("2026-11-01T01:00:00-05:00"), form.at)
    }

    /** A time in the hour passed twice, chosen by hand: the first while it is still to come, the second once the first has gone. */
    @Test
    fun anHourPassedTwiceIsTheFirstUntilItHasGone() {
        val zone = ZoneId.of("America/New_York")
        val day = LocalDate.of(2026, 11, 1)
        assertEquals(at("2026-11-01T01:30:00-04:00"), ScheduleClock.instant(day, 1, 30, zone, at("2026-11-01T00:30:00-04:00")))
        assertEquals(at("2026-11-01T01:30:00-05:00"), ScheduleClock.instant(day, 1, 30, zone, at("2026-11-01T01:40:00-04:00")))
        assertEquals("both gone: java.time's own reading",
            at("2026-11-01T01:30:00-04:00"), ScheduleClock.instant(day, 1, 30, zone, at("2026-11-01T01:40:00-05:00")))
    }

    /** Santiago puts its clocks forward at midnight, from 00:00 to 01:00: ten minutes to midnight the next quarter is 01:00 on the next day. */
    @Test
    fun clocksPutForwardAtMidnightStartOnTheNextDay() {
        val form = starting(at("2026-09-05T23:50:00-04:00"), "America/Santiago")
        assertEquals(LocalDate.of(2026, 9, 6), form.day)
        assertEquals(listOf(1, 0), listOf(form.hour, form.minute))
        assertEquals(at("2026-09-06T01:00:00-03:00"), form.at)
    }

    /**
     * Every minute of the two days around each change of the clocks, and of
     * the turn of a month and a year: the form starts at the next quarter
     * hour, ahead of now.
     */
    @Test
    fun everyMinuteAroundTheChangesStartsAtTheNextQuarter() {
        val cases = listOf(
            "America/New_York" to "2026-03-07T12:00:00-05:00",
            "America/New_York" to "2026-10-31T12:00:00-04:00",
            "Europe/London" to "2026-03-28T12:00:00+00:00",
            "Europe/London" to "2026-10-24T12:00:00+01:00",
            "America/Santiago" to "2026-09-05T12:00:00-04:00",
            "America/Santiago" to "2026-04-04T12:00:00-03:00",
            "Australia/Lord_Howe" to "2026-10-03T12:00:00+10:30",
            "Asia/Kathmandu" to "2026-12-31T12:00:00+05:45",
            "Asia/Hong_Kong" to "2026-12-31T12:00:00+08:00",
        )
        for ((zone, from) in cases) {
            val start = at(from)
            for (minute in 0 until 48 * 60) {
                val now = start.plusSeconds(minute * 60L + 17)
                val form = starting(now, zone)
                assertEquals("$zone at $now", ScheduleClock.nextQuarter(now), form.at)
                assertTrue("$zone at $now", form.at.isAfter(now))
            }
        }
    }
}
