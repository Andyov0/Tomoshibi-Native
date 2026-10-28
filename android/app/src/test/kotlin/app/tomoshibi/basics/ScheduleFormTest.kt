package app.tomoshibi.basics

import java.time.LocalDate
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The schedule's form opened long after the lobby that keeps it was made;
 * the Apple apps' ScheduleFormTests, case for case. What these guard: the
 * starting time worked out once, when the lobby was, and offered hours later
 * already gone; and a day or a time somebody chose written over by the next
 * opening.
 */
class ScheduleFormTest {
    private val hongKong = ZoneId.of("Asia/Hong_Kong")

    private fun at(text: String) = OffsetDateTime.parse(text).toInstant()

    /** The lobby made at 10:07, its form opened three hours later: the next quarter hour from then, ahead of then. */
    @Test
    fun openedHoursLaterStartsFromTheOpening() {
        val made = ScheduleForm.start(at("2026-10-27T10:07:00+08:00"), hongKong)
        assertEquals(listOf(10, 15), listOf(made.hour, made.minute))
        val later = at("2026-10-27T13:07:00+08:00")
        val form = made.opened(later, hongKong)
        assertEquals(listOf(13, 15), listOf(form.hour, form.minute))
        assertEquals(at("2026-10-27T13:15:00+08:00"), form.instant(hongKong, later))
        assertTrue("ahead of the opening, not gone", form.instant(hongKong, later).isAfter(later))
    }

    /** The lobby made in the evening, the form opened the next morning: the new day as well as the new time. */
    @Test
    fun openedTheNextDayStartsOnIt() {
        val later = at("2026-10-28T08:52:00+08:00")
        val form = ScheduleForm.start(at("2026-10-27T21:40:00+08:00"), hongKong).opened(later, hongKong)
        assertEquals(LocalDate.of(2026, 10, 28), form.day.toLocalDate())
        assertEquals(listOf(9, 0), listOf(form.hour, form.minute))
        assertTrue(form.instant(hongKong, later).isAfter(later))
    }

    /** A time somebody picked stays through another opening, gone or not; the day nobody picked follows the clock. */
    @Test
    fun aTimeSomebodyChoseIsKept() {
        val form = ScheduleForm.start(at("2026-10-27T10:07:00+08:00"), hongKong)
            .choosingHour(16).choosingMinute(30)
            .opened(at("2026-10-28T09:20:00+08:00"), hongKong)
        assertEquals(listOf(16, 30), listOf(form.hour, form.minute))
        assertEquals("the day nobody picked, today's", LocalDate.of(2026, 10, 28), form.day.toLocalDate())
    }

    /** A day somebody picked stays; the time nobody picked starts from the opening. */
    @Test
    fun aDaySomebodyChoseIsKept() {
        val form = ScheduleForm.start(at("2026-10-27T10:07:00+08:00"), hongKong)
            .choosingDay(LocalDateTime.of(2026, 11, 3, 10, 15))
            .opened(at("2026-10-27T14:50:00+08:00"), hongKong)
        assertEquals(LocalDate.of(2026, 11, 3), form.day.toLocalDate())
        assertEquals(listOf(15, 0), listOf(form.hour, form.minute))
    }

    /** Once a meeting is arranged from it, the next opening starts afresh. */
    @Test
    fun arrangingLetsTheNextOpeningStartAfresh() {
        val form = ScheduleForm.start(at("2026-10-27T10:07:00+08:00"), hongKong)
            .choosingDay(LocalDateTime.of(2026, 10, 29, 10, 15))
            .choosingHour(16)
            .arranged()
            .opened(at("2026-10-27T18:01:00+08:00"), hongKong)
        assertEquals(LocalDate.of(2026, 10, 27), form.day.toLocalDate())
        assertEquals(listOf(18, 15), listOf(form.hour, form.minute))
    }

    /** Opened again with nothing chosen and no quarter gone by: as it was. */
    @Test
    fun openedAgainInTheSameQuarterIsUnchanged() {
        val made = ScheduleForm.start(at("2026-10-27T10:07:00+08:00"), hongKong)
        assertEquals(made, made.opened(at("2026-10-27T10:13:00+08:00"), hongKong))
    }
}
