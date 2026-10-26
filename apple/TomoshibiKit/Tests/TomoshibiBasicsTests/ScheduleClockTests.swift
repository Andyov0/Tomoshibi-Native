import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * Where the schedule's form starts and what it names. What these guard: the
 * form starting on today's date with a time after midnight, which had gone
 * before anybody touched it (from 23:45 on, at the end of a month and of a
 * year as well), and a time the clocks pass twice read as the one that has
 * gone, in zones that put their clocks back and forward, one of them at
 * midnight.
 */
final class ScheduleClockTests: XCTestCase {
    private func calendar(_ zone: String) -> Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: zone)!
        return calendar
    }

    private func instant(_ text: String) -> Date {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: text)!
    }

    /// The day the form starts on, as year, month and day in `calendar`.
    private func ymd(_ date: Date, _ calendar: Calendar) -> [Int] {
        let parts = calendar.dateComponents([.year, .month, .day], from: date)
        return [parts.year!, parts.month!, parts.day!]
    }

    /// The form as it starts at `now`, and the instant it names then.
    private func starting(_ now: Date, _ calendar: Calendar) -> (day: [Int], hour: Int, minute: Int, at: Date?) {
        let start = ScheduleClock.start(now: now, calendar: calendar)
        let at = ScheduleClock.instant(day: start.day, hour: start.hour, minute: start.minute, calendar: calendar, now: now)
        return (ymd(start.day, calendar), start.hour, start.minute, at)
    }

    func testMiddayStartsAtTheNextQuarterOfTheSameDay() {
        let hongKong = calendar("Asia/Hong_Kong")
        let now = instant("2026-10-27T10:07:30+08:00")
        let form = starting(now, hongKong)
        XCTAssertEqual(form.day, [2026, 10, 27])
        XCTAssertEqual([form.hour, form.minute], [10, 15])
        XCTAssertEqual(form.at, instant("2026-10-27T10:15:00+08:00"))
    }

    func testTenToMidnightStartsAtMidnightOnTheNextDay() {
        let hongKong = calendar("Asia/Hong_Kong")
        let now = instant("2026-10-27T23:50:00+08:00")
        let form = starting(now, hongKong)
        XCTAssertEqual(form.day, [2026, 10, 28], "the next day, not today")
        XCTAssertEqual([form.hour, form.minute], [0, 0])
        XCTAssertEqual(form.at, instant("2026-10-28T00:00:00+08:00"), "ten minutes ahead, not a day before")
    }

    func testAMinuteToMidnightStartsAtMidnightOnTheNextDay() {
        let hongKong = calendar("Asia/Hong_Kong")
        let now = instant("2026-10-27T23:59:59+08:00")
        let form = starting(now, hongKong)
        XCTAssertEqual(form.day, [2026, 10, 28])
        XCTAssertEqual([form.hour, form.minute], [0, 0])
        XCTAssertEqual(form.at, instant("2026-10-28T00:00:00+08:00"))
    }

    func testTheLastNightOfAMonthStartsOnTheFirstOfTheNext() {
        let hongKong = calendar("Asia/Hong_Kong")
        let now = instant("2026-10-31T23:50:00+08:00")
        let form = starting(now, hongKong)
        XCTAssertEqual(form.day, [2026, 11, 1])
        XCTAssertEqual(form.at, instant("2026-11-01T00:00:00+08:00"))
    }

    func testTheLastNightOfAYearStartsOnNewYearsDay() {
        let hongKong = calendar("Asia/Hong_Kong")
        let now = instant("2026-12-31T23:55:00+08:00")
        let form = starting(now, hongKong)
        XCTAssertEqual(form.day, [2027, 1, 1])
        XCTAssertEqual([form.hour, form.minute], [0, 0])
        XCTAssertEqual(form.at, instant("2027-01-01T00:00:00+08:00"))
    }

    /// New York puts its clocks forward at 02:00 to 03:00: ten minutes
    /// before, the next quarter is 03:00.
    func testClocksPutForwardStartAfterTheSkippedHour() {
        let newYork = calendar("America/New_York")
        let now = instant("2026-03-08T01:50:00-05:00")
        let form = starting(now, newYork)
        XCTAssertEqual(form.day, [2026, 3, 8])
        XCTAssertEqual([form.hour, form.minute], [3, 0])
        XCTAssertEqual(form.at, instant("2026-03-08T03:00:00-04:00"))
    }

    /// New York puts its clocks back at 02:00 to 01:00: ten minutes before,
    /// the next quarter is the second 01:00, which the system reads as the
    /// first, an hour before now.
    func testClocksPutBackStartAtTheSecondPassOfTheHour() {
        let newYork = calendar("America/New_York")
        let now = instant("2026-11-01T01:50:00-04:00")
        let form = starting(now, newYork)
        XCTAssertEqual(form.day, [2026, 11, 1])
        XCTAssertEqual([form.hour, form.minute], [1, 0])
        XCTAssertEqual(form.at, instant("2026-11-01T01:00:00-05:00"), "the second 01:00, ten minutes ahead")
    }

    /// A time in the hour passed twice, chosen by hand: the first while it
    /// is still to come, the second once the first has gone.
    func testAnHourPassedTwiceIsTheFirstUntilItHasGone() {
        let newYork = calendar("America/New_York")
        let day = instant("2026-11-01T12:00:00-05:00")
        XCTAssertEqual(ScheduleClock.instant(day: day, hour: 1, minute: 30, calendar: newYork,
                                             now: instant("2026-11-01T00:30:00-04:00")),
                       instant("2026-11-01T01:30:00-04:00"))
        XCTAssertEqual(ScheduleClock.instant(day: day, hour: 1, minute: 30, calendar: newYork,
                                             now: instant("2026-11-01T01:40:00-04:00")),
                       instant("2026-11-01T01:30:00-05:00"))
        XCTAssertEqual(ScheduleClock.instant(day: day, hour: 1, minute: 30, calendar: newYork,
                                             now: instant("2026-11-01T01:40:00-05:00")),
                       instant("2026-11-01T01:30:00-04:00"), "both gone: the system's own reading")
    }

    /// Santiago puts its clocks forward at midnight, from 00:00 to 01:00:
    /// ten minutes to midnight the next quarter is 01:00 on the next day.
    func testClocksPutForwardAtMidnightStartOnTheNextDay() {
        let santiago = calendar("America/Santiago")
        let now = instant("2026-09-05T23:50:00-04:00")
        let form = starting(now, santiago)
        XCTAssertEqual(form.day, [2026, 9, 6])
        XCTAssertEqual([form.hour, form.minute], [1, 0])
        XCTAssertEqual(form.at, instant("2026-09-06T01:00:00-03:00"))
    }

    /// Every minute of the two days around each change of the clocks, and
    /// of the turn of a month and a year: the form starts at the next
    /// quarter hour, ahead of now and at most a quarter hour ahead.
    func testEveryMinuteAroundTheChangesStartsAtTheNextQuarter() {
        let cases: [(String, String)] = [
            ("America/New_York", "2026-03-07T12:00:00-05:00"),
            ("America/New_York", "2026-10-31T12:00:00-04:00"),
            ("Europe/London", "2026-03-28T12:00:00+00:00"),
            ("Europe/London", "2026-10-24T12:00:00+01:00"),
            ("America/Santiago", "2026-09-05T12:00:00-04:00"),
            ("America/Santiago", "2026-04-04T12:00:00-03:00"),
            ("Australia/Lord_Howe", "2026-10-03T12:00:00+10:30"),
            ("Asia/Kathmandu", "2026-12-31T12:00:00+05:45"),
            ("Asia/Hong_Kong", "2026-12-31T12:00:00+08:00"),
        ]
        for (zone, from) in cases {
            let calendar = calendar(zone)
            let start = instant(from)
            for minute in stride(from: 0, to: 48 * 60, by: 1) {
                let now = start.addingTimeInterval(TimeInterval(minute * 60 + 17))
                let form = starting(now, calendar)
                guard let at = form.at else { return XCTFail("\(zone) \(now): no instant") }
                XCTAssertEqual(at, ScheduleClock.nextQuarter(after: now), "\(zone) at \(now)")
                XCTAssertGreaterThan(at, now, "\(zone) at \(now)")
                if at != ScheduleClock.nextQuarter(after: now) { return }
            }
        }
    }
}
