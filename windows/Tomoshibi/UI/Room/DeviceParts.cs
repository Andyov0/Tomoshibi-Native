using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// A level as a row of small bars lit from the left (kit/Settings.kt
/// <c>LevelMeter</c>): the tally where lit, a faint white where not, each
/// bar's colour easing to the next reading.
/// </summary>
internal sealed class LevelMeter : StackPanel
{
    private readonly List<Border> _segments = new();
    private int _lit = -1;

    public LevelMeter(int segments = 14)
    {
        Orientation = Orientation.Horizontal;
        Spacing = 2.5;
        VerticalAlignment = VerticalAlignment.Center;
        for (var i = 0; i < segments; i++)
        {
            var segment = new Border
            {
                Width = 4,
                Height = 14,
                CornerRadius = new CornerRadius(2),
                Background = Ink.White(0.12f),
                BackgroundTransition = new BrushTransition { Duration = TimeSpan.FromMilliseconds(120) },
            };
            _segments.Add(segment);
            Children.Add(segment);
        }
        this.Quiet();
    }

    /// <summary>How full, 0 to 1.</summary>
    public void Show(double level)
    {
        var lit = 0;
        for (var i = 0; i < _segments.Count; i++) if ((double)i / _segments.Count < level) lit = i + 1;
        if (lit == _lit) return;
        _lit = lit;
        for (var i = 0; i < _segments.Count; i++) _segments[i].Background = i < lit ? Ink.Tally : Ink.White(0.12f);
    }
}

/// <summary>
/// How loud the microphone is as it goes, and what that means in words
/// (DevicesPanel.kt <c>MicrophoneMeterRow</c>, MicLevel.tsx): "Say
/// something" until anything has been heard, "Hearing you" once it has, "No
/// sound from it" when the microphone hands over nothing at all for two
/// seconds, which a meter that never moves cannot tell from silence. The
/// question somebody opening the devices is asking is "is it this one", and
/// a meter that moves when they speak answers it.
/// </summary>
internal sealed class MicrophoneMeterRow : StackPanel
{
    private readonly Func<double> _rms;
    private readonly LevelMeter _meter = new(18);
    private readonly Grid _words = new();
    private readonly DispatcherQueueTimer _timer;
    private DateTime _opened;
    private bool _anything, _heard;
    private string _said = "";

    public MicrophoneMeterRow(Func<double> rms)
    {
        _rms = rms;
        var symbol = new Border { Width = 26, Child = K.Symbol("waveform", 12.5, Ink.Muted) };
        var meter = new Border { Child = _meter }.Named(T("Microphone level"));
        var line = K.Line(12, K.Fixed(symbol), K.Fixed(meter), K.Flex(new Border { MinWidth = 8 }), K.Fixed(_words));
        line.MinHeight = 40;
        line.Padding = new Thickness(14, 0, 14, 0);
        Children.Add(line);
        Children.Add(K.Rule(52));
        _timer = DispatcherQueue.GetForCurrentThread().CreateTimer();
        // About thirty times a second, as the Swift's timeline reads it.
        _timer.Interval = TimeSpan.FromMilliseconds(33);
        _timer.Tick += (_, _) => Read();
        Loaded += (_, _) =>
        {
            _opened = DateTime.UtcNow;
            _anything = _heard = false;
            Read();
            _timer.Start();
        };
        Unloaded += (_, _) => _timer.Stop();
    }

    private void Read()
    {
        var rms = _rms();
        if (rms > 0) _anything = true;
        var shown = VoiceCapture.Shown(rms);
        _meter.Show(shown);
        if (shown > VoiceCapture.Heard) _heard = true;
        var silent = !_anything && DateTime.UtcNow - _opened > TimeSpan.FromSeconds(2);
        var said = silent ? T("No sound from it") : _heard ? T("Hearing you") : T("Say something");
        if (said == _said) return;
        _said = said;
        // The words cross-fade: the old ones leave on the leave curve as the
        // new arrive on the settle curve (Crossfade in the Compose panel).
        var words = K.Text(said, 11.5, ink: _heard && !silent ? Ink.Foreground : Ink.Muted, lines: 1).At(HorizontalAlignment.Right, VerticalAlignment.Center).Arrives(1f);
        _words.Children.Clear();
        _words.Children.Add(words);
    }
}
