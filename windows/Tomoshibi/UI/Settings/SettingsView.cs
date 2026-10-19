using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Settings;

/// <summary>
/// The settings (Android's ui/settings/SettingsView.kt): on a wide window a
/// sidebar of the sections beside the page chosen, on a narrow one the list
/// of them, each opening as a page of its own with the way back.
///
/// TODO(windows 2b): Watch together and Music have settings on the other
/// platforms (how sharp a video watched together may get, how music sounds);
/// both features are the next phase's on Windows, and their sections come
/// with them rather than as choices about something that is not there.
/// </summary>
internal sealed class SettingsView : Grid
{
    public enum Topic { General, Media, Account, About }

    private readonly AppModel _app;
    private readonly Action _close;
    private Topic? _topic;
    private bool _chosen;
    private bool? _wide;
    private Grid? _page;
    private Border? _plate;
    private readonly List<(Topic Topic, Tap Row)> _rows = new();

    /// <summary>
    /// The page the person chose, as the constructor takes it, so that the
    /// settings can be opened again where they were; none while the wide
    /// window shows General only because something has to be shown, so that
    /// dragged narrow it opens on the list, as it opened.
    /// </summary>
    public string? Section => !_chosen ? null : _topic switch
    {
        Topic.Media => "media",
        Topic.Account => "account",
        Topic.About => "about",
        Topic.General => "general",
        _ => null,
    };

    public SettingsView(AppModel app, Action close, string? section)
    {
        _app = app;
        _close = close;
        _chosen = section != null;
        _topic = section switch
        {
            "media" => Topic.Media,
            "account" => Topic.Account,
            "about" => Topic.About,
            "general" => Topic.General,
            _ => null,
        };
        MinHeight = 520;
        SizeChanged += (_, e) => Build(e.NewSize.Width >= 600);
        // A choice made here shows at once, here as everywhere else.
        void Redraw()
        {
            _wide = null;
            Build(ActualWidth >= 600);
        }
        Appearance.Shared.Changed += Redraw;
        Localization.Shared.Changed += Redraw;
        Unloaded += (_, _) =>
        {
            Appearance.Shared.Changed -= Redraw;
            Localization.Shared.Changed -= Redraw;
        };
    }

    public static string Title(Topic topic) => topic switch
    {
        Topic.General => T("General"),
        Topic.Media => T("Audio & video"),
        Topic.Account => T("Account"),
        _ => T("About"),
    };

    public static string Detail(Topic topic) => topic switch
    {
        Topic.General => T("Language, colour, and how a call is laid out."),
        Topic.Media => T("What starts on when you join, and which devices."),
        Topic.Account => T("Who you are here, and what you can use."),
        _ => T("The app, and where its code can be read."),
    };

    public static string Symbol(Topic topic) => topic switch
    {
        Topic.General => "slider.horizontal.3",
        Topic.Media => "video.fill",
        Topic.Account => "person.crop.circle.fill",
        _ => "info.circle.fill",
    };

    private void Build(bool wide)
    {
        if (_wide == wide) return;
        _wide = wide;
        Children.Clear();
        _rows.Clear();
        Children.Add(new Backdrop());
        if (wide)
        {
            _topic ??= Topic.General;
            var grid = new Grid();
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(256) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.Children.Add(Sidebar());
            _page = new Grid();
            Grid.SetColumn(_page, 1);
            grid.Children.Add(_page);
            Children.Add(grid);
            ShowPage(animated: false);
        }
        else
        {
            _page = new Grid();
            Children.Add(_page);
            if (_topic == null) ShowList();
            else ShowPage(animated: false);
        }
    }

    private Border Sidebar()
    {
        var column = K.Column(3);
        column.Padding = new Thickness(10);
        var close = Tap.Icon("xmark", T("Close"), Look.Plain, 26, 10.5);
        close.Click += (_, _) => _close();
        var head = K.Line(8, K.Flex(K.Text(T("Settings"), 21, K.Bold, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(close));
        head.Padding = new Thickness(10, 6, 4, 10);
        column.Children.Add(head);
        var rows = new Grid();
        _plate = new Border
        {
            Background = Ink.White(0.11f),
            BorderBrush = Ink.White(0.07f),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(12),
            VerticalAlignment = VerticalAlignment.Top,
        };
        Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.SetIsTranslationEnabled(_plate, true);
        rows.Children.Add(_plate);
        var list = K.Column(3);
        foreach (var topic in Enum.GetValues<Topic>())
        {
            var row = SidebarRow(topic);
            _rows.Add((topic, row));
            list.Children.Add(row);
        }
        rows.Children.Add(list);
        column.Children.Add(rows);
        // Who is signed in, at the sidebar's foot.
        var shell = new Grid();
        shell.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.Children.Add(column);
        if (_app.Account is { } account)
        {
            var foot = K.Card(K.Line(10,
                K.Fixed(Faces.AccountAvatar(account.Name, _app.AvatarAddress, 32)),
                K.Flex(K.Column(1, K.Text(account.Name, 12.5, K.Semibold, lines: 1), K.Text(account.Trip, 11, mono: true, ink: Ink.Muted, lines: 1)))), 14, 0.05f, 0, new Thickness(10));
            foot.Margin = new Thickness(10, 0, 10, 10);
            Grid.SetRow(foot, 1);
            foot.Arrives();
            shell.Children.Add(foot);
        }
        var sidebar = K.Glass(shell, 22);
        sidebar.Margin = new Thickness(10);
        rows.Loaded += (_, _) => MovePlate(animated: false);
        return sidebar;
    }

    private Tap SidebarRow(Topic topic)
    {
        var tap = new Tap(RowLine(topic), Look.Clear, 44, padding: 8)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(12),
        };
        tap.Named(Title(topic));
        tap.Click += (_, _) =>
        {
            if (_topic == topic) return;
            _topic = topic;
            _chosen = true;
            foreach (var (each, row) in _rows) row.Content = RowLine(each);
            MovePlate(animated: true);
            ShowPage(animated: true);
        };
        return tap;
    }

    private Grid RowLine(Topic topic)
    {
        var chosen = topic == _topic;
        return K.Line(10,
            K.Fixed(Pieces.SectionTile(Symbol(topic), 26, chosen)),
            K.Flex(K.Text(Title(topic), 13.5, chosen ? K.Semibold : K.Regular, chosen ? Ink.Foreground : Ink.ForegroundAt(0.82f), lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)));
    }

    /// <summary>The selection plate slides to the chosen row (the first time it is just there).</summary>
    private void MovePlate(bool animated)
    {
        if (_plate == null) return;
        var row = _rows.FirstOrDefault(r => r.Topic == _topic).Row;
        if (row == null || row.ActualHeight <= 0) return;
        var y = row.TransformToVisual((UIElement)_plate.Parent).TransformPoint(new Windows.Foundation.Point(0, 0)).Y;
        _plate.Height = row.ActualHeight;
        var visual = Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(_plate);
        if (!animated)
        {
            visual.Properties.InsertVector3("Translation", new System.Numerics.Vector3(0, (float)y, 0));
            return;
        }
        var compositor = visual.Compositor;
        var move = compositor.CreateVector3KeyFrameAnimation();
        move.InsertKeyFrame(1f, new System.Numerics.Vector3(0, (float)y, 0), Motion.SettleCurve(compositor));
        move.Duration = Motion.Settle;
        visual.StartAnimation("Translation", move);
    }

    /// <summary>The narrow window's list (SettingsView.kt <c>Home</c>): the account first, then the sections.</summary>
    private void ShowList()
    {
        if (_page == null) return;
        var column = K.Column(18);
        // Below the title bar: on a phone's width the settings are the whole
        // window, under the caption buttons.
        column.Padding = new Thickness(16, Shared.TitleBar + 22, 16, 30);
        var close = Tap.Icon("xmark", T("Close"), Look.Plain, 36, 13);
        close.Click += (_, _) => _close();
        column.Children.Add(K.Line(8, K.Flex(K.Text(T("Settings"), 32, K.Bold, lines: 1)), K.Fixed(close)).Rises());

        UIElement accountCard;
        if (_app.Account is { } account)
        {
            accountCard = K.Line(14,
                K.Fixed(Faces.AccountAvatar(account.Name, _app.AvatarAddress, 54)),
                K.Flex(K.Column(3, K.Text(account.Name, 19, K.Semibold, lines: 1), K.Text(account.Trip, 13, mono: true, ink: Ink.Muted, lines: 1)).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
                K.Fixed(K.Symbol("chevron.right", 13, Ink.MutedAt(0.7f))));
        }
        else
        {
            accountCard = K.Line(14,
                K.Fixed(new Mark(54)),
                K.Flex(K.Column(3, K.Text(T("Account"), 19, K.Semibold), K.Text(T("Sign in to change your passphrase or picture."), 13, ink: Ink.Muted))),
                K.Fixed(K.Symbol("chevron.right", 13, Ink.MutedAt(0.7f))));
        }
        var card = new Tap(accountCard, Look.Plain, 80, padding: 16)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(26),
        };
        card.Height = double.NaN;
        card.Padding = new Thickness(16);
        card.Named(T("Account"));
        card.Click += (_, _) => Open(Topic.Account);
        card.Rises(50);
        column.Children.Add(card);

        var list = K.Column(0);
        var topics = new[] { Topic.General, Topic.Media, Topic.About };
        for (var i = 0; i < topics.Length; i++)
        {
            var topic = topics[i];
            var summary = topic switch
            {
                Topic.General => Localization.Shared.FollowsSystem ? T("Same as the system") : Localization.Shared.Language.DisplayName(),
                Topic.Media => string.Join(" · ", new[] { _app.StartWithMicrophone ? T("Microphone") : null, _app.StartWithCamera ? T("Camera") : null }.Where(s => s != null)) is { Length: > 0 } on ? on : Detail(topic),
                _ => Detail(topic),
            };
            var line = K.Line(13,
                K.Fixed(Pieces.SectionTile(Symbol(topic), 32, true)),
                K.Flex(K.Column(2, K.Text(Title(topic), 16, K.Medium, lines: 1), K.Text(summary, 12.5, ink: Ink.Muted, lines: 2))),
                K.Fixed(K.Symbol("chevron.right", 13, Ink.MutedAt(0.7f))));
            var row = new Tap(line, Look.Clear, 60, padding: 14)
            {
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                CornerRadius = new CornerRadius(0),
            };
            row.Height = double.NaN;
            row.Padding = new Thickness(14, 11, 14, 11);
            row.Named(Title(topic));
            row.Click += (_, _) => Open(topic);
            list.Children.Add(row);
            if (i < topics.Length - 1) list.Children.Add(K.Rule(59));
        }
        var listCard = K.Card(list, 26, 0.07f, 0.09f);
        listCard.Rises(100);
        column.Children.Add(listCard);
        Swap(new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto }, fromX: -60);
    }

    private void Open(Topic topic)
    {
        _topic = topic;
        _chosen = true;
        ShowPage(animated: true);
    }

    private void ShowPage(bool animated)
    {
        if (_page == null || _topic is not { } topic) return;
        var wide = _wide == true;
        var body = topic switch
        {
            Topic.General => General.Page(_app),
            Topic.Media => MediaSettings.Page(_app),
            Topic.Account => AccountSettings.Page(_app),
            _ => About.Page(_app),
        };
        var column = K.Column(24);
        if (!wide)
        {
            var back = Tap.Words(T("Settings"), "chevron.left", height: 36, size: 14);
            back.HorizontalAlignment = HorizontalAlignment.Left;
            back.Click += (_, _) =>
            {
                _topic = null;
                ShowList();
            };
            column.Children.Add(Shared.OnGlass(back).Pad(0, Shared.TitleBar + 22, 0, -6));
        }
        column.Children.Add(Pieces.PageHeading(Title(topic), Detail(topic), Symbol(topic)).Rises());
        body.Rises(50);
        column.Children.Add(body);
        column.Padding = wide ? new Thickness(28, 44, 28, 32) : new Thickness(16, 0, 16, 34);
        column.MaxWidth = 620;
        var scroller = new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        if (!animated) Swap(scroller, 0, 0);
        else if (wide) Swap(scroller, 0, 10);
        else Swap(scroller, 400, 0);
    }

    private void Swap(FrameworkElement next, float fromX = 0, float fromY = 0)
    {
        if (_page == null) return;
        foreach (var old in _page.Children.ToList()) _page.Children.Remove(old);
        if (fromX != 0 || fromY != 0) next.Arrives(1f, fromX: fromX, fromY: fromY);
        _page.Children.Add(next);
    }
}

/// <summary>Pieces the settings' pages share (settings/Pieces.kt).</summary>
internal static class Pieces
{
    /// <summary>A section's symbol on its tile, lit in the tally when it is the one chosen.</summary>
    public static Border SectionTile(string symbol, double size, bool lit)
    {
        var tile = new Border
        {
            Width = size,
            Height = size,
            CornerRadius = new CornerRadius(size * 0.28),
            BorderThickness = new Thickness(1),
            Child = K.Symbol(symbol, size * 0.44, lit ? Ink.TallyForeground : Ink.Foreground),
        };
        if (lit)
        {
            var brush = new Microsoft.UI.Xaml.Media.LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0.5, 0), EndPoint = new Windows.Foundation.Point(0.5, 1) };
            brush.GradientStops.Add(new Microsoft.UI.Xaml.Media.GradientStop { Color = Ink.Of(Ink.TallyShade.WithAlpha(0.95f)), Offset = 0 });
            brush.GradientStops.Add(new Microsoft.UI.Xaml.Media.GradientStop { Color = Ink.Of(Ink.TallyShade.WithAlpha(0.72f)), Offset = 1 });
            tile.Background = brush;
            tile.BorderBrush = Ink.White(0.22f);
        }
        else
        {
            tile.Background = Ink.White(0.10f);
            tile.BorderBrush = Ink.White(0.06f);
        }
        return tile;
    }

    public static Grid PageHeading(string title, string detail, string symbol) =>
        K.Line(14, K.Fixed(SectionTile(symbol, 44, true)), K.Flex(K.Column(3, K.Text(title, 24, K.Bold), K.Text(detail, 12.5, ink: Ink.Muted)).At(HorizontalAlignment.Left, VerticalAlignment.Center)));
}
