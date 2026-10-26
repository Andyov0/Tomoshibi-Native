using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// How loud everybody else is (components/room/SoundPanel.tsx; Android's
/// ui/call/SoundPanel.kt).
///
/// A list of what can be heard rather than of what can be seen: the person
/// who is too loud may be on no tile at all. One row per thing there is to
/// hear, because a shared screen's sound and the voice describing it arrive
/// on separate tracks and are turned down separately; nobody hears their
/// own microphone, so there is no row for it. Each person's rows share a
/// card, so that whose sound a slider is never has to be read twice.
/// </summary>
internal sealed class SoundPanel : Grid
{
    private readonly HostFeature _feature;
    private readonly StackPanel _people = K.Flowing(8);
    private readonly Grid _nobody;
    private readonly Dictionary<string, PersonSounds> _cards = new();

    public SoundPanel(HostFeature feature, Action close)
    {
        _feature = feature;
        _nobody = new Grid { Padding = new Thickness(0, 26, 0, 26) };
        var empty = K.Column(10, K.Symbol("speaker.wave.2", 22, Ink.ForegroundAt(0.4f)), K.Text(T("Nobody else is here."), 12, ink: Ink.Muted, align: TextAlignment.Center));
        empty.HorizontalAlignment = HorizontalAlignment.Center;
        _nobody.Children.Add(empty);
        _nobody.Arrives();
        var body = K.Column(8, _nobody, _people);
        body.Padding = new Thickness(10);
        var column = K.Column(0, Panels.Heading(T("Sound"), "speaker.wave.2.fill", T("Close sound"), close), Panels.Rule(), body);
        Children.Add(K.Glass(new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto }, 22));
        Hearing.Shared.Changed += Update;
        Unloaded += (_, _) => Hearing.Shared.Changed -= Update;
        Update();
    }

    public void Update()
    {
        var others = _feature.Call.People;
        _nobody.Visibility = others.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        var ids = others.Select(p => p.Id).ToHashSet();
        foreach (var gone in _cards.Keys.Where(id => !ids.Contains(id)).ToList())
        {
            _people.Children.Remove(_cards[gone]);
            _cards.Remove(gone);
        }
        foreach (var person in others)
        {
            if (!_cards.TryGetValue(person.Id, out var card))
            {
                card = new PersonSounds(_feature);
                card.Arrives();
                _cards[person.Id] = card;
                _people.Children.Add(card);
            }
            card.Show(person);
        }
    }

    /// <summary>Everything one person can be heard through.</summary>
    private sealed class PersonSounds : Grid
    {
        private readonly HostFeature _feature;
        private readonly Grid _monogram = new();
        private readonly TextBlock _name = K.Text("", 13, K.Semibold, lines: 1);
        private readonly Border _off;
        private readonly SoundRow _voice, _screen, _music;
        private string? _identity;

        public PersonSounds(HostFeature feature)
        {
            _feature = feature;
            _off = new Border
            {
                Background = Ink.Wash(0.07f),
                CornerRadius = new CornerRadius(9),
                Padding = new Thickness(7, 2, 7, 2),
                VerticalAlignment = VerticalAlignment.Center,
                Child = K.Text(T("microphone off"), 10.5, K.Medium, Ink.Muted, lines: 1),
            };
            _off.Arrives(0.9f);
            var head = K.Line(9, K.Fixed(_monogram), K.Fixed(_name.At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(_off), K.Flex(K.Spacer()));
            _voice = new SoundRow(feature, Sound.Voice);
            _screen = new SoundRow(feature, Sound.Screen);
            _music = new SoundRow(feature, Sound.Music);
            _screen.Arrives();
            _music.Arrives();
            var column = K.Flowing(8);
            column.Children.Add(head);
            column.Children.Add(_voice);
            column.Children.Add(_screen);
            column.Children.Add(_music);
            Children.Add(new Border
            {
                Child = column,
                CornerRadius = new CornerRadius(16),
                Background = Ink.Wash(0.05f),
                BorderBrush = Ink.Wash(0.07f),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(12, 10, 12, 8),
            });
        }

        public void Show(Person person)
        {
            var name = person.Shown;
            if (_identity != person.Id + name)
            {
                _identity = person.Id + name;
                _monogram.Children.Clear();
                _monogram.Children.Add(Faces.Monogram(name, person.Id, 26));
            }
            _name.Text = name;
            // `isMicrophoneEnabled`: there and not muted. Said rather than
            // drawn as a state of the slider, which is about what the
            // listener chose.
            var on = person.Track(MediaSource.Microphone) is { Muted: false };
            _off.Visibility = on ? Visibility.Collapsed : Visibility.Visible;
            _voice.Show(person, name, null);
            // A screen's row only while it is shared with sound; the song
            // desk's music, a track of its own, the same.
            _screen.Visibility = person.Track(MediaSource.ScreenShareAudio) != null ? Visibility.Visible : Visibility.Collapsed;
            _screen.Show(person, T("{name} (screen)", ("name", name)), "rectangle.on.rectangle");
            _music.Visibility = person.MusicTrack != null ? Visibility.Visible : Visibility.Collapsed;
            _music.Show(person, T("{name} (music)", ("name", name)), "music.note");
        }
    }

    /// <summary>
    /// One sound's mute and volume (SoundPanel.tsx <c>SoundRow</c>): named
    /// above the slider when it is not the voice, which the card's own
    /// heading already names. The slider is turned off rather than hidden
    /// while the sound is blocked: nothing is arriving to be made louder, and
    /// a slider that moves while the sound stays off is a control that lies.
    /// </summary>
    internal sealed class SoundRow : StackPanel
    {
        private readonly HostFeature _feature;
        private readonly Sound _sound;
        private readonly StackPanel _label;
        private readonly FrameworkElement _labelSymbolHost;
        private readonly TextBlock _labelText = K.Text("", 11.5, K.Medium, Ink.Muted, lines: 1);
        private readonly Tap _mute;
        private readonly Slider _slider;
        private readonly TextBlock _amount = K.Text("", 11, K.Medium, Ink.Muted, lines: 1, align: TextAlignment.Right, tabular: true);
        private Person? _person;
        private bool _setting;

        public SoundRow(HostFeature feature, Sound sound)
        {
            _feature = feature;
            _sound = sound;
            Spacing = 4;
            _labelSymbolHost = new Border();
            _label = K.Row(6, _labelSymbolHost, _labelText);
            _label.Margin = new Thickness(2, 0, 0, 0);
            _mute = new Tap(K.Symbol("speaker.wave.2.fill", 11.5, Ink.Muted), Look.Clear, 28, circle: true);
            _mute.Click += (_, _) =>
            {
                if (_person is not { } person) return;
                _feature.SetBlocked(person, _sound, !HostFeature.Setting(person, _sound).Blocked);
            };
            _slider = GlassSlider();
            _slider.ValueChanged += (_, e) =>
            {
                if (_setting || _person is not { } person) return;
                _feature.SetVolume(person, _sound, e.NewValue);
            };
            _amount.MinWidth = 42;
            var line = K.Line(10, K.Fixed(_mute), K.Flex(_slider), K.Fixed(_amount.At(HorizontalAlignment.Right, VerticalAlignment.Center)));
            Children.Add(_label);
            Children.Add(line);
        }

        /// <summary>
        /// The other apps' slider (Notices.kt <c>GlassSlider</c>) on a
        /// Windows one, which keeps the keyboard and the screen reader: a
        /// dark track with a hairline, the part below the value in white,
        /// a white thumb, in twentieths. Left as it
        /// comes, the system's slider is drawn in the accent colour, which
        /// here is the tally, the colour of things that are on.
        /// </summary>
        private static Slider GlassSlider()
        {
            var slider = new Slider
            {
                Minimum = 0,
                Maximum = 1,
                StepFrequency = 0.05,
                SmallChange = 0.05,
                LargeChange = 0.25,
                IsThumbToolTipEnabled = false,
                VerticalAlignment = VerticalAlignment.Center,
            };
            var track = Ink.Well(0.28f);
            var fill = Ink.ForegroundAt(0.9f);
            var thumb = Ink.Brush(Shade.White);
            foreach (var state in new[] { "", "PointerOver", "Pressed", "Disabled" })
            {
                slider.Resources["SliderTrackFill" + state] = track;
                slider.Resources["SliderTrackValueFill" + state] = fill;
                slider.Resources["SliderThumbBackground" + state] = thumb;
                slider.Resources["SliderOuterThumbBackground" + state] = thumb;
            }
            return slider;
        }

        public void Show(Person person, string name, string? symbol)
        {
            _person = person;
            var setting = HostFeature.Setting(person, _sound);
            _label.Visibility = symbol == null ? Visibility.Collapsed : Visibility.Visible;
            var ink = setting.Silenced ? Ink.MutedAt(0.7f) : Ink.Muted;
            if (symbol != null)
            {
                ((Border)_labelSymbolHost).Child = K.Symbol(symbol, 10, ink);
                _labelText.Text = name;
                _labelText.Foreground = ink;
            }
            // Named for what pressing it does, not for the state it is in:
            // the two read as opposites, and only one is being decided.
            var label = setting.Blocked ? T("Unmute {name}", ("name", name)) : T("Mute {name}", ("name", name));
            _mute.Content = K.Symbol(setting.Blocked ? "speaker.slash.fill" : "speaker.wave.2.fill", 11.5, setting.Blocked ? Ink.Foreground : Ink.Muted);
            _mute.Resting = Ink.WashShade(setting.Blocked ? 0.16f : 0.06f);
            _mute.Named(label, "sound." + _sound.Key() + "." + person.Id);
            _mute.Tip(label);
            _slider.IsEnabled = !setting.Blocked;
            _slider.Named(T("{name}'s volume", ("name", name)));
            if (Math.Abs(_slider.Value - setting.Volume) > 0.001)
            {
                _setting = true;
                _slider.Value = setting.Volume;
                _setting = false;
            }
            // Read at a glance, because a slider near one end tells nobody
            // whether it is at nought.
            _amount.Text = setting.Blocked ? T("Muted") : $"{Math.Round(setting.Volume * 100)}%";
        }
    }
}
