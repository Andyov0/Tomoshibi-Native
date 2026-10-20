using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Media;
using Tomoshibi.Model.Watch;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Watch;

/// <summary>
/// The video everybody is watching, drawn on the stage (Android's
/// ui/watch/WatchStage.kt; WatchStageView.swift; WatchScreen.tsx).
///
/// The controls drive the show, not this player: pressing pause asks the
/// show to pause, and everybody pauses. This player also does it at once,
/// so the press is felt without waiting for the show's answer. In the
/// strip, when somebody has pinned another picture, it is a tile like the
/// others: the picture goes on playing, with no controls. Made once per
/// room and lent to whichever tile shows it, so the picture is not loaded
/// again when the stage and the strip trade places.
/// </summary>
internal sealed class WatchStage : Grid
{
    private readonly WatchEngine _engine;
    private readonly Action _openPanel;
    private readonly Grid _faces = new();
    private readonly Border _label;
    private UIElement? _face;
    private string _faceKey = "";
    private PlayerStage? _player;
    private bool _compact;

    public WatchStage(WatchEngine engine, Action openPanel)
    {
        _engine = engine;
        _openPanel = openPanel;
        Background = Ink.Brush(Shade.Black);
        Children.Add(_faces);
        _label = new Border
        {
            Child = K.Text(T("Watching together"), 10.5, K.Medium, lines: 1),
            Background = Ink.Black(0.55f),
            CornerRadius = new CornerRadius(9),
            Padding = new Thickness(6, 3, 6, 3),
            Margin = new Thickness(4),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            IsHitTestVisible = false,
            Visibility = Visibility.Collapsed,
        };
        _label.Arrives();
        Children.Add(_label);
        Loaded += (_, _) =>
        {
            engine.Changed -= Update;
            engine.Changed += Update;
            Update();
        };
        Unloaded += (_, _) => engine.Changed -= Update;
        SizeChanged += (_, e) =>
        {
            var compact = e.NewSize.Height < 200;
            if (compact == _compact) return;
            _compact = compact;
            _faceKey = "";
            Update();
        };
        Update();
    }

    /// <summary>The stage's face, read without asking anything of the server.</summary>
    private (string Key, Func<UIElement> Make) Face()
    {
        var video = _engine.Show?.Now;
        if (video == null) return ($"nothing|{_compact}", Nothing);
        _engine.Info(video.Ticket);
        return _engine.Infos.TryGetValue(video.Ticket, out var info) ? info switch
        {
            WatchInfo.Known when _engine.Player is { } player && player.Video.Key == video.Key => ($"playing {video.Key}|{player.GetHashCode()}|{_compact}", () => _player = new PlayerStage(_engine, player, _compact, _openPanel)),
            WatchInfo.Known => ("waiting", Waiting),
            WatchInfo.Unavailable => ($"cannot {video.Key}|{_compact}", () => Cannot(video, unavailable: true)),
            _ => ($"cannot {video.Key}|{_compact}", () => Cannot(video, unavailable: false)),
        } : ("waiting", Waiting);
    }

    public void Update()
    {
        var (key, make) = Face();
        if (key != _faceKey)
        {
            _faceKey = key;
            if (_face != null) _faces.Children.Remove(_face);
            _player?.Gone();
            _player = null;
            _face = make();
            if (_face is FrameworkElement shown) shown.Arrives();
            _faces.Children.Add(_face);
        }
        _player?.Update();
        _label.Visibility = _compact ? Visibility.Visible : Visibility.Collapsed;
    }

    private UIElement Nothing()
    {
        var column = K.Column(12, K.Symbol("film", _compact ? 18 : 28, Ink.Muted).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        if (!_compact)
        {
            column.Children.Add(K.Text(T("Nothing is playing. Paste a video link to watch together."), 13, ink: Ink.Muted, align: TextAlignment.Center));
            var add = StageButton(T("Add a video"), "plus", _openPanel, prominent: true);
            add.HorizontalAlignment = HorizontalAlignment.Center;
            column.Children.Add(add);
        }
        column.Padding = new Thickness(16);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        return column;
    }

    private static UIElement Waiting() => K.Spinner(26, Ink.White(0.85f)).At(HorizontalAlignment.Center, VerticalAlignment.Center);

    private UIElement Cannot(WatchVideo video, bool unavailable)
    {
        // Not "try again" for a video that plays only through a relay the
        // server will not run for it: no retry plays it.
        var column = K.Column(12, K.Text(unavailable ? T("That video cannot be played here.") : T("The video could not be read. Try again."), 13, ink: Ink.White(0.7f), align: TextAlignment.Center));
        if (!_compact)
        {
            var next = StageButton(T("Next video"), "forward.end.fill", () => _engine.Skip(video.Key));
            next.HorizontalAlignment = HorizontalAlignment.Center;
            column.Children.Add(next);
        }
        column.Padding = new Thickness(16);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        return column;
    }

    /// <summary>
    /// The few buttons said over the video (<c>StageButton</c>): capsules,
    /// the tally for the one that is the way on, dark rather than glass,
    /// since a video is drawn on a surface a blur cannot sample.
    /// </summary>
    public static Tap StageButton(string words, string symbol, Action act, bool prominent = false)
    {
        var ink = prominent ? Ink.TallyForeground : Ink.Foreground;
        var row = K.Row(6, K.Symbol(symbol, 12, ink), K.Text(words, 12.5, K.Semibold, ink, lines: 1));
        row.VerticalAlignment = VerticalAlignment.Center;
        var tap = new Tap(row, prominent ? Look.Prominent : Look.Plain, 32, padding: 14);
        if (!prominent)
        {
            tap.Resting = Shade.Black.WithAlpha(0.52f);
            tap.BorderBrush = Ink.White(0.14f);
            tap.BorderThickness = new Thickness(1);
        }
        tap.Named(words);
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>A line across the top of the stage that says something is happening, dark over the video.</summary>
    public static Border StageNotice(string text, string? symbol = null)
    {
        var row = K.Row(6);
        if (symbol != null) row.Children.Add(K.Symbol(symbol, 13, Ink.Foreground));
        row.Children.Add(K.Text(text, 13, K.Medium, lines: 1));
        return new Border
        {
            Child = row,
            Background = Ink.Black(0.52f),
            BorderBrush = Ink.White(0.14f),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(17),
            Padding = new Thickness(14, 8, 14, 8),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
    }
}

/// <summary>A player and everything said over it (WatchStage.kt <c>PlayerStage</c>).</summary>
internal sealed class PlayerStage : Grid
{
    private readonly WatchEngine _engine;
    private readonly WatchPlayer _player;
    private readonly bool _compact;
    private readonly Action _openPanel;
    private readonly Grid _surface = new() { Background = Ink.Brush(Shade.Black) };
    private readonly Border _catcher = new() { Background = K.Clear };
    private readonly Grid _overlays = new();
    private readonly StackPanel _notices = K.Column(8);
    private readonly Grid _controlsHost = new() { VerticalAlignment = VerticalAlignment.Bottom };
    private readonly DispatcherQueueTimer _clock;
    private IWatchBackend? _drawn;
    private MediaPlayerElement? _element;
    private FrameworkElement? _spinner, _unreachable, _halted, _resuming, _tapToJoin;
    private Grid? _controls;
    private TextBlock? _time;
    private Slider? _scrubber, _volume;
    private Tap? _playPause, _mute;
    private double? _scrubbing;
    private DateTime _touched = DateTime.MinValue;
    private bool _pointerIn;
    private string _controlsSaid = "";

    public PlayerStage(WatchEngine engine, WatchPlayer player, bool compact, Action openPanel)
    {
        _engine = engine;
        _player = player;
        _compact = compact;
        _openPanel = openPanel;
        Children.Add(_surface);
        // YouTube's frame takes no presses except while it is refused; a
        // file's picture takes none either way. Covered by something that
        // passes every press on to the room, whose own press on the stage
        // pins and fills it.
        Children.Add(_catcher);
        Children.Add(_overlays);
        _notices.HorizontalAlignment = HorizontalAlignment.Center;
        _notices.VerticalAlignment = VerticalAlignment.Top;
        _notices.Margin = new Thickness(0, 12, 0, 0);
        _notices.IsHitTestVisible = false;
        Children.Add(_notices);
        Children.Add(_controlsHost);
        // Shown while the pointer moves over the video and for three seconds
        // after, while it is paused, and while it is scrubbed: what a video
        // player on a desktop does, where Android, with no pointer, shows
        // them for a while after a touch.
        PointerMoved += (_, _) => Touch();
        PointerPressed += (_, _) => Touch();
        PointerEntered += (_, _) =>
        {
            _pointerIn = true;
            Touch();
        };
        PointerExited += (_, _) =>
        {
            _pointerIn = false;
            _touched = DateTime.UtcNow - TimeSpan.FromSeconds(2.5);
        };
        _clock = DispatcherQueue.GetForCurrentThread().CreateTimer();
        _clock.Interval = TimeSpan.FromMilliseconds(WatchPlayer.KeepMs);
        _clock.Tick += (_, _) => Tick();
        _clock.Start();
        SizeChanged += (_, _) => Report();
        Update();
    }

    /// <summary>Lets the player's picture go: the stage is no longer this one's.</summary>
    public void Gone()
    {
        _clock.Stop();
        Detach();
    }

    private void Touch()
    {
        _touched = DateTime.UtcNow;
        UpdateControls();
    }

    private void Tick()
    {
        UpdateControls();
        if (_controls == null || _scrubbing != null) return;
        var video = _player.Video;
        var length = video.Duration > 0 ? video.Duration : _player.Info.Duration;
        if (_scrubber != null) MediaKit.Set(_scrubber, Math.Min(_player.Position, length), length);
        if (_time != null) _time.Text = video.Live ? T("Live") : TimeText(_player.Position, length);
    }

    public void Update()
    {
        Attach();
        _catcher.Visibility = _player.TapYoutube ? Visibility.Collapsed : Visibility.Visible;
        _player.Backend?.SetTappable(_player.TapYoutube);
        Show(ref _spinner, _player.Waiting && _player.Unreachable == null && _player.Stopping != WatchPlayer.Halted.Stopped,
            () => K.Spinner(26, Ink.White(0.85f)).At(HorizontalAlignment.Center, VerticalAlignment.Center), _overlays);
        Show(ref _unreachable, _player.Unreachable != null, () => Unreachable(_player.Unreachable ?? WatchPlayer.Lost.Network, _player.Info.Link), _overlays);
        Show(ref _halted, _player.Stopping == WatchPlayer.Halted.Stopped, Halted, _overlays);
        Show(ref _resuming, _player.Stopping == WatchPlayer.Halted.Resuming, () => WatchStage.StageNotice(T("Reconnecting…")), _notices);
        Show(ref _tapToJoin, _player.TapYoutube && _player.Unreachable == null, () => WatchStage.StageNotice(T("Tap the video to join"), "play.fill"), _notices);
        UpdateControls();
    }

    private static void Show(ref FrameworkElement? shown, bool wanted, Func<FrameworkElement> make, Panel into)
    {
        if (wanted && shown == null)
        {
            shown = make().Arrives();
            into.Children.Add(shown);
        }
        else if (!wanted && shown != null)
        {
            into.Children.Remove(shown);
            shown = null;
        }
    }

    /// <summary>
    /// Where the player's picture is drawn: Windows' media player element,
    /// or YouTube's web view, both owned by the player rather than this
    /// view, which only lends them a place; shown whole, black either side.
    /// </summary>
    private void Attach()
    {
        var backend = _player.Backend;
        if (ReferenceEquals(backend, _drawn)) return;
        Detach();
        _drawn = backend;
        switch (backend)
        {
            case FileBackend file:
                _element = new MediaPlayerElement { AreTransportControlsEnabled = false, Stretch = Stretch.Uniform, IsHitTestVisible = false };
                _element.SetMediaPlayer(file.Player);
                _surface.Children.Add(_element);
                Report();
                break;
            case YouTubeBackend youtube:
                // Taken from wherever it was last lent, which the place it is
                // leaving may not have let go of yet.
                if (youtube.Web.Parent is Panel old) old.Children.Remove(youtube.Web);
                _surface.Children.Add(youtube.Web);
                break;
        }
    }

    private void Detach()
    {
        if (_element != null)
        {
            _element.SetMediaPlayer(null);
            _surface.Children.Remove(_element);
            _element = null;
        }
        if (_drawn is YouTubeBackend youtube) _surface.Children.Remove(youtube.Web);
        _drawn = null;
    }

    /// <summary>The size the picture is shown at, in pixels, for a player held to it.</summary>
    private void Report()
    {
        if (_drawn is not FileBackend file || XamlRoot == null) return;
        var scale = XamlRoot.RasterizationScale;
        file.Shown((int)(ActualWidth * scale), (int)(ActualHeight * scale));
    }

    // What is said over it

    /// <summary>Where the video does not reach this viewer, with no relay to fall back on.</summary>
    private static FrameworkElement Unreachable(WatchPlayer.Lost lost, string link)
    {
        var site = WatchWords.Site(link);
        var circle = new Border
        {
            Width = 48,
            Height = 48,
            CornerRadius = new CornerRadius(24),
            Background = Ink.White(0.08f),
            Child = K.Symbol(lost == WatchPlayer.Lost.Format ? "film.stack" : "icloud.slash", 20, Ink.White(0.75f)),
            HorizontalAlignment = HorizontalAlignment.Center,
            Margin = new Thickness(0, 0, 0, 4),
        };
        var column = K.Column(8, circle,
            K.Text(T("This video does not reach you"), 14, K.Semibold, align: TextAlignment.Center),
            // Not "everybody else goes on watching" for what the site
            // refused: what it refuses here, it refuses everybody.
            K.Text(Sentence(lost, site), 11.5, ink: Ink.White(0.6f), align: TextAlignment.Center));
        // The sharer's link, which may have a web host and still not be an
        // address (a port that is not a number): no button rather than a
        // press that throws (WebAddress).
        if (WatchWords.Openable(link) && WebAddress.Of(link) is { } address)
        {
            var open = WatchStage.StageButton(T("Open the link"), "arrow.up.right.square", () => _ = Windows.System.Launcher.LaunchUriAsync(address));
            open.HorizontalAlignment = HorizontalAlignment.Center;
            open.Margin = new Thickness(0, 4, 0, 0);
            column.Children.Add(open);
        }
        column.MaxWidth = 288;
        column.Padding = new Thickness(16);
        column.VerticalAlignment = VerticalAlignment.Center;
        return new Grid { Background = Ink.Black(0.85f), Children = { column } };
    }

    private static string Sentence(WatchPlayer.Lost lost, string site) => lost switch
    {
        // A kind of video the player here cannot open, from where it is and
        // through the relay alike: the browsers in the call can, so the
        // show goes on for them.
        WatchPlayer.Lost.Format => T("This app cannot play this kind of video. Everybody else goes on watching."),
        WatchPlayer.Lost.Refused when site.Length > 0 => T("It plays only on {site} itself.", ("site", site)),
        WatchPlayer.Lost.Failed when site.Length > 0 => T("{site} would not play it here.", ("site", site)),
        _ when site.Length == 0 => T("Your connection cannot reach where it is. Everybody else goes on watching."),
        _ => T("Your connection cannot reach {site}. Everybody else goes on watching.", ("site", site)),
    };

    private FrameworkElement Halted()
    {
        var again = WatchStage.StageButton(T("Try again"), "arrow.clockwise", _player.TryAgain, prominent: true);
        again.HorizontalAlignment = HorizontalAlignment.Center;
        var column = K.Column(12, K.Symbol("icloud.slash", 22, Ink.White(0.7f)).At(HorizontalAlignment.Center, VerticalAlignment.Center),
            K.Text(T("The video stopped coming through."), 13, align: TextAlignment.Center), again);
        column.Padding = new Thickness(16);
        column.VerticalAlignment = VerticalAlignment.Center;
        column.HorizontalAlignment = HorizontalAlignment.Center;
        return new Grid { Background = Ink.Black(0.85f), Children = { column } };
    }

    // The controls under the video

    private void UpdateControls()
    {
        var playing = _engine.Show?.Playing ?? false;
        var wanted = !_compact && (_pointerIn && DateTime.UtcNow - _touched < TimeSpan.FromSeconds(3) || !playing || _scrubbing != null);
        if (wanted && _controls == null)
        {
            _controls = Controls();
            _controls.Arrives(1f, fromY: 24);
            _controlsHost.Children.Add(_controls);
        }
        else if (!wanted && _controls != null)
        {
            _controlsHost.Children.Remove(_controls);
            _controls = null;
            _controlsSaid = "";
        }
        if (_controls == null) return;
        var said = $"{playing}|{_player.Muted}|{_player.Volume}|{_player.Relayed}|{Localization.Shared.Language}";
        if (said == _controlsSaid) return;
        _controlsSaid = said;
        Paint(_playPause!, playing ? "pause.fill" : "play.fill", playing ? T("Pause for everybody") : T("Play for everybody"));
        Paint(_mute!, _player.Muted || _player.Volume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill", _player.Muted ? T("Unmute the video") : T("Mute the video"));
        if (_volume != null) MediaKit.Set(_volume, _player.Muted ? 0 : _player.Volume);
    }

    private static void Paint(Tap tap, string symbol, string label)
    {
        tap.Content = K.Symbol(symbol, 13, Ink.Foreground);
        tap.Named(label);
        tap.Tip(label);
    }

    private Grid Controls()
    {
        var video = _player.Video;
        var length = video.Duration > 0 ? video.Duration : _player.Info.Duration;
        var by = new List<string> { T("Added by {name}", ("name", video.By)) };
        if (_player.Relayed) by.Add(T("Relayed"));
        var heading = K.Line(8,
            K.Flex(K.Text(_player.Info.Title, 13, K.Medium, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
            K.Fixed(K.Text(string.Join(" · ", by), 11, ink: Ink.White(0.6f), lines: 1).At(HorizontalAlignment.Right, VerticalAlignment.Center)));
        var column = K.Column(6, heading);
        _scrubber = null;
        if (length > 0 && !video.Live)
        {
            _scrubber = MediaKit.Slider(Math.Min(_player.Position, length), to => _scrubbing = to, 0, length, tint: Ink.Tally, label: T("Position in the video"), editing: editing =>
            {
                if (editing) return;
                if (_scrubbing is { } to)
                {
                    _engine.Seek(to);
                    _scrubbing = null;
                }
            });
            column.Children.Add(_scrubber);
        }
        _playPause = Control("play.fill", T("Play for everybody"), _engine.Toggle);
        var next = Control("forward.end.fill", T("Next video"), () => _engine.Skip(video.Key));
        // The exact position too, for whoever is checking two players
        // against each other, as the web carries it in data-position.
        _time = K.Text(video.Live ? T("Live") : TimeText(_player.Position, length), 11, ink: Ink.White(0.8f), lines: 1, tabular: true);
        _time.VerticalAlignment = VerticalAlignment.Center;
        _time.Margin = new Thickness(4, 0, 4, 0);
        _mute = Control("speaker.wave.2.fill", T("Mute the video"), () => _player.Muted = !_player.Muted);
        _volume = MediaKit.Slider(_player.Muted ? 0 : _player.Volume, level =>
        {
            _player.Muted = false;
            _player.Volume = level;
        }, 0, 1, 0.05, Ink.Foreground, T("Video volume"));
        _volume.Width = 84;
        var list = Control("list.bullet.rectangle", T("Videos"), _openPanel);
        var row = K.Line(4, K.Fixed(_playPause), K.Fixed(next), K.Fixed(_time), K.Flex(K.Spacer()), K.Fixed(_mute), K.Fixed(_volume), K.Fixed(list));
        column.Children.Add(row);
        var ground = new LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0, 0), EndPoint = new Windows.Foundation.Point(0, 1) };
        ground.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.Black.WithAlpha(0f)), Offset = 0 });
        ground.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.Black.WithAlpha(0.8f)), Offset = 1 });
        var grid = new Grid { Background = ground, Padding = new Thickness(12, 32, 12, 8) };
        grid.Children.Add(column);
        _controlsSaid = "";
        return grid;
    }

    private static string TimeText(double at, double length) =>
        length > 0 ? $"{WatchWords.Clock(at)} / {WatchWords.Clock(length)}" : WatchWords.Clock(at);

    /// <summary>One round press over the video: white, lighting under the pointer.</summary>
    private static Tap Control(string symbol, string label, Action act)
    {
        var tap = new Tap(K.Symbol(symbol, 13, Ink.Foreground), Look.Clear, 34, circle: true, label: label);
        tap.Named(label);
        tap.Click += (_, _) => act();
        return tap;
    }
}
