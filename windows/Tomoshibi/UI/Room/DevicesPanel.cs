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
/// the only settings a call has. A device picked moves the tick at once and
/// back again if it did not take, and is remembered once it did.
/// Speakers picked here take effect in the call at once: its sound is
/// played through an audio graph opened again on them, every sound moved
/// across before the old one closes (Playback.UseOutput).
/// </summary>
internal sealed class DevicesPanel : Grid
{
    private readonly AppModel _app;
    private readonly CallModel _call;
    private readonly StackPanel _sections = K.Column(18);

    public DevicesPanel(AppModel app, CallModel call, Action close)
    {
        _app = app;
        _call = call;
        var head = K.Line(10, K.Flex(K.Text(T("Devices"), 15, K.Semibold).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(CloseButton(close)));
        _sections.Children.Add(head);
        _sections.Padding = new Thickness(16);
        Children.Add(K.Glass(new ScrollViewer { Content = _sections, VerticalScrollBarVisibility = ScrollBarVisibility.Auto }, 22));
        _ = Fill();
    }

    public static Tap CloseButton(Action close)
    {
        var tap = Tap.Icon("xmark", T("Close"), Look.Plain, 26, 10.5);
        tap.Click += (_, _) => close();
        return tap;
    }

    private async Task Fill()
    {
        var microphones = await DeviceList.Microphones();
        var speakers = await DeviceList.Speakers();
        var cameras = await DeviceList.Cameras();
        var memory = _app.DeviceMemory;

        var microphoneRows = Choices(microphones, "mic", memory.Choice(DeviceMemory.Kind.Microphone), async id => await _call.UseMicrophone(id));
        var original = Fields.Switch(_call.OriginalSound, async on => await _call.SetOriginalSound(on), T("Original sound"));
        microphoneRows.Children.Add(Rows.Row(T("Original sound"), original, T("No noise suppression or automatic volume. For headphones and music."), "waveform.path", last: true));
        _sections.Children.Add(Rows.Section(T("Microphone"), K.Card(microphoneRows, 16)));

        var speakerRows = Choices(speakers, "speaker.wave.2", memory.Choice(DeviceMemory.Kind.Speakers), async id =>
        {
            // Remembered by the call once it took; a device that would not
            // open puts the tick back and leaves the sound where it was.
            if (!await _call.UseSpeakers(id)) throw new InvalidOperationException("the speakers picked would not open");
        });
        _sections.Children.Add(Rows.Section(T("Speakers"), K.Card(speakerRows, 16)));

        var cameraRows = Choices(cameras, "video", memory.Choice(DeviceMemory.Kind.Camera), async id => await _call.UseCamera(id));
        _sections.Children.Add(Rows.Section(T("Camera"), K.Card(cameraRows, 16)));

        // The language, here as well as in the settings, since a call has no settings of its own.
        var languages = K.Column(0);
        var words = Localization.Shared;
        foreach (var language in Languages.All)
        {
            var chosen = language;
            languages.Children.Add(ChoiceRow(language.DisplayName(), null, words.Language == language, () => words.Choose(chosen), last: language == Languages.All[^1]));
        }
        _sections.Children.Add(Rows.Section(T("Language"), K.Card(languages, 16)));
    }

    /// <summary>A list of devices, one ticked; nothing found says so.</summary>
    private StackPanel Choices(IReadOnlyList<DeviceOption> devices, string symbol, string? current, Func<string?, Task> pick)
    {
        var column = K.Column(0);
        if (devices.Count == 0)
        {
            column.Children.Add(K.Text(T("No devices found"), 13, ink: Ink.Muted).Pad(14, 12, 14, 12));
            return column;
        }
        var chosen = devices.Any(d => d.Id == current) ? current : devices[0].Id;
        var rows = new List<Tap>();
        foreach (var device in devices)
        {
            var id = device.Id;
            Tap? row = null;
            row = ChoiceRow(device.Name, symbol, id == chosen, async () =>
            {
                var was = chosen;
                chosen = id;
                Tick(rows, devices, chosen, symbol);
                try
                {
                    await pick(id);
                }
                catch (Exception)
                {
                    // Did not take: the tick goes back where it was.
                    chosen = was;
                    Tick(rows, devices, chosen, symbol);
                }
            }, last: false);
            rows.Add(row);
            column.Children.Add(row);
        }
        return column;
    }

    private void Tick(List<Tap> rows, IReadOnlyList<DeviceOption> devices, string? chosen, string symbol)
    {
        for (var i = 0; i < rows.Count; i++) rows[i].Content = RowContent(devices[i].Name, symbol, devices[i].Id == chosen);
    }

    /// <summary>One choice in a list (kit/Choosing.kt <c>ChoiceRow</c>): its symbol lit and a tick when chosen.</summary>
    private static Tap ChoiceRow(string title, string? symbol, bool chosen, Action pick, bool last)
    {
        var tap = new Tap(RowContent(title, symbol, chosen), Look.Clear, 40, padding: 14)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(0),
        };
        tap.Named(title);
        tap.Click += (_, _) => pick();
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
