using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Screens;
using Tomoshibi.UI.Settings;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// The lobby of somebody signed in (Android's ui/lobby/Lobby.kt): on a wide
/// window the sidebar of the control layer's material with the camera
/// filling the room behind it, and the home, the scheduled meetings or the
/// relays beside it; on a narrow one the phone's lobby, a list under a bar
/// with the join bar at its foot. A window dragged across the 600-point line
/// keeps where it was and what was half typed.
/// </summary>
internal sealed class LobbyScreen : Screen
{
    public enum Place { Home, Scheduled, Servers }

    private readonly Account _account;
    private Place _place = Place.Home;
    private string _typed = "";
    private bool? _wide;
    private double _width;
    private Action? _closeSheet;
    private Action? _closeSettings;
    private Action? _closeSchedule;
    private SettingsView? _settings;
    private bool _settingsWide;
    private MeetingSheet? _meeting;

    // What the current layout keeps to update.
    private Shared.ProblemToast? _toast;
    private Grid? _section;
    private Place? _sectionShown;
    private StackPanel? _sidebarLists;
    private StackPanel? _homeLists;
    private string? _listsSaid;
    private Shared.CameraStage? _stage;
    private Grid? _sidebarFoot;
    private Grid? _servers;
    private Tap? _joinTyped;

    public LobbyScreen(AppModel app, Account account) : base(app)
    {
        _account = account;
        _ = app.Lobby.RefreshArrangements();
        SizeChanged += (_, e) =>
        {
            _width = e.NewSize.Width;
            Build(Wide(e.NewSize.Width));
        };
        Loaded += (_, _) => OpenFromLaunch();
        // The lists change with the lobby's own reads (the meetings, the
        // relays measured), not only with the app's.
        app.Lobby.Changed += Update;
    }

    private string? AvatarAddress => App.AvatarAddress;

    /// <summary>What a test asked to open over the lobby (TOMOSHIBI_SHOW), for pictures of it: Android's LobbyDebug.</summary>
    private void OpenFromLaunch()
    {
        switch (App.Launch.Show)
        {
            case "start": OpenSheet(start: true, ""); break;
            case "join": OpenSheet(start: false, ""); break;
            case "schedule": Go(Place.Scheduled); break;
            case "servers": Go(Place.Servers); break;
            case { } show when show.StartsWith("settings"): OpenSettings(show.Contains(':') ? show[(show.IndexOf(':') + 1)..] : null); break;
        }
    }

    private void Build(bool wide)
    {
        if (_wide == wide) return;
        _wide = wide;
        Relaying();
        Children.Clear();
        _section = null;
        _sectionShown = null;
        _sidebarLists = null;
        _homeLists = null;
        _listsSaid = null;
        _servers = null;
        if (wide) BuildWide();
        else BuildNarrow();
        _toast = new Shared.ProblemToast(App, wide ? 28 : 92);
        Children.Add(_toast);
        Reopen(wide);
        Update();
    }

    /// <summary>
    /// What was open over the lobby as the window crossed the line is made
    /// again in the new width's form, keeping what was typed and chosen, as
    /// Android redraws its sheet when a phone is unfolded with it open: the
    /// start sheet opened in a wide window and kept as a dialog in a narrow
    /// one was the dialog squeezed, not the phone's sheet. A join under way
    /// is left where it is.
    /// </summary>
    private void Reopen(bool wide)
    {
        if (_meeting is { Working: false } meeting && meeting.Wide != wide)
        {
            var (start, room, sealing) = (meeting.Starting, meeting.Room, meeting.Sealing);
            _closeSheet?.Invoke();
            OpenSheet(start, room, sealing);
        }
        if (_settings is { } settings && _settingsWide != wide)
        {
            var section = settings.Section;
            _closeSettings?.Invoke();
            OpenSettings(section);
        }
        // The schedule is a section of the wide lobby and a sheet of the narrow one.
        if (wide) _closeSchedule?.Invoke();
        else if (_place == Place.Scheduled) OpenSchedule();
    }

    public override void Update()
    {
        _toast?.Update();
        _stage?.Update();
        if (!App.OffersRelayChoice && _place == Place.Servers) Go(Place.Home);
        var said = Said();
        if (said != _listsSaid)
        {
            _listsSaid = said;
            if (_sidebarLists != null) FillSidebar(_sidebarLists);
            if (_homeLists != null) FillHome(_homeLists);
        }
        if (_section != null && _sectionShown != _place) ShowSection();
        if (_joinTyped != null) _joinTyped.IsEnabled = RoomNames.ValidTyped(_typed) || SignInScreen.PastedLink(_typed) != null;
        _meeting?.Update();
    }

    /// <summary>What the lists show, so that they are drawn again only when it changes.</summary>
    private string Said()
    {
        var lobby = App.Lobby;
        return string.Join("|",
            _place,
            App.OffersRelayChoice,
            App.RelayChoice,
            lobby.FastestRelay?.Name,
            App.Relays?.Online,
            string.Join(",", lobby.Upcoming.Select(a => a.Id + a.Started)),
            string.Join(",", lobby.RecentHere.Select(r => r.Id + r.At.ToUnixTimeSeconds() / 60)),
            Localization.Shared.Language);
    }

    public override bool Back()
    {
        if (_place != Place.Home)
        {
            Go(Place.Home);
            return true;
        }
        return false;
    }

    public override void Gone()
    {
        App.Lobby.Changed -= Update;
        _closeSheet?.Invoke();
        _closeSettings?.Invoke();
        _closeSchedule?.Invoke();
    }

    public override bool Refresh()
    {
        // Said again in the new words, keeping the section and the sheet.
        var wide = _wide ?? true;
        _wide = null;
        Build(wide);
        _meeting?.Refresh();
        return true;
    }

    private void Go(Place place)
    {
        _place = place;
        Overlay.Current?.ClosePopover();
        if (_wide == false)
        {
            if (place == Place.Scheduled) OpenSchedule();
            ShowServersPage(place == Place.Servers);
            if (place == Place.Home) _closeSchedule?.Invoke();
        }
        Update();
    }

    // The sheet that starts or joins a meeting.

    private void OpenSheet(bool start, string room, bool? sealing = null)
    {
        if (_closeSheet != null) return;
        _meeting = new MeetingSheet(App, _account, start, room, () => _closeSheet?.Invoke(), _wide == true, sealing);
        var close = Overlay.Current!.ShowDialog(_meeting, 560, null, () =>
        {
            _meeting?.Cancel();
        }, sheet: _wide == false, scrolls: _wide == true, cover: _wide == false);
        _closeSheet = () =>
        {
            close();
            _closeSheet = null;
            _meeting = null;
        };
    }

    private void OpenSettings(string? section = null)
    {
        if (_closeSettings != null) return;
        var view = new SettingsView(App, () => _closeSettings?.Invoke(), section);
        _settings = view;
        _settingsWide = _wide == true;
        // A dialog on a wide window; on a phone's width the whole window,
        // as the phone's settings are the whole screen.
        var close = Overlay.Current!.ShowDialog(view, 900, _wide == false ? null : 700, () => _closeSettings?.Invoke(), sheet: _wide == false, scrolls: false, cover: _wide == false);
        _closeSettings = () =>
        {
            close();
            _closeSettings = null;
            _settings = null;
        };
    }

    private void OpenSchedule()
    {
        if (_closeSchedule != null) return;
        var view = new ScheduleView(App, _account, wide: false, close: () => Go(Place.Home));
        var close = Overlay.Current!.ShowDialog(view, 560, null, () => Go(Place.Home), sheet: true);
        _closeSchedule = () =>
        {
            close();
            _closeSchedule = null;
        };
    }

    // Wide

    private void BuildWide()
    {
        Children.Add(new Backdrop());
        // The camera fills the window behind the sidebar on the home, and the
        // evening room shows in the other sections.
        _stage = new Shared.CameraStage(App);
        // Clear: the window's own ground is behind it already.
        _stage.Background = null;
        var cameraLayer = new Grid { Children = { _stage } };
        Children.Add(cameraLayer);
        var sidebarWidth = _width < 760 ? 250 : 284;
        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(sidebarWidth) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var sidebar = Sidebar();
        sidebar.Margin = new Thickness(10, Shared.TitleBar + 4, 10, 10);
        sidebar.Rises();
        row.Children.Add(sidebar);
        _section = new Grid();
        Grid.SetColumn(_section, 1);
        row.Children.Add(_section);
        Children.Add(row);
        ShowSection();
    }

    private void ShowSection()
    {
        if (_section == null) return;
        _sectionShown = _place;
        foreach (var old in _section.Children.ToList()) _section.Children.Remove(old);
        UIElement chosen = _place switch
        {
            Place.Scheduled => new ScheduleView(App, _account, wide: true, close: () => Go(Place.Home)),
            Place.Servers => new ServersView(App, wide: true, back: () => Go(Place.Home)),
            _ => HomeStage(),
        };
        ((FrameworkElement)chosen).Arrives(1f, fromY: 10);
        _section.Children.Add(chosen);
        if (_stage != null) Motion.FadeTo(_stage, _place == Place.Home ? 1 : 0);
    }

    /// <summary>The home beside the sidebar (Lobby.kt <c>HomeStage</c>): a greeting over the camera, the switches at its foot, the relay at its corner.</summary>
    private Grid HomeStage()
    {
        var grid = new Grid();
        var greeting = K.Column(6,
            Shadowed.Text(K.Text(T("Hello, {name}", ("name", _account.Name)), 34, K.Bold)),
            Shadowed.Text(K.Text(T("Start one from the sidebar when you are ready."), 15, ink: Ink.White(0.78f))));
        greeting.Margin = new Thickness(36, Shared.TitleBar + 40, 24, 0);
        greeting.VerticalAlignment = VerticalAlignment.Top;
        greeting.Rises();
        grid.Children.Add(greeting);
        var switches = K.Glass(Shared.DeviceSwitches(App, 44), 30, new Thickness(8));
        switches.HorizontalAlignment = HorizontalAlignment.Center;
        switches.VerticalAlignment = VerticalAlignment.Bottom;
        switches.Margin = new Thickness(0, 0, 0, 28);
        switches.Rises(80);
        grid.Children.Add(switches);
        if (App.OffersRelayChoice)
        {
            var server = Shared.OnGlass(Shared.ServerMenu(App, compact: false, id: "server"));
            server.HorizontalAlignment = HorizontalAlignment.Right;
            server.Margin = new Thickness(0, Shared.TitleBar + 12, 16, 0);
            server.Arrives();
            grid.Children.Add(server);
        }
        return grid;
    }

    /// <summary>The sidebar (Lobby.kt <c>Sidebar</c>): start and join, the sections, what is coming and where they have been, and who is signed in.</summary>
    private Border Sidebar()
    {
        var start = Tap.Words(T("Start"), "video.fill", Look.Prominent, 36);
        start.Tip(T("Start a meeting"));
        start.HorizontalAlignment = HorizontalAlignment.Stretch;
        start.Click += (_, _) => OpenSheet(start: true, "");
        var join = Tap.Words(T("Join"), "link", Look.Plain, 36);
        join.Tip(T("Join a meeting"));
        join.HorizontalAlignment = HorizontalAlignment.Stretch;
        join.Click += (_, _) => OpenSheet(start: false, "");
        var actions = new Grid { ColumnSpacing = 8, Padding = new Thickness(12, 12, 12, 4) };
        actions.ColumnDefinitions.Add(new ColumnDefinition());
        actions.ColumnDefinitions.Add(new ColumnDefinition());
        Grid.SetColumn(join, 1);
        actions.Children.Add(start);
        actions.Children.Add(join);

        _sidebarLists = K.Flowing(2);
        _sidebarLists.Padding = new Thickness(8, 6, 8, 6);
        var scroller = new ScrollViewer { Content = _sidebarLists, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };

        _sidebarFoot = new Grid();
        var shell = new Grid();
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Grid.SetRow(scroller, 1);
        Grid.SetRow(_sidebarFoot, 2);
        shell.Children.Add(actions);
        shell.Children.Add(scroller);
        shell.Children.Add(_sidebarFoot);
        return K.Glass(shell, 22);
    }

    private void FillSidebar(StackPanel lists)
    {
        lists.Children.Clear();
        lists.Children.Add(SidebarItem(T("Home"), "house", Place.Home));
        lists.Children.Add(SidebarItem(T("Scheduled meetings"), "calendar", Place.Scheduled, App.Lobby.Upcoming.Count));
        if (App.OffersRelayChoice) lists.Children.Add(SidebarItem(T("Server"), "server.rack", Place.Servers).Arrives());
        var upcoming = App.Lobby.Upcoming.Take(5).ToList();
        if (upcoming.Count > 0)
        {
            lists.Children.Add(SidebarHeading(T("Upcoming")));
            foreach (var one in upcoming) lists.Children.Add(MeetingButton(one, compact: true));
        }
        var recent = App.Lobby.RecentHere.Take(6).ToList();
        if (recent.Count > 0)
        {
            lists.Children.Add(SidebarHeading(T("Recent")));
            foreach (var item in recent) lists.Children.Add(RecentSidebarRow(item));
        }
        // The foot: the fleet's light, and the account.
        if (_sidebarFoot != null)
        {
            _sidebarFoot.Children.Clear();
            var foot = K.Column(8);
            foot.Padding = new Thickness(10);
            if (Shared.FleetLine(App) is { } fleet)
            {
                fleet.Margin = new Thickness(6, 0, 6, 0);
                foot.Children.Add(fleet);
            }
            foot.Children.Add(AccountMenu());
            _sidebarFoot.Children.Add(foot);
        }
    }

    private Tap SidebarItem(string title, string symbol, Place place, int badge = 0)
    {
        var selected = _place == place;
        var parts = new List<(UIElement, double)>
        {
            K.Fixed(new Border { Width = 20, Child = K.Symbol(symbol, 14, Ink.Tally) }),
            K.Flex(K.Text(title, 13.5, selected ? K.Semibold : K.Regular, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
        };
        if (badge > 0) parts.Add(K.Fixed(K.Text(badge.ToString(), 12, K.Medium, Ink.Muted, tabular: true).At(HorizontalAlignment.Right, VerticalAlignment.Center)));
        var tap = new Tap(K.Line(10, parts.ToArray()), Look.Clear, 34, padding: 10)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(10),
        };
        if (selected) tap.Resting = Ink.TallyShade.WithAlpha(0.20f);
        tap.Named(title);
        tap.Click += (_, _) => Go(place);
        return tap;
    }

    private static TextBlock SidebarHeading(string title) => K.Text(title, 11, K.Semibold, Ink.Muted).Pad(10, 14, 0, 4);

    /// <summary>A meeting, pressed to open its link (Schedule.kt <c>MeetingRow</c> under a press).</summary>
    private Tap MeetingButton(Arrangement one, bool compact)
    {
        var tap = new Tap(ScheduleView.MeetingRow(App, one), Look.Clear, 40, padding: compact ? 8 : 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(10),
        };
        tap.Height = double.NaN;
        tap.Padding = compact ? new Thickness(8, 6, 8, 6) : new Thickness(14, 11, 14, 11);
        tap.Named(one.Room);
        tap.Click += (_, _) =>
        {
            if (App.Lobby.Link(one) is { } link) _ = App.Open(link);
        };
        return tap;
    }

    private Grid RecentSidebarRow(LobbyModel.Recent item)
    {
        var line = K.Line(10,
            K.Fixed(Faces.Initial(item.Room, 26)),
            K.Flex(K.Column(1, K.Text(item.Room, 13, K.Medium, lines: 1), K.Text(Times.Relative(item.At), 11, ink: Ink.Muted, lines: 1))));
        var open = new Tap(line, Look.Clear, 40, padding: 8)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(10),
        };
        open.Height = double.NaN;
        open.Padding = new Thickness(8, 5, 8, 5);
        open.Named(item.Room);
        open.Click += (_, _) => OpenSheet(start: false, item.Room);
        var forget = Tap.Icon("xmark", T("Remove {room} from recent", ("room", item.Room)), Look.Plain, 20, 7.2);
        forget.Click += (_, _) => App.Lobby.Forget(item);
        forget.Margin = new Thickness(0, 0, 6, 0);
        var row = K.Line(6, K.Flex(open), K.Fixed(forget));
        row.Arrives();
        return row;
    }

    /// <summary>The account at the sidebar's foot, and its menu: the settings and signing out (Lobby.kt <c>AccountMenu</c>).</summary>
    private Tap AccountMenu()
    {
        var line = K.Line(10,
            K.Fixed(Faces.AccountAvatar(_account.Name, AvatarAddress, 30)),
            K.Flex(K.Text(_account.Name, 13, K.Semibold, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
            K.Fixed(K.Symbol("chevron.up.chevron.down", 10, Ink.Muted)));
        var tap = new Tap(line, Look.Plain, 46, padding: 10)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(16),
        };
        tap.Named(_account.Name, "lobby.account");
        tap.Click += (_, _) => Overlay.Current?.Toggle("account", tap, () => Menu.Card(
            Menu.Item(T("Settings"), () => { Overlay.Current?.ClosePopover(); OpenSettings(); }, "gearshape"),
            Menu.Rule(),
            Menu.Item(T("Sign out"), () => { Overlay.Current?.ClosePopover(); _ = App.SignOut(); }, "rectangle.portrait.and.arrow.right", role: Menu.Role.Destructive)),
            width: 262, edge: Edge.Above, align: Align.Start);
        return tap;
    }

    // Narrow

    private void BuildNarrow()
    {
        Background = Ink.Brush(Shade.Black);
        var content = K.Column(22);
        content.Padding = new Thickness(16, 0, 16, 0);
        var title = K.Text("Tomoshibi", 34, K.Bold, lines: 1).Pad(0, 0, 0, -12);
        var hero = Hero();
        hero.Rises();
        _homeLists = K.Flowing(22);
        _homeLists.Rises(60);
        content.Children.Add(title);
        content.Children.Add(hero);
        content.Children.Add(_homeLists);
        var column = K.Column(0, K.Spacer(height: Shared.TitleBar + 52), content, K.Spacer(height: 96));
        var scroller = new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Children.Add(scroller);

        // The bar over the top, whose title comes in once the large one has scrolled away.
        var barTitle = K.Text("Tomoshibi", 17, K.Semibold, lines: 1).At(HorizontalAlignment.Center, VerticalAlignment.Center);
        Motion.Hide(barTitle);
        var settings = new Tap(Faces.AccountAvatar(_account.Name, AvatarAddress, 30), Look.Plain, 40, circle: true, label: T("Settings"));
        settings.Click += (_, _) => OpenSettings();
        var barGround = new Border { Opacity = 0 };
        var fade = new LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0.5, 0), EndPoint = new Windows.Foundation.Point(0.5, 1) };
        fade.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.Black.WithAlpha(0.92f)), Offset = 0 });
        fade.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.Black.WithAlpha(0.6f)), Offset = 0.6 });
        fade.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.Black.WithAlpha(0f)), Offset = 1 });
        barGround.Background = fade;
        var bar = new Grid { Height = Shared.TitleBar + 52, VerticalAlignment = VerticalAlignment.Top };
        bar.Children.Add(barGround);
        var barRow = new Grid { Margin = new Thickness(16, Shared.TitleBar, 16, 0) };
        barRow.Children.Add(barTitle);
        barRow.Children.Add(Shared.OnGlass(settings).At(HorizontalAlignment.Right, VerticalAlignment.Center));
        bar.Children.Add(barRow);
        Children.Add(bar);
        var past = false;
        scroller.ViewChanged += (_, _) =>
        {
            var now = scroller.VerticalOffset > 40;
            if (now == past) return;
            past = now;
            Motion.FadeTo(barTitle, now ? 1 : 0);
            barGround.Opacity = now ? 1 : 0;
        };

        Children.Add(JoinBar());
        _servers = null;
        if (_place == Place.Servers) ShowServersPage(true);
        if (_place == Place.Scheduled) OpenSchedule();
    }

    /// <summary>The picture of yourself with the way to start and to schedule (Lobby.kt <c>Hero</c>).</summary>
    private Grid Hero()
    {
        _stage = new Shared.CameraStage(App, 30);
        var start = Tap.Words(T("Start a meeting"), "video.fill", Look.Prominent, 52, 16);
        start.HorizontalAlignment = HorizontalAlignment.Stretch;
        start.Click += (_, _) => OpenSheet(start: true, "");
        var schedule = Tap.Icon("calendar", T("Schedule a meeting"), Look.Plain, 52, 16);
        schedule.Click += (_, _) => Go(Place.Scheduled);
        var actions = K.Line(8, K.Flex(start), K.Fixed(Shared.OnGlass(schedule)));
        actions.VerticalAlignment = VerticalAlignment.Bottom;
        actions.Margin = new Thickness(12);
        var switches = Shared.DeviceSwitches(App, 40);
        switches.HorizontalAlignment = HorizontalAlignment.Right;
        switches.VerticalAlignment = VerticalAlignment.Top;
        switches.Margin = new Thickness(12);
        return new Grid { Height = 292, Children = { _stage, switches, actions } };
    }

    private void FillHome(StackPanel lists)
    {
        lists.Children.Clear();
        var upcoming = App.Lobby.Upcoming.Take(4).ToList();
        if (upcoming.Count > 0)
        {
            var all = new Tap(K.Text(T("All of them"), 16, ink: Ink.Tally), Look.Clear, 36, padding: 8);
            all.Click += (_, _) => Go(Place.Scheduled);
            var rows = upcoming.Select((one, i) => (UIElement)K.Column(0, MeetingButton(one, compact: false), i < upcoming.Count - 1 ? K.Rule(62, 0.12f) : K.Spacer())).ToArray();
            lists.Children.Add(Section(T("Upcoming"), all, rows).Arrives());
        }
        var recent = App.Lobby.RecentHere.Take(4).ToList();
        if (recent.Count > 0)
        {
            var rows = recent.Select((item, i) => (UIElement)K.Column(0, RecentLine(item), i < recent.Count - 1 ? K.Rule(62, 0.12f) : K.Spacer())).ToArray();
            lists.Children.Add(Section(T("Recent"), null, rows).Arrives());
        }
        if (App.OffersRelayChoice) lists.Children.Add(ServerCard().Arrives());
    }

    private static StackPanel Section(string title, UIElement? trailing, UIElement[] rows)
    {
        var header = trailing == null
            ? (UIElement)K.Text(title, 20, K.Bold).Pad(6, 0, 6, 0)
            : K.Line(0, K.Flex(K.Text(title, 20, K.Bold).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(trailing)).Pad(6, 0, 6, 0);
        var card = new Border { CornerRadius = new CornerRadius(24), Background = Ink.Surface, Child = K.Column(0, rows) };
        return K.Column(8, header, card);
    }

    /// <summary>A room joined lately, opened again in one press, with the way to take it off the list (Lobby.kt <c>RecentLine</c>).</summary>
    private Grid RecentLine(LobbyModel.Recent item)
    {
        var chevron = K.Symbol("chevron.right", 13, Ink.MutedAt(0.6f));
        var line = K.Line(12,
            K.Fixed(Faces.Initial(item.Room, 36)),
            K.Flex(K.Column(2, K.Text(item.Room, 16, K.Semibold, lines: 1), K.Text(Times.Relative(item.At), 13, ink: Ink.Muted, lines: 1))),
            K.Fixed(chevron));
        var open = new Tap(line, Look.Clear, 58, padding: 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(0),
        };
        open.Named(item.Room);
        open.Click += (_, _) => OpenSheet(start: false, item.Room);
        // Windows has no swipe on a list a mouse reaches; the row's own
        // close mark, and its context menu, take it off the list.
        var forget = Tap.Icon("xmark", T("Remove {room} from recent", ("room", item.Room)), Look.Plain, 24, 8);
        forget.Click += (_, _) => App.Lobby.Forget(item);
        forget.Margin = new Thickness(0, 0, 8, 0);
        forget.HorizontalAlignment = HorizontalAlignment.Right;
        forget.VerticalAlignment = VerticalAlignment.Center;
        var menu = new MenuFlyout();
        var remove = new MenuFlyoutItem { Text = T("Remove"), Icon = new FontIcon { Glyph = "" } };
        remove.Click += (_, _) => App.Lobby.Forget(item);
        menu.Items.Add(remove);
        open.ContextFlyout = menu;
        // The mark takes the chevron's place while the pointer is on the row
        // or the keyboard on the mark, so that the list at rest is the
        // phone's: a name, when, and the way in. A finger holds the row for
        // its menu instead.
        var row = new Grid { Children = { open, forget } };
        // Hidden, it takes no presses: a finger's press where the chevron is
        // opens the room, and never takes it off the list unseen.
        var over = false;
        Motion.FadeTo(forget, 0);
        forget.IsHitTestVisible = false;
        void Show()
        {
            var shown = over || forget.FocusState != FocusState.Unfocused;
            forget.IsHitTestVisible = shown;
            Motion.FadeTo(forget, shown ? 1 : 0);
            Motion.FadeTo(chevron, shown ? 0 : 1);
        }
        row.PointerEntered += (_, _) => { over = true; Show(); };
        row.PointerExited += (_, _) => { over = false; Show(); };
        forget.GotFocus += (_, _) => Show();
        forget.LostFocus += (_, _) => Show();
        return row;
    }

    /// <summary>The relay, as a card in the narrow lobby (Lobby.kt <c>ServerCard</c>).</summary>
    private Tap ServerCard()
    {
        var subtitle = K.Column(2, K.Text(T("Server"), 16, K.Semibold));
        if (Shared.FleetLine(App) is { } fleet) subtitle.Children.Add(fleet);
        var line = K.Line(12,
            K.Fixed(new Border { Width = 36, Height = 36, CornerRadius = new CornerRadius(11), Background = Ink.White(0.08f), Child = K.Symbol("server.rack", 16, Ink.Muted) }),
            K.Flex(subtitle),
            K.Fixed(K.Text(App.RelayChoice != null ? App.Lobby.Label(App.RelayChoice) : T("Automatic"), 14, ink: Ink.Muted, lines: 1).At(HorizontalAlignment.Right, VerticalAlignment.Center)),
            K.Fixed(K.Symbol("chevron.right", 13, Ink.MutedAt(0.6f))));
        var tap = new Tap(line, Look.Clear, 64, padding: 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(24),
            Resting = Theme.Surface,
        };
        tap.Named(T("Server"));
        tap.Click += (_, _) => Go(Place.Servers);
        return tap;
    }

    /// <summary>The join bar at the foot (Lobby.kt <c>JoinBar</c>): a room's name or a link, and in.</summary>
    private Grid JoinBar()
    {
        var field = new RoomField(T("Room name or invite link"), _typed, _account.Scopes, _account.Admin, capsule: true);
        field.Changed += text =>
        {
            _typed = text;
            Update();
        };
        field.Submitted += JoinTyped;
        _joinTyped = Tap.Icon("arrow.right", T("Join"), Look.Prominent, 54, 20);
        _joinTyped.Click += (_, _) => JoinTyped();
        var fieldGlass = new Border { Child = field, Background = K.Acrylic(), CornerRadius = new CornerRadius(27) };
        var row = K.Line(10, K.Flex(fieldGlass), K.Fixed(_joinTyped));
        row.VerticalAlignment = VerticalAlignment.Bottom;
        row.Margin = new Thickness(16, 0, 16, 6);
        return new Grid { Children = { row }, VerticalAlignment = VerticalAlignment.Bottom };
    }

    private void JoinTyped()
    {
        if (SignInScreen.PastedLink(_typed) is { } link)
        {
            _typed = "";
            _ = App.Open(link);
            return;
        }
        if (string.IsNullOrWhiteSpace(_typed)) return;
        var room = RoomNames.Normalise(_typed);
        if (!RoomNames.ValidTyped(room)) return;
        OpenSheet(start: false, room);
        _typed = "";
    }

    /// <summary>The relays as a page of their own over the narrow lobby, sliding in from the right (Lobby.kt).</summary>
    private void ShowServersPage(bool show)
    {
        if (show && _servers == null)
        {
            var page = new ServersView(App, wide: false, back: () => Go(Place.Home));
            _servers = new Grid { Children = { page } };
            _servers.Arrives(1f, fromX: 420);
            Children.Add(_servers);
        }
        else if (!show && _servers != null)
        {
            Children.Remove(_servers);
            _servers = null;
        }
    }
}
