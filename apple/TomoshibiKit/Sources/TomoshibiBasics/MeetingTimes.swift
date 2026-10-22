import Foundation

/// A meeting's time in words, in the reader's language and zone, written the
/// same by the lobby, the join screens and the Messages extension's cards.
public enum MeetingTimes {
    /// meeting.ts `whenSaid`: the date, the time and the zone, in the
    /// reader's language. The zone is said because a card or a link is read
    /// by people in other zones, and a time without one is a guess.
    public static func said(_ date: Date) -> String {
        date.formatted(Date.FormatStyle(date: .abbreviated, time: .shortened, locale: Localization.shared.language.locale)
            .timeZone(.specificName(.short)))
    }

    /// A meeting's time as a row has room for it: this year's go without
    /// the year, which with the zone cut the time short on a phone ("Oct
    /// 18, 2026 at 10:00 AM G…").
    public static func short(_ date: Date) -> String {
        guard Calendar.current.isDate(date, equalTo: Date(), toGranularity: .year) else { return said(date) }
        return date.formatted(Date.FormatStyle(locale: Localization.shared.language.locale)
            .month(.abbreviated).day().hour().minute().timeZone(.specificName(.short)))
    }
}
