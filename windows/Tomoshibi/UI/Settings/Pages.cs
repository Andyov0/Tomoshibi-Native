using System.Reflection;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;
using Tomoshibi.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
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
        var tile = new Border
        {
            Width = 104,
            Height = 66,
            CornerRadius = new CornerRadius(12),
            BorderBrush = Ink.Tally,
            BorderThickness = new Thickness(2),
            Background = Ink.Brush(new Shade(0.2f, 0.25f, 0.32f)),
            Child = Faces.Initial("Mika", 30).At(HorizontalAlignment.Center, VerticalAlignment.Center),
        };
        var join = new Border { Height = 28, CornerRadius = new CornerRadius(14), Background = Ink.Tally, Padding = new Thickness(16, 0, 16, 0), Child = K.Text(T("Join"), 12.5, K.Semibold, Ink.TallyForeground, lines: 1).At(HorizontalAlignment.Center, VerticalAlignment.Center) };
        var switchOn = new ToggleSwitch { IsOn = true, IsEnabled = false, OnContent = "", OffContent = "", MinWidth = 0 };
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
        if (appearance.ConnectionBadge != ConnectionBadge.Hidden)
        {
            var bars = K.Row(2);
            foreach (var height in new[] { 6.0, 10.0, 14.0 }) bars.Children.Add(new Rectangle { Width = 3.5, Height = height, RadiusX = 1, RadiusY = 1, Fill = Ink.Good, VerticalAlignment = VerticalAlignment.Bottom });
            var row = K.Row(7, bars);
            if (appearance.ConnectionBadge == ConnectionBadge.Detailed) row.Children.Add(K.Text("32 ms", 11.5, K.Medium, Ink.Muted, tabular: true));
            var badge = K.Glass(row, 14, new Thickness(10, 7, 10, 7), shadow: false);
            badge.HorizontalAlignment = HorizontalAlignment.Left;
            badge.VerticalAlignment = VerticalAlignment.Top;
            badge.Margin = new Thickness(10);
            preview.Children.Add(badge.Arrives());
        }
        return K.Card(K.Column(14, preview.Quiet(), choice), 16, padding: new Thickness(14));
    }

    /// <summary>Where the controls sit in a call: always, out of the way while nothing happens, or down the side.</summary>
    private static Border ControlsCard()
    {
        var appearance = Appearance.Shared;
        var row = new Grid { ColumnSpacing = 10 };
        var placements = Enum.GetValues<ControlsPlacement>();
        for (var i = 0; i < placements.Length; i++)
        {
            row.ColumnDefinitions.Add(new ColumnDefinition());
            var placement = placements[i];
            var chosen = placement == appearance.ControlsPlacement;
            var card = new Tap(K.Column(9, Schematic(placement, chosen), K.Text(placement.DisplayName(), 11.5, chosen ? K.Semibold : K.Regular, chosen ? Ink.Foreground : Ink.Muted, align: TextAlignment.Center)), Look.Plain, 120, padding: 8)
            {
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                VerticalContentAlignment = VerticalAlignment.Top,
                CornerRadius = new CornerRadius(14),
            };
            card.Height = double.NaN;
            card.Padding = new Thickness(8);
            if (chosen)
            {
                card.Resting = Ink.TallyShade.WithAlpha(0.12f);
                card.BorderBrush = Ink.TallyAt(0.6f);
                card.BorderThickness = new Thickness(1.5);
            }
            card.Named(placement.DisplayName());
            card.Click += (_, _) => appearance.Choose(placement);
            Grid.SetColumn(card, i);
            row.Children.Add(card);
        }
        return K.Card(row, 16, padding: new Thickness(12));
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
/// Audio and video (settings/MediaSettings.kt): the picture of yourself, what
/// starts on when you join, and which devices; the voice as it is.
///
/// TODO(windows 2b): the level meter under the picture needs the microphone
/// opened while the settings show; blurring the background needs a
/// segmentation model on Windows.
/// </summary>
internal static class MediaSettings
{
    public static StackPanel Page(AppModel app)
    {
        var page = K.Column(22);
        var preview = new Shared.CameraStage(app, 20);
        var frame = new Grid { Children = { preview }, BorderBrush = Ink.White(0.08f), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(20) };
        frame.SizeChanged += (_, e) => frame.Height = e.NewSize.Width * 9 / 16;
        if (!app.StartWithCamera) frame.Children.Add(K.Symbol("video.fill", 28, Ink.Muted).Arrives());
        page.Children.Add(frame);

        var microphone = Fields.Switch(app.StartWithMicrophone, on => app.StartWithMicrophone = on, T("Start with the microphone on"));
        var camera = Fields.Switch(app.StartWithCamera, on =>
        {
            app.StartWithCamera = on;
            preview.Update();
        }, T("Start with the camera on"));
        var original = Fields.Switch(app.DeviceMemory.OriginalSound, on => app.DeviceMemory.OriginalSound = on, T("Original sound"));
        page.Children.Add(Rows.Section(T("When you join"), Rows.Card(
            Rows.Row(T("Start with the microphone on"), microphone, symbol: "mic.fill"),
            Rows.Row(T("Start with the camera on"), camera, symbol: "video.fill"),
            Rows.Row(T("Original sound"), original, T("No noise suppression or automatic volume. For headphones and music."), "waveform", last: true))));

        var devices = K.Column(0);
        var section = Rows.Section(T("Devices"), Rows.Card(devices), app.DeviceMemory.Choices().Count > 0 ? T("Calls start on the microphone, camera and speakers you last picked in one, while they are plugged in.") : null);
        page.Children.Add(section);
        _ = FillDevices(app, devices);
        return page;
    }

    private static async Task FillDevices(AppModel app, StackPanel devices)
    {
        var memory = app.DeviceMemory;
        var cameras = await DeviceList.Cameras();
        var microphones = await DeviceList.Microphones();
        var speakers = await DeviceList.Speakers();
        devices.Children.Add(Rows.Row(T("Camera"), Pick(cameras, memory.Choice(DeviceMemory.Kind.Camera), id =>
        {
            memory.Remember(DeviceMemory.Kind.Camera, id);
            app.Media.CameraId = id;
        }), symbol: "video"));
        devices.Children.Add(Rows.Row(T("Microphone"), Pick(microphones, memory.Choice(DeviceMemory.Kind.Microphone), id =>
        {
            memory.Remember(DeviceMemory.Kind.Microphone, id);
            app.Media.MicrophoneId = id;
        }), symbol: "mic", last: speakers.Count == 0));
        if (speakers.Count > 0) devices.Children.Add(Rows.Row(T("Speakers"), Pick(speakers, memory.Choice(DeviceMemory.Kind.Speakers), id => memory.Remember(DeviceMemory.Kind.Speakers, id)), symbol: "speaker.wave.2", last: memory.Choices().Count == 0));
        if (memory.Choices().Count > 0)
        {
            var forget = Tap.Words(T("Forget"), height: 30);
            forget.HorizontalAlignment = HorizontalAlignment.Right;
            forget.Margin = new Thickness(14, 9, 14, 9);
            forget.Click += (_, _) =>
            {
                memory.ForgetChoices();
                app.Media.CameraId = null;
                app.Media.MicrophoneId = null;
                forget.IsEnabled = false;
            };
            devices.Children.Add(forget);
        }
    }

    /// <summary>Windows' own drop-down of the devices, the system's first ("Same as the system").</summary>
    private static ComboBox Pick(IReadOnlyList<DeviceOption> options, string? chosen, Action<string?> pick)
    {
        var box = new ComboBox { MinWidth = 200, MaxWidth = 280 };
        box.Items.Add(T("Same as the system"));
        foreach (var option in options) box.Items.Add(option.Name);
        var index = options.ToList().FindIndex(o => o.Id == chosen);
        box.SelectedIndex = index < 0 ? 0 : index + 1;
        box.SelectionChanged += (_, _) => pick(box.SelectedIndex <= 0 ? null : options[box.SelectedIndex - 1].Id);
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
        var version = Assembly.GetExecutingAssembly().GetName().Version;
        var appVersion = version == null ? "" : $"{version.Major}.{version.Minor}.{version.Build} ({version.Revision})";
        var mark = new Mark(76);
        var header = K.Row(18, mark, K.Column(4, K.Text("Tomoshibi", 26, K.Bold), K.Text(appVersion, 13, mono: true, ink: Ink.Muted)));
        foreach (var child in header.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        var rows = new List<UIElement>
        {
            Rows.Row(T("Version"), K.Text(appVersion, 12.5, mono: true, ink: Ink.Muted), symbol: "app.badge"),
            Rows.Row(T("Media core"), K.Text(uniffi.tomoshibi_core.TomoshibiCoreMethods.CoreVersion(), 12.5, mono: true, ink: Ink.Muted), symbol: "cpu", last: app.Info == null),
        };
        if (app.Info is { } info)
        {
            var build = string.IsNullOrEmpty(info.Build) ? "—" : info.Build[..Math.Min(12, info.Build.Length)];
            rows.Add(Rows.Row(T("Server"), K.Text(build, 12.5, mono: true, ink: Ink.Muted), symbol: "server.rack", last: string.IsNullOrEmpty(info.Source)));
            if (!string.IsNullOrEmpty(info.Source))
            {
                var source = info.Source;
                var line = Rows.Row("AGPL-3.0 · source", K.Symbol("arrow.up.right", 11.5, Ink.Muted), source, "chevron.left.forwardslash.chevron.right", last: true);
                var tap = new Tap(line, Look.Clear, 48, padding: 0)
                {
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Stretch,
                    CornerRadius = new CornerRadius(0),
                };
                tap.Height = double.NaN;
                tap.Named("AGPL-3.0 · source");
                tap.Click += async (_, _) => await Windows.System.Launcher.LaunchUriAsync(new Uri(source));
                rows.Add(tap);
            }
        }
        return K.Column(22, header, Rows.Section(T("About"), Rows.Card(rows.ToArray())));
    }
}
