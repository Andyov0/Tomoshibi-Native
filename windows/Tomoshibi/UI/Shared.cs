using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI;

/// <summary>The pieces the screens before a call share (Android's ui/screens/Screens.kt, ui/lobby/Pieces.kt, kit/Fields.kt).</summary>
internal static class Shared
{
    /// <summary>The window's title bar, which the content runs under: what sits at the top keeps clear of it and of the caption buttons.</summary>
    public const double TitleBar = 32;

    /// <summary>A control over the content wearing the control layer's material: acrylic behind a clear button.</summary>
    public static Border OnGlass(Tap tap, double? radius = null)
    {
        tap.Look = Look.Clear;
        var glass = K.Glass(tap, radius ?? tap.Height / 2, shadow: false);
        glass.HorizontalAlignment = HorizontalAlignment.Left;
        glass.VerticalAlignment = VerticalAlignment.Top;
        return glass;
    }

    /// <summary>
    /// The language, in the corner of the screens before a call (SignIn.kt
    /// <c>LanguageMenu</c>): the system's, or one of the four, each named in
    /// itself, since a language picker is read by somebody who cannot yet
    /// read the language on the screen.
    /// </summary>
    public static Border LanguageMenu()
    {
        var tap = Tap.Icon("globe", T("Language"), size: 36, glyph: 14);
        tap.Click += (_, _) => Overlay.Current?.Toggle("language", tap, () =>
        {
            var words = Localization.Shared;
            var system = Languages.Detect(SystemLanguages());
            var items = new List<UIElement>
            {
                Menu.Item(T("Same as the system"), () => { Overlay.Current?.ClosePopover(); words.FollowSystem(); }, "globe", system.DisplayName(), check: words.FollowsSystem),
                Menu.Rule(),
            };
            foreach (var language in Languages.All)
            {
                items.Add(Menu.Item(language.DisplayName(), () => { Overlay.Current?.ClosePopover(); words.Choose(language); }, check: !words.FollowsSystem && words.Language == language));
            }
            return Menu.Card(items.ToArray());
        }, width: 230);
        var glass = OnGlass(tap);
        glass.HorizontalAlignment = HorizontalAlignment.Right;
        glass.Margin = new Thickness(0, TitleBar + 12, 16, 0);
        return glass;
    }

    /// <summary>The languages Windows lists for this person, in their order.</summary>
    public static IEnumerable<string> SystemLanguages()
    {
        try
        {
            return Windows.System.UserProfile.GlobalizationPreferences.Languages.ToList();
        }
        catch (Exception)
        {
            return new[] { System.Globalization.CultureInfo.CurrentUICulture.Name };
        }
    }

    /// <summary>The plainer screens' heading (Screens.kt <c>Heading</c>).</summary>
    public static StackPanel Heading(string title, string? detail = null, bool mono = false)
    {
        var column = K.Column(6, K.Text(title, mono ? 20 : 24, mono ? K.Regular : K.Bold, mono: mono, lines: 2, align: TextAlignment.Center));
        if (detail != null) column.Children.Add(K.Text(detail, 13.5, ink: Ink.Muted, align: TextAlignment.Center));
        column.HorizontalAlignment = HorizontalAlignment.Stretch;
        return column;
    }

    /// <summary>A line under the form saying something about it, in the muted voice (Screens.kt <c>Note</c>).</summary>
    public static TextBlock Note(string text, bool danger = false)
    {
        var note = K.Text(text, 12.5, ink: danger ? Ink.Danger : Ink.Muted, align: TextAlignment.Center);
        note.HorizontalAlignment = HorizontalAlignment.Stretch;
        return note;
    }

    /// <summary>A quiet way back (Screens.kt <c>BackButton</c>).</summary>
    public static Tap BackButton(Action back)
    {
        var tap = Tap.Words(T("Back"), "chevron.left", height: 32, size: 13);
        tap.HorizontalAlignment = HorizontalAlignment.Center;
        tap.Click += (_, _) => back();
        return tap;
    }

    /// <summary>
    /// The main button of a form: its words, or the spinner while what it
    /// asked is out, crossfading (Screens.kt <c>Pressing</c>).
    /// </summary>
    public sealed class Pressing : Tap
    {
        private readonly Grid _face;
        private readonly TextBlock _words;
        private readonly ProgressRing _spinner;
        private bool _busy;

        public Pressing(string words, double height = 48, double size = 15, Look look = Look.Prominent)
            : this(K.Text(words, size, K.Semibold, InkFor(look), lines: 1), look, height)
        {
        }

        private Pressing(TextBlock words, Look look, double height) : base(new Grid(), look, height)
        {
            _words = words;
            _spinner = K.Spinner(16, InkFor(look));
            _face = (Grid)Content;
            _face.Children.Add(_words);
            _face.Children.Add(_spinner);
            Motion.Hide(_spinner);
            HorizontalAlignment = HorizontalAlignment.Stretch;
            this.Named(words.Text);
        }

        public bool Busy
        {
            get => _busy;
            set
            {
                if (value == _busy) return;
                _busy = value;
                Motion.FadeTo(_words, value ? 0 : 1);
                Motion.FadeTo(_spinner, value ? 1 : 0);
            }
        }

        public string Words
        {
            set
            {
                _words.Text = value;
                this.Named(value);
            }
        }
    }

    /// <summary>
    /// Why the last attempt did not take, in red under the form
    /// (Screens.kt <c>ProblemSlot</c>); one about why they are not in a
    /// meeting can be closed rather than fading.
    /// </summary>
    public sealed class ProblemSlot : Grid
    {
        private readonly AppModel _app;
        private readonly TextBlock _words = K.Text("", 12.5, ink: Ink.Danger);
        private readonly Tap _close;
        private bool _shown;

        public ProblemSlot(AppModel app)
        {
            _app = app;
            _close = Tap.Icon("xmark", T("Close"), Look.Clear, 18, 9.5);
            _close.Click += (_, _) => app.Problem = null;
            Background = Ink.DangerAt(0.10f);
            CornerRadius = new CornerRadius(13);
            Padding = new Thickness(12, 9, 12, 9);
            Children.Add(K.Line(8, K.Fixed(K.Symbol("exclamationmark.circle.fill", 12.5, Ink.Danger).Pad(0, 1, 0, 0).At(HorizontalAlignment.Left, VerticalAlignment.Top)), K.Flex(_words), K.Fixed(_close)));
            this.Arrives();
            Visibility = Visibility.Collapsed;
            Update();
        }

        public void Update()
        {
            if (_app.Problem is { } problem) _words.Text = problem;
            _close.Visibility = _app.ProblemStays ? Visibility.Visible : Visibility.Collapsed;
            var shown = _app.Problem != null;
            if (shown == _shown) return;
            _shown = shown;
            Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
        }
    }

    /// <summary>
    /// Something to tell the person before a call, at the foot of the
    /// screen (Pieces.kt <c>ProblemToast</c>): a capsule of the control
    /// layer's material that rises into view; one that does not stay is
    /// cleared after ten seconds, one that stays has a close button.
    /// </summary>
    public sealed class ProblemToast : Grid
    {
        private readonly AppModel _app;
        private readonly Border _capsule;
        private readonly TextBlock _words = K.Text("", 13);
        private readonly FrameworkElement _mark;
        private readonly Tap _close;
        private string? _said;
        private int _generation;

        public ProblemToast(AppModel app, double clearance = 28)
        {
            _app = app;
            VerticalAlignment = VerticalAlignment.Bottom;
            HorizontalAlignment = HorizontalAlignment.Center;
            _close = Tap.Icon("xmark", T("Close"), size: 24, glyph: 10);
            _close.Click += (_, _) => app.Problem = null;
            _mark = new Grid();
            _capsule = K.Glass(null, 22, new Thickness(14, 9, 7, 9));
            _capsule.MaxWidth = 520;
            _capsule.Margin = new Thickness(20, 0, 20, clearance);
            _capsule.Arrives(1f, fromY: 60);
            _capsule.Visibility = Visibility.Collapsed;
            _capsule.Tapped += (_, _) =>
            {
                if (!app.ProblemStays) app.Problem = null;
            };
            Children.Add(_capsule);
            Update();
        }

        public void Update()
        {
            var problem = _app.Problem;
            if (problem == _said) return;
            _said = problem;
            if (problem == null)
            {
                _capsule.Visibility = Visibility.Collapsed;
                return;
            }
            _words.Text = problem;
            var stays = _app.ProblemStays;
            var symbol = K.Symbol(stays ? "exclamationmark.circle.fill" : "info.circle", 14, stays ? Ink.Tally : Ink.Muted);
            _capsule.Padding = new Thickness(14, 9, stays ? 7 : 16, 9);
            var parts = new List<(UIElement, double)> { K.Fixed(symbol), K.Flex(_words) };
            if (stays) parts.Add(K.Fixed(_close));
            _capsule.Child = K.Line(10, parts.ToArray());
            _capsule.Visibility = Visibility.Visible;
            if (!stays)
            {
                var mine = ++_generation;
                _ = Clear(mine, problem);
            }
        }

        private async Task Clear(int generation, string problem)
        {
            await Task.Delay(TimeSpan.FromSeconds(10));
            if (generation == _generation && _app.Problem == problem && !_app.ProblemStays) _app.Problem = null;
        }
    }

    /// <summary>
    /// The microphone and the camera a join starts with (Pieces.kt
    /// <c>DeviceSwitches</c>): red when off, and remembered across launches.
    /// </summary>
    public static StackPanel DeviceSwitches(AppModel app, double size = 44, Look onLook = Look.Plain)
    {
        Tap? microphone = null, camera = null;
        void Paint()
        {
            var mic = app.StartWithMicrophone;
            var cam = app.StartWithCamera;
            microphone!.Content = K.Symbol(mic ? "mic.fill" : "mic.slash.fill", size * 0.40, Tap.InkFor(mic ? onLook : Look.Destructive));
            microphone.Look = mic ? onLook : Look.Destructive;
            microphone.Tip(mic ? T("Mute microphone") : T("Unmute microphone"));
            camera!.Content = K.Symbol(cam ? "video.fill" : "video.slash.fill", size * 0.38, Tap.InkFor(cam ? onLook : Look.Destructive));
            camera.Look = cam ? onLook : Look.Destructive;
            camera.Tip(cam ? T("Turn camera off") : T("Turn camera on"));
        }
        microphone = new Tap(new Grid(), onLook, size, circle: true);
        camera = new Tap(new Grid(), onLook, size, circle: true);
        microphone.Click += (_, _) => { app.StartWithMicrophone = !app.StartWithMicrophone; Paint(); };
        camera.Click += (_, _) => { app.StartWithCamera = !app.StartWithCamera; Paint(); };
        Paint();
        var row = K.Row(8, microphone, camera);
        row.HorizontalAlignment = HorizontalAlignment.Center;
        return row;
    }

    /// <summary>
    /// The picture of yourself before a call (SelfView.kt <c>CameraStage</c>):
    /// the evening room, and the camera over it while it is to start on.
    /// </summary>
    public sealed class CameraStage : Grid
    {
        private readonly AppModel _app;
        private SelfView? _self;

        public CameraStage(AppModel app, double radius = 0)
        {
            _app = app;
            CornerRadius = new CornerRadius(radius);
            // The surface where it sits on a rounded card, as a tile with its
            // camera off is; the background where it fills the screen.
            Background = radius > 0 ? Ink.Surface : Ink.Background;
            Update();
        }

        public void Update()
        {
            var wanted = _app.StartWithCamera && _app.Call == null && !_app.Joining;
            if (wanted && _self == null)
            {
                _self = new SelfView(_app.Media).Arrives(1f);
                Children.Add(_self);
            }
            else if (!wanted && _self != null)
            {
                Children.Remove(_self);
                _self = null;
            }
        }
    }

    /// <summary>
    /// Who somebody is, as one control (Identity.tsx; Screens.kt
    /// <c>IdentityBox</c>): a name and the passphrase that proves it are one
    /// answer, so they are one box with a hairline through it. A name an
    /// invitation was made for is shown and cannot be changed.
    /// </summary>
    public static Border IdentityBox(
        string name, Action<string> onName,
        string passphrase, Action<string> onPassphrase,
        bool nameOnly = false, bool fixedName = false,
        string? namePrompt = null, Action? submit = null)
    {
        UIElement nameField;
        if (fixedName)
        {
            var shown = K.Text(name, 15, ink: Ink.Muted, lines: 1);
            shown.IsTextSelectionEnabled = true;
            nameField = shown;
        }
        else
        {
            var box = Fields.Text(namePrompt ?? T("Your name"), name, onName, submit: submit);
            Plain(box);
            nameField = box;
        }
        var column = K.Column(0, IdentityRow(fixedName ? "person.fill.checkmark" : "person", fixedName ? Ink.Tally : Ink.Muted, nameField));
        if (!nameOnly)
        {
            column.Children.Add(new Border { Height = 1, Background = Ink.White(0.07f) });
            var key = K.Symbol("key", 15, passphrase.Length == 0 ? Ink.Muted : Ink.Tally);
            var secret = Fields.Secret(T("Passphrase"), passphrase, text =>
            {
                onPassphrase(text);
                if (key is FontIcon icon) icon.Foreground = text.Length == 0 ? Ink.Muted : Ink.Tally;
            }, submit: submit);
            Plain(secret);
            column.Children.Add(IdentityRow(key, secret));
        }
        return new Border
        {
            Child = column,
            CornerRadius = new CornerRadius(18),
            Background = Ink.Black(0.22f),
            BorderBrush = Ink.White(0.10f),
            BorderThickness = new Thickness(1),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
    }

    private static Grid IdentityRow(string symbol, Brush tint, UIElement field) => IdentityRow(K.Symbol(symbol, 15, tint), field);

    private static Grid IdentityRow(FrameworkElement symbol, UIElement field)
    {
        var row = K.Line(12, K.Fixed(new Border { Width = 18, Child = symbol }), K.Flex(field));
        row.Height = 50;
        row.Padding = new Thickness(16, 0, 10, 0);
        return row;
    }

    /// <summary>A field drawn into the box around it, without its own fill or edge.</summary>
    private static void Plain(Control field)
    {
        foreach (var key in new[] { "TextControlBackground", "TextControlBackgroundPointerOver", "TextControlBackgroundFocused" })
        {
            field.Resources[key] = K.Clear;
        }
        foreach (var key in new[] { "TextControlBorderBrush", "TextControlBorderBrushPointerOver", "TextControlBorderBrushFocused" })
        {
            field.Resources[key] = K.Clear;
        }
        field.FontSize = 15;
        field.BorderThickness = new Thickness(0);
        field.Padding = new Thickness(0, 6, 0, 6);
        field.MinHeight = 0;
        field.VerticalAlignment = VerticalAlignment.Center;
    }

    /// <summary>
    /// Sealing a call (kit/Fields.kt <c>SealControl</c>): a switch, and while
    /// it is on the word everybody types the same, with what it does.
    /// </summary>
    public static StackPanel SealControl(bool on, Action<bool> onSealing, string word, Action<string> onWord)
    {
        var column = K.Flowing(10);
        var sealedNow = on && word.Length > 0;
        var lockIcon = K.Symbol(sealedNow ? "lock.fill" : "lock", 13, sealedNow ? Ink.Tally : Ink.Muted);
        var title = K.Text(sealedNow ? T("This call is sealed") : T("Seal this call"), 14, sealedNow ? K.Semibold : K.Regular, sealedNow ? Ink.Tally : Ink.Foreground);
        var wordBox = Fields.Secret(T("Sealing word"), word, _ => { }, height: 40);
        var details = K.Column(8, wordBox, K.Text(T("Everybody who types the same word can hear each other, and the server carrying the call cannot. Tell people the word some other way."), 11.5, ink: Ink.Muted));
        details.Arrives();
        details.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        void Say(bool switchOn, string typed)
        {
            var isSealed = switchOn && typed.Length > 0;
            if (lockIcon is FontIcon icon)
            {
                icon.Glyph = isSealed ? "" : "";
                icon.Foreground = isSealed ? Ink.Tally : Ink.Muted;
            }
            title.Text = isSealed ? T("This call is sealed") : T("Seal this call");
            title.Foreground = isSealed ? Ink.Tally : Ink.Foreground;
            title.FontWeight = isSealed ? K.Semibold : K.Regular;
        }
        var toggle = Fields.Switch(on, value =>
        {
            onSealing(value);
            // Turning it off also clears the word.
            if (!value) wordBox.Password = "";
            details.Visibility = value ? Visibility.Visible : Visibility.Collapsed;
            Say(value, wordBox.Password);
        }, T("Seal this call"));
        wordBox.PasswordChanged += (_, _) =>
        {
            onWord(wordBox.Password);
            Say(toggle.IsOn, wordBox.Password);
        };
        column.Children.Add(K.Line(10, K.Fixed(new Border { Width = 18, Child = lockIcon }), K.Flex(title), K.Fixed(toggle)));
        column.Children.Add(details);
        return column;
    }

    /// <summary>
    /// The relay a call goes through, as a capsule opening the list
    /// (MeetingSheet.kt <c>ServerMenu</c>): Automatic, and the relays under
    /// their regions with their round trips, measured on first sight.
    /// </summary>
    public static Tap ServerMenu(AppModel app, bool compact, string id, Action? changed = null)
    {
        var lobby = app.Lobby;
        if (lobby.Measured == null) _ = lobby.Measure();
        var row = K.Row(6);
        row.VerticalAlignment = VerticalAlignment.Center;
        var tap = new Tap(row, Look.Plain, compact ? 30 : 34, padding: compact ? 12 : 14);
        void Paint()
        {
            row.Children.Clear();
            if (!compact) row.Children.Add(K.Symbol("server.rack", 12));
            var choice = app.RelayChoice;
            row.Children.Add(K.Text(choice != null ? lobby.Label(choice) : T("Automatic"), 12.5, K.Medium, lines: 1));
            var ms = choice != null ? (lobby.Measured is { } m && m.TryGetValue(choice, out var c) ? c : null) : lobby.FastestRelay?.Millis;
            if (ms is { } millis) row.Children.Add(K.Text($"{(int)millis} ms", 12.5, K.Medium, Ink.Latency(millis), tabular: true));
            row.Children.Add(K.Symbol("chevron.up.chevron.down", 9, Ink.Muted));
            tap.Named(T("Server"));
        }
        Paint();
        lobby.Changed += Paint;
        tap.Unloaded += (_, _) => lobby.Changed -= Paint;
        tap.Click += (_, _) => Overlay.Current?.Toggle(id, tap, () =>
        {
            var fastest = lobby.FastestRelay;
            var items = new List<UIElement>
            {
                Menu.Item(T("Automatic"), () => { app.RelayChoice = null; Overlay.Current?.ClosePopover(); changed?.Invoke(); }, "wand.and.stars",
                    fastest != null ? $"{T("Whichever answers fastest")} · {lobby.Label(fastest.Name)}" : T("Whichever answers fastest"),
                    check: app.RelayChoice == null),
            };
            foreach (var group in lobby.Grouped())
            {
                if (group.Region.Length > 0) items.Add(Menu.Heading(group.Region));
                foreach (var relay in group.Relays)
                {
                    var named = Say(relay.Label ?? relay.Name) + (relay.AdminOnly ? " [Admin]" : "");
                    string? detail = relay.Maintenance ? T("Not taking calls") : lobby.Measured is { } measured
                        ? measured.TryGetValue(relay.Name, out var ms) && ms is { } millis ? $"{(int)millis} ms" : T("Timeout")
                        : null;
                    var name = relay.Name;
                    items.Add(Menu.Item(named, () => { app.RelayChoice = name; Overlay.Current?.ClosePopover(); changed?.Invoke(); }, detail: detail, check: app.RelayChoice == name, enabled: !relay.Maintenance));
                }
            }
            return Menu.Card(items.ToArray());
        }, width: 290);
        return tap;
    }

    /// <summary>
    /// The choices a join makes besides the name (MeetingSheet.kt
    /// <c>OptionsCard</c>): the relay, where there is a choice, and sealing.
    /// </summary>
    public static Border OptionsCard(AppModel app, bool sealing, Action<bool> onSealing, string serverId, bool offerServer = true)
    {
        var column = K.Column(0);
        if (offerServer && app.OffersRelayChoice)
        {
            var server = K.Line(12,
                K.Fixed(new Border { Width = 18, Child = K.Symbol("server.rack", 13, Ink.Muted) }),
                K.Flex(K.Text(T("Server"), 14).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
                K.Fixed(ServerMenu(app, compact: true, id: serverId)));
            server.MinHeight = 48;
            server.Padding = new Thickness(14, 0, 14, 0);
            column.Children.Add(server);
            column.Children.Add(K.Rule(0));
        }
        var seal = SealControl(sealing, onSealing, app.SealingWord, word => app.SealingWord = word);
        seal.Margin = new Thickness(14, 12, 14, 12);
        column.Children.Add(seal);
        return new Border
        {
            Child = column,
            CornerRadius = new CornerRadius(18),
            Background = Ink.Black(0.22f),
            BorderBrush = Ink.White(0.08f),
            BorderThickness = new Thickness(1),
        };
    }

    /// <summary>The fleet's light (Lobby.tsx <c>Fleet</c>): how many relays are answering, as a dot and a count.</summary>
    public static StackPanel? FleetLine(AppModel app)
    {
        if (app.Relays is not { Total: > 0 } relays) return null;
        var colour = relays.Online == 0 ? Ink.Danger : relays.Online == relays.Total ? Ink.Good : Ink.Tally;
        var row = K.Row(6, K.Dot(6, colour), K.Text($"{relays.Online}/{relays.Total}", 11, ink: Ink.Muted, tabular: true), K.Text(T("Relays answering"), 11, ink: Ink.Muted));
        foreach (var child in row.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        return row;
    }
}
