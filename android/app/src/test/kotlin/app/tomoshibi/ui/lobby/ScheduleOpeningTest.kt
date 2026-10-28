package app.tomoshibi.ui.lobby

import app.tomoshibi.basics.ScheduleClock
import java.time.Duration
import java.time.Instant
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The lobby keeps the schedule's draft from when it is made; what these
 * guard is the schedule opened later starting from a time worked out then,
 * which had gone by the time anybody looked, and a time somebody picked
 * written over when the schedule opens again. The lobby's clock is the
 * test's, moved on between making the lobby and opening the schedule.
 */
class ScheduleOpeningTest {
    private var clock = Instant.parse("2026-10-27T02:07:00Z")
    private val zone = ZoneId.systemDefault()

    private fun lobby() = LobbyState("sealing-word") { clock }

    private fun LobbyState.scheduledFor(): Instant =
        ScheduleClock.instant(schedule.day.toLocalDate(), schedule.hour, schedule.minute, zone, clock)

    @Test
    fun theScheduleOpenedHoursAfterTheLobbyStartsAheadOfTheOpening() {
        val lobby = lobby()
        clock = clock.plus(Duration.ofHours(3))
        lobby.place = Place.Scheduled
        assertTrue("ahead of the opening, not three hours gone", lobby.scheduledFor().isAfter(clock))
        assertEquals(ScheduleClock.nextQuarter(clock), lobby.scheduledFor())
    }

    @Test
    fun theScheduleOpenedTheNextDayStartsOnIt() {
        val lobby = lobby()
        clock = clock.plus(Duration.ofHours(26))
        lobby.place = Place.Scheduled
        assertEquals(ScheduleClock.nextQuarter(clock), lobby.scheduledFor())
        assertEquals(dayAt(clock), lobby.schedule.day.toLocalDate())
    }

    @Test
    fun aTimeSomebodyPickedStaysWhenTheScheduleOpensAgain() {
        val lobby = lobby()
        lobby.place = Place.Scheduled
        lobby.schedule.hour = 21
        lobby.schedule.minute = 45
        lobby.place = Place.Home
        clock = clock.plus(Duration.ofHours(2))
        lobby.place = Place.Scheduled
        assertEquals(listOf(21, 45), listOf(lobby.schedule.hour, lobby.schedule.minute))
    }

    @Test
    fun theScheduleStaysAsItIsWhileItIsOpen() {
        val lobby = lobby()
        lobby.place = Place.Scheduled
        val shown = lobby.schedule.form
        clock = clock.plus(Duration.ofHours(1))
        lobby.place = Place.Scheduled
        assertEquals("set again to where it already is, nothing moves", shown, lobby.schedule.form)
    }

    private fun dayAt(at: Instant) = at.atZone(zone).toLocalDate()
}
