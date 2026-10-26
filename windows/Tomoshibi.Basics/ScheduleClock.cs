namespace Tomoshibi.Basics;

/// <summary>
/// The schedule's form holds a day and a time of day; this is where they
/// start and what instant they name (ScheduleClock.swift on Apple,
/// ScheduleClock.kt on Android, the same rules).
///
/// The form started at the next quarter hour worked out in the offset the
/// clock had at that moment, and read the day and time it held back in the
/// offset the clock had when it was read, whatever day was chosen. Across a
/// change of the clocks both were an hour out: ten minutes before New York
/// puts its clocks forward the form offered 02:00, a time that day does not
/// have, and a meeting chosen for 10:00 on a day after the clocks went back
/// was arranged for 09:00 by them. The next quarter hour is now an instant,
/// said on the wall clock of the zone it falls in, and the day and time are
/// read in the zone's own offset for that day.
/// </summary>
public static class ScheduleClock
{
    private const long Quarter = 15 * 60;

    /// <summary>
    /// The next quarter hour strictly after <paramref name="now"/>
    /// (Schedule.tsx <c>nextQuarter</c>), on the clock rather than by setting
    /// fields. Every zone in use is a whole number of quarter hours from UTC,
    /// so a quarter here is a quarter on the wall.
    /// </summary>
    public static DateTimeOffset NextQuarter(DateTimeOffset now) =>
        DateTimeOffset.FromUnixTimeSeconds(now.ToUnixTimeSeconds() / Quarter * Quarter + Quarter);

    /// <summary>
    /// Where the form starts: the next quarter hour, its day and its time of
    /// day both read from that one instant on <paramref name="zone"/>'s wall
    /// clock, so a quarter after midnight is on the next day.
    /// </summary>
    public static (DateTime Day, TimeSpan Time) Start(DateTimeOffset now, TimeZoneInfo zone)
    {
        var wall = TimeZoneInfo.ConvertTime(NextQuarter(now), zone);
        return (wall.Date, wall.TimeOfDay);
    }

    /// <summary>
    /// The instant the form's <paramref name="day"/> and <paramref name="time"/>
    /// name in <paramref name="zone"/>.
    ///
    /// A time the clocks pass twice, in the hour they are put back, is the
    /// first of the two unless that one has gone by <paramref name="now"/> and
    /// the second has not, as on the other platforms (.NET by itself takes
    /// the second). A time the clocks skip is moved on by the length of the
    /// gap, read in the offset that held before it, as java.time moves it.
    /// </summary>
    public static DateTimeOffset Instant(DateTime day, TimeSpan time, TimeZoneInfo zone, DateTimeOffset now)
    {
        var wall = DateTime.SpecifyKind(day.Date + time, DateTimeKind.Unspecified);
        if (zone.IsAmbiguousTime(wall))
        {
            var offsets = zone.GetAmbiguousTimeOffsets(wall);
            var first = new DateTimeOffset(wall, offsets.Max());
            var last = new DateTimeOffset(wall, offsets.Min());
            return first < now && last >= now ? last : first;
        }
        if (zone.IsInvalidTime(wall))
        {
            var before = wall;
            while (zone.IsInvalidTime(before)) before = before.AddMinutes(-15);
            return new DateTimeOffset(wall, zone.GetUtcOffset(before));
        }
        return new DateTimeOffset(wall, zone.GetUtcOffset(wall));
    }
}
