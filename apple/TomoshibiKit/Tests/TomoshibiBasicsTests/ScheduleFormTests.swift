import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The schedule's form opened long after the lobby that keeps it was made.
 * What these guard: the starting time worked out once, when the lobby was,
 * and offered hours later already gone; and a day or a time somebody chose
 * written over by the next opening.
 */
final class ScheduleFormTests: XCTestCase {
    private let hongKong: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Hong_Kong")!
        return calendar
    }()

    private func instant(_ text: String) -> Date {
        ISO8601DateFormatter().date(from: text)!
    }

    private func ymd(_ date: Date) -> [Int] {
        let parts = hongKong.dateComponents([.year, .month, .day], from: date)
        return [parts.year!, parts.month!, parts.day!]
    }

    /// The lobby made at 10:07, its form opened three hours later: the next
    /// quarter hour from then, ahead of then.
    func testOpenedHoursLaterStartsFromTheOpening() {
        var form = ScheduleForm(now: instant("2026-10-27T10:07:00+08:00"), calendar: hongKong)
        XCTAssertEqual([form.hour, form.minute], [10, 15])
        let later = instant("2026-10-27T13:07:00+08:00")
        form.open(now: later, calendar: hongKong)
        XCTAssertEqual([form.hour, form.minute], [13, 15])
        let at = form.instant(calendar: hongKong, now: later)
        XCTAssertEqual(at, instant("2026-10-27T13:15:00+08:00"))
        XCTAssertGreaterThan(at!, later, "ahead of the opening, not gone")
    }

    /// The lobby made in the evening, the form opened after midnight: the
    /// new day as well as the new time.
    func testOpenedTheNextDayStartsOnIt() {
        var form = ScheduleForm(now: instant("2026-10-27T21:40:00+08:00"), calendar: hongKong)
        let later = instant("2026-10-28T08:52:00+08:00")
        form.open(now: later, calendar: hongKong)
        XCTAssertEqual(ymd(form.day), [2026, 10, 28])
        XCTAssertEqual([form.hour, form.minute], [9, 0])
        XCTAssertGreaterThan(form.instant(calendar: hongKong, now: later)!, later)
    }

    /// A time somebody picked stays through the form closing and opening
    /// again later, gone or not; the day nobody picked follows the clock.
    func testATimeSomebodyChoseIsKept() {
        var form = ScheduleForm(now: instant("2026-10-27T10:07:00+08:00"), calendar: hongKong)
        form.choose(hour: 16)
        form.choose(minute: 30)
        form.open(now: instant("2026-10-28T09:20:00+08:00"), calendar: hongKong)
        XCTAssertEqual([form.hour, form.minute], [16, 30])
        XCTAssertEqual(ymd(form.day), [2026, 10, 28], "the day nobody picked, today's")
    }

    /// A day somebody picked stays; the time nobody picked starts from the
    /// opening.
    func testADaySomebodyChoseIsKept() {
        var form = ScheduleForm(now: instant("2026-10-27T10:07:00+08:00"), calendar: hongKong)
        form.choose(day: instant("2026-11-03T10:15:00+08:00"))
        form.open(now: instant("2026-10-27T14:50:00+08:00"), calendar: hongKong)
        XCTAssertEqual(ymd(form.day), [2026, 11, 3])
        XCTAssertEqual([form.hour, form.minute], [15, 0])
    }

    /// Once a meeting is arranged from it, the next opening starts afresh.
    func testArrangingLetsTheNextOpeningStartAfresh() {
        var form = ScheduleForm(now: instant("2026-10-27T10:07:00+08:00"), calendar: hongKong)
        form.choose(day: instant("2026-10-29T10:15:00+08:00"))
        form.choose(hour: 16)
        form.arranged()
        let later = instant("2026-10-27T18:01:00+08:00")
        form.open(now: later, calendar: hongKong)
        XCTAssertEqual(ymd(form.day), [2026, 10, 27])
        XCTAssertEqual([form.hour, form.minute], [18, 15])
    }

    /// Opened again with nothing chosen and no quarter gone by: as it was.
    func testOpenedAgainInTheSameQuarterIsUnchanged() {
        var form = ScheduleForm(now: instant("2026-10-27T10:07:00+08:00"), calendar: hongKong)
        let before = form
        form.open(now: instant("2026-10-27T10:13:00+08:00"), calendar: hongKong)
        XCTAssertEqual(form, before)
    }
}
