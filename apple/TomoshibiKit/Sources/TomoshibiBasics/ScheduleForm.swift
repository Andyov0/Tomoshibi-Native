import Foundation

/// The day and the time on the schedule's form, which of them somebody has
/// chosen, and where they start each time the form is opened.
///
/// The form's draft is kept by the lobby, which is made when somebody signs
/// in and stays for as long as the app does, and it worked out its starting
/// time once, then. Opened hours later, the form offered a quarter hour long
/// gone and said "That time has already gone." before anybody had touched
/// it. Now every opening starts what nobody has chosen at the next quarter
/// hour from that moment (ScheduleClock `start`), and keeps what somebody
/// has: a day picked stays picked, and so does a time.
public struct ScheduleForm: Equatable, Sendable {
    public private(set) var day: Date
    public private(set) var hour: Int
    public private(set) var minute: Int
    /// Somebody picked the day.
    public private(set) var dayChosen = false
    /// Somebody picked the hour or the minute.
    public private(set) var timeChosen = false

    public init(now: Date, calendar: Calendar) {
        let start = ScheduleClock.start(now: now, calendar: calendar)
        day = start.day
        hour = start.hour
        minute = start.minute
    }

    public mutating func choose(day: Date) {
        self.day = day
        dayChosen = true
    }

    public mutating func choose(hour: Int) {
        self.hour = hour
        timeChosen = true
    }

    public mutating func choose(minute: Int) {
        self.minute = minute
        timeChosen = true
    }

    /// The form opened at `now`: what nobody chose starts at the next
    /// quarter hour from now, its day and time read together.
    public mutating func open(now: Date, calendar: Calendar) {
        let start = ScheduleClock.start(now: now, calendar: calendar)
        if !dayChosen { day = start.day }
        if !timeChosen {
            hour = start.hour
            minute = start.minute
        }
    }

    /// A meeting arranged from the form: what was chosen went into it, and
    /// the next opening starts afresh, as the web's form forgets its day.
    public mutating func arranged() {
        dayChosen = false
        timeChosen = false
    }

    /// The instant the form names (ScheduleClock `instant`).
    public func instant(calendar: Calendar, now: Date) -> Date? {
        ScheduleClock.instant(day: day, hour: hour, minute: minute, calendar: calendar, now: now)
    }
}
