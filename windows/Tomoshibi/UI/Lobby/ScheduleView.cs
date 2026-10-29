using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// The meetings this account has arranged, and arranging one (Android's
/// ui/lobby/Schedule.kt; Schedule.tsx): pick a day and a time, hand out the
/// link before it starts. The day and the time are Windows' own calendar
/// and time picker, in the reader's language, the hours on a 24-hour clock
/// and the minutes in quarters as the other apps offer them.
/// </summary>
internal sealed class ScheduleView : Grid, IRestyles
{
    /// <summary>A meeting can be arranged this far ahead (Schedule.kt <c>AHEAD</c>).</summary>
    private const int AheadDays = 60;

    private readonly AppModel _app;
    private readonly Account _account;
    private readonly bool _wide;
    private string _planning = "";
    private DateTimeOffset _day;
    private TimeSpan _time;
    private bool _arranging;
    private string? _copied;
    private DateTimeOffset _now = DateTimeOffset.Now;

    private readonly StackPanel _list = K.Flowing(10);
    private StackPanel? _verdict;
    private Shared.Pressing? _arrange;
    private string? _listSaid;

    public ScheduleView(AppModel app, Account account, bool wide, Action close)
    {
        _app = app;
        _account = account;
        _wide = wide;
        (var day, _time) = ScheduleClock.Start(DateTimeOffset.Now, TimeZoneInfo.Local);
        _day = day;
        _close = close;
        _ = app.Lobby.RefreshArrangements();
        Build();
        app.Lobby.Changed += Fill;
        Unloaded += (_, _) => app.Lobby.Changed -= Fill;
    }

    private readonly Action _close;

    /// <summary>The system went light or dark while the schedule is open as a sheet: drawn again, keeping what was typed and picked.</summary>
    public void Restyle()
    {
        Children.Clear();
        Build();
    }

    private void Build()
    {
        var wide = _wide;
        var close = _close;
        // The list is kept across a drawing again and filled again in it.
        if (_list.Parent is Panel holding) holding.Children.Remove(_list);
        _listSaid = null;
        var heading = K.Column(4,
            K.Text(T("Scheduled meetings"), wide ? 26 : 22, K.Bold),
            K.Text(T("Pick a time and hand out the link before it starts."), wide ? 14 : 13, ink: Ink.Muted));
        var form = Form();
        var page = K.Column(20);
        page.Padding = wide ? new Thickness(28, Shared.TitleBar + 16, 28, 28) : new Thickness(16, 22, 16, 24);
        if (wide)
        {
            page.Children.Add(heading);
            var sideBySide = new Grid { ColumnSpacing = 20 };
            sideBySide.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(360) });
            sideBySide.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            var stacked = K.Column(20);
            void Lay(double width)
            {
                // Side by side once both fit (360 + 20 + 420, and the page's padding).
                var together = width >= 856;
                sideBySide.Children.Clear();
                stacked.Children.Clear();
                if (together)
                {
                    sideBySide.Children.Add(form);
                    Grid.SetColumn(_list, 1);
                    sideBySide.Children.Add(_list);
                }
                else
                {
                    form.MaxWidth = 560;
                    _list.MaxWidth = 560;
                    stacked.Children.Add(form);
                    stacked.Children.Add(_list);
                }
            }
            var holder = new Grid { Children = { sideBySide, stacked } };
            holder.SizeChanged += (_, e) => Lay(e.NewSize.Width + 56);
            page.Children.Add(holder);
        }
        else
        {
            var closeTap = Tap.Icon("xmark", T("Close"), Look.Plain, 32, 13);
            closeTap.Click += (_, _) => close();
            page.Children.Add(K.Line(12, K.Flex(heading), K.Fixed(closeTap.At(HorizontalAlignment.Right, VerticalAlignment.Top))));
            page.Children.Add(form);
            page.Children.Add(_list);
        }
        heading.Rises();
        form.Rises(40);
        _list.Rises(80);
        var scroller = new ScrollViewer { Content = page, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        if (!wide) Children.Add(new Backdrop { Opacity = 0.6 });
        Children.Add(scroller);
        Fill();
    }

    /// <summary>The day and time chosen, in the offset this zone has on that day (ScheduleClock).</summary>
    private DateTimeOffset At => ScheduleClock.Instant(_day.Date, _time, TimeZoneInfo.Local, _now);

    private string? Problem()
    {
        if (At < _now) return T("That time has already gone.");
        if (At > _now.AddDays(AheadDays)) return T("That is more than sixty days away.");
        return null;
    }

    private Border Form()
    {
        var column = K.Column(14);
        var canArrange = _account.Can == null || _account.Can.Contains("arrange");
        if (!canArrange)
        {
            column.Children.Add(K.Text(T("Arranging meetings has not been opened to you."), 13, ink: Ink.Muted));
            return K.Card(column, Sizing.Lobby.CardRadius, 0.07f, 0.09f, new Thickness(18));
        }
        column.Children.Add(Fields.Text(T("Room name"), _planning, text => _planning = text, mono: true));
        column.Children.Add(K.Text(T("Name it yourself, or leave it blank for one nobody has used."), 11.5, ink: Ink.Muted));

        var today = DateTimeOffset.Now.Date;
        var calendar = new CalendarView
        {
            SelectionMode = CalendarViewSelectionMode.Single,
            MinDate = today,
            MaxDate = today.AddDays(AheadDays),
            Language = Localization.Shared.Language.Code(),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            CalendarViewDayItemStyle = Days(),
        };
        calendar.SelectedDates.Add(_day);
        calendar.SetDisplayDate(_day);
        calendar.SelectedDatesChanged += (_, e) =>
        {
            if (e.AddedDates.Count > 0) _day = e.AddedDates[0].Date;
            Say();
        };
        column.Children.Add(new Border { Child = calendar, Background = Ink.Well(0.18f), CornerRadius = new CornerRadius(14), Padding = new Thickness(12) });

        var time = new TimePicker
        {
            ClockIdentifier = "24HourClock",
            MinuteIncrement = 15,
            Time = _time,
            Language = Localization.Shared.Language.Code(),
        };
        time.Named(T("Time"));
        time.SelectedTimeChanged += (_, e) =>
        {
            if (e.NewTime is { } chosen) _time = chosen;
            Say();
        };
        column.Children.Add(K.Line(8, K.Flex(Label(T("Time"), "clock")), K.Fixed(time)));
        if (_app.OffersRelayChoice) column.Children.Add(K.Line(8, K.Flex(Label(T("Server"), "server.rack")), K.Fixed(Shared.ServerMenu(_app, compact: true, id: "schedule.server"))));
        column.Children.Add(K.Rule(0));
        _verdict = K.Row(7);
        column.Children.Add(_verdict);
        _arrange = new Shared.Pressing(T("Arrange"), 40, 13.5);
        _arrange.Click += (_, _) => Plan();
        column.Children.Add(_arrange);
        Say();
        return K.Card(column, Sizing.Lobby.CardRadius, 0.07f, 0.09f, new Thickness(18));
    }

    /// <summary>
    /// The calendar's days 44 high, where the system draws them 40 square,
    /// the smallest things on the sheet (the web's became 44 as well). Only
    /// their height: seven of them across at 44 and the margins would not fit
    /// the narrow sheet. Laid over the system's own style, which still draws
    /// them; this sets nothing else.
    /// </summary>
    private static Style Days()
    {
        var style = new Style(typeof(CalendarViewDayItem));
        style.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, (double)Sizing.Lobby.Day));
        return style;
    }

    private static StackPanel Label(string text, string symbol)
    {
        var row = K.Row(6, K.Symbol(symbol, 12, Ink.Muted), K.Text(text, 13, ink: Ink.Muted));
        row.VerticalAlignment = VerticalAlignment.Center;
        return row;
    }

    /// <summary>What the time chosen is: gone, too far, or when it is arranged for, in the reader's own zone.</summary>
    private void Say()
    {
        if (_verdict == null) return;
        var problem = Problem();
        var ink = problem != null ? Ink.DangerText : Ink.Muted;
        _verdict.Children.Clear();
        _verdict.Children.Add(K.Symbol(problem != null ? "exclamationmark.circle" : "calendar.badge.clock", 12, ink).Pad(0, 1, 0, 0));
        _verdict.Children.Add(K.Text(problem ?? T("Arranged for {when}", ("when", LobbyModel.Said(At))), 12.5, ink: ink));
        if (_arrange != null)
        {
            _arrange.IsEnabled = !_arranging && problem == null;
            _arrange.Busy = _arranging;
        }
    }

    private async void Plan()
    {
        if (_arranging) return;
        _now = DateTimeOffset.Now;
        if (Problem() != null)
        {
            Say();
            return;
        }
        var room = RoomNames.ValidTyped(_planning) ? RoomNames.Normalise(_planning) : RoomNames.Generate();
        _arranging = true;
        Say();
        var (made, refusal) = await _app.Lobby.Arrange(room, At, _app.RelayChoice);
        _arranging = false;
        if (made != null)
        {
            _planning = "";
            // The link is what is handed out, so it is on the clipboard at once.
            if (_app.Lobby.Link(made) is { } link)
            {
                Pasteboard.Copy(link);
                _copied = made.Id;
                _ = Uncopy(made.Id);
            }
        }
        else if (refusal != null)
        {
            _app.Problem = refusal;
        }
        _listSaid = null;
        Say();
        Fill();
    }

    private async Task Uncopy(string id)
    {
        await Task.Delay(1600);
        if (_copied == id) _copied = null;
        _listSaid = null;
        Fill();
    }

    /// <summary>The list under the form (Schedule.kt <c>MeetingList</c>).</summary>
    private void Fill()
    {
        var lobby = _app.Lobby;
        var said = $"{lobby.State}|{string.Join(",", lobby.Arrangements.Select(a => a.Id + a.Started + a.Ended))}|{_copied}";
        if (said == _listSaid) return;
        _listSaid = said;
        _list.Children.Clear();
        if (lobby.State == LobbyModel.Status.SignedOut)
        {
            _list.Children.Add(Notice(T("Your session has expired. Sign in again."), "person.crop.circle.badge.exclamationmark"));
            return;
        }
        if (lobby.State == LobbyModel.Status.Failed)
        {
            var again = Tap.Words(T("Try again"), height: 30, size: 12.5);
            again.Click += async (_, _) => await lobby.RefreshArrangements();
            _list.Children.Add(Notice(T("Could not load your meetings."), "exclamationmark.triangle", again));
            return;
        }
        if (lobby.Arrangements.Count == 0)
        {
            if (lobby.State == LobbyModel.Status.Loading)
            {
                _list.Children.Add(K.Card(K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center), Sizing.Lobby.CardRadius, 0.07f, 0.09f).Tall(140));
                return;
            }
            var empty = K.Column(10, K.Symbol("calendar.badge.clock", 26, Ink.Muted), K.Text(T("No meetings arranged yet."), 14, K.Semibold, align: TextAlignment.Center));
            empty.VerticalAlignment = VerticalAlignment.Center;
            _list.Children.Add(K.Card(empty, Sizing.Lobby.CardRadius, 0.07f, 0.09f).Tall(140));
            return;
        }
        var rows = K.Column(0);
        for (var i = 0; i < lobby.Arrangements.Count; i++)
        {
            rows.Children.Add(MeetingLine(lobby.Arrangements[i]));
            if (i < lobby.Arrangements.Count - 1) rows.Children.Add(K.Rule(_wide ? 0 : 60, 0.12f));
        }
        _list.Children.Add(K.Card(rows, Sizing.Lobby.CardRadius, 0.07f, 0.09f));
    }

    private static Border Notice(string text, string symbol, UIElement? trailing = null)
    {
        var parts = new List<(UIElement, double)> { K.Fixed(K.Symbol(symbol, 14, Ink.Muted)), K.Flex(K.Text(text, 13, ink: Ink.Muted)) };
        if (trailing != null) parts.Add(K.Fixed(trailing));
        return K.Card(K.Line(10, parts.ToArray()), 18, 0.07f, 0.09f, new Thickness(14));
    }

    /// <summary>A meeting in the list (Schedule.kt <c>MeetingLine</c>): opened by a press, its link copied, put in a calendar, or called off.</summary>
    private Grid MeetingLine(Arrangement one)
    {
        var open = new Tap(MeetingRow(_app, one), Look.Clear, 48, padding: 0)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(10),
            IsEnabled = !one.Ended,
            Opacity = one.Ended ? 0.5 : 1,
        };
        open.Height = double.NaN;
        open.Named(one.Room);
        open.Click += (_, _) =>
        {
            if (_app.Lobby.Link(one) is { } link) _ = _app.Open(link);
        };
        var parts = new List<(UIElement, double)> { K.Flex(open) };
        var copied = _copied == one.Id;
        var copy = Tap.Icon(copied ? "checkmark" : "doc.on.doc", copied ? T("Link copied") : T("Copy link"), Look.Plain, 32, 12);
        copy.Click += (_, _) =>
        {
            if (_app.Lobby.Link(one) is not { } link) return;
            Pasteboard.Copy(link);
            _copied = one.Id;
            _listSaid = null;
            Fill();
            _ = Uncopy(one.Id);
        };
        parts.Add(K.Fixed(copy));
        if (_app.Lobby.Link(one) != null)
        {
            var calendar = Tap.Icon("calendar.badge.plus", T("Add to calendar"), Look.Plain, 32, 12);
            calendar.Click += async (_, _) => await AddToCalendar(one);
            parts.Add(K.Fixed(calendar));
        }
        var cancel = Tap.Icon("xmark", T("Cancel meeting"), Look.Plain, 32, 12);
        cancel.Content = K.Symbol("xmark", 12, Ink.DangerText);
        cancel.Click += async (_, _) =>
        {
            if (await _app.Lobby.Cancel(one) is { } refusal) _app.Problem = refusal;
        };
        parts.Add(K.Fixed(cancel));
        var line = K.Line(6, parts.ToArray());
        line.Padding = new Thickness(14, 10, 14, 10);
        line.Arrives();
        return line;
    }

    /// <summary>
    /// The meeting as a calendar file, handed to whatever opens one here
    /// (calendar.ts <c>icsFor</c>; the Apple apps write the same file): the
    /// person's own calendar app takes it from there.
    /// </summary>
    private async Task AddToCalendar(Arrangement one)
    {
        if (_app.Lobby.CalendarText(one) is not { } text) return;
        var path = Path.Combine(Path.GetTempPath(), $"tomoshibi-{one.Room}.ics");
        await File.WriteAllTextAsync(path, text);
        var file = await Windows.Storage.StorageFile.GetFileFromPathAsync(path);
        await Windows.System.Launcher.LaunchFileAsync(file);
    }

    /// <summary>A meeting's line (Schedule.kt <c>MeetingRow</c>): started or not, its room, when, and the relay it is held on.</summary>
    public static Grid MeetingRow(AppModel app, Arrangement one)
    {
        var started = one.Started;
        var badge = new Border
        {
            Width = 34,
            Height = 34,
            CornerRadius = new CornerRadius(10),
            Background = started ? Ink.TallyAt(0.18f) : Ink.Wash(0.08f),
            Child = K.Symbol(started ? "video.fill" : "calendar", 15, started ? Ink.TallyText : Ink.Muted),
        };
        var when = Refusals.Instant(one.At) is { } at ? MeetingSaid(at) : one.At;
        if (!string.IsNullOrEmpty(one.Relay)) when += " · " + app.Lobby.Label(one.Relay);
        var words = K.Column(2, K.Text(one.Room, 14, K.Semibold, lines: 1), K.Words(when, 12, ink: Ink.Muted, lines: 2));
        var parts = new List<(UIElement, double)> { K.Fixed(badge), K.Flex(words) };
        if (started)
        {
            parts.Add(K.Fixed(new Border
            {
                Background = Ink.Tally,
                CornerRadius = new CornerRadius(10),
                Padding = new Thickness(8, 3, 8, 3),
                VerticalAlignment = VerticalAlignment.Center,
                Child = K.Text(T("Under way"), 11, K.Semibold, Ink.TallyForeground, lines: 1),
            }));
        }
        var row = K.Line(12, parts.ToArray());
        row.VerticalAlignment = VerticalAlignment.Center;
        return row;
    }

    /// <summary>When a meeting is: without the year in this one, with it in another, and the zone (Schedule.kt <c>meetingSaid</c>).</summary>
    private static string MeetingSaid(DateTimeOffset at)
    {
        var local = at.ToLocalTime();
        if (local.Year != DateTimeOffset.Now.Year) return LobbyModel.Said(at);
        return Times.Skeleton(at, "MMMdjmm", "month.abbreviated day hour minute") + " " + LobbyModel.Zone(local);
    }
}

internal static class Heights
{
    public static T Tall<T>(this T element, double height) where T : FrameworkElement
    {
        element.MinHeight = height;
        return element;
    }
}
