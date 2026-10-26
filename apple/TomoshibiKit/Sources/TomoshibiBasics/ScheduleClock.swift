import Foundation

/// The schedule's form holds a day, an hour and a minute; this is where they
/// start and what instant they name.
///
/// The form used to start on today's date with the hour and the minute of
/// the next quarter hour. From 23:45 on, that quarter is midnight, so the
/// form offered today at 00:00, which had already gone, and said so before
/// anybody had touched it. The day is now read from the same instant as the
/// hour and the minute, so a quarter after midnight is on the next day, the
/// next month or the next year as the calendar has it.
public enum ScheduleClock {
    /// The next quarter hour strictly after `now` (Schedule.tsx
    /// `nextQuarter`), on the clock rather than by setting fields: setting
    /// the seconds to zero searched forward for the next such moment, and
    /// the minute it landed on was one the minute list does not offer, which
    /// left the list blank. Every zone in use is a whole number of quarter
    /// hours from UTC, so a quarter here is a quarter on the wall.
    public static func nextQuarter(after now: Date) -> Date {
        let seconds = now.timeIntervalSinceReferenceDate
        return Date(timeIntervalSinceReferenceDate: ((seconds / 900).rounded(.down) + 1) * 900)
    }

    /// Where the form starts: the next quarter hour, its day, hour and
    /// minute all read from that one instant in `calendar`. One reading of
    /// the clock for all three, so a form made on the stroke of a quarter
    /// does not take the day from one side of it and the time from the other.
    public static func start(now: Date, calendar: Calendar) -> (day: Date, hour: Int, minute: Int) {
        let first = nextQuarter(after: now)
        let parts = calendar.dateComponents([.hour, .minute], from: first)
        return (first, parts.hour ?? 0, parts.minute ?? 0)
    }

    /// The instant the form's day, hour and minute name in `calendar`.
    ///
    /// A time the clocks pass twice, in the hour they are put back, is the
    /// first of the two, as the system takes it, unless that one has gone by
    /// `now` and the second has not. The form starting at 01:50 on that
    /// night offered 01:00 (the second one, ten minutes ahead) and read it as
    /// the first, an hour before, which had gone. A time the clocks skip is
    /// the next that exists.
    public static func instant(day: Date, hour: Int, minute: Int, calendar: Calendar, now: Date) -> Date? {
        guard let first = calendar.date(bySettingHour: hour, minute: minute, second: 0, of: day,
                                        matchingPolicy: .nextTime, repeatedTimePolicy: .first,
                                        direction: .forward) else { return nil }
        guard first < now,
              let last = calendar.date(bySettingHour: hour, minute: minute, second: 0, of: day,
                                       matchingPolicy: .nextTime, repeatedTimePolicy: .last,
                                       direction: .forward),
              last >= now else { return first }
        return last
    }
}
