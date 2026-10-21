using System.Reflection;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;
using Tomoshibi.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Lobby;
using Tomoshibi.UI.Room;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Settings;

/// <summary>
/// General (settings/General.kt): the language, the colour, the connection
/// badge and where the controls sit. Each takes at once and is kept under
/// the other apps' names.
/// </summary>
internal static class General
{
    public static StackPanel Page(AppModel app)
    {
        var page = K.Column(22);
        page.Children.Add(Rows.Section(T("Language"), LanguageCard()));
        page.Children.Add(Rows.Section(T("Accent colour"), AccentCard()));
        page.Children.Add(Rows.Section(T("Connection badge"), BadgeCard()));
        page.Children.Add(Rows.Section(T("Controls"), ControlsCard()));
        return page;
    }

    /// <summary>
    /// The system's language or one of the four, each named in itself: a
    /// language picker is read by somebody who cannot yet read the language
    /// on the screen.
    /// </summary>
    private static Border LanguageCard()
    {
        var words = Localization.Shared;
        var system = Languages.Detect(Shared.SystemLanguages());
        var column = K.Column(8);
        column.Children.Add(Choice(T("Same as the system"), system.DisplayName(), "globe", words.FollowsSystem, words.FollowSystem));
        var grid = new VariableSizedWrapGrid { Orientation = Orientation.Horizontal, ItemWidth = 132, ItemHeight = 58 };
        foreach (var language in Languages.All)
        {
            var chosen = language;
            var tile = Choice(language.DisplayName(), language.Code(), null, !words.FollowsSystem && words.Language == language, () => words.Choose(chosen));
            tile.Margin = new Thickness(0, 0, 8, 8);
            grid.Children.Add(tile);
        }
        column.Children.Add(grid);
        return K.Card(column, 16, padding: new Thickness(12));
    }

    /// <summary>A tile to choose (General.kt <c>ChoiceTile</c>): washed and ringed in the tally when chosen, with its tick.</summary>
    private static Tap Choice(string title, string detail, string? symbol, bool chosen, Action pick)
    {
        var parts = new List<(UIElement, double)>();
        if (symbol != null) parts.Add(K.Fixed(K.Symbol(symbol, 15, chosen ? Ink.Tally : Ink.Muted)));
        parts.Add(K.Flex(K.Column(1, K.Text(title, 14, chosen ? K.Semibold : K.Regular, lines: 1), K.Text(detail, 11, ink: Ink.Muted, lines: 1)).At(HorizontalAlignment.Left, VerticalAlignment.Center)));
        var tick = K.Symbol("checkmark.circle.fill", 15, Ink.Tally);
        tick.Opacity = chosen ? 1 : 0;
        parts.Add(K.Fixed(tick));
        var tap = new Tap(K.Line(10, parts.ToArray()), Look.Plain, 50, padding: 12)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(13),
        };
        if (chosen)
        {
            tap.Resting = Ink.TallyShade.WithAlpha(0.13f);
            tap.BorderBrush = Ink.TallyAt(0.55f);
            tap.BorderThickness = new Thickness(1.5);
        }
        tap.Named(title);
        tap.Click += (_, _) => pick();
        return tap;
    }

    /// <summary>The colours on offer as beads, the chosen one ringed and ticked, and how it looks in a call beside them.</summary>
    private static Border AccentCard()
    {
        var appearance = Appearance.Shared;
        var beads = new VariableSizedWrapGrid { Orientation = Orientation.Horizontal, ItemWidth = 48, ItemHeight = 50 };
        foreach (var accent in Accents.All)
        {
            var chosen = accent == appearance.Accent;
            var colour = accent.Color();
            var bead = new Grid { Width = 42, Height = 42 };
            // The colour, flat, with a fine light edge (General.kt `Swatch`):
            // a bead of glass, lit and shaded, until the gradients went.
            bead.Children.Add(new Ellipse { Width = 34, Height = 34, Fill = Ink.Brush(colour), Stroke = Ink.White(0.25f), StrokeThickness = 0.5 });
            if (chosen)
            {
                bead.Children.Add(new Ellipse { Width = 42, Height = 42, Stroke = Ink.Brush(colour), StrokeThickness = 2 });
                bead.Children.Add(K.Symbol("checkmark", 13, Ink.Brush(accent.OnColor())));
            }
            var tap = new Tap(bead, Look.Clear, 42, circle: true, label: accent.DisplayName());
            var picked = accent;
            tap.Click += (_, _) => appearance.Choose(picked);
            beads.Children.Add(tap);
        }
        // How it looks: a picture ringed in it, the Join button, a switch on.
        var scene = K.Row(12);
        var tag = new Border
        {
            Background = Ink.Black(0.45f),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(6, 3, 6, 3),
            Margin = new Thickness(6),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            Child = K.Row(4, K.Symbol("waveform", 8, Ink.Tally), K.Text("Mika", 9, K.Semibold, lines: 1)),
        };
        var tile = new Border
        {
            Width = 104,
            Height = 66,
            CornerRadius = new CornerRadius(12),
            BorderBrush = Ink.Tally,
            BorderThickness = new Thickness(2),
            Background = Ink.Surface,
            Child = K.Stack(Faces.Initial("Mika", 30).At(HorizontalAlignment.Center, VerticalAlignment.Center), tag),
        };
        var join = new Border { Height = 28, CornerRadius = new CornerRadius(14), Background = Ink.Tally, Padding = new Thickness(16, 0, 16, 0), Child = K.Text(T("Join"), 12.5, K.Semibold, Ink.TallyForeground, lines: 1).At(HorizontalAlignment.Center, VerticalAlignment.Center) };
        // A switch drawn on, not a control: the preview is a picture.
        var switchOn = new Border
        {
            Width = 38,
            Height = 23,
            CornerRadius = new CornerRadius(11.5),
            Background = Ink.Tally,
            Padding = new Thickness(2.5),
            Child = new Ellipse { Width = 18, Height = 18, Fill = Ink.Brush(Shade.White), HorizontalAlignment = HorizontalAlignment.Right },
        };
        scene.Children.Add(tile);
        scene.Children.Add(K.Column(10, K.Row(10, join, switchOn), K.Row(6, K.Symbol("bubble.left.fill", 14, Ink.ForegroundAt(0.85f)), K.Symbol("hand.raised.fill", 14, Ink.Tally))));
        var preview = new Border
        {
            Background = Ink.Black(0.22f),
            CornerRadius = new CornerRadius(16),
            Padding = new Thickness(12),
            Child = K.Line(12, K.Fixed(scene), K.Flex(K.Text(appearance.Accent.DisplayName(), 17, K.Semibold, Ink.Tally, lines: 1).At(HorizontalAlignment.Right, VerticalAlignment.Center))),
        }.Quiet();
        return K.Card(K.Column(16, beads, preview), 16, padding: new Thickness(14));
    }

    /// <summary>How the connection is shown in a call: the bars alone, with the figures, or not at all.</summary>
    private static Border BadgeCard()
    {
        var appearance = Appearance.Shared;
        var options = Enum.GetValues<ConnectionBadge>();
        var choice = new Segmented(options.Select(o => o.DisplayName()).ToList(), Array.IndexOf(options, appearance.ConnectionBadge), 32);
        choice.HorizontalAlignment = HorizontalAlignment.Left;
        choice.Chose += index => appearance.Choose(options[index]);
        // A strip of a call with the badge in its corner as it would be.
        var preview = new Grid { Height = 132, CornerRadius = new CornerRadius(14) };
        preview.Children.Add(new Backdrop());
        var tiles = new Grid { ColumnSpacing = 6, Margin = new Thickness(14, 46, 14, 14) };
        for (var i = 0; i < 3; i++)
        {
            tiles.ColumnDefinitions.Add(new ColumnDefinition());
            var block = new Border { CornerRadius = new CornerRadius(7), Background = Ink.White(i == 1 ? 0.13f : 0.08f) };
            Grid.SetColumn(block, i);
            tiles.Children.Add(block);
        }
        preview.Children.Add(tiles);
        // The badge as chosen, changed where it stands when the choice
        // changes: the one shown leaves and the new one comes in, while the
        // picker slides to it, and the page around them stays.
        var place = new Grid();
        preview.Children.Add(place);
        var shown = appearance.ConnectionBadge;
        void Show()
        {
            place.Children.Clear();
            if (shown == ConnectionBadge.Hidden) return;
            var bars = K.Row(2);
            foreach (var height in new[] { 6.0, 10.0, 14.0 }) bars.Children.Add(new Rectangle { Width = 3.5, Height = height, RadiusX = 1, RadiusY = 1, Fill = Ink.Good, VerticalAlignment = VerticalAlignment.Bottom });
            var row = K.Row(7, bars);
            if (shown == ConnectionBadge.Detailed) row.Children.Add(K.Text("32 ms", 11.5, K.Medium, Ink.Muted, tabular: true));
            var badge = K.Glass(row, 14, new Thickness(10, 7, 10, 7), shadow: false);
            badge.HorizontalAlignment = HorizontalAlignment.Left;
            badge.VerticalAlignment = VerticalAlignment.Top;
            badge.Margin = new Thickness(10);
            place.Children.Add(badge.Arrives());
        }
        Show();
        void Changed()
        {
            if (appearance.ConnectionBadge == shown) return;
            shown = appearance.ConnectionBadge;
            choice.Choose(Array.IndexOf(options, shown), tell: false);
            Show();
        }
        var card = K.Card(K.Column(14, preview.Quiet(), choice), 16, padding: new Thickness(14));
        card.Loaded += (_, _) => appearance.Changed += Changed;
        card.Unloaded += (_, _) => appearance.Changed -= Changed;
        return card;
    }

    /// <summary>
    /// Where the controls sit in a call: always, out of the way while nothing
    /// happens, or down the side. The three are as tall as the tallest, a
    /// name that runs to two lines included, and the choice moves between
    /// them where they stand: the light fades from one to the other.
    /// </summary>
    private static Border ControlsCard()
    {
        var appearance = Appearance.Shared;
        var row = new Grid { ColumnSpacing = 10 };
        var placements = Enum.GetValues<ControlsPlacement>();
        var lights = new List<(ControlsPlacement Placement, Border Light, Border Drawing, TextBlock Name)>();
        for (var i = 0; i < placements.Length; i++)
        {
            row.ColumnDefinitions.Add(new ColumnDefinition());
            var placement = placements[i];
            var chosen = placement == appearance.ControlsPlacement;
            // The light: the tally's wash and edge over the whole card, faded
            // in and out rather than painted on, so that a choice is seen to
            // move.
            var light = new Border
            {
                Background = Ink.TallyAt(0.12f),
                BorderBrush = Ink.TallyAt(0.6f),
                BorderThickness = new Thickness(1.5),
                CornerRadius = new CornerRadius(14),
                Opacity = chosen ? 1 : 0,
                OpacityTransition = new ScalarTransition { Duration = Motion.Settle },
                IsHitTestVisible = false,
            };
            var drawing = new Border { Child = Schematic(placement, chosen) };
            var name = K.Text(placement.DisplayName(), 11.5, chosen ? K.Semibold : K.Regular, chosen ? Ink.Foreground : Ink.Muted, align: TextAlignment.Center);
            var column = K.Column(9, drawing, name);
            column.Margin = new Thickness(8);
            column.VerticalAlignment = VerticalAlignment.Top;
            var card = new Tap(new Grid { Children = { light, column } }, Look.Plain, 120, padding: 0)
            {
                HorizontalAlignment = HorizontalAlignment.Stretch,
                VerticalAlignment = VerticalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                VerticalContentAlignment = VerticalAlignment.Stretch,
                CornerRadius = new CornerRadius(14),
            };
            card.Height = double.NaN;
            card.Padding = new Thickness(0);
            card.Named(placement.DisplayName());
            card.Click += (_, _) => appearance.Choose(placement);
            Grid.SetColumn(card, i);
            row.Children.Add(card);
            lights.Add((placement, light, drawing, name));
        }
        var shown = appearance.ControlsPlacement;
        void Changed()
        {
            if (appearance.ControlsPlacement == shown) return;
            shown = appearance.ControlsPlacement;
            foreach (var (placement, light, drawing, name) in lights)
            {
                var chosen = placement == shown;
                light.Opacity = chosen ? 1 : 0;
                drawing.Child = Schematic(placement, chosen);
                name.FontWeight = chosen ? K.Semibold : K.Regular;
                name.Foreground = chosen ? Ink.Foreground : Ink.Muted;
            }
        }
        var whole = K.Card(row, 16, padding: new Thickness(12));
        whole.Loaded += (_, _) => appearance.Changed += Changed;
        whole.Unloaded += (_, _) => appearance.Changed -= Changed;
        return whole;
    }

    /// <summary>A call in miniature with its bar where the choice puts it (General.kt <c>PlacementCard</c>).</summary>
    private static Grid Schematic(ControlsPlacement placement, bool chosen)
    {
        var grid = new Grid { Height = 72, CornerRadius = new CornerRadius(8), Background = Ink.Black(0.35f) };
        var tiles = new Grid { RowSpacing = 3, ColumnSpacing = 3, Margin = new Thickness(6, 6, placement == ControlsPlacement.Side ? 20 : 6, placement == ControlsPlacement.Side ? 6 : 18) };
        tiles.RowDefinitions.Add(new RowDefinition());
        tiles.RowDefinitions.Add(new RowDefinition());
        tiles.ColumnDefinitions.Add(new ColumnDefinition());
        tiles.ColumnDefinitions.Add(new ColumnDefinition());
        for (var i = 0; i < 4; i++)
        {
            var block = new Border { CornerRadius = new CornerRadius(3), Background = Ink.White(0.13f) };
            Grid.SetRow(block, i / 2);
            Grid.SetColumn(block, i % 2);
            tiles.Children.Add(block);
        }
        grid.Children.Add(tiles);
        var colour = chosen ? Ink.Tally : Ink.White(0.55f);
        var bar = placement == ControlsPlacement.Side
            ? new Rectangle { Width = 7, Height = 36, RadiusX = 3.5, RadiusY = 3.5, HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 6, 0) }
            : new Rectangle { Height = 7, Width = 44, RadiusX = 3.5, RadiusY = 3.5, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Bottom, Margin = new Thickness(0, 0, 0, 5.5) };
        if (placement == ControlsPlacement.Idle)
        {
            bar.Stroke = colour;
            bar.StrokeThickness = 1.2;
            bar.StrokeDashArray = new DoubleCollection { 2.5, 2 };
        }
        else
        {
            bar.Fill = colour;
        }
        grid.Children.Add(bar);
        return grid.Quiet();
    }
}

/// <summary>
/// Audio and video (settings/MediaSettings.kt): the picture of yourself with
/// how loudly the microphone hears you over its foot, what starts on when
/// you join, and which devices; the voice as it is.
///
/// The camera and the microphone are open while the page shows, and only
/// then (CallMedia.StartCheck, Android's DeviceCheck): the web's join screen
/// answers exactly those two questions about the two devices (PreJoin.tsx,
/// MicLevel.tsx), and this is where they are chosen ahead of any call. A
/// device picked is remembered at once and shown at once, and a call that
/// is on moves to it (AppModel.ChooseDevice). The lists follow the devices
/// as they come and go.
/// </summary>
internal static class MediaSettings
{
    public static StackPanel Page(AppModel app)
    {
        var page = K.Column(22);
        page.Children.Add(new DeviceCheck(app));

        var microphone = Fields.Switch(app.StartWithMicrophone, on => app.StartWithMicrophone = on, T("Start with the microphone on"));
        var camera = Fields.Switch(app.StartWithCamera, on => app.StartWithCamera = on, T("Start with the camera on"));
        var original = Fields.Switch(app.DeviceMemory.OriginalSound, on => app.DeviceMemory.OriginalSound = on, T("Original sound"));
        page.Children.Add(Rows.Section(T("When you join"), Rows.Card(
            Rows.Row(T("Start with the microphone on"), microphone, symbol: "mic.fill"),
            Rows.Row(T("Start with the camera on"), camera, symbol: "video.fill"),
            Rows.Row(T("Original sound"), original, T("No noise suppression or automatic volume. For headphones and music."), "waveform", last: true))));

        page.Children.Add(new DeviceChoices(app));
        return page;
    }
}

/// <summary>
/// The camera as the room will see it, with how loudly the microphone hears
/// you over its foot (MediaSettings.kt <c>DevicePreview</c>): open while the
/// page shows, closed when it goes.
/// </summary>
internal sealed class DeviceCheck : Grid
{
    private readonly AppModel _app;
    private readonly LevelMeter _meter = new(12);
    private readonly Grid _symbol = new();
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _timer;
    private bool _loud;

    public DeviceCheck(AppModel app)
    {
        _app = app;
        CornerRadius = new CornerRadius(20);
        BorderBrush = Ink.White(0.08f);
        BorderThickness = new Thickness(1);
        // A camera-off tile's grey, as the call's framed preview is.
        Background = Ink.Surface;
        SizeChanged += (_, e) => Height = e.NewSize.Width * 9 / 16;
        // Under the picture, showing until the camera's first frame covers it.
        Children.Add(K.Symbol("video.fill", 28, Ink.Muted));
        Children.Add(new SelfView(app.Media).Arrives(1f));
        Paint(false);
        var pill = K.Line(9, K.Fixed(_symbol), K.Fixed(K.Text(T("Hearing you"), 12, K.Medium, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(_meter));
        var glass = K.Glass(pill, 16, new Thickness(12, 0, 12, 0), shadow: false);
        glass.Height = 32;
        glass.HorizontalAlignment = HorizontalAlignment.Left;
        glass.VerticalAlignment = VerticalAlignment.Bottom;
        glass.Margin = new Thickness(12);
        Children.Add(glass.Arrives(1f, fromY: 6));
        _timer = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread().CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(33);
        _timer.Tick += (_, _) =>
        {
            var shown = VoiceCapture.Shown(_app.Media.MicrophoneRms);
            _meter.Show(shown);
            Paint(shown > 0.08);
        };
        Loaded += (_, _) =>
        {
            _ = _app.Media.StartCheck();
            _timer.Start();
        };
        Unloaded += (_, _) =>
        {
            _timer.Stop();
            _app.Media.StopCheck();
        };
    }

    private bool? _painted;

    private void Paint(bool loud)
    {
        _loud = loud;
        if (_painted == loud) return;
        _painted = loud;
        _symbol.Children.Clear();
        _symbol.Children.Add(K.Symbol("mic.fill", 12, _loud ? Ink.Tally : Ink.Foreground));
    }
}

/// <summary>
/// Which camera, microphone and speakers calls start on (MediaSettings.kt
/// <c>DeviceChoice</c>): Windows' own drop-downs, the system's first ("Same
/// as the system", kept as no choice at all), read again whenever a device
/// comes or goes; and Forget, while anything is picked.
/// </summary>
internal sealed class DeviceChoices : StackPanel
{
    private readonly AppModel _app;
    private readonly StackPanel _rows = K.Column(0);
    private readonly StackPanel _forget = K.Column(0);
    private readonly StackPanel _footer = K.Column(0);
    private int _filling;

    public DeviceChoices(AppModel app)
    {
        _app = app;
        Spacing = 8;
        Children.Add(K.Text(T("Devices"), 12, K.Semibold, Ink.Muted).Pad(6, 0, 0, 0));
        Children.Add(K.Card(K.Column(0, _rows, _forget), 16));
        Children.Add(_footer);
        Loaded += (_, _) =>
        {
            _app.Media.DevicesChanged += Fill;
            Fill();
        };
        Unloaded += (_, _) => _app.Media.DevicesChanged -= Fill;
    }

    private async void Fill()
    {
        var mine = ++_filling;
        var cameras = await DeviceList.Cameras();
        var microphones = await DeviceList.Microphones();
        var speakers = await DeviceList.Speakers();
        if (mine != _filling) return;
        var memory = _app.DeviceMemory;
        var picked = memory.Choices().Count > 0;
        _rows.Children.Clear();
        _rows.Children.Add(AdaptiveRow(T("Camera"), "video", Pick(cameras, DeviceMemory.Kind.Camera)));
        // The web's "Blur my background" tick, kept as the web keeps it
        // (meet-live.blur) and applied at once: to the picture above, and to
        // a call that is on. Under the camera, as on Android (MediaSettings.kt).
        _rows.Children.Add(Rows.Row(T("Blur my background"), Fields.Switch(_app.Blurring, _app.SetBlur, T("Blur my background")), symbol: "person.and.background.dotted"));
        _rows.Children.Add(AdaptiveRow(T("Microphone"), "mic", Pick(microphones, DeviceMemory.Kind.Microphone), last: speakers.Count == 0 && !picked));
        if (speakers.Count > 0) _rows.Children.Add(AdaptiveRow(T("Speakers"), "speaker.wave.2", Pick(speakers, DeviceMemory.Kind.Speakers), last: !picked));
        Picked();
    }

    /// <summary>Forget and the footer, arriving with the first device picked and leaving when they are forgotten.</summary>
    private void Picked()
    {
        var picked = _app.DeviceMemory.Choices().Count > 0;
        if (picked && _forget.Children.Count == 0)
        {
            var forget = Tap.Words(T("Forget"), height: 30);
            forget.HorizontalAlignment = HorizontalAlignment.Right;
            forget.Margin = new Thickness(14, 9, 14, 9);
            forget.Click += async (_, _) =>
            {
                await _app.ForgetDevices();
                Fill();
            };
            _forget.Children.Add(forget.Arrives(1f, fromY: -6));
            _footer.Children.Add(K.Text(T("Calls start on the microphone, camera and speakers you last picked in one, while they are plugged in."), 11.5, ink: Ink.Muted).Pad(6, 0, 6, 0).Arrives(1f));
        }
        else if (!picked && _forget.Children.Count > 0)
        {
            _forget.Children.Clear();
            _footer.Children.Clear();
        }
    }

    /// <summary>
    /// A row whose drop-down goes under its words where the two will not fit
    /// side by side (Pieces.kt <c>AdaptiveRow</c>, Settings.swift's
    /// <c>ViewThatFits</c>): on a phone's width "Microphone" beside a list
    /// of device names was broken across two lines.
    /// </summary>
    private static StackPanel AdaptiveRow(string title, string symbol, ComboBox control, bool last = false)
    {
        var row = new AdaptiveRowPanel(Rows.Tile(symbol), K.Text(title, 13.5, lines: 1), control);
        var column = K.Column(0, row);
        if (!last) column.Children.Add(K.Rule(52));
        return column;
    }

    /// <summary>
    /// One kind's drop-down. A device remembered but not plugged in is shown
    /// as the system's, which is what a call would start on (preferences.ts
    /// <c>restorable</c>), and stays remembered for when it is.
    /// </summary>
    private ComboBox Pick(IReadOnlyList<DeviceOption> options, DeviceMemory.Kind kind)
    {
        var box = new ComboBox { MinWidth = 200, MaxWidth = 280 };
        box.Named(T(kind switch { DeviceMemory.Kind.Camera => "Camera", DeviceMemory.Kind.Microphone => "Microphone", _ => "Speakers" }));
        box.Items.Add(T("Same as the system"));
        foreach (var option in options) box.Items.Add(option.Name);
        var index = options.ToList().FindIndex(o => o.Id == _app.DeviceMemory.Choice(kind));
        box.SelectedIndex = index < 0 ? 0 : index + 1;
        box.SelectionChanged += async (_, _) =>
        {
            if (box.SelectedIndex < 0) return;
            await _app.ChooseDevice(kind, box.SelectedIndex == 0 ? null : options[box.SelectedIndex - 1].Id);
            Picked();
        };
        return box;
    }
}

/// <summary>
/// The account (settings/Account.kt): who you are here, with your picture;
/// what has been opened to you; the passphrase; and, for whoever may, links
/// that let somebody outside your scope in as a guest.
/// </summary>
internal static class AccountSettings
{
    public static StackPanel Page(AppModel app)
    {
        var page = K.Column(22);
        if (app.Account is not { } account)
        {
            page.Children.Add(K.Text(T("Sign in to change your passphrase or picture."), 12.5, ink: Ink.Muted));
            return page;
        }
        var said = K.Text("", 12.5, ink: Ink.Danger);
        said.Visibility = Visibility.Collapsed;
        said.Arrives();
        void Say(string? text)
        {
            said.Text = text ?? "";
            said.Visibility = text == null ? Visibility.Collapsed : Visibility.Visible;
        }
        page.Children.Add(Profile(app, account, Say));
        page.Children.Add(Abilities(app, account));
        page.Children.Add(Passphrase(app, Say));
        if (app.Holds("invite") && (account.Admin || account.Scopes.Length > 0)) page.Children.Add(GuestLinks.Section(app, account));
        page.Children.Add(said);
        return page;
    }

    private static Border Profile(AppModel app, uniffi.tomoshibi_protocol.Account account, Action<string?> say)
    {
        var avatar = Faces.AccountAvatar(account.Name, app.AvatarAddress, 72);
        var trip = K.Text(account.Trip, 12, mono: true, ink: Ink.Muted, lines: 1);
        trip.IsTextSelectionEnabled = true;
        var top = K.Line(16, K.Fixed(avatar), K.Flex(K.Column(4, K.Text(account.Name, 21, K.Bold, lines: 1), trip).At(HorizontalAlignment.Left, VerticalAlignment.Center)));
        top.Padding = new Thickness(16);
        var choose = Tap.Words(T("Choose a picture"), "photo", height: 32, size: 12.5);
        var buttons = K.Row(8, choose);
        if (account.Avatar != null)
        {
            var remove = Tap.Words(T("Remove"), "trash", height: 32, size: 12.5);
            remove.Click += async (_, _) => say(await app.RemovePicture());
            buttons.Children.Add(remove);
        }
        var signOut = Tap.Words(T("Sign out"), "rectangle.portrait.and.arrow.right", height: 32, size: 12.5);
        signOut.Click += async (_, _) => await app.SignOut();
        buttons.Children.Add(signOut);
        choose.Click += async (_, _) =>
        {
            if (await Picture.Choose() is { } dataUrl) say(await app.SetPicture(dataUrl));
        };
        var bottom = K.Column(10, K.Text(T("Shown beside your name in a call. Scaled down before it is sent."), 11.5, ink: Ink.Muted), buttons);
        bottom.Padding = new Thickness(14);
        return K.Card(K.Column(0, top, K.Rule(0), bottom), 20);
    }

    /// <summary>What has been opened to this account (Account.kt <c>Abilities</c>), and its scopes and age.</summary>
    private static StackPanel Abilities(AppModel app, uniffi.tomoshibi_protocol.Account account)
    {
        var permissions = new (string Key, string Name, string Symbol)[]
        {
            ("watch", T("Watch together"), "play.rectangle"),
            ("emby", T("Emby library"), "film"),
            ("music", T("Music"), "music.note"),
            ("arrange", T("Arrange meetings"), "calendar.badge.clock"),
            ("invite", T("Guest links"), "link"),
            ("open", T("Open new rooms"), "door.left.hand.open"),
        };
        var held = new VariableSizedWrapGrid { Orientation = Orientation.Horizontal };
        var withheld = new VariableSizedWrapGrid { Orientation = Orientation.Horizontal };
        foreach (var (key, name, symbol) in permissions)
        {
            if (app.Holds(key)) held.Children.Add(Chip(name, symbol, lit: key == "emby"));
            else if (key != "emby") withheld.Children.Add(Chip(name, symbol, withheld: true));
        }
        var column = K.Column(12, held);
        if (withheld.Children.Count > 0) column.Children.Add(K.Column(6, K.Text(T("Not opened to this account"), 11, ink: Ink.Muted), withheld));
        if (account.Scopes.Length > 0 || account.Created != null)
        {
            column.Children.Add(K.Rule(0));
            if (account.Scopes.Length > 0)
            {
                var scopes = K.Row(5);
                foreach (var scope in account.Scopes) scopes.Children.Add(Chip("@" + scope, null, mono: true));
                column.Children.Add(K.Line(14, K.Fixed(K.Text(T("Your scopes"), 12, ink: Ink.Muted)), K.Flex(scopes)));
            }
            if (Refusals.Instant(account.Created) is { } created)
            {
                column.Children.Add(K.Line(14, K.Fixed(K.Text(T("Account created"), 12, ink: Ink.Muted)), K.Flex(K.Text(Times.Written(created, "longdate"), 12.5))));
            }
        }
        return Rows.Section(T("What you can use"), K.Card(column, 16, padding: new Thickness(14)), T("Set by whoever runs this deployment."));
    }

    private static Border Chip(string text, string? symbol, bool lit = false, bool withheld = false, bool mono = false)
    {
        var row = K.Row(5);
        if (symbol != null) row.Children.Add(K.Symbol(symbol, 10.5, lit ? Ink.Tally : withheld ? Ink.MutedAt(0.7f) : Ink.Muted));
        row.Children.Add(K.Text(text, mono ? 11.5 : 12, mono: mono, ink: withheld ? Ink.Muted : Ink.Foreground, lines: 1));
        foreach (var child in row.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        return new Border
        {
            Child = row,
            Padding = new Thickness(9, 5, 9, 5),
            CornerRadius = new CornerRadius(12),
            Margin = new Thickness(0, 0, 6, 6),
            Background = withheld ? null : lit ? Ink.TallyAt(0.14f) : Ink.White(0.08f),
            BorderBrush = lit ? Ink.TallyAt(0.4f) : Ink.White(withheld ? 0.18f : 0.08f),
            BorderThickness = new Thickness(1),
        };
    }

    /// <summary>Changing the passphrase (Account.kt <c>Passphrase</c>): the current one, the new one of at least eight characters.</summary>
    private static StackPanel Passphrase(AppModel app, Action<string?> say)
    {
        string current = "", next = "";
        var changed = K.Row(6, K.Symbol("checkmark.circle.fill", 12, Ink.Good), K.Text(T("Changed."), 12, ink: Ink.Good));
        changed.Visibility = Visibility.Collapsed;
        changed.Arrives();
        var change = new Shared.Pressing(T("Change it"), 32, 12.5) { HorizontalAlignment = HorizontalAlignment.Left };
        void Ready() => change.IsEnabled = !change.Busy && current.Length > 0 && next.Length >= 8;
        var currentBox = Fields.Secret(T("Current passphrase"), "", text => { current = text; Ready(); });
        PasswordBox? nextBox = null;
        async void Change()
        {
            if (!change.IsEnabled) return;
            change.Busy = true;
            changed.Visibility = Visibility.Collapsed;
            Ready();
            var refusal = await app.ChangePassphrase(current, next);
            change.Busy = false;
            if (refusal != null) say(refusal);
            else
            {
                currentBox.Password = "";
                nextBox!.Password = "";
                say(null);
                changed.Visibility = Visibility.Visible;
            }
            Ready();
        }
        nextBox = Fields.Secret(T("New passphrase"), "", text => { next = text; Ready(); }, submit: Change);
        change.Click += (_, _) => Change();
        Ready();
        var column = K.Column(10,
            K.Text(T("This is how you are recognised, in a call and here. At least eight characters."), 11.5, ink: Ink.Muted),
            currentBox, nextBox, K.Row(10, change, changed));
        return Rows.Section(T("Passphrase"), K.Card(column, 16, padding: new Thickness(14)),
            T("Your passphrase is not stored here and cannot be sent to you. If you lose it, an administrator sets a new one."));
    }
}

/// <summary>About (settings/About.kt): the app's version, the media core's, the server's build, and where the code can be read; never the server's address.</summary>
internal static class About
{
    public static StackPanel Page(AppModel app)
    {
        // The package's version where the app is installed from one, which is
        // what an update is compared with: 0.1.0.7 written 0.1.0 (7), as the
        // other apps write theirs.
        var parts = ClientDescription.Version.Split('.');
        var appVersion = parts.Length == 4 ? $"{parts[0]}.{parts[1]}.{parts[2]} ({parts[3]})" : ClientDescription.Version;
        var mark = new Mark(76);
        var header = K.Row(18, mark, K.Column(4, K.Text("Tomoshibi", 26, K.Bold), K.Text(appVersion, 13, mono: true, ink: Ink.Muted)));
        foreach (var child in header.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        var rows = new List<UIElement>
        {
            Rows.Row(T("Version"), K.Text(appVersion, 12.5, mono: true, ink: Ink.Muted), symbol: "app.badge"),
        };
        // Only where the app looks for updates at all: not in a debug build
        // started by hand, which would ask the home deployment.
        if (app.Updates is { } updates) rows.Add(UpdateRow(updates));
        rows.AddRange(new UIElement[]
        {
            Rows.Row(T("Media core"), K.Text(uniffi.tomoshibi_core.TomoshibiCoreMethods.CoreVersion(), 12.5, mono: true, ink: Ink.Muted), symbol: "cpu", last: app.Info == null),
        });
        if (app.Info is { } info)
        {
            var build = string.IsNullOrEmpty(info.Build) ? "—" : info.Build[..Math.Min(12, info.Build.Length)];
            rows.Add(Rows.Row(T("Server"), K.Text(build, 12.5, mono: true, ink: Ink.Muted), symbol: "server.rack", last: string.IsNullOrEmpty(info.Source)));
            if (WebAddress.Of(info.Source) is { } address)
            {
                var source = info.Source!;
                var line = Rows.Row("AGPL-3.0 · source", K.Symbol("arrow.up.right", 11.5, Ink.Muted), source, "chevron.left.forwardslash.chevron.right", last: true);
                var tap = new Tap(line, Look.Clear, 48, padding: 0)
                {
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Stretch,
                    CornerRadius = new CornerRadius(0),
                };
                tap.Height = double.NaN;
                tap.Named("AGPL-3.0 · source");
                tap.Click += async (_, _) => await Windows.System.Launcher.LaunchUriAsync(address);
                rows.Add(tap);
            }
        }
        return K.Column(22, header, Rows.Section(T("About"), Rows.Card(rows.ToArray())));
    }

    /// <summary>
    /// Where the app's updates stand, and the check somebody can ask for
    /// (About.kt <c>UpdateRow</c>, Settings.swift): the version on offer with
    /// "Update", a download's progress with "Cancel", and the answer to a
    /// check, which says so even when there is nothing newer. What it says
    /// changes with a fade.
    /// </summary>
    private static UIElement UpdateRow(Updates updates)
    {
        var line = K.Text("", 11.5, ink: Ink.Muted);
        var lineHolder = new Unfolding();
        var words = K.Column(2, K.Text(T("Updates"), 13.5), lineHolder);
        words.VerticalAlignment = VerticalAlignment.Center;
        var action = new Grid { VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(22, 0, 0, 0) };
        var row = K.Line(12, K.Fixed(Rows.Tile("arrow.clockwise")), K.Flex(words), K.Fixed(action));
        row.MinHeight = 48;
        row.Padding = new Thickness(14, 9, 14, 9);
        var column = K.Column(0, row, K.Rule(52));
        column.Named(T("Updates"), "settings.updates");
        string? saidLine = null;
        string? saidAction = null;

        void Refresh()
        {
            var step = updates.Now;
            var said = updates.Answer;
            var text = step switch
            {
                Updates.Step.Downloading d => T("{received} of {total}", ("received", UpdateBanner.Megabytes(d.Received)), ("total", UpdateBanner.Megabytes(d.Total))),
                Updates.Step.Held h => T("{received} of {total}", ("received", UpdateBanner.Megabytes(h.Received)), ("total", UpdateBanner.Megabytes(h.Total))),
                Updates.Step.Installing i => T("Installing Tomoshibi {version}", ("version", i.Offer.Version)),
                Updates.Step.Offered o => o.Problem ?? T("Version {version} is available.", ("version", o.Offer.Version)),
                _ => said switch
                {
                    Updates.Said.Checking => T("Checking for updates…"),
                    Updates.Said.Current => T("You have the latest version."),
                    Updates.Said.Failed => T("Could not check for updates. Try again later."),
                    _ => null,
                },
            };
            var problem = step is Updates.Step.Offered { Problem: not null };
            if (text != saidLine)
            {
                // Progress is said in place; anything else fades across.
                var progressOnly = saidLine != null && text != null && step is Updates.Step.Downloading or Updates.Step.Held && saidAction == "cancel";
                saidLine = text;
                if (progressOnly)
                {
                    line.Text = text!;
                }
                else
                {
                    line = K.Text(text ?? "", 11.5, ink: problem ? Ink.Danger : Ink.Muted);
                    lineHolder.Show(text == null ? null : line);
                }
            }
            var kind = step switch
            {
                Updates.Step.Offered => "update",
                Updates.Step.Downloading or Updates.Step.Held => "cancel",
                _ => said == Updates.Said.Checking ? "checking" : step is Updates.Step.None ? "check" : "check-off",
            };
            if (kind == saidAction) return;
            saidAction = kind;
            action.Children.Clear();
            var label = kind switch { "update" => T("Update"), "cancel" => T("Cancel"), _ => T("Check for updates") };
            var button = Tap.Words(label, look: kind == "update" ? Look.Prominent : Look.Plain, height: 30, size: 13);
            button.Named(label, kind switch { "update" => "settings.update", "cancel" => "settings.cancelUpdate", _ => "settings.checkUpdates" });
            button.Click += (_, _) =>
            {
                switch (kind)
                {
                    case "update": updates.Accept(); break;
                    case "cancel": updates.Cancel(); break;
                    default: updates.CheckNow(); break;
                }
            };
            button.IsEnabled = kind == "update" || kind == "cancel" || kind == "check";
            if (kind == "checking")
            {
                button.Content = K.Row(6, K.Spinner(12), K.Text(T("Check for updates"), 13, K.Semibold, lines: 1));
            }
            button.Arrives();
            action.Children.Add(button);
        }

        Refresh();
        column.Loaded += (_, _) =>
        {
            updates.Changed -= Refresh;
            updates.Changed += Refresh;
            Refresh();
        };
        column.Unloaded += (_, _) => updates.Changed -= Refresh;
        return column;
    }
}

/// <summary>
/// The tile, the words and the control of a settings row, side by side where
/// they fit and the control under the words where they do not (Pieces.kt
/// <c>AdaptiveRow</c>). Decided in the panel's own measure: deciding it in a
/// SizeChanged handler measured the children again inside layout, and a
/// phone's width threw "Layout cycle detected".
/// </summary>
internal sealed class AdaptiveRowPanel : Panel
{
    private const double PadX = 14, PadY = 9, Gap = 12, ControlGap = 22, LineGap = 8;
    private readonly FrameworkElement _tile, _words, _control;
    private bool _side = true;

    public AdaptiveRowPanel(FrameworkElement tile, FrameworkElement words, FrameworkElement control)
    {
        _tile = tile;
        _words = words;
        _control = control;
        Children.Add(tile);
        Children.Add(words);
        Children.Add(control);
        MinHeight = 48;
    }

    protected override Windows.Foundation.Size MeasureOverride(Windows.Foundation.Size available)
    {
        var unbounded = new Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity);
        _tile.Measure(unbounded);
        _words.Measure(unbounded);
        _control.Measure(unbounded);
        var inner = double.IsInfinity(available.Width) ? double.PositiveInfinity : Math.Max(0, available.Width - 2 * PadX);
        var tile = _tile.DesiredSize;
        var control = _control.DesiredSize;
        _side = tile.Width + Gap + _words.DesiredSize.Width + ControlGap + control.Width <= inner;
        double height;
        if (_side)
        {
            height = Math.Max(Math.Max(tile.Height, _words.DesiredSize.Height), control.Height);
        }
        else
        {
            _words.Measure(new Windows.Foundation.Size(Math.Max(0, inner - tile.Width - Gap), double.PositiveInfinity));
            height = Math.Max(tile.Height, _words.DesiredSize.Height) + LineGap + control.Height;
        }
        var width = double.IsInfinity(available.Width) ? tile.Width + Gap + _words.DesiredSize.Width + ControlGap + control.Width + 2 * PadX : available.Width;
        return new Windows.Foundation.Size(width, Math.Max(MinHeight, height + 2 * PadY));
    }

    protected override Windows.Foundation.Size ArrangeOverride(Windows.Foundation.Size final)
    {
        var tile = _tile.DesiredSize;
        var control = _control.DesiredSize;
        var lead = PadX + tile.Width + Gap;
        if (_side)
        {
            var middle = final.Height / 2;
            _tile.Arrange(new Windows.Foundation.Rect(PadX, middle - tile.Height / 2, tile.Width, tile.Height));
            var wordsWidth = Math.Max(0, final.Width - lead - ControlGap - control.Width - PadX);
            _words.Arrange(new Windows.Foundation.Rect(lead, middle - _words.DesiredSize.Height / 2, wordsWidth, _words.DesiredSize.Height));
            _control.Arrange(new Windows.Foundation.Rect(final.Width - PadX - control.Width, middle - control.Height / 2, control.Width, control.Height));
        }
        else
        {
            var top = Math.Max(tile.Height, _words.DesiredSize.Height);
            _tile.Arrange(new Windows.Foundation.Rect(PadX, PadY + (top - tile.Height) / 2, tile.Width, tile.Height));
            _words.Arrange(new Windows.Foundation.Rect(lead, PadY + (top - _words.DesiredSize.Height) / 2, Math.Max(0, final.Width - lead - PadX), _words.DesiredSize.Height));
            var width = Math.Min(control.Width, Math.Max(0, final.Width - lead - PadX));
            _control.Arrange(new Windows.Foundation.Rect(lead, PadY + top + LineGap, width, control.Height));
        }
        return final;
    }
}
