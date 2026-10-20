using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// The devices in a call (Android's ui/settings/DevicesPanel.kt): which
/// microphone, speakers and camera, the voice as it is, and the language,
/// the only settings a call has.
///
/// Each list ticks what the call is using now and follows the devices: a
/// headset plugged in while the panel is open arrives in it, one pulled out
/// leaves, and the tick moves with a call that moved off a device that went
/// away, as the web's menu listens for <c>devicechange</c> (DeviceMenu.tsx).
/// A device picked moves the tick at once and back again if it did not take,
/// and is remembered once it did. Speakers picked here take effect in the
/// call at once: its sound is played through an audio graph opened again on
/// them, every sound moved across before the old one closes
/// (Playback.UseOutput).
/// </summary>
internal sealed class DevicesPanel : Grid
{
    private readonly AppModel _app;
    private readonly CallModel _call;
    private readonly StackPanel _sections = K.Column(18);
    private readonly DeviceRows _microphones, _speakers, _cameras;
    private readonly StackPanel _microphoneMeter = K.Column(0);
    private MicrophoneMeterRow? _meter;

    public DevicesPanel(AppModel app, CallModel call, Action close)
    {
        _app = app;
        _call = call;
        var head = K.Line(10, K.Flex(K.Text(T("Devices"), 15, K.Semibold).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(CloseButton(close)));
        _sections.Children.Add(head);
        _sections.Padding = new Thickness(16);
        Children.Add(K.Glass(new ScrollViewer { Content = _sections, VerticalScrollBarVisibility = ScrollBarVisibility.Auto }, 22));

        _microphones = new DeviceRows("mic", "devices.microphone", async id => await _call.UseMicrophone(id));
        var original = Fields.Switch(_call.OriginalSound, async on => await _call.SetOriginalSound(on), T("Original sound"));
        var microphoneCard = K.Column(0, _microphones, _microphoneMeter, Rows.Row(T("Original sound"), original, T("No noise suppression or automatic volume. For headphones and music."), "waveform.path", last: true));
        _sections.Children.Add(Rows.Section(T("Microphone"), K.Card(microphoneCard, 16)));

        _speakers = new DeviceRows("speaker.wave.2", "devices.speakers", async id =>
        {
            // Remembered by the call once it took; a device that would not
            // open puts the tick back and leaves the sound where it was.
            if (!await _call.UseSpeakers(id)) throw new InvalidOperationException("the speakers picked would not open");
        }, last: true);
        _sections.Children.Add(Rows.Section(T("Speakers"), K.Card(_speakers, 16)));

        // Every camera has the hairline under it: the blur's row ends the card.
        _cameras = new DeviceRows("video", "devices.camera", async id => await _call.UseCamera(id));
        // The call's own blur, from the next frame, in what is sent and in
        // the picture of yourself (Android's DevicesPanel). Beside the list
        // rather than in it, since the list reorders its own rows as devices
        // come and go.
        var blur = Fields.Switch(_app.Blurring, _app.SetBlur, T("Blur my background"));
        var cameraCard = K.Column(0, _cameras, Rows.Row(T("Blur my background"), blur, symbol: "person.and.background.dotted", last: true));
        _sections.Children.Add(Rows.Section(T("Camera"), K.Card(cameraCard, 16)));

        // The language, here as well as in the settings, since a call has no settings of its own.
        var languages = K.Column(0);
        var words = Localization.Shared;
        foreach (var language in Languages.All)
        {
            var chosen = language;
            var row = DeviceRows.ChoiceRow(language.DisplayName(), null, words.Language == language, last: language == Languages.All[^1]);
            row.Click += (_, _) => words.Choose(chosen);
            languages.Children.Add(row);
            if (language != Languages.All[^1]) languages.Children.Add(K.Rule(14));
        }
        _sections.Children.Add(Rows.Section(T("Language"), K.Card(languages, 16)));

        Loaded += (_, _) =>
        {
            _app.Media.DevicesChanged += Refresh;
            _call.Changed += Meter;
            Refresh();
        };
        Unloaded += (_, _) =>
        {
            _app.Media.DevicesChanged -= Refresh;
            _call.Changed -= Meter;
        };
    }

    public static Tap CloseButton(Action close)
    {
        var tap = Tap.Icon("xmark", T("Close"), Look.Plain, 26, 10.5);
        tap.Click += (_, _) => close();
        return tap;
    }

    /// <summary>The meter shows while the microphone is on, arriving and leaving with it.</summary>
    private void Meter()
    {
        if (_call.MicrophoneOn && _meter == null)
        {
            _meter = new MicrophoneMeterRow(() => _call.MicrophoneLevel).Arrives(1f, fromY: -6);
            _microphoneMeter.Children.Add(_meter);
        }
        else if (!_call.MicrophoneOn && _meter != null)
        {
            _microphoneMeter.Children.Remove(_meter);
            _meter = null;
        }
    }

    private int _refreshing;

    /// <summary>The lists read again, and the ticks put on what the call uses now.</summary>
    private async void Refresh()
    {
        var mine = ++_refreshing;
        var media = _app.Media;
        var microphones = await DeviceList.Microphones();
        var speakers = await DeviceList.Speakers();
        var cameras = await DeviceList.Cameras();
        var microphone = await media.Current(DeviceKind.Microphone);
        var speaker = await media.Current(DeviceKind.Speakers, _call.SpeakersInUse);
        var camera = await media.Current(DeviceKind.Camera);
        // A later refresh overtook this one while it listed.
        if (mine != _refreshing) return;
        _microphones.Show(microphones, microphone);
        _speakers.Show(speakers, speaker);
        _cameras.Show(cameras, camera);
        Meter();
    }
}

/// <summary>
/// One kind's devices as a list of choices (kit/Choosing.kt
/// <c>ChoiceRow</c>), following the devices: a row arrives and leaves with
/// its device, the others keep their place; an empty list says so.
/// </summary>
internal sealed class DeviceRows : StackPanel
{
    private readonly string _symbol;
    private readonly string _automation;
    private readonly Func<string, Task> _pick;
    /// <summary>Each device's row, in a column with the hairline under it.</summary>
    private readonly Dictionary<string, (StackPanel Item, Tap Row, string Name, Border Rule)> _rows = new();
    private readonly bool _last;
    private FrameworkElement? _empty;
    private string? _chosen;
    private bool _shown;

    public DeviceRows(string symbol, string automation, Func<string, Task> pick, bool last = false)
    {
        _symbol = symbol;
        _automation = automation;
        _pick = pick;
        _last = last;
    }

    public void Show(IReadOnlyList<DeviceOption> devices, string? current)
    {
        if (!_pressing) _chosen = current;
        // Nothing animates the first time: the panel arrives as one piece.
        var arrive = _shown;
        _shown = true;
        foreach (var gone in _rows.Keys.Where(id => devices.All(d => d.Id != id)).ToList())
        {
            Children.Remove(_rows[gone].Item);
            _rows.Remove(gone);
        }
        if (devices.Count == 0 && _empty == null)
        {
            _empty = K.Text(T("No devices found"), 13, ink: Ink.Muted).Pad(14, 12, 14, 12);
            if (arrive) _empty.Arrives(1f);
            Children.Add(_empty);
        }
        else if (devices.Count > 0 && _empty != null)
        {
            Children.Remove(_empty);
            _empty = null;
        }
        for (var i = 0; i < devices.Count; i++)
        {
            var device = devices[i];
            if (!_rows.TryGetValue(device.Id, out var held) || held.Name != device.Name)
            {
                if (held.Item != null) Children.Remove(held.Item);
                var id = device.Id;
                var row = ChoiceRow(device.Name, _symbol, false, last: false);
                row.Named(device.Name, $"{_automation}.{i}");
                row.Click += async (_, _) => await Press(id);
                // The hairline starts under the words, as Android's rows' do.
                var rule = K.Rule(52);
                var item = K.Column(0, row, rule);
                if (arrive) item.Arrives(1f, fromY: -6);
                _rows[id] = (item, row, device.Name, rule);
                held = _rows[id];
            }
            // The last of a card that ends with the list has no hairline under it.
            held.Rule.Visibility = _last && i == devices.Count - 1 ? Visibility.Collapsed : Visibility.Visible;
            // In the order listed: a device plugged in takes its place
            // among the others, which move down rather than go and come back.
            var at = Children.IndexOf(held.Item);
            if (at == i) continue;
            if (at >= 0) Children.RemoveAt(at);
            Children.Insert(Math.Min(i, Children.Count), held.Item);
        }
        Tick();
    }

    private bool _pressing;

    /// <summary>The tick moves at once and back if the device did not take (DeviceMenu.tsx <c>select</c>).</summary>
    private async Task Press(string id)
    {
        var was = _chosen;
        _chosen = id;
        _pressing = true;
        Tick();
        try
        {
            await _pick(id);
        }
        catch (Exception)
        {
            _chosen = was;
            Tick();
        }
        finally
        {
            _pressing = false;
        }
    }

    private void Tick()
    {
        foreach (var (id, (_, row, name, _)) in _rows) row.Content = RowContent(name, _symbol, id == _chosen);
    }

    /// <summary>One choice in a list (kit/Choosing.kt <c>ChoiceRow</c>): its symbol lit and a tick when chosen.</summary>
    public static Tap ChoiceRow(string title, string? symbol, bool chosen, bool last)
    {
        var tap = new Tap(RowContent(title, symbol, chosen), Look.Clear, 40, padding: 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(0),
        };
        tap.Named(title);
        return tap;
    }

    private static Grid RowContent(string title, string? symbol, bool chosen)
    {
        var parts = new List<(UIElement, double)>();
        if (symbol != null) parts.Add(K.Fixed(new Border { Width = 26, Child = K.Symbol(symbol, 12.5, chosen ? Ink.Tally : Ink.ForegroundAt(0.75f)) }));
        var text = K.Text(title, 13.5, chosen ? K.Medium : K.Regular, lines: 1);
        text.TextTrimming = TextTrimming.CharacterEllipsis;
        parts.Add(K.Flex(text.At(HorizontalAlignment.Left, VerticalAlignment.Center)));
        var tick = K.Symbol("checkmark", 11.5, Ink.Tally);
        tick.Opacity = chosen ? 1 : 0;
        parts.Add(K.Fixed(tick));
        return K.Line(12, parts.ToArray());
    }
}
