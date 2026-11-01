//! The schedule's form holds a day and a time of day; this is where they
//! start and what instant they name (ScheduleClock.swift, ScheduleClock.kt
//! and ScheduleClock.cs, the same rules).
//!
//! The other apps' first forms started at the next quarter hour worked out
//! in the offset the clock had at that moment, and read the day and time
//! back in the offset the clock had when it was read, whatever day was
//! chosen. Across a change of the clocks both were an hour out: ten minutes
//! before New York puts its clocks forward the form offered 02:00, a time
//! that day does not have, and a meeting chosen for 10:00 on a day after the
//! clocks went back was arranged for 09:00. The next quarter hour is an
//! instant, said on the wall clock of the zone it falls in, and the day and
//! time are read in the zone's own offset for that day.

/// Fifteen minutes. Every zone in use is a whole number of quarter hours
/// from UTC, so a quarter here is a quarter on the wall.
pub const QUARTER: i64 = 15 * 60;

/// How far ahead a meeting can be arranged (Schedule.kt `AHEAD`).
pub const AHEAD_DAYS: i64 = 60;

/// A time zone, as the schedule needs it: the offset from UTC, in seconds
/// east, that holds at an instant.
pub trait Zone {
    fn offset_at(&self, utc: i64) -> i32;
}

/// This machine's zone, as GLib reads it (TZ, /etc/localtime), with its
/// whole history of changes of the clocks.
pub struct LocalZone;

impl Zone for LocalZone {
    fn offset_at(&self, utc: i64) -> i32 {
        let zone = glib::TimeZone::local();
        let interval = zone.find_interval(glib::TimeType::Universal, utc);
        if interval < 0 {
            return 0;
        }
        zone.offset(interval)
    }
}

/// The civil date of a day counted from 1970-01-01 (Howard Hinnant's
/// `civil_from_days`), for any day in the proleptic Gregorian calendar.
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// The day, counted from 1970-01-01, of a civil date.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A day of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Day {
    pub fn days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    pub fn of_days(days: i64) -> Day {
        let (year, month, day) = civil_from_days(days);
        Day { year, month, day }
    }

    pub fn add_days(self, count: i64) -> Day {
        Day::of_days(self.days() + count)
    }
}

/// The next quarter hour strictly after `now` (Schedule.tsx `nextQuarter`),
/// on the clock rather than by setting fields.
pub fn next_quarter(now: i64) -> i64 {
    now.div_euclid(QUARTER) * QUARTER + QUARTER
}

/// Where the form starts: the next quarter hour, its day and its time of day
/// both read from that one instant on the zone's wall clock, so that a
/// quarter after midnight is on the next day.
pub fn start(now: i64, zone: &dyn Zone) -> (Day, u32, u32) {
    let at = next_quarter(now);
    let wall = at + zone.offset_at(at) as i64;
    let seconds = wall.rem_euclid(86_400);
    (
        Day::of_days(wall.div_euclid(86_400)),
        (seconds / 3600) as u32,
        ((seconds % 3600) / 60) as u32,
    )
}

/// The instant a day and a time of day name in `zone`.
///
/// A time the clocks pass twice, in the hour they are put back, is the first
/// of the two unless that one has gone by `now` and the second has not, as
/// on the other platforms. A time the clocks skip is moved on by the length
/// of the gap, read in the offset that held before it, as java.time moves
/// it: 02:30 on the morning New York goes to summer time is 03:30.
pub fn instant(day: Day, hour: u32, minute: u32, zone: &dyn Zone, now: i64) -> i64 {
    let naive = day.days() * 86_400 + hour as i64 * 3600 + minute as i64 * 60;
    // The offsets either side: no zone changes its clocks twice in a day
    // and a half, so these are the one before and the one after any change
    // near this wall time.
    let before = zone.offset_at(naive - 18 * 3600);
    let after = zone.offset_at(naive + 18 * 3600);
    let mut valid: Vec<i64> = [before, after]
        .into_iter()
        .map(|offset| (offset, naive - offset as i64))
        .filter(|(offset, at)| zone.offset_at(*at) == *offset)
        .map(|(_, at)| at)
        .collect();
    valid.sort();
    valid.dedup();
    match valid.as_slice() {
        [] => naive - before as i64,
        [one] => *one,
        [first, .., last] => {
            if *first < now && *last >= now {
                *last
            } else {
                *first
            }
        }
    }
}

/// What the schedule's form holds (ScheduleForm.swift): the day and the
/// time, and whether somebody chose them. What nobody chose starts again at
/// every opening from the next quarter hour then, since the lobby's form can
/// be hours old by the time anybody opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleForm {
    pub day: Day,
    pub hour: u32,
    pub minute: u32,
    pub day_chosen: bool,
    pub time_chosen: bool,
}

/// Why the form's time cannot be arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooSoonOrLate {
    /// "That time has already gone."
    Gone,
    /// "That is more than sixty days away."
    TooFar,
}

impl ScheduleForm {
    pub fn new(now: i64, zone: &dyn Zone) -> ScheduleForm {
        let (day, hour, minute) = start(now, zone);
        ScheduleForm {
            day,
            hour,
            minute,
            day_chosen: false,
            time_chosen: false,
        }
    }

    pub fn choose_day(&mut self, day: Day) {
        self.day = day;
        self.day_chosen = true;
    }

    pub fn choose_time(&mut self, hour: u32, minute: u32) {
        self.hour = hour.min(23);
        self.minute = minute.min(59);
        self.time_chosen = true;
    }

    /// The form opened at `now`: what nobody chose starts at the next quarter
    /// hour from now, its day and time read together.
    pub fn open(&mut self, now: i64, zone: &dyn Zone) {
        let (day, hour, minute) = start(now, zone);
        if !self.day_chosen {
            self.day = day;
        }
        if !self.time_chosen {
            self.hour = hour;
            self.minute = minute;
        }
    }

    /// A meeting arranged from it: the next opening starts afresh, as the
    /// web's form forgets its day.
    pub fn arranged(&mut self) {
        self.day_chosen = false;
        self.time_chosen = false;
    }

    pub fn instant(&self, zone: &dyn Zone, now: i64) -> i64 {
        instant(self.day, self.hour, self.minute, zone, now)
    }

    /// Whether the time chosen can be arranged at `now` (ScheduleView.cs
    /// `Problem`).
    pub fn problem(&self, zone: &dyn Zone, now: i64) -> Option<TooSoonOrLate> {
        let at = self.instant(zone, now);
        if at < now {
            Some(TooSoonOrLate::Gone)
        } else if at > now + AHEAD_DAYS * 86_400 {
            Some(TooSoonOrLate::TooFar)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New York in 2026: summer time from 07:00 UTC on 8 March to 06:00 UTC
    /// on 1 November.
    struct NewYork;

    const SPRING: i64 = 1_772_953_200; // 2026-03-08T07:00:00Z
    const AUTUMN: i64 = 1_793_512_800; // 2026-11-01T06:00:00Z

    impl Zone for NewYork {
        fn offset_at(&self, utc: i64) -> i32 {
            if (SPRING..AUTUMN).contains(&utc) {
                -4 * 3600
            } else {
                -5 * 3600
            }
        }
    }

    struct Utc;
    impl Zone for Utc {
        fn offset_at(&self, _: i64) -> i32 {
            0
        }
    }

    fn day(year: i32, month: u32, day: u32) -> Day {
        Day { year, month, day }
    }

    #[test]
    fn the_calendar_counts_days_both_ways() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2026, 11, 1), 20_758);
        assert_eq!(civil_from_days(20_758), (2026, 11, 1));
        assert_eq!(civil_from_days(days_from_civil(2028, 2, 29)), (2028, 2, 29));
        assert_eq!(day(2026, 12, 31).add_days(1), day(2027, 1, 1));
    }

    #[test]
    fn the_form_starts_at_the_next_quarter_hour_on_its_own_day() {
        // 23:50 UTC: the next quarter is midnight, on the next day.
        let now = days_from_civil(2026, 6, 1) * 86_400 + 23 * 3600 + 50 * 60;
        assert_eq!(start(now, &Utc), (day(2026, 6, 2), 0, 0));
        // On a quarter exactly, the next one.
        let on = days_from_civil(2026, 6, 1) * 86_400 + 9 * 3600;
        assert_eq!(start(on, &Utc), (day(2026, 6, 1), 9, 15));
    }

    #[test]
    fn ten_minutes_before_the_clocks_go_forward_the_form_offers_three() {
        // 01:50 in New York on 8 March, ten minutes before 02:00 becomes
        // 03:00: the next quarter is 03:00 on the wall, not 02:00.
        let now = SPRING - 10 * 60;
        assert_eq!(start(now, &NewYork), (day(2026, 3, 8), 3, 0));
    }

    #[test]
    fn a_skipped_time_moves_on_by_the_gap() {
        let at = instant(day(2026, 3, 8), 2, 30, &NewYork, 0);
        // 02:30 read at -5 is 07:30 UTC, which New York calls 03:30.
        assert_eq!(at, SPRING + 30 * 60);
    }

    #[test]
    fn a_time_passed_twice_is_the_first_unless_it_has_gone() {
        // 01:30 on 1 November happens at 05:30 UTC (at -4) and 06:30 UTC (at -5).
        let first = AUTUMN - 30 * 60;
        let second = AUTUMN + 30 * 60;
        assert_eq!(instant(day(2026, 11, 1), 1, 30, &NewYork, first - 3600), first);
        assert_eq!(instant(day(2026, 11, 1), 1, 30, &NewYork, first + 60), second);
        assert_eq!(instant(day(2026, 11, 1), 1, 30, &NewYork, second + 60), first);
    }

    #[test]
    fn a_day_after_the_clocks_went_back_is_read_in_its_own_offset() {
        // Chosen in October (at -4) for 10:00 on 2 November (at -5): 15:00 UTC.
        let now = AUTUMN - 10 * 86_400;
        let at = instant(day(2026, 11, 2), 10, 0, &NewYork, now);
        assert_eq!(at, days_from_civil(2026, 11, 2) * 86_400 + 15 * 3600);
    }

    #[test]
    fn what_nobody_chose_starts_again_and_what_was_chosen_stays() {
        let now = days_from_civil(2026, 6, 1) * 86_400 + 9 * 3600;
        let mut form = ScheduleForm::new(now, &Utc);
        form.choose_day(day(2026, 6, 5));
        form.open(now + 3 * 3600, &Utc);
        assert_eq!(form.day, day(2026, 6, 5));
        assert_eq!((form.hour, form.minute), (12, 15));
        form.arranged();
        form.open(now, &Utc);
        assert_eq!(form.day, day(2026, 6, 1));
    }

    #[test]
    fn a_time_gone_or_too_far_is_said() {
        let now = days_from_civil(2026, 6, 1) * 86_400 + 9 * 3600;
        let mut form = ScheduleForm::new(now, &Utc);
        assert_eq!(form.problem(&Utc, now), None);
        form.choose_time(8, 0);
        assert_eq!(form.problem(&Utc, now), Some(TooSoonOrLate::Gone));
        form.choose_day(day(2026, 8, 15));
        form.choose_time(9, 0);
        assert_eq!(form.problem(&Utc, now), Some(TooSoonOrLate::TooFar));
    }
}
