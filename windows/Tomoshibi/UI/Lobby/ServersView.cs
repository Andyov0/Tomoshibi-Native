using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// The relays, measured from here (Android's ui/lobby/Servers.kt;
/// ServerPicker.tsx): Automatic, which is whichever answers fastest, and
/// every relay under its region with its round trip, in the order the first
/// measurement settled on so that a row does not move from under a pointer
/// already reaching for it.
/// </summary>
internal sealed class ServersView : Grid
{
    private readonly AppModel _app;
    private readonly bool _wide;
    private readonly Action _back;
    private readonly StackPanel _sections = K.Column(20);
    private Tap? _measure;
    private string? _said;

    public ServersView(AppModel app, bool wide, Action back)
    {
        _app = app;
        _wide = wide;
        _back = back;
        if (!wide)
        {
            Children.Add(new Border { Background = Ink.Background });
            Children.Add(new Backdrop());
        }
        if (app.Lobby.Measured == null) _ = app.Lobby.Measure();

        _measure = Tap.Words(T("Measure again"), "arrow.clockwise", height: 32, size: 12.5);
        _measure.Click += async (_, _) => await app.Lobby.Measure();
        var column = K.Column(20);
        column.MaxWidth = 680;
        column.Padding = new Thickness(wide ? 28 : 16, Shared.TitleBar + (wide ? 16 : 52), wide ? 28 : 16, 28);
        var header = new Grid();
        UIElement heading = wide ? K.Column(4, K.Text(T("Server"), 26, K.Bold)) : new Border();
        if (wide && Shared.FleetLine(app) is { } fleet) ((StackPanel)heading).Children.Add(fleet);
        if (!wide && Shared.FleetLine(app) is { } fleetNarrow) heading = fleetNarrow;
        header.Children.Add(heading);
        header.Children.Add(_measure.At(HorizontalAlignment.Right, VerticalAlignment.Center));
        header.Rises();
        column.Children.Add(header);
        _sections.Rises(40);
        column.Children.Add(_sections);
        var scroller = new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Children.Add(scroller);
        if (!wide) Children.Add(PageBar(T("Server"), back));
        // Presses on the page are the page's, not the lobby's under it.
        Background = K.Clear;
        app.Lobby.Changed += Fill;
        Unloaded += (_, _) => app.Lobby.Changed -= Fill;
        Fill();
    }

    /// <summary>A narrow window's page bar (Servers.kt <c>PageBar</c>): back, and the title in the middle.</summary>
    public static Grid PageBar(string title, Action back)
    {
        var backTap = Tap.Icon("chevron.left", T("Back"), Look.Plain, 40, 16);
        backTap.Click += (_, _) => back();
        var bar = new Grid { Height = 52, Margin = new Thickness(16, Shared.TitleBar, 16, 0), VerticalAlignment = VerticalAlignment.Top };
        bar.Children.Add(Shared.OnGlass(backTap).At(HorizontalAlignment.Left, VerticalAlignment.Center));
        bar.Children.Add(K.Text(title, 17, K.Semibold, lines: 1).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        return bar;
    }

    private void Fill()
    {
        var lobby = _app.Lobby;
        var said = $"{_app.RelayChoice}|{lobby.Measuring}|{string.Join(",", lobby.Measured?.Select(p => p.Key + p.Value) ?? Array.Empty<string>())}|{_app.Relays?.Relays.Length}";
        if (said == _said) return;
        _said = said;
        if (_measure != null) _measure.IsEnabled = !lobby.Measuring;
        _sections.Children.Clear();
        var fastest = lobby.FastestRelay;
        _sections.Children.Add(Rows.Section(T("Automatic"), Rows.Card(RelayRow(
            T("Automatic"),
            fastest != null ? $"{T("Whichever answers fastest")} · {lobby.Label(fastest.Name)}" : T("Whichever answers fastest"),
            "wand.and.stars", _app.RelayChoice == null, last: true, control: null, off: false, pick: () => _app.RelayChoice = null))));
        foreach (var group in lobby.Grouped())
        {
            var rows = new List<UIElement>();
            for (var i = 0; i < group.Relays.Count; i++)
            {
                var relay = group.Relays[i];
                var named = Say(relay.Label ?? relay.Name);
                var title = relay.AdminOnly ? named + " [Admin]" : named;
                var detail = relay.Maintenance ? T("Not taking calls") : relay.Fallback ? "[Fallback]" : null;
                UIElement? control = null;
                if (lobby.Measured is not { } measured)
                {
                    if (lobby.Measuring) control = K.Spinner(12);
                }
                else if (measured.TryGetValue(relay.Name, out var ms) && ms is { } millis)
                {
                    control = K.Text($"{Math.Round(millis)} ms", 13, ink: Ink.Latency(millis), tabular: true);
                }
                else if (!relay.Maintenance)
                {
                    control = K.Text(T("Timeout"), 12, ink: Ink.Danger);
                }
                var name = relay.Name;
                rows.Add(RelayRow(title, detail, "server.rack", _app.RelayChoice == name, i == group.Relays.Count - 1, control, relay.Maintenance, () => _app.RelayChoice = name));
            }
            _sections.Children.Add(Rows.Section(group.Region.Length > 0 ? group.Region : T("Server"), Rows.Card(rows.ToArray())));
        }
    }

    /// <summary>One relay, or Automatic (Servers.kt <c>RelayRow</c>): lit faintly in the tally and ticked when chosen.</summary>
    private static Tap RelayRow(string title, string? detail, string symbol, bool chosen, bool last, UIElement? control, bool off, Action pick)
    {
        var trailing = K.Row(10);
        if (control != null) trailing.Children.Add(control);
        var tick = K.Symbol("checkmark.circle.fill", 15, Ink.Tally);
        tick.Opacity = chosen ? 1 : 0;
        trailing.Children.Add(tick);
        foreach (var child in trailing.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        var row = Rows.Row(title, trailing, detail, symbol, last);
        var tap = new Tap(row, Look.Clear, 48, padding: 0)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(0),
            IsEnabled = !off,
        };
        tap.Height = double.NaN;
        if (chosen) tap.Resting = Ink.TallyShade.WithAlpha(0.08f);
        tap.Named(title);
        tap.Click += (_, _) => pick();
        return tap;
    }
}
