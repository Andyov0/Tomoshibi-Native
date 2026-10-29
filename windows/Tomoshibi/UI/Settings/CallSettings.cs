using System.Diagnostics;
using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.Graphics.Canvas.Geometry;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.Model.Music;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;
using MusicQuality = uniffi.tomoshibi_protocol.MusicQuality;

namespace Tomoshibi.UI.Settings;

// The pages about what a call fetches and plays (settings/CallSettings.kt,
// Settings.swift `WatchingSettings` and `MusicSettings`, the web's
// account/Account.tsx), each over a small drawing of the choice, so that it
// is seen as well as read. Windows had neither: how sharp a video watched
// together may get was "automatic" with no way to change it, since nothing
// called WatchCeilings.Remember, and the music's two choices were only in
// the call's music panel, out of reach before a call.
//
// Both are written where a call reads them, under the web's names, in the
// store the call reads (DeviceMemory's, which a rehearsal without a store of
// its own leaves empty, as it leaves the call's).

/// <summary>
/// How sharp a video watched together may get here (Account.tsx
/// <c>WatchSettings</c>): offered to everybody, whether or not they may start
/// a showing, since it is about what this screen fetches and everybody in a
/// room is a viewer. A player reads it as a video is opened, so a change
/// applies from the next video, which the footer says.
/// </summary>
internal static class WatchingSettings
{
    /// <summary>A ceiling as the settings name it (Pieces.kt <c>watchLabel</c>).</summary>
    public static string Label(WatchCeiling ceiling) => ceiling switch
    {
        WatchCeiling.P1080 => T("1080p"),
        WatchCeiling.P720 => T("720p"),
        _ => T("Automatic"),
    };

    public static StackPanel Page(AppModel app)
    {
        var defaults = app.DeviceMemory.Defaults;
        var chosen = WatchCeilings.Stored(defaults);
        var preview = new FilmPreview(chosen);
        var footer = K.Text(T("Where a video comes in several qualities, it is never fetched sharper than this here. It applies from the next video you open."), 11.5, ink: Ink.Muted).Pad(6, 0, 6, 0);
        footer.Arrives(1f);
        footer.Visibility = chosen == WatchCeiling.Auto ? Visibility.Collapsed : Visibility.Visible;
        var choice = new Segmented(WatchCeilings.All.Select(Label).ToList(), Array.IndexOf(WatchCeilings.All, chosen), 30);
        choice.Named(T("Picture quality"), "settings.watch.quality");
        choice.Chose += index =>
        {
            var ceiling = WatchCeilings.All[index];
            ceiling.Remember(defaults);
            preview.Show(ceiling);
            footer.Visibility = ceiling == WatchCeiling.Auto ? Visibility.Collapsed : Visibility.Visible;
        };
        var row = new AdaptiveRowPanel(Rows.Tile("sparkles.tv"), K.Text(T("Picture quality"), 13.5, lines: 1), choice);
        return K.Column(18, preview, CallSettingsPieces.Section(T("Watch together"), Rows.Card(row), footer));
    }
}

/// <summary>
/// How music sounds in a call (Account.tsx <c>MusicSettings</c>): lowering it
/// under speech is the listener's own choice, and somebody who may not open
/// the library still hears what another person plays, so it is offered to
/// everybody; the library's quality only to whoever may fetch from it. In a
/// call the song desk is told at once, so the change is heard now, as the
/// panel's own switch is.
/// </summary>
internal static class MusicSettings
{
    public static string Label(bool best) => best ? T("Highest") : T("Lossless");

    /// <summary>The library's quality as kept: lossless unless the highest was chosen (preferences.ts <c>meet-live.music-quality</c>).</summary>
    public static bool Best(AppModel app) => app.DeviceMemory.Defaults?.String(MusicKeys.Quality) == "best";

    public static StackPanel Page(AppModel app)
    {
        var defaults = app.DeviceMemory.Defaults;
        var library = app.Holds("music");
        var best = Best(app);
        var ducks = defaults?.String(MusicKeys.Duck) != "off";
        var preview = new DuckPreview(ducks);
        var duck = Fields.Switch(ducks, on =>
        {
            if (app.Call is { } call) SongDesk.Of(call).Ducks = on;
            else if (on) defaults?.Remove(MusicKeys.Duck);
            else defaults?.Set(MusicKeys.Duck, "off");
            preview.Show(on);
        }, T("Lower the music while anybody talks"));
        var rows = new List<UIElement> { Rows.Row(T("Lower the music while anybody talks"), duck, symbol: "speaker.wave.1.fill", last: !library) };
        var footer = K.Text(T("Hi-Res and masters are large, and take longer to start."), 11.5, ink: Ink.Muted).Pad(6, 0, 6, 0);
        footer.Arrives(1f);
        footer.Visibility = best && library ? Visibility.Visible : Visibility.Collapsed;
        if (library)
        {
            var quality = new Segmented(new List<string> { Label(false), Label(true) }, best ? 1 : 0, 30);
            quality.Named(T("Quality"), "settings.music.quality");
            quality.Chose += index =>
            {
                var highest = index == 1;
                if (app.Call is { } call) SongDesk.Of(call).Quality = highest ? MusicQuality.Best : MusicQuality.Lossless;
                else if (highest) defaults?.Set(MusicKeys.Quality, "best");
                else defaults?.Remove(MusicKeys.Quality);
                footer.Visibility = highest ? Visibility.Visible : Visibility.Collapsed;
            };
            rows.Add(new AdaptiveRowPanel(Rows.Tile("waveform.path"), K.Text(T("Quality"), 13.5, lines: 1), quality));
        }
        return K.Column(18, preview, CallSettingsPieces.Section(T("Music"), Rows.Card(rows.ToArray()), footer));
    }
}

internal static class CallSettingsPieces
{
    /// <summary>Rows.Section with a footer that comes and goes with the choice it explains.</summary>
    public static StackPanel Section(string title, UIElement card, FrameworkElement footer) =>
        K.Column(8, K.Text(title, 12, K.Semibold, Ink.Muted).Pad(6, 0, 0, 0), card, footer);

    /// <summary>
    /// A value moved to another over the settle curve (320 ms), for what is
    /// drawn by hand rather than by a composition animation: the film's
    /// softness, the music sinking under a voice.
    /// </summary>
    public sealed class Eased
    {
        private readonly Action _moved;
        private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _timer;
        private double _from, _to;
        private long _began;

        public Eased(double value, Action moved, Microsoft.UI.Dispatching.DispatcherQueue queue)
        {
            Value = _from = _to = value;
            _moved = moved;
            _timer = queue.CreateTimer();
            _timer.Interval = TimeSpan.FromMilliseconds(16);
            _timer.Tick += (_, _) => Step();
        }

        public double Value { get; private set; }

        public void To(double target)
        {
            if (target == _to) return;
            _from = Value;
            _to = target;
            _began = Stopwatch.GetTimestamp();
            _timer.Start();
        }

        public void Stop() => _timer.Stop();

        private void Step()
        {
            var t = Math.Min(1, Stopwatch.GetElapsedTime(_began).TotalMilliseconds / Theme.SettleMs);
            Value = _from + (_to - _from) * Settle(t);
            if (t >= 1) _timer.Stop();
            _moved();
        }

        /// <summary>The settle curve, a cubic Bézier through (0,0) and (1,1), at the share <paramref name="x"/> of its time.</summary>
        private static double Settle(double x)
        {
            var (x1, y1, x2, y2) = Theme.SettleCurve;
            static double At(double t, double a, double b) => 3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t;
            // The time along the curve whose x is the one asked, by halving.
            double lo = 0, hi = 1, s = x;
            for (var i = 0; i < 24; i++)
            {
                s = (lo + hi) / 2;
                if (At(s, x1, x2) < x) lo = s;
                else hi = s;
            }
            return At(s, y1, y2);
        }
    }
}

/// <summary>
/// A film on the backdrop, wearing the ceiling chosen for it: drawn softer
/// the lower the ceiling (CallSettings.kt <c>WatchPreview</c>), with the
/// ceiling's chip in its corner and a play mark in its middle. The still is
/// flat shapes, the call's grey with a lighter band along its foot and a
/// pale disc high on the right, since a blur of a flat ground shows nothing;
/// a picture, and so in the dark's greys in either appearance.
/// </summary>
internal sealed class FilmPreview : Grid
{
    private CanvasControl? _canvas;
    private readonly CallSettingsPieces.Eased _softness;
    private readonly Grid _corner = new() { HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Bottom, Margin = new Thickness(12) };

    public FilmPreview(WatchCeiling ceiling)
    {
        CornerRadius = new CornerRadius(18);
        BorderBrush = Ink.Wash(0.08f);
        BorderThickness = new Thickness(1);
        _softness = new CallSettingsPieces.Eased(Softness(ceiling), () => _canvas?.Invalidate(), DispatcherQueue);
        _canvas = new CanvasControl();
        _canvas.Draw += Draw;
        Children.Add(_canvas);
        var play = K.Glass(K.Symbol("play.fill", 20, Ink.Foreground), 27, shadow: false);
        play.Width = play.Height = 54;
        play.HorizontalAlignment = HorizontalAlignment.Center;
        play.VerticalAlignment = VerticalAlignment.Center;
        Children.Add(play);
        Children.Add(_corner);
        _corner.Children.Add(Chip(ceiling));
        // As wide as the page, 2.6 times as wide as it is tall.
        SizeChanged += (_, e) =>
        {
            var tall = Math.Round(e.NewSize.Width / 2.6);
            if (Math.Abs(Height - tall) > 0.5 || double.IsNaN(Height)) Height = tall;
        };
        // A Win2D control holds its device until it is told it is gone.
        Unloaded += (_, _) =>
        {
            _softness.Stop();
            _canvas?.RemoveFromVisualTree();
            _canvas = null;
        };
        this.Quiet();
    }

    /// <summary>How soft the film is drawn under each ceiling, in points of blur (CallSettings.kt).</summary>
    private static double Softness(WatchCeiling ceiling) => ceiling switch
    {
        WatchCeiling.P720 => 6,
        WatchCeiling.P1080 => 2.5,
        _ => 0,
    };

    public void Show(WatchCeiling ceiling)
    {
        _softness.To(Softness(ceiling));
        // The chip changes where it stands: the one shown leaves, the new one comes.
        _corner.Children.Clear();
        _corner.Children.Add(Chip(ceiling));
    }

    private static Border Chip(WatchCeiling ceiling)
    {
        var lit = ceiling != WatchCeiling.Auto;
        var ink = lit ? Ink.TallyForeground : Ink.OnPicture;
        var chip = new Border
        {
            Child = K.Row(5, K.Symbol("sparkles.tv", 11, ink), K.Text(WatchingSettings.Label(ceiling), 11.5, K.Semibold, ink, lines: 1)),
            Background = lit ? Ink.Tally : Ink.Black(0.45f),
            BorderBrush = Ink.White(0.12f),
            BorderThickness = new Thickness(lit ? 0 : 0.5),
            CornerRadius = new CornerRadius(11),
            Padding = new Thickness(8, 3, 9, 3),
        };
        chip.Arrives();
        return chip;
    }

    private void Draw(CanvasControl sender, CanvasDrawEventArgs args)
    {
        var width = (float)sender.ActualWidth;
        var height = (float)sender.ActualHeight;
        if (width <= 0 || height <= 0) return;
        var still = Theme.Dark;
        using var film = new CanvasCommandList(sender);
        using (var pen = film.CreateDrawingSession())
        {
            pen.FillRectangle(0, 0, width, height, Ink.Of(still.Surface));
            pen.FillRectangle(0, height * 0.6f, width, height * 0.4f, Ink.Of(still.SurfaceHigh));
            pen.FillCircle(new Vector2(width * 0.75f, height * 0.35f), height * 0.14f, Ink.Of(still.Foreground.WithAlpha(0.18f)));
        }
        var soft = (float)_softness.Value;
        using var cut = CanvasGeometry.CreateRoundedRectangle(sender, 0, 0, width, height, 17, 17);
        using (args.DrawingSession.CreateLayer(1, cut))
        {
            if (soft < 0.05f)
            {
                args.DrawingSession.DrawImage(film);
                return;
            }
            // Blurred within its own edges, as Compose's Rectangle edge
            // treatment keeps them: no dark rim bleeding in from outside.
            using var blur = new GaussianBlurEffect { Source = film, BlurAmount = soft * 0.6f, BorderMode = EffectBorderMode.Hard, Optimization = EffectOptimization.Balanced };
            args.DrawingSession.DrawImage(blur);
        }
    }
}

/// <summary>
/// The music as a row of bars and a voice under it (CallSettings.kt
/// <c>DuckPreview</c>): where somebody speaks the music sinks, if it is asked
/// to (duck.ts), and stays put if not.
/// </summary>
internal sealed class DuckPreview : Grid
{
    private const int Bars = 44;
    private readonly CallSettingsPieces.Eased _sunk;
    private readonly List<CanvasControl> _canvases = new();

    public DuckPreview(bool ducks)
    {
        Background = Ink.Well(0.22f);
        BorderBrush = Ink.Wash(0.08f);
        BorderThickness = new Thickness(1);
        CornerRadius = new CornerRadius(18);
        Padding = new Thickness(16);
        _sunk = new CallSettingsPieces.Eased(ducks ? 1 : 0, () =>
        {
            foreach (var canvas in _canvases) canvas.Invalidate();
        }, DispatcherQueue);
        var tally = Ink.TallyShade;
        var voice = Ink.Palette.Foreground.WithAlpha(0.85f);
        var still = Ink.WashShade(0.18f);
        var music = Row("music.note", index =>
        {
            var at = 0.45 + 0.4 * Math.Abs(Math.Sin(index * 0.55)) + 0.15 * Math.Abs(Math.Sin(index * 1.7));
            return Speaking(index) ? at * (1 - 0.68 * _sunk.Value) : at;
        }, _ => tally);
        var talk = Row("person.wave.2.fill", index => Speaking(index) ? 0.3 + 0.6 * Math.Abs(Math.Sin(index * 1.3)) : 0.05, index => Speaking(index) ? voice : still);
        Children.Add(K.Column(10, music, talk));
        Unloaded += (_, _) =>
        {
            _sunk.Stop();
            foreach (var canvas in _canvases) canvas.RemoveFromVisualTree();
            _canvases.Clear();
        };
        this.Quiet();
    }

    public void Show(bool ducks) => _sunk.To(ducks ? 1 : 0);

    private static bool Speaking(int index) => index >= Bars * 2 / 5 && index < Bars * 4 / 5;

    private Grid Row(string symbol, Func<int, double> tall, Func<int, Shade> colour)
    {
        var canvas = new CanvasControl { Height = 36 };
        canvas.Draw += (sender, args) =>
        {
            var width = (float)sender.ActualWidth;
            var height = (float)sender.ActualHeight;
            if (width <= 0) return;
            const float gap = 3;
            var bar = (width - gap * (Bars - 1)) / Bars;
            for (var index = 0; index < Bars; index++)
            {
                var drawn = MathF.Max(3, 34 * (float)tall(index));
                args.DrawingSession.FillRoundedRectangle(index * (bar + gap), (height - drawn) / 2, bar, drawn, bar / 2, bar / 2, Ink.Of(colour(index)));
            }
        };
        _canvases.Add(canvas);
        return K.Line(12, K.Fixed(new Border { Width = 20, Child = K.Symbol(symbol, 13, Ink.Muted) }), K.Flex(canvas));
    }
}
