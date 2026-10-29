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
    private string? _sidebarSaid;
    private string? _footSaid;
    private bool _serverRowShown;
    private Shared.CameraStage? _stage;
    private Grid? _standing;
    private PreviewGlass? _stood;
    private Grid? _sidebarFoot;
    private Grid? _servers;
    private Tap? _joinTyped;
    private (TextBlock Title, TextBlock Line, Grid TitleShadow, Grid LineShadow)? _greeting;
    private bool? _greetingOverPicture;

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
            // A posed update (App.PoseUpdate) seen from the settings' About.
            case { } show when show.StartsWith("update:") && show.EndsWith(":about"): OpenSettings("about"); break;
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
        _standing = null;
        _stood = null;
        _sidebarLists = null;
        _homeLists = null;
        _listsSaid = null;
        _sidebarSaid = null;
        _footSaid = null;
        _serverRowShown = false;
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
            OpenSheet(start, room, sealing, again: true);
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
        Stand();
        PaintGreeting();
        if (!App.OffersRelayChoice && _place == Place.Servers) Go(Place.Home);
        var said = Said();
        if (said != _listsSaid)
        {
            _listsSaid = said;
            if (_homeLists != null) FillHome(_homeLists);
        }
        // The sidebar on its own key, and its foot on another: built again
        // whenever anything the home lists show changed, the sidebar was
        // emptied and filled again for every relay measured, many times in a
        // window's first seconds against a fleet of twenty. A row pressed
        // then was gone before the press was let go, so the press did nothing
        // or landed on what had taken its place, while the rows drawn lagged
        // behind where they were by the transitions of the panel; a person
        // with the pointer on Servers had Scheduled meetings open.
        var sidebar = SidebarSaid();
        if (sidebar != _sidebarSaid)
        {
            _sidebarSaid = sidebar;
            if (_sidebarLists != null) FillSidebar(_sidebarLists);
        }
        var foot = FootSaid();
        if (foot != _footSaid)
        {
            _footSaid = foot;
            FillFoot();
        }
        if (_section != null && _sectionShown != _place) ShowSection();
        if (_joinTyped != null) _joinTyped.IsEnabled = RoomNames.ValidTyped(_typed) || SignInScreen.PastedLink(_typed) != null;
        _meeting?.Update();
    }

    /// <summary>What the sidebar's rows show: which is chosen, whether there is a server to choose, and the meetings listed.</summary>
    private string SidebarSaid()
    {
        var lobby = App.Lobby;
        return string.Join("|",
            _place,
            App.OffersRelayChoice,
            string.Join(",", lobby.Upcoming.Take(5).Select(a => a.Id + a.Started)),
            lobby.Upcoming.Count,
            string.Join(",", lobby.RecentHere.Take(6).Select(r => r.Id + r.At.ToUnixTimeSeconds() / 60)),
            Localization.Shared.Language);
    }

    /// <summary>What the sidebar's foot shows: the fleet's light and the account.</summary>
    private string FootSaid() => string.Join("|", App.Relays?.Online, App.Relays?.Total, App.Account?.Name, Localization.Shared.Language);

    /// <summary>
    /// What the home lists show, so that they are drawn again only when it
    /// changes. Not the fastest relay, which nothing here shows and which
    /// changed with every reading of a measurement, each time drawing every
    /// section again with its arrival.
    /// </summary>
    private string Said()
    {
        var lobby = App.Lobby;
        return string.Join("|",
            _place,
            App.OffersRelayChoice,
            App.RelayChoice,
            App.Relays?.Online,
            App.Relays?.Total,
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

    /// <param name="again">Made again in the new width's form (Reopen), which is the sheet already open, not a new way into a call.</param>
    private void OpenSheet(bool start, string room, bool? sealing = null, bool again = false)
    {
        if (_closeSheet != null) return;
        // Not opened while an update is being installed, rather than opened
        // to refuse its own Start or Join: the reason is said at the foot.
        if (!again && App.RefusedForUpdate()) return;
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
        row.Children.Add(K.Holding(sidebar).Rises());
        // The deployment's notice at the top of the content beside the
        // sidebar, whichever section it is, with the section moved down under
        // it (Android's Lobby.kt): the picture of yourself still fills the
        // stage behind it, and the greeting and the server menu start below.
        _section = new Grid();
        var main = new Grid();
        main.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        main.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        // Every section keeps the title bar's height clear at its top. Under
        // the notice that band is below the notice, not the title bar, and
        // the greeting stood 72 below the card where the narrow lobby and the
        // Apple apps put what follows the notice at the section's own
        // spacing: 40 here, 12 for the server menu. So the notice takes the
        // band over: drawn the title bar's height lower than the room it
        // opens (the margins), its last points lie across the section's top
        // band, which holds nothing, and what is under it starts at its own
        // spacing below the card. The room still opens and closes with the
        // notice, so the section still moves down and up with it as it comes
        // and goes. With no notice nothing of this takes any room.
        var notice = new LobbyNoticeCard(App.Remote, new Thickness(36, 12, 24, 0))
        {
            Margin = new Thickness(0, Shared.TitleBar, 0, -Shared.TitleBar),
        };
        Grid.SetRow(_section, 1);
        main.Children.Add(_section);
        // Over the section, which lies under the card's foot: its close and
        // its words take the presses there.
        main.Children.Add(notice);
        Grid.SetColumn(main, 1);
        row.Children.Add(main);
        Children.Add(row);
        ShowSection();
    }

    private void ShowSection()
    {
        if (_section == null) return;
        _sectionShown = _place;
        _standing = null;
        _stood = null;
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
        // The animals drawn while the camera is off, on the stage where the
        // picture would be, between the greeting and the switches
        // (PlaceStanding).
        _standing = new Grid { IsHitTestVisible = false, HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top };
        _stood = null;
        grid.Children.Add(_standing);
        grid.SizeChanged += (_, e) => PlaceStanding(e.NewSize);
        Stand();
        var hello = K.Text(T("Hello, {name}", ("name", _account.Name)), 34, K.Bold);
        var helloLine = K.Text(T("Start one from the sidebar when you are ready."), 15, ink: Ink.White(0.78f));
        var helloShadow = Shadowed.Text(hello);
        var helloLineShadow = Shadowed.Text(helloLine);
        _greeting = (hello, helloLine, helloShadow, helloLineShadow);
        _greetingOverPicture = null;
        PaintGreeting();
        var greeting = K.Column(6, helloShadow, helloLineShadow);
        // A newer build, at the top of the content beside the sidebar
        // (Lobby.kt puts it there), opening a space above the greeting rather
        // than lying over it; clear of the server menu at the corner.
        var top = K.Column(0);
        top.Children.Add(greeting);
        top.Margin = new Thickness(36, Shared.TitleBar + 40, 24, 0);
        top.VerticalAlignment = VerticalAlignment.Top;
        greeting.Rises();
        grid.Children.Add(top);
        var switches = K.Glass(Shared.DeviceSwitches(App, 44), 30, new Thickness(8));
        switches.HorizontalAlignment = HorizontalAlignment.Center;
        switches.VerticalAlignment = VerticalAlignment.Bottom;
        switches.Margin = new Thickness(0, 0, 0, 28);
        grid.Children.Add(K.Holding(switches).Rises(80));
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

    /// <summary>
    /// The wide home's portraits, while the camera is off: the iPhone's card
    /// has them on a pane of glass; here the stage beside the sidebar is where
    /// the picture of yourself is, and they stand on its foot.
    /// </summary>
    private void Stand()
    {
        if (_standing == null) return;
        var wanted = _stage?.Showing != true;
        if (wanted && _stood == null)
        {
            _stood = new PreviewGlass(0, pane: false, App.Launch.Portrait).Arrives(1f);
            _standing.Children.Add(_stood);
        }
        else if (!wanted && _stood != null)
        {
            _standing.Children.Remove(_stood);
            _stood = null;
        }
    }

    /// <summary>
    /// Where the animals stand on the wide stage: art/README.md's desktop
    /// formula, which the Mac's lobby follows (PortraitStage): the canvas a
    /// square in the band between the greeting and the switches, at most 480
    /// across, the drawing's middle in the band's. They stood on the window's
    /// foot, as tall as the stage under the greeting allowed up to 560, and
    /// were drawn behind the switches, which the README keeps them clear of.
    /// The space given the drawing is the square widened by the card's side
    /// inset each way, which the drawing's own placement (bottom-center)
    /// keeps clear, so that the canvas is the square itself.
    /// </summary>
    private void PlaceStanding(Windows.Foundation.Size stage)
    {
        if (_standing == null) return;
        var (side, x, y) = PortraitStage.Place(stage.Width, stage.Height);
        _standing.Width = side + 2 * PortraitArt.Inset;
        _standing.Height = side;
        _standing.Margin = new Thickness(x - side / 2 - PortraitArt.Inset, y - side / 2, 0, 0);
    }

    /// <summary>
    /// The greeting reads over the camera, which fills the stage behind it on
    /// the home: in the dark ink over a soft shadow, as words over a picture
    /// are, always in the dark scheme and in the light one while the camera
    /// shows. Over the light ground with no picture it is the scheme's own ink
    /// with no shadow, where near-white words were lost on the grey. Its
    /// colour moves over the settle curve as the camera comes and goes.
    /// </summary>
    private void PaintGreeting()
    {
        if (_greeting is not { } g) return;
        var overPicture = !Ink.Light || _stage?.Showing == true;
        if (overPicture == _greetingOverPicture) return;
        var first = _greetingOverPicture == null;
        _greetingOverPicture = overPicture;
        var title = Ink.Of(overPicture ? Theme.Dark.Foreground : Ink.Palette.Foreground);
        var line = Ink.Of(overPicture ? Shade.White.WithAlpha(0.78f) : Ink.Palette.Muted);
        foreach (var (text, colour) in new[] { (g.Title, title), (g.Line, line) })
        {
            if (first) text.Foreground = new SolidColorBrush(colour);
            else Motion.Recolour(text, colour);
        }
        foreach (var shadow in new[] { g.TitleShadow.Children[0], g.LineShadow.Children[0] })
        {
            if (first && !overPicture) Motion.Hide(shadow);
            else if (!first) Motion.FadeTo(shadow, overPicture ? 1 : 0);
        }
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
        // A newer build, in the foot above the relays' line and the account,
        // where it covers nothing: over the stage it lay across the greeting.
        // Apart from the foot, which is filled again whenever the relays
        // answer, so that the card is not made again with them.
        var foot = K.Column(0);
        if (App.Updates is { } updates)
            foot.Children.Add(new UpdateBanner(updates, sidebar: true, top: 0, below: 0) { Margin = new Thickness(10, 0, 10, 0) });
        foot.Children.Add(_sidebarFoot);
        var shell = new Grid();
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Grid.SetRow(scroller, 1);
        Grid.SetRow(foot, 2);
        shell.Children.Add(actions);
        shell.Children.Add(scroller);
        shell.Children.Add(foot);
        return K.Glass(shell, 22);
    }

    private void FillSidebar(StackPanel lists)
    {
        lists.Children.Clear();
        lists.Children.Add(SidebarItem(T("Home"), "house", Place.Home));
        lists.Children.Add(SidebarItem(T("Scheduled meetings"), "calendar", Place.Scheduled, App.Lobby.Upcoming.Count));
        // Arriving once, when there first is a server to choose, and drawn
        // still every time the rows are made again after that.
        if (App.OffersRelayChoice)
        {
            var server = SidebarItem(T("Server"), "server.rack", Place.Servers);
            lists.Children.Add(_serverRowShown ? server : server.Arrives());
            _serverRowShown = true;
        }
        else
        {
            _serverRowShown = false;
        }
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
    }

    /// <summary>The sidebar's foot: the fleet's light, and the account.</summary>
    private void FillFoot()
    {
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
            K.Fixed(new Border { Width = 20, Child = K.Symbol(symbol, 14, Ink.TallyText) }),
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
            Menu.Item(T("Sign out"), () => { Overlay.Current?.ClosePopover(); SignOutPrompt.Ask(App); }, "rectangle.portrait.and.arrow.right", role: Menu.Role.Destructive)),
            width: 262, edge: Edge.Above, align: Align.Start);
        return tap;
    }

    // Narrow

    private void BuildNarrow()
    {
        // The phone's lobby: black under its cards in the dark scheme, the
        // light one's grey under white cards, as the phones' grouped lists are.
        Background = Ink.Light ? Ink.Background : Ink.Brush(Shade.Black);
        var content = K.Column(22);
        content.Padding = new Thickness(16, 0, 16, 0);
        var title = K.Text("Tomoshibi", 34, K.Bold, lines: 1).Pad(0, 0, 0, -12);
        var hero = Hero();
        hero.Rises();
        _homeLists = K.Flowing(22);
        _homeLists.Rises(60);
        // The deployment's notice first, then a newer build, under the large
        // title, each opening a space for itself (Android's Lobby.kt).
        var head = K.Column(0, title, new LobbyNoticeCard(App.Remote, new Thickness(0, 22, 0, 0)));
        if (App.Updates is { } updates) head.Children.Add(new UpdateBanner(updates, inline: true, top: 22));
        content.Children.Add(head);
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
        barGround.Background = BarGround();
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

    /// <summary>
    /// What the bar over the narrow lobby lies on once the list has scrolled
    /// under it: the narrow lobby's own ground, flat and nearly opaque, black
    /// at night and the light scheme's grey by day. It was a black fade down
    /// its height at night, and the owner's rule is no gradients anywhere
    /// (2026-10-29); the list now ends at the bar's straight foot, as it
    /// ends at the join bar's band.
    /// </summary>
    private static Brush BarGround() => Ink.Light ? Ink.BackgroundAt(0.94f) : Ink.Black(0.94f);

    /// <summary>The picture of yourself with the way to start and to schedule (Lobby.kt <c>Hero</c>).</summary>
    private Grid Hero()
    {
        _stage = new Shared.CameraStage(App, 30, glass: true);
        var start = Tap.Words(T("Start a meeting"), "video.fill", Look.Prominent, 52, 16);
        start.HorizontalAlignment = HorizontalAlignment.Stretch;
        start.Click += (_, _) => OpenSheet(start: true, "");
        var schedule = Tap.Icon("calendar", T("Schedule a meeting"), Look.Plain, 52, 16);
        schedule.Click += (_, _) => Go(Place.Scheduled);
        var actions = K.Line(8, K.Flex(start), K.Fixed(Shared.OnGlass(schedule)));
        actions.VerticalAlignment = VerticalAlignment.Bottom;
        actions.Margin = new Thickness(12);
        var switches = Shared.DeviceSwitches(App, 40, onPicture: true);
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
            var all = new Tap(K.Text(T("All of them"), 16, ink: Ink.TallyText), Look.Clear, 36, padding: 8);
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
            K.Fixed(new Border { Width = 36, Height = 36, CornerRadius = new CornerRadius(11), Background = Ink.Wash(0.08f), Child = K.Symbol("server.rack", 16, Ink.Muted) }),
            K.Flex(subtitle),
            K.Fixed(K.Words(App.RelayChoice != null ? App.Lobby.Label(App.RelayChoice) : T("Automatic"), 14, ink: Ink.Muted, lines: 1).At(HorizontalAlignment.Right, VerticalAlignment.Center)),
            K.Fixed(K.Symbol("chevron.right", 13, Ink.MutedAt(0.6f))));
        var tap = new Tap(line, Look.Clear, 64, padding: 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(24),
            Resting = Ink.Palette.Surface,
        };
        tap.Named(T("Server"));
        tap.Click += (_, _) => Go(Place.Servers);
        return tap;
    }

    /// <summary>
    /// The join bar at the foot (Lobby.kt <c>JoinBar</c>): a room's name or a
    /// link, and in, on a band of the page's own ground across the window's
    /// foot. Without the band only the field had a surface, its acrylic; the
    /// arrow beside it and the gaps round both had none, and the list
    /// scrolling under the bar showed its words through them. The band is
    /// the page's colour, flat and whole (no fade into the list: the day
    /// mode has no gradients, and the owner finds them cheap), a little
    /// taller than the bar, so the list ends at a straight line above it as
    /// the iPhone's ends at its safe area's inset.
    /// </summary>
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
        row.Margin = new Thickness(16, 10, 16, 6);
        // The narrow lobby's own ground (BuildNarrow): black at night, the
        // light scheme's grey by day.
        var band = new Border { Background = Ink.Light ? Ink.Background : Ink.Brush(Shade.Black) };
        return new Grid { Children = { band, row }, VerticalAlignment = VerticalAlignment.Bottom };
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
