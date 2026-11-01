//! What the deployment's refusals and its instants read as: the sentences of
//! api.ts `explain`, Account.tsx and Schedule.tsx (Refusals.cs and Android's
//! AppModel and LobbyModel, sentence for sentence), and a time said in the
//! reader's language and zone (meeting.ts `whenSaid`). The server sends codes
//! and the client owns the sentence.

use tomoshibi_protocol::ProtocolError;

use crate::i18n::{language, t, t_with, Language};

/// The code of a refusal, or none for anything else.
pub fn code(fault: &ProtocolError) -> Option<&str> {
    match fault {
        ProtocolError::Refused { code, .. } => Some(code),
        _ => None,
    }
}

/// api.ts `explain`, for the room somebody was trying.
pub fn explain_join(fault: &ProtocolError, room: &str) -> String {
    let scope = scope_of(room);
    match fault {
        ProtocolError::Network { .. } => t("Could not reach the server."),
        ProtocolError::Refused { code, .. } => match code.as_str() {
            "rate_limited" => t("Too many attempts. Try again in a moment."),
            "invalid_room" => t("Room names can only use lowercase letters, numbers and dashes."),
            "room_not_open" => t_with(
                "{room} isn't open. Ask the organiser for the link.",
                &[("room", &room)],
            ),
            "not_invited" => t_with(
                "{room} is by invitation. Ask the organiser for a link.",
                &[("room", &room)],
            ),
            "not_in_scope" => t_with(
                "Only members of {scope} can join {room}.",
                &[("room", &room), ("scope", &scope)],
            ),
            "relay_not_allowed" => t("Access denied. That server is for administrators."),
            "blocked" => t("This passphrase cannot join. Ask the organiser."),
            "not_yours" => t("That name and passphrase do not go together."),
            "server_error" => t("Something went wrong. Try again."),
            _ => t_with("Could not join {room}.", &[("room", &room)]),
        },
        _ => t("Something went wrong. Try again."),
    }
}

/// Whether the problem shown is one of the two refusals a knock answers
/// (PreJoin.tsx ~521).
pub fn can_knock(problem: Option<&str>, room: &str) -> bool {
    let Some(problem) = problem else {
        return false;
    };
    let scope = scope_of(room);
    problem
        == t_with(
            "{room} is by invitation. Ask the organiser for a link.",
            &[("room", &room)],
        )
        || problem
            == t_with(
                "Only members of {scope} can join {room}.",
                &[("room", &room), ("scope", &scope)],
            )
}

/// Account.tsx `explainLink`.
pub fn explain_link(fault: &ProtocolError) -> String {
    match code(fault) {
        Some("not_yours") => t("You can make links only into rooms of your own scopes."),
        Some("name_too_long") => t("That name is too long."),
        Some("bad_time") => {
            t("That window cannot be used. It has to end after it starts, and after now.")
        }
        Some("invalid_room") => t("Room names can only use lowercase letters, numbers and dashes."),
        Some("rate_limited") => t("Too many attempts. Try again in a moment."),
        Some("not_allowed") => t("Guest links have not been opened to you."),
        _ => t("That link could not be made. Try again."),
    }
}

/// Account.tsx `Passphrase`'s `explain`.
pub fn explain_passphrase(fault: &ProtocolError) -> String {
    match code(fault) {
        Some("not_yours") => t("That is not your current passphrase."),
        Some("passphrase_too_short") => t("At least eight characters."),
        Some("passphrase_unchanged") => t("That is the passphrase you already have."),
        Some("passphrase_in_use") => {
            t("Somebody else already uses that passphrase. Choose another.")
        }
        _ => t("That could not be changed. Try again."),
    }
}

/// Schedule.tsx `explainArranging`.
pub fn explain_arranging(fault: &ProtocolError) -> String {
    match code(fault) {
        Some("room_already_arranged") => t("That room already has a meeting arranged."),
        Some("bad_time") => t("That time is not one a meeting can be arranged for."),
        Some("meetings_need_invites") => {
            t("Meetings here ask for an account, so a link cannot let anybody in.")
        }
        Some("not_yours") => t("That is somebody else's room."),
        Some("too_many_meetings") => t("You have as many meetings arranged as you can have."),
        Some("invalid_room") => t("Room names can only use lowercase letters, numbers and dashes."),
        Some("scoped_room") => t(
            "A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.",
        ),
        Some("relay_not_allowed") => t("Access denied. That server is for administrators."),
        Some("not_signed_in") => t("Your session has expired. Sign in again."),
        Some("not_allowed") => t("Arranging meetings has not been opened to you."),
        _ => t("Something went wrong. Try again."),
    }
}

/// The scope a room is held under, or empty for a plain room.
pub fn scope_of(room: &str) -> String {
    tomoshibi_protocol::identity::scope_of(room.to_owned())
}

/// An instant the server wrote (RFC 3339, with or without fractions of a
/// second), as seconds since 1970.
pub fn instant(text: &str) -> Option<i64> {
    if text.trim().is_empty() {
        return None;
    }
    glib::DateTime::from_iso8601(text.trim(), Some(&glib::TimeZone::utc()))
        .ok()
        .map(|at| at.to_unix())
}

/// Seconds since 1970, now.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

/// The reader's wall clock at an instant: the fields and the offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wall {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// Seconds east of UTC.
    pub offset: i32,
}

impl Wall {
    /// The instant on this machine's clock, in its own zone.
    pub fn local(at: i64) -> Wall {
        let offset = super::schedule::LocalZone.offset_at(at);
        Wall::at(at, offset)
    }

    pub fn at(at: i64, offset: i32) -> Wall {
        let wall = at + offset as i64;
        let days = wall.div_euclid(86_400);
        let seconds = wall.rem_euclid(86_400);
        let (year, month, day) = super::schedule::civil_from_days(days);
        Wall {
            year,
            month,
            day,
            hour: (seconds / 3600) as u32,
            minute: ((seconds % 3600) / 60) as u32,
            offset,
        }
    }
}

use super::schedule::Zone as _;

/// The marks Chinese and Japanese write a date with (year, month, day), and
/// the halves of the day traditional Chinese puts before the hour, as Intl
/// writes them; escaped, so that the source stays ASCII as the repository
/// asks of everything that is not a phrase table.
const YEAR: &str = "\u{5e74}";
const MONTH: &str = "\u{6708}";
const DAY: &str = "\u{65e5}";
const MORNING: &str = "\u{4e0a}\u{5348}";
const AFTERNOON: &str = "\u{4e0b}\u{5348}";

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The zone as a browser writes it where it has no short name for it: UTC,
/// or GMT and the offset (LobbyModel.cs `Zone`).
pub fn zone(offset: i32) -> String {
    if offset == 0 {
        return "UTC".into();
    }
    let sign = if offset < 0 { '-' } else { '+' };
    let whole = offset.unsigned_abs();
    let (hours, minutes) = (whole / 3600, (whole % 3600) / 60);
    if minutes == 0 {
        format!("GMT{sign}{hours}")
    } else {
        format!("GMT{sign}{hours}:{minutes:02}")
    }
}

/// The time of day as the reader's language writes a short one.
fn time_of_day(wall: &Wall, language: Language) -> String {
    match language {
        Language::En => {
            let hour = match wall.hour % 12 {
                0 => 12,
                other => other,
            };
            let half = if wall.hour < 12 { "AM" } else { "PM" };
            format!("{hour}:{:02} {half}", wall.minute)
        }
        Language::ZhHant => {
            let hour = match wall.hour % 12 {
                0 => 12,
                other => other,
            };
            let half = if wall.hour < 12 { MORNING } else { AFTERNOON };
            format!("{half}{hour}:{:02}", wall.minute)
        }
        Language::ZhHans | Language::Ja => format!("{:02}:{:02}", wall.hour, wall.minute),
    }
}

/// The date at medium length, as Intl writes it for the language.
fn date(wall: &Wall, language: Language) -> String {
    match language {
        Language::En => format!(
            "{} {}, {}",
            MONTHS[(wall.month as usize).saturating_sub(1) % 12],
            wall.day,
            wall.year
        ),
        Language::ZhHans | Language::ZhHant => {
            format!("{}{YEAR}{}{MONTH}{}{DAY}", wall.year, wall.month, wall.day)
        }
        Language::Ja => format!("{}/{:02}/{:02}", wall.year, wall.month, wall.day),
    }
}

/// meeting.ts `whenSaid`: the date, the time and the zone, in the reader's
/// language, on a wall clock given.
pub fn said_on(wall: &Wall, language: Language) -> String {
    let joiner = if language == Language::En { ", " } else { " " };
    format!(
        "{}{joiner}{} {}",
        date(wall, language),
        time_of_day(wall, language),
        zone(wall.offset)
    )
}

/// `said_on` for an instant, in this machine's zone and the language on
/// screen.
pub fn said(at: i64) -> String {
    said_on(&Wall::local(at), language())
}

/// When a recent room was last joined, as short as a sidebar row wants it:
/// the time of day for today, the day and month otherwise.
pub fn lately_on(wall: &Wall, today: &Wall, language: Language) -> String {
    if (wall.year, wall.month, wall.day) == (today.year, today.month, today.day) {
        return time_of_day(wall, language);
    }
    match language {
        Language::En => format!(
            "{} {}",
            MONTHS[(wall.month as usize).saturating_sub(1) % 12],
            wall.day
        ),
        _ => format!("{}{MONTH}{}{DAY}", wall.month, wall.day),
    }
}

pub fn lately(at: i64) -> String {
    lately_on(&Wall::local(at), &Wall::local(now()), language())
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The days of the week from Sunday, as narrow as a calendar's heading
/// writes them (veryShortStandaloneWeekdaySymbols): Chinese numbers the
/// days after Sunday, Japanese names them.
const WEEKDAYS_EN: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
const WEEKDAYS_ZH: [&str; 7] = [
    "\u{65e5}", "\u{4e00}", "\u{4e8c}", "\u{4e09}", "\u{56db}", "\u{4e94}", "\u{516d}",
];
const WEEKDAYS_JA: [&str; 7] = [
    "\u{65e5}", "\u{6708}", "\u{706b}", "\u{6c34}", "\u{6728}", "\u{91d1}", "\u{571f}",
];

/// A month as a calendar's heading says it in the app's language (the
/// Mac's `.year().month(.wide)`), rather than in the system's, which is what
/// GtkCalendar followed: a Japanese window had an English month.
pub fn month_title(year: i32, month: u32, language: Language) -> String {
    match language {
        Language::En => format!(
            "{} {year}",
            MONTH_NAMES[(month as usize).saturating_sub(1) % 12]
        ),
        _ => format!("{year}{YEAR}{month}{MONTH}"),
    }
}

/// The day a week starts on in the language's own region, 0 for Sunday:
/// Monday in mainland China, Sunday in the United States, Taiwan and Japan.
pub fn first_weekday(language: Language) -> u32 {
    match language {
        Language::ZhHans => 1,
        _ => 0,
    }
}

/// The calendar's heading of weekdays, starting on [`first_weekday`].
pub fn weekday_letters(language: Language) -> [&'static str; 7] {
    let names = match language {
        Language::En => WEEKDAYS_EN,
        Language::ZhHans | Language::ZhHant => WEEKDAYS_ZH,
        Language::Ja => WEEKDAYS_JA,
    };
    let first = first_weekday(language) as usize;
    std::array::from_fn(|index| names[(first + index) % 7])
}

/// A day of the calendar as the language writes the date, for a day's
/// button to be read out by.
pub fn day_said(year: i32, month: u32, day: u32, language: Language) -> String {
    date(
        &Wall {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            offset: 0,
        },
        language,
    )
}

/// A size in megabytes with one decimal, as the update's progress says it.
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calendar_speaks_the_app_s_language() {
        assert_eq!(month_title(2026, 11, Language::En), "November 2026");
        assert_eq!(
            month_title(2026, 11, Language::Ja),
            "2026\u{5e74}11\u{6708}"
        );
        assert_eq!(weekday_letters(Language::En)[0], "S");
        // Japanese: Sunday first, the days named.
        assert_eq!(weekday_letters(Language::Ja)[1], "\u{6708}");
        // Mainland Chinese: Monday first.
        assert_eq!(weekday_letters(Language::ZhHans)[0], "\u{4e00}");
        assert_eq!(weekday_letters(Language::ZhHans)[6], "\u{65e5}");
        assert_eq!(weekday_letters(Language::ZhHant)[0], "\u{65e5}");
    }

    #[test]
    fn instants_are_read_with_or_without_fractions() {
        assert_eq!(instant("2026-11-01T10:00:00Z"), Some(1_793_527_200));
        assert_eq!(instant("2026-11-01T10:00:00.250Z"), Some(1_793_527_200));
        assert_eq!(instant("2026-11-01T18:00:00+08:00"), Some(1_793_527_200));
        assert_eq!(instant("soon"), None);
        assert_eq!(instant(""), None);
    }

    #[test]
    fn a_time_is_said_as_a_browser_says_it() {
        // 10:30 UTC on 2026-11-01, read at GMT+8.
        let wall = Wall::at(1_793_527_200 + 30 * 60, 8 * 3600);
        assert_eq!((wall.hour, wall.minute), (18, 30));
        assert_eq!(said_on(&wall, Language::En), "Nov 1, 2026, 6:30 PM GMT+8");
        assert_eq!(
            said_on(&wall, Language::ZhHans),
            format!("2026{YEAR}11{MONTH}1{DAY} 18:30 GMT+8")
        );
        assert_eq!(
            said_on(&wall, Language::ZhHant),
            format!("2026{YEAR}11{MONTH}1{DAY} {AFTERNOON}6:30 GMT+8")
        );
        assert_eq!(said_on(&wall, Language::Ja), "2026/11/01 18:30 GMT+8");
        assert_eq!(zone(0), "UTC");
        assert_eq!(zone(-(3 * 3600 + 30 * 60)), "GMT-3:30");
    }

    #[test]
    fn midnight_and_noon_are_twelve() {
        let midnight = Wall::at(1_793_491_200, 0);
        assert_eq!((midnight.hour, midnight.day), (0, 1));
        assert_eq!(time_of_day(&midnight, Language::En), "12:00 AM");
        let noon = Wall::at(1_793_491_200 + 12 * 3600, 0);
        assert_eq!(time_of_day(&noon, Language::En), "12:00 PM");
    }

    #[test]
    fn lately_is_the_time_today_and_the_day_before() {
        let today = Wall::at(1_793_527_200, 0);
        let earlier = Wall::at(1_793_527_200 - 3600, 0);
        let yesterday = Wall::at(1_793_527_200 - 86_400, 0);
        assert_eq!(lately_on(&earlier, &today, Language::ZhHans), "09:00");
        assert_eq!(lately_on(&yesterday, &today, Language::En), "Oct 31");
        assert_eq!(
            lately_on(&yesterday, &today, Language::Ja),
            format!("10{MONTH}31{DAY}")
        );
    }

    #[test]
    fn a_refusal_is_a_sentence_for_its_room() {
        let refused = |code: &str| ProtocolError::Refused {
            status: 403,
            code: code.into(),
        };
        assert_eq!(
            explain_join(&refused("not_invited"), "team"),
            "team is by invitation. Ask the organiser for a link."
        );
        assert_eq!(
            explain_join(&refused("not_in_scope"), "team@acme"),
            "Only members of acme can join team@acme."
        );
        assert_eq!(
            explain_join(&refused("odd"), "team"),
            "Could not join team."
        );
        assert!(can_knock(
            Some("team is by invitation. Ask the organiser for a link."),
            "team"
        ));
        assert!(!can_knock(Some("Could not join team."), "team"));
        assert_eq!(
            explain_arranging(&refused("room_already_arranged")),
            "That room already has a meeting arranged."
        );
    }
}
