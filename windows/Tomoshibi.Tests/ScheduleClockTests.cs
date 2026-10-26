using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// Where the schedule's form starts and what it names (ScheduleClock.cs); the
/// Apple apps' ScheduleClockTests and Android's ScheduleClockTest, case for
/// case. What these guard: the form starting on today's date with a time
/// after midnight, which had gone before anybody touched it (from 23:45 on,
/// at the end of a month and of a year as well), a time the clocks pass
/// twice read as the one that has gone, and a day after a change of the
/// clocks read in the offset of the day the form was opened on.
/// </summary>
public class ScheduleClockTests
{
    private static TimeZoneInfo Zone(string id) => TimeZoneInfo.FindSystemTimeZoneById(id);

    private static DateTimeOffset At(string text) => DateTimeOffset.Parse(text, System.Globalization.CultureInfo.InvariantCulture);

    /// <summary>The form as it starts at <paramref name="now"/>, and the instant it names then.</summary>
    private static (DateTime Day, TimeSpan Time, DateTimeOffset At) Starting(DateTimeOffset now, string zone)
    {
        var (day, time) = ScheduleClock.Start(now, Zone(zone));
        return (day, time, ScheduleClock.Instant(day, time, Zone(zone), now));
    }

    [Fact]
    public void MiddayStartsAtTheNextQuarterOfTheSameDay()
    {
        var form = Starting(At("2026-10-27T10:07:30+08:00"), "Asia/Hong_Kong");
        Assert.Equal(new DateTime(2026, 10, 27), form.Day);
        Assert.Equal(new TimeSpan(10, 15, 0), form.Time);
        Assert.Equal(At("2026-10-27T10:15:00+08:00"), form.At);
    }

    [Fact]
    public void TenToMidnightStartsAtMidnightOnTheNextDay()
    {
        var form = Starting(At("2026-10-27T23:50:00+08:00"), "Asia/Hong_Kong");
        Assert.Equal(new DateTime(2026, 10, 28), form.Day);
        Assert.Equal(TimeSpan.Zero, form.Time);
        Assert.Equal(At("2026-10-28T00:00:00+08:00"), form.At);
    }

    [Fact]
    public void AMinuteToMidnightStartsAtMidnightOnTheNextDay()
    {
        var form = Starting(At("2026-10-27T23:59:59+08:00"), "Asia/Hong_Kong");
        Assert.Equal(new DateTime(2026, 10, 28), form.Day);
        Assert.Equal(TimeSpan.Zero, form.Time);
        Assert.Equal(At("2026-10-28T00:00:00+08:00"), form.At);
    }

    [Fact]
    public void TheLastNightOfAMonthStartsOnTheFirstOfTheNext()
    {
        var form = Starting(At("2026-10-31T23:50:00+08:00"), "Asia/Hong_Kong");
        Assert.Equal(new DateTime(2026, 11, 1), form.Day);
        Assert.Equal(At("2026-11-01T00:00:00+08:00"), form.At);
    }

    [Fact]
    public void TheLastNightOfAYearStartsOnNewYearsDay()
    {
        var form = Starting(At("2026-12-31T23:55:00+08:00"), "Asia/Hong_Kong");
        Assert.Equal(new DateTime(2027, 1, 1), form.Day);
        Assert.Equal(TimeSpan.Zero, form.Time);
        Assert.Equal(At("2027-01-01T00:00:00+08:00"), form.At);
    }

    /// <summary>New York puts its clocks forward at 02:00 to 03:00: ten minutes before, the next quarter is 03:00, not a 02:00 that day does not have.</summary>
    [Fact]
    public void ClocksPutForwardStartAfterTheSkippedHour()
    {
        var form = Starting(At("2026-03-08T01:50:00-05:00"), "America/New_York");
        Assert.Equal(new DateTime(2026, 3, 8), form.Day);
        Assert.Equal(new TimeSpan(3, 0, 0), form.Time);
        Assert.Equal(At("2026-03-08T03:00:00-04:00"), form.At);
    }

    /// <summary>New York puts its clocks back at 02:00 to 01:00: ten minutes before, the next quarter is the second 01:00.</summary>
    [Fact]
    public void ClocksPutBackStartAtTheSecondPassOfTheHour()
    {
        var form = Starting(At("2026-11-01T01:50:00-04:00"), "America/New_York");
        Assert.Equal(new DateTime(2026, 11, 1), form.Day);
        Assert.Equal(new TimeSpan(1, 0, 0), form.Time);
        Assert.Equal(At("2026-11-01T01:00:00-05:00"), form.At);
    }

    /// <summary>A time in the hour passed twice, chosen by hand: the first while it is still to come, the second once the first has gone.</summary>
    [Fact]
    public void AnHourPassedTwiceIsTheFirstUntilItHasGone()
    {
        var zone = Zone("America/New_York");
        var day = new DateTime(2026, 11, 1);
        var half = new TimeSpan(1, 30, 0);
        Assert.Equal(At("2026-11-01T01:30:00-04:00"), ScheduleClock.Instant(day, half, zone, At("2026-11-01T00:30:00-04:00")));
        Assert.Equal(At("2026-11-01T01:30:00-05:00"), ScheduleClock.Instant(day, half, zone, At("2026-11-01T01:40:00-04:00")));
        Assert.Equal(At("2026-11-01T01:30:00-04:00"), ScheduleClock.Instant(day, half, zone, At("2026-11-01T01:40:00-05:00")));
    }

    /// <summary>
    /// A day after the clocks go back, chosen before they do, is read in its
    /// own offset: 10:00 on 10 November in New York is 15:00 UTC, where the
    /// form arranged it for 14:00, an hour early, in October's offset.
    /// </summary>
    [Fact]
    public void ADayAfterTheClocksChangeIsReadInItsOwnOffset()
    {
        var zone = Zone("America/New_York");
        Assert.Equal(At("2026-11-10T10:00:00-05:00"),
            ScheduleClock.Instant(new DateTime(2026, 11, 10), new TimeSpan(10, 0, 0), zone, At("2026-10-27T09:00:00-04:00")));
        Assert.Equal(At("2026-03-20T10:00:00-04:00"),
            ScheduleClock.Instant(new DateTime(2026, 3, 20), new TimeSpan(10, 0, 0), zone, At("2026-03-01T09:00:00-05:00")));
    }

    /// <summary>A time the clocks skip is moved on by the gap: 02:30 on the night New York goes forward is 03:30.</summary>
    [Fact]
    public void ASkippedTimeIsMovedOnByTheGap()
    {
        var zone = Zone("America/New_York");
        Assert.Equal(At("2026-03-08T03:30:00-04:00"),
            ScheduleClock.Instant(new DateTime(2026, 3, 8), new TimeSpan(2, 30, 0), zone, At("2026-03-07T12:00:00-05:00")));
    }

    /// <summary>Santiago puts its clocks forward at midnight, from 00:00 to 01:00: ten minutes to midnight the next quarter is 01:00 on the next day.</summary>
    [Fact]
    public void ClocksPutForwardAtMidnightStartOnTheNextDay()
    {
        var form = Starting(At("2026-09-05T23:50:00-04:00"), "America/Santiago");
        Assert.Equal(new DateTime(2026, 9, 6), form.Day);
        Assert.Equal(new TimeSpan(1, 0, 0), form.Time);
        Assert.Equal(At("2026-09-06T01:00:00-03:00"), form.At);
    }

    /// <summary>
    /// Every minute of the two days around each change of the clocks, and of
    /// the turn of a month and a year: the form starts at the next quarter
    /// hour, ahead of now.
    /// </summary>
    [Theory]
    [InlineData("America/New_York", "2026-03-07T12:00:00-05:00")]
    [InlineData("America/New_York", "2026-10-31T12:00:00-04:00")]
    [InlineData("Europe/London", "2026-03-28T12:00:00+00:00")]
    [InlineData("Europe/London", "2026-10-24T12:00:00+01:00")]
    [InlineData("America/Santiago", "2026-09-05T12:00:00-04:00")]
    [InlineData("America/Santiago", "2026-04-04T12:00:00-03:00")]
    [InlineData("Australia/Lord_Howe", "2026-10-03T12:00:00+10:30")]
    [InlineData("Asia/Kathmandu", "2026-12-31T12:00:00+05:45")]
    [InlineData("Asia/Hong_Kong", "2026-12-31T12:00:00+08:00")]
    public void EveryMinuteAroundTheChangesStartsAtTheNextQuarter(string zone, string from)
    {
        var start = At(from);
        for (var minute = 0; minute < 48 * 60; minute++)
        {
            var now = start.AddSeconds(minute * 60 + 17);
            var form = Starting(now, zone);
            Assert.True(form.At == ScheduleClock.NextQuarter(now), $"{zone} at {now:O}: {form.At:O}");
            Assert.True(form.At > now, $"{zone} at {now:O}");
        }
    }
}
