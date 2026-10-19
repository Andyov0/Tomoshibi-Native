using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Screens;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// The call (Android's ui/room/RoomView.kt, Room.tsx): every picture placed
/// by the shared planner, the room's name and the connection at the top,
/// the panels at the top right, what was said and what happened in the
/// corners, and the bar at the foot. A picture pressed goes on the stage or
/// back off it, pressed twice it fills the screen; a call of two is the
/// other person filling the window and you a card at its edge.
/// </summary>
internal sealed class RoomScreen : Screen
{
    public enum Panel { Messages, Devices }

    public readonly CallModel Call;
    public new AppModel App => base.App;

    private readonly Canvas _pictures = new();
    private readonly Dictionary<string, Tile> _tiles = new();
    private readonly Grid _chrome = new();
    private StagePin _pin = new();
    private int _page;
    private bool _filled;
    private Panel? _panel;
    private bool _aloneDismissed;
    private DateTime _lastPress = DateTime.MinValue;
    private string? _lastPressed;

    private ControlBar? _bar;
    private Grid? _barHost;
    private Grid? _top;
    private SignalBadge? _badge;
    private Grid? _noticeHost;
    private Grid? _aloneHost;
    private string? _aloneSaid;
    private Grid? _panelHost;
    private UIElement? _panelShown;
    private Panel? _panelKind;
    private ChatPanel? _chat;
    private SaidColumn? _said;
    private ToastStack? _toasts;
    private Grid? _pager;
    private TextBlock? _pageText;
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _stats;
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _idle;
    private DateTime _lastMoved = DateTime.UtcNow;
    private bool _barShown = true;

    public RoomScreen(AppModel app, CallModel call) : base(app)
    {
        Call = call;
        Background = Ink.Background;
        Children.Add(_pictures);
        Children.Add(_chrome);
        BuildChrome();
        SizeChanged += (_, _) => Lay();
        _pictures.Tapped += (_, _) => Pressed(null);
        PointerMoved += (_, _) => Poke();
        PointerPressed += (_, _) => Poke();

        // What was drawn and heard, every two seconds, for the tests to read.
        _stats = DispatcherQueue.CreateTimer();
        _stats.Interval = TimeSpan.FromSeconds(2);
        _stats.Tick += (_, _) => Report();
        _stats.Start();
        // Controls that may step aside do after three seconds alone (useIdle.ts).
        _idle = DispatcherQueue.CreateTimer();
        _idle.Interval = TimeSpan.FromMilliseconds(250);
        _idle.Tick += (_, _) => Idle();
        _idle.Start();
        Loaded += (_, _) => OpenFromLaunch();
        // The call speaks for itself, on the UI thread, whenever it changes.
        call.Changed += Update;
        Update();
    }

    private void OpenFromLaunch()
    {
        switch (App.Launch.Show)
        {
            case "chat": Toggle(Panel.Messages); break;
            case "devices": Toggle(Panel.Devices); break;
            case "leave": _bar?.OpenLeave(); break;
        }
    }

    public Panel? Showing => _panel;

    public override void Gone()
    {
        Call.Changed -= Update;
        _stats.Stop();
        _idle.Stop();
        Report();
        Unfill();
        Overlay.Current?.ClosePopover();
    }

    public override bool Refresh()
    {
        // The pictures stay; the words and colours around them are said again.
        _chrome.Children.Clear();
        _panelShown = null;
        _panelKind = null;
        _chat = null;
        BuildChrome();
        Update();
        return true;
    }

    public override bool Back()
    {
        if (_panel != null)
        {
            Toggle(_panel.Value);
            return true;
        }
        if (_filled)
        {
            Unfill();
            Lay();
            return true;
        }
        // Escape never leaves the call by itself: it asks.
        _bar?.OpenLeave();
        return true;
    }

    private void BuildChrome()
    {
        // The top row: the room and the connection.
        _top = new Grid { VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(14, Shared.TitleBar + 12, 14, 0) };
        _badge = new SignalBadge(Call);
        var row = K.Row(8, RoomTitle.Title(Call.Room), _badge);
        _top.Children.Add(row);
        _top.Arrives();
        _chrome.Children.Add(_top);

        _noticeHost = new Grid { VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(0, Shared.TitleBar + 12, 0, 0), IsHitTestVisible = false };
        _chrome.Children.Add(_noticeHost);

        _pager = new Grid { IsHitTestVisible = true };
        _chrome.Children.Add(_pager);

        _aloneHost = new Grid { VerticalAlignment = VerticalAlignment.Bottom, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(16, 0, 16, Plan.Island + 2) };
        _chrome.Children.Add(_aloneHost);

        _said = new SaidColumn(Call) { Margin = new Thickness(12, 0, 0, Plan.Island + 8) };
        _chrome.Children.Add(_said);

        _toasts = new ToastStack(Call) { Margin = new Thickness(0, 0, 16, Plan.Island + 8) };
        _chrome.Children.Add(_toasts);

        _panelHost = new Grid { HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Top };
        _chrome.Children.Add(_panelHost);

        _bar = new ControlBar(this, Call);
        _barHost = new Grid { VerticalAlignment = VerticalAlignment.Bottom, Margin = new Thickness(0, 0, 0, 20), Children = { _bar } };
        _barHost.Arrives(1f, fromY: 110);
        _chrome.Children.Add(_barHost);
        _aloneSaid = null;
    }

    // The model

    public override void Update()
    {
        if (_bar == null) return;
        _bar.Update();
        _badge!.Update();
        _badge.Visibility = Appearance.Shared.ConnectionBadge != ConnectionBadge.Hidden && Call.State == CallState.Connected ? Visibility.Visible : Visibility.Collapsed;
        _chat?.Update();
        _said!.Update();
        _toasts!.Update();
        UpdateNotice();
        UpdateAlone();
        Lay();
    }

    private string? _noticeSaid;

    private void UpdateNotice()
    {
        var said = Call.State != CallState.Connected
            ? Call.State == CallState.Reconnecting ? T("Reconnecting…") : T("Connecting…")
            : Call.Notice;
        if (said == _noticeSaid) return;
        _noticeSaid = said;
        _noticeHost!.Children.Clear();
        if (said != null) _noticeHost.Children.Add(RoomTitle.Notice(said).Arrives());
    }

    /// <summary>Whether a plain address is a way into this room (Stage.kt <c>linkWorks</c>): a door anybody may come through, and no scope.</summary>
    private bool LinkWorks => App.Info?.JoinedBy == Joining.Anyone && AppModel.SafeScope(Call.Room).Length == 0;

    private string? Link => App.Deployment is { } deployment ? deployment.BaseUrl().TrimEnd('/') + "/#/" + Call.Room : null;

    private void UpdateAlone()
    {
        var alone = Call.People.Count == 0 && _pin.Pinned == null && !_aloneDismissed && !_filled && Call.State == CallState.Connected;
        var said = alone ? $"{LinkWorks}|{Call.Hosting}|{Localization.Shared.Language}" : null;
        if (said == _aloneSaid) return;
        _aloneSaid = said;
        _aloneHost!.Children.Clear();
        if (alone)
        {
            var card = RoomTitle.EmptyRoomCard(App, Call, LinkWorks, Link, () =>
            {
                _aloneDismissed = true;
                UpdateAlone();
            });
            card.Arrives();
            _aloneHost.Children.Add(card);
        }
    }

    // Where everything goes

    /// <summary>Everybody's pictures in the order they are drawn (RoomView.kt): you first, then the rest, by when they were last heard once the room runs to more than a page.</summary>
    private List<string> Pictures(out Dictionary<string, Surface> surfaces)
    {
        var all = Call.Surfaces;
        var identities = Call.Identities;
        var order = Call.People.Count + 1 > Plan.TilesPerPage ? Call.Speaking.Order(identities) : identities;
        var rank = order.Select((id, i) => (id, i)).ToDictionary(p => p.id, p => p.i);
        surfaces = all.ToDictionary(s => s.Id);
        var ids = new List<string> { "me/camera" };
        ids.AddRange(all.Select((s, i) => (s, i))
            .OrderBy(p => rank.TryGetValue(p.s.Person.Id, out var r) ? r : int.MaxValue)
            .ThenBy(p => p.i)
            .Select(p => p.s.Id));
        return ids;
    }

    private void Lay()
    {
        if (ActualWidth <= 0 || ActualHeight <= 0 || _bar == null) return;
        var ids = Pictures(out var surfaces);
        var shares = surfaces.Values.Where(s => s.Kind == Surface.Kinds.Screen).Select(s => s.Id).ToList();
        _pin = _pin.Update(ids, shares);
        if (_pin.Pinned == null && _filled) Unfill();
        var size = new Size((float)ActualWidth, (float)ActualHeight);
        var room = Planned(ids);
        _page = room.Page;

        // Pictures that have gone fade out where they were.
        foreach (var gone in _tiles.Keys.Where(id => !room.Spots.ContainsKey(id)).ToList())
        {
            _pictures.Children.Remove(_tiles[gone]);
            _tiles.Remove(gone);
        }
        foreach (var (id, spot) in room.Spots)
        {
            if (!_tiles.TryGetValue(id, out var tile))
            {
                tile = new Tile(this, id);
                _tiles[id] = tile;
                _pictures.Children.Add(tile);
            }
            tile.Place(spot);
            // A smaller picture lies above a larger one: the card in a call of two.
            Canvas.SetZIndex(tile, -(int)(spot.Width * spot.Height / 100));
            ShowTile(tile, id, surfaces);
        }

        _bar.Fit(ActualWidth);
        LayPager(room, size);
        LayPanel(size);
        LiftNames();
    }

    /// <summary>
    /// Raises the name on each picture that runs under the controls while
    /// they are out (RoomView.kt <c>underBar</c>), where the bar is wide
    /// enough to reach it: not on a picture wider than 900, whose name is
    /// clear of a bar in its middle.
    /// </summary>
    private void LiftNames()
    {
        if (_bar == null) return;
        var barWidth = _bar.ActualWidth > 0 ? _bar.ActualWidth : ControlBar.FullLength;
        var left = (ActualWidth - barWidth) / 2;
        var right = left + barWidth;
        var bottom = ActualHeight - 20;
        foreach (var tile in _tiles.Values)
        {
            var spot = tile.Spot;
            var under = spot.Left < right && spot.Left + spot.Width > left && spot.Top + spot.Height > bottom - Plan.Island + 0.5;
            tile.Lift(under && _barShown && spot.Width < 900 ? Plan.Island - Plan.Gap : 0);
        }
    }

    private void ShowTile(Tile tile, string id, Dictionary<string, Surface> surfaces)
    {
        var onStage = _pin.Pinned == id;
        if (id == "me/camera")
        {
            tile.Show(T("{name} (you)", ("name", Call.LocalName)), Call.LocalIdentity, Call.LocalName, null, Call.CameraOn, mine: true,
                speaking: Call.MeSpeaking && Call.MicrophoneOn, muted: !Call.MicrophoneOn, hand: Call.HandUp,
                selected: onStage, onStage: onStage, filled: _filled, Call.Reactions.Where(r => r.From == Call.LocalIdentity));
            return;
        }
        if (!surfaces.TryGetValue(id, out var surface)) return;
        var person = surface.Person;
        var camera = surface.Kind == Surface.Kinds.Camera;
        var name = person.Shown;
        var microphone = person.Track(MediaSource.Microphone);
        tile.Show(camera ? name : T("{name} (screen)", ("name", name)), person.Id, name, surface.Sid, camera, mine: false,
            speaking: camera && person.Speaking, muted: camera && (microphone == null || microphone.Muted), hand: camera && person.HandUp,
            selected: onStage, onStage: onStage, filled: _filled, camera ? Call.Reactions.Where(r => r.From == person.Id) : Array.Empty<ShownReaction>());
    }

    /// <summary>The arrows beside the pictures they turn, and which page of how many (RoomView.kt <c>Pager</c>).</summary>
    private void LayPager(Plan.Room room, Size size)
    {
        var paging = room.Pages > 1 && !_filled;
        if (!paging)
        {
            _pager!.Children.Clear();
            _pageText = null;
            return;
        }
        if (_pager!.Children.Count == 0)
        {
            var previous = Tap.Icon("chevron.left", T("Previous page"), Look.Clear, 38, 14);
            previous.Click += (_, _) => Turn(-1);
            var next = Tap.Icon("chevron.right", T("Next page"), Look.Clear, 38, 14);
            next.Click += (_, _) => Turn(+1);
            _pager.Children.Add(Shared.OnGlass(previous).At(HorizontalAlignment.Left, VerticalAlignment.Center).Pad(10, 0, 0, Plan.Island).Arrives());
            _pager.Children.Add(Shared.OnGlass(next).At(HorizontalAlignment.Right, VerticalAlignment.Center).Pad(0, 0, 10, Plan.Island).Arrives());
            _pageText = K.Text("", 11.5, K.Semibold, Ink.ForegroundAt(0.85f), tabular: true);
            var indicator = K.Glass(_pageText, 12, new Thickness(10, 3, 10, 3), shadow: false);
            indicator.HorizontalAlignment = HorizontalAlignment.Center;
            indicator.VerticalAlignment = VerticalAlignment.Top;
            indicator.Margin = new Thickness(0, Shared.TitleBar + (size.Width < 560 ? 52 : 15), 0, 0);
            indicator.Arrives();
            _pager.Children.Add(indicator);
        }
        _pageText!.Text = $"{room.Page + 1} / {room.Pages}";
    }

    /// <summary>
    /// The shared planner's room, below the title bar: the window's caption
    /// buttons are over its top 32, and a grid of several, which the planner
    /// starts at the very top when its rows fill the height, had its first
    /// row's names and presses under them. A picture the planner gives the
    /// whole window (one filled, or the other person's in a call of two) is
    /// still the whole window, under the caption buttons as the phone's is
    /// under its status bar.
    /// </summary>
    private Plan.Room Planned(IReadOnlyList<string> ids)
    {
        var top = (float)Shared.TitleBar;
        var below = new Size((float)ActualWidth, (float)Math.Max(0, ActualHeight - top));
        var room = Plan.ForRoom(below, ids, "me/camera", _pin.Pinned, _filled, _page, Plan.Island);
        var spots = new OrderedDictionary<string, Rect>();
        foreach (var (id, spot) in room.Spots)
        {
            var whole = spot.Top <= 0.5f && spot.Left <= 0.5f && spot.Width >= below.Width - 0.5f;
            spots[id] = whole ? spot with { Top = 0, Height = spot.Height + top } : spot with { Top = spot.Top + top };
        }
        return room with { Spots = spots };
    }

    private void Turn(int by)
    {
        var room = Planned(Pictures(out _));
        _page = (room.Page + by + room.Pages) % room.Pages;
        Lay();
    }

    // Presses on pictures

    /// <summary>A press on a picture, or on the ground with none: within 300 ms of the last, the second fills the screen with the first's picture.</summary>
    public void Pressed(string? id)
    {
        var now = DateTime.UtcNow;
        if (_lastPressed != null && now - _lastPress < TimeSpan.FromMilliseconds(300))
        {
            var first = _lastPressed;
            _lastPressed = null;
            Expand(first);
            return;
        }
        if (id == null)
        {
            _lastPressed = null;
            return;
        }
        _lastPressed = id;
        _lastPress = now;
        if (_filled) return;
        _pin = _pin.Toggle(id);
        Lay();
    }

    /// <summary>On to the stage and the whole screen, or out of it again.</summary>
    public void Expand(string id)
    {
        if (_filled)
        {
            Unfill();
        }
        else
        {
            _pin = _pin.Pin(id);
            Fill();
        }
        Lay();
    }

    private void Fill()
    {
        if (_pin.Pinned == null) return;
        _filled = true;
        // The stage the whole window, and the window the whole screen, as a
        // video fills a monitor on Windows.
        if (MainWindow.Current?.AppWindow is { } window) window.SetPresenter(AppWindowPresenterKind.FullScreen);
        Poke();
    }

    private void Unfill()
    {
        if (!_filled) return;
        _filled = false;
        if (MainWindow.Current?.AppWindow is { } window) window.SetPresenter(AppWindowPresenterKind.Overlapped);
    }

    /// <summary>The menu a right press on a picture opens (RoomView.kt, the long press).</summary>
    public void MenuFor(string id, FrameworkElement tile)
    {
        var onStage = _pin.Pinned == id;
        Pictures(out var surfaces);
        var name = id == "me/camera" ? Call.LocalName : surfaces.TryGetValue(id, out var surface) ? surface.Person.Shown : id;
        var items = new List<UIElement>
        {
            onStage
                ? Menu.Item(T("Show everybody"), () => { Overlay.Current?.ClosePopover(); _pin = _pin.Toggle(id); Lay(); }, "arrow.down.right.and.arrow.up.left")
                : Menu.Item(T("Show {name} larger", ("name", name)), () => { Overlay.Current?.ClosePopover(); _pin = _pin.Toggle(id); Lay(); }, "arrow.up.left.and.arrow.down.right"),
        };
        if (onStage)
        {
            items.Add(_filled
                ? Menu.Item(T("Leave fullscreen"), () => { Overlay.Current?.ClosePopover(); Unfill(); Lay(); }, "arrow.down.right.and.arrow.up.left")
                : Menu.Item(T("Fill the screen"), () => { Overlay.Current?.ClosePopover(); Fill(); Lay(); }, "arrow.up.left.and.arrow.down.right"));
        }
        if (LinkWorks && Link is { } link)
        {
            items.Add(Menu.Rule());
            items.Add(Menu.Item(T("Copy link"), () => { Overlay.Current?.ClosePopover(); Pasteboard.Copy(link); }, "link"));
        }
        Overlay.Current?.Show("picture", tile, Menu.Card(items.ToArray()), 260, Edge.Below, Align.Start);
    }

    // Panels

    public void Toggle(Panel panel)
    {
        _panel = _panel == panel ? null : panel;
        Call.ChatOpen = _panel == Panel.Messages;
        Update();
    }

    /// <summary>The panel's width (RoomShape.kt <c>panelWidth</c>): 360 beside the pictures, the window's width less a margin where that leaves them too little.</summary>
    private static double PanelWidth(Size size)
    {
        var beside = Math.Min(360, size.Width - 32);
        return size.Width < beside + 24 + 172 + 22 ? Math.Max(0, size.Width - 24) : beside;
    }

    private void LayPanel(Size size)
    {
        var width = PanelWidth(size);
        if (_panel != _panelKind)
        {
            if (_panelShown != null) _panelHost!.Children.Remove(_panelShown);
            _panelShown = null;
            _chat = null;
            _panelKind = _panel;
            if (_panel == Panel.Messages)
            {
                _chat = new ChatPanel(Call, () => Toggle(Panel.Messages), 330);
                _panelShown = _chat;
            }
            else if (_panel == Panel.Devices)
            {
                _panelShown = new DevicesPanel(App, Call, () => Toggle(Panel.Devices));
            }
            if (_panelShown is FrameworkElement shown)
            {
                shown.Arrives(1f, fromX: (float)width);
                _panelHost!.Children.Add(shown);
            }
        }
        _panelHost!.Width = width;
        _panelHost.Margin = new Thickness(12, Shared.TitleBar + 12 + (size.Width < width + 24 + 172 + 22 ? 38 : 0), 12, 0);
        _panelHost.MaxHeight = Math.Max(160, size.Height - Plan.Island - 24 - Shared.TitleBar);
        // Notices go beside an open panel where there is room for both.
        var beside = _panel != null && size.Width - width - 48 >= 280 + 320;
        _toasts!.Margin = new Thickness(0, 0, beside ? width + 32 : 16, Plan.Island + 8);
    }

    // The bar stepping aside

    private void Poke()
    {
        _lastMoved = DateTime.UtcNow;
        if (!_barShown) ShowBar(true);
    }

    private void Idle()
    {
        var hides = Appearance.Shared.ControlsPlacement == ControlsPlacement.Idle || _filled;
        var alone = DateTime.UtcNow - _lastMoved > TimeSpan.FromSeconds(Plan.IdleAfter);
        var shown = !hides || !alone || Overlay.Current?.Open != null || _panel != null;
        if (shown != _barShown) ShowBar(shown);
    }

    private void ShowBar(bool shown)
    {
        _barShown = shown;
        if (_barHost != null) _barHost.Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
        LiftNames();
        if (_top != null && _filled) _top.Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
    }

    private void Report()
    {
        var pictures = _tiles.Values.Where(t => t.Picture != null).Select(t => $"video {t.Picture!.Sid} drawn={t.Picture.FramesDrawn}{(t.Picture.Whole ? " whole" : "")}");
        Log.Line($"stats {string.Join("; ", pictures.Append(Call.Reading()))}");
        if (App.Launch.LogFile != null) _ = Receiving();
    }

    /// <summary>
    /// What WebRTC itself has received of each picture, beside what was
    /// drawn of it: a picture that never comes is told apart from one whose
    /// packets arrive and do not decode, and from one never sent at all.
    /// Only for a launch that writes a log.
    /// </summary>
    private async Task Receiving()
    {
        foreach (var sid in _tiles.Values.Where(t => t.Picture != null).Select(t => t.Picture!.Sid).ToList())
        {
            var r = await Call.Core.ReceiveReport(sid);
            Log.Line(r == null
                ? $"receive {sid} none"
                : $"receive {sid} packets={r.Packets} bytes={r.Bytes} frames={r.FramesReceived} decoded={r.FramesDecoded} keys={r.KeyFramesDecoded} pli={r.Pli} fir={r.Fir} nack={r.Nack} {r.Width}x{r.Height} ssrc={r.Ssrc} mid={r.Mid} {r.Decoder}");
        }
    }
}
