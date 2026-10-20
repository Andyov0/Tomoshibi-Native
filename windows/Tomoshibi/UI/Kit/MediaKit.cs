using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Tomoshibi.Basics;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The pieces the song desk and watching together are drawn with (Android's
/// ui/kit/Media.kt; Kit+Media.swift): the panel's glass and header, its small
/// round buttons, a row, artwork, a field and a slider, with the same colour
/// and curves as everything else.
/// </summary>
internal static class MediaKit
{
    /// <summary>
    /// About as tall as the web's panel (<c>h-[min(34rem,...)]</c>): the
    /// room lets a panel be as tall as it asks and scrolls it where the
    /// window is shorter, so the panel asks for a height of its own rather
    /// than one its lists would collapse to.
    /// </summary>
    public const double PanelHeight = 548;

    /// <summary>
    /// The glass a panel beside the call is drawn on, darkened from inside
    /// (<c>MediaPanelGlass</c>): these panels sit over the stage, and the
    /// stage is often a film, whose colours came straight through clear glass
    /// and left grey words on it unreadable.
    /// </summary>
    public static Border Glass(UIElement content, double height = PanelHeight)
    {
        var shade = new Border { Background = Ink.Brush(Theme.Background.WithAlpha(0.68f)), CornerRadius = new CornerRadius(22), Child = content };
        var glass = K.Glass(shade, 22);
        glass.Height = height;
        return glass;
    }

    /// <summary>The top of a panel beside the call: its symbol on the tally, its name, and the button that closes it (<c>MediaPanelHeader</c>).</summary>
    public static Grid Header(string title, string closeLabel, Action close, string? symbol = null)
    {
        var parts = new List<(UIElement, double)>();
        if (symbol != null)
        {
            parts.Add(K.Fixed(new Border
            {
                Width = 24,
                Height = 24,
                CornerRadius = new CornerRadius(7),
                Background = Ink.Tally,
                Child = K.Symbol(symbol, 11.5, Ink.TallyForeground),
            }));
        }
        parts.Add(K.Flex(K.Text(title, 14, K.Semibold, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)));
        var shut = IconButton("xmark", closeLabel, close);
        parts.Add(K.Fixed(shut));
        var line = K.Line(10, parts.ToArray());
        line.Padding = new Thickness(14, 10, 10, 10);
        return line;
    }

    /// <summary>A hairline between the parts of a panel (<c>MediaRule</c>).</summary>
    public static Border Rule() => new() { Height = 1, Background = Ink.White(0.07f) };

    /// <summary>
    /// A small round button that lights under the pointer: closing, taking a
    /// video off, playing one next (<c>MediaIconButton</c>). Filled with the
    /// tally for something that is on, or for the one thing to press.
    /// </summary>
    public static Tap IconButton(string symbol, string label, Action act, double size = 26, bool lit = false, bool prominent = false)
    {
        var filled = lit || prominent;
        var tap = new Tap(K.Symbol(symbol, size * 0.42, filled ? Ink.TallyForeground : Ink.Muted), filled ? Look.Prominent : Look.Clear, size, circle: true, label: label);
        if (!filled)
        {
            tap.PointerEntered += (_, _) => tap.Content = K.Symbol(symbol, size * 0.42, Ink.Foreground);
            tap.PointerExited += (_, _) => tap.Content = K.Symbol(symbol, size * 0.42, Ink.Muted);
        }
        tap.Named(label);
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>A button inside a panel: a capsule of light, the tally for the one thing the panel is for, or no fill at all for a quiet one (<c>MediaButton</c>).</summary>
    public static Tap Button(string words, string? symbol, Action act, bool prominent = false, bool quiet = false, double height = 30)
    {
        var ink = prominent ? Ink.TallyForeground : quiet ? Ink.Muted : Ink.Foreground;
        var row = K.Row(6);
        row.VerticalAlignment = VerticalAlignment.Center;
        if (symbol != null) row.Children.Add(K.Symbol(symbol, 12, ink));
        row.Children.Add(K.Text(words, 12.5, K.Semibold, ink, lines: 1));
        var tap = new Tap(row, prominent ? Look.Prominent : quiet ? Look.Clear : Look.Plain, height, padding: 12);
        tap.Named(words);
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>Something said quietly where there is nothing to show yet: a symbol in a soft circle, and a sentence (<c>MediaEmpty</c>).</summary>
    public static StackPanel Empty(string symbol, string text)
    {
        var circle = new Border
        {
            Width = 44,
            Height = 44,
            CornerRadius = new CornerRadius(22),
            Background = Ink.White(0.07f),
            Child = K.Symbol(symbol, 17, Ink.Muted),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var column = K.Column(10, circle, K.Text(text, 12, ink: Ink.Muted, align: TextAlignment.Center));
        column.Padding = new Thickness(24, 28, 24, 28);
        return column;
    }

    /// <summary>A row of a list in a panel that lights under the pointer, and lit for good when it is the one playing (<c>MediaRow</c>).</summary>
    public static Tap Row(UIElement content, bool lit = false, Action? act = null)
    {
        var tap = new Tap(content, Look.Clear, 0, padding: 8)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            Height = double.NaN,
            MinHeight = 0,
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(8, 6, 8, 6),
        };
        if (lit) tap.Resting = Shade.White.WithAlpha(0.09f);
        if (act != null) tap.Click += (_, _) => act();
        else tap.IsTabStop = false;
        return tap;
    }

    /// <summary>
    /// A cover or a poster where there is one, and a soft square in the
    /// tally with a symbol where there is not; faded in when it arrives
    /// (<c>MediaArtwork</c>). <paramref name="load"/> fetches a picture the
    /// platform's own fetching cannot (a media server's poster, which wants
    /// the app's session).
    /// </summary>
    public static Grid Artwork(string url, double width, double height, string symbol = "music.note", double corner = 8, Func<string, Task<byte[]?>>? load = null)
    {
        var ground = new LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0, 0), EndPoint = new Windows.Foundation.Point(1, 1) };
        ground.GradientStops.Add(new GradientStop { Color = Ink.Of(Appearance.Shared.Tally.WithAlpha(0.32f)), Offset = 0 });
        ground.GradientStops.Add(new GradientStop { Color = Ink.Of(Theme.SurfaceHigh), Offset = 1 });
        var grid = new Grid { Width = width, Height = height, CornerRadius = new CornerRadius(corner), Background = ground, BorderBrush = Ink.White(0.08f), BorderThickness = new Thickness(1) };
        grid.Children.Add(K.Symbol(symbol, 13, Ink.ForegroundAt(0.7f)).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        if (url.StartsWith("http", StringComparison.Ordinal))
        {
            var image = new Image { Stretch = Stretch.UniformToFill };
            Motion.Hide(image);
            grid.Children.Add(image);
            _ = Fill(image, url, (int)(Math.Max(width, height) * 2), load);
        }
        return grid;
    }

    private static readonly Dictionary<string, BitmapImage> Pictures = new();

    private static async Task Fill(Image image, string url, int pixels, Func<string, Task<byte[]?>>? load)
    {
        if (!Pictures.TryGetValue(url, out var bitmap))
        {
            bitmap = new BitmapImage { DecodePixelWidth = pixels, DecodePixelType = DecodePixelType.Physical };
            if (load != null)
            {
                var bytes = await load(url);
                if (bytes == null) return;
                using var stream = new MemoryStream(bytes);
                try
                {
                    await bitmap.SetSourceAsync(stream.AsRandomAccessStream());
                }
                catch (Exception)
                {
                    return;
                }
            }
            else
            {
                bitmap.UriSource = new Uri(url);
            }
            if (Pictures.Count > 64) Pictures.Clear();
            Pictures[url] = bitmap;
        }
        image.Source = bitmap;
        Motion.FadeTo(image, 1f);
    }

    /// <summary>A few words in a small capsule: what a song is, that it is paused (<c>MediaBadge</c>).</summary>
    public static Border Badge(string text, string? symbol = null, bool lit = false)
    {
        var ink = lit ? Ink.Tally : Ink.ForegroundAt(0.75f);
        var row = K.Row(4);
        if (symbol != null) row.Children.Add(K.Symbol(symbol, 8.5, ink));
        row.Children.Add(K.Text(text, 10.5, K.Semibold, ink, lines: 1));
        return new Border
        {
            Child = row,
            CornerRadius = new CornerRadius(9),
            Background = lit ? Ink.Brush(Appearance.Shared.Tally.WithAlpha(0.16f)) : Ink.White(0.09f),
            BorderBrush = lit ? Ink.Brush(Appearance.Shared.Tally.WithAlpha(0.35f)) : null,
            BorderThickness = new Thickness(lit ? 1 : 0),
            Padding = new Thickness(7, 3, 7, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
    }

    /// <summary>
    /// A field in a capsule that lights while it is typed in, with a symbol
    /// before it (<c>MediaField</c>); Enter submits. What is typed is a link
    /// or a search, so nothing is corrected or capitalised.
    /// </summary>
    public static Border Field(string prompt, string text, Action<string> changed, Action submit, string symbol, int limit, out TextBox box)
    {
        var field = new TextBox
        {
            Text = text,
            PlaceholderText = prompt,
            IsSpellCheckEnabled = false,
            IsTextPredictionEnabled = false,
            MaxLength = limit,
            FontSize = 13,
            BorderThickness = new Thickness(0),
            Background = K.Clear,
            VerticalAlignment = VerticalAlignment.Center,
            MinHeight = 0,
            Padding = new Thickness(0, 4, 4, 4),
        };
        // The system's field lights its own ground under the pointer and
        // when focused; the capsule does the lighting here.
        foreach (var state in new[] { "", "PointerOver", "Focused", "Disabled" })
        {
            field.Resources["TextControlBackground" + state] = K.Clear;
            field.Resources["TextControlBorderBrush" + state] = K.Clear;
        }
        field.Named(prompt);
        field.TextChanged += (_, _) => changed(field.Text);
        field.KeyDown += (_, e) =>
        {
            if (e.Key != Windows.System.VirtualKey.Enter) return;
            e.Handled = true;
            submit();
        };
        var line = K.Line(8, K.Fixed(K.Symbol(symbol, 12, Ink.Muted).At(HorizontalAlignment.Center, VerticalAlignment.Center)), K.Flex(field));
        var capsule = new Border
        {
            Child = line,
            Height = 34,
            CornerRadius = new CornerRadius(17),
            Background = Ink.White(0.06f),
            BorderBrush = Ink.White(0.10f),
            BorderThickness = new Thickness(1),
            Padding = new Thickness(12, 0, 6, 0),
        };
        field.GotFocus += (_, _) => capsule.BorderBrush = Ink.Brush(Appearance.Shared.Tally.WithAlpha(0.6f));
        field.LostFocus += (_, _) => capsule.BorderBrush = Ink.White(0.10f);
        box = field;
        return capsule;
    }

    /// <summary>
    /// A capsule track that fills as it rises, with a white knob: for a
    /// volume and for a place in a video (<c>MediaSlider</c>), on a Windows
    /// slider so the keyboard and the screen reader keep it. <paramref
    /// name="editing"/> is told when a change starts and ends, for a scrubber
    /// that seeks only once it is let go: a drag let go, a drag the system
    /// takes away, and a step from the keyboard, which is a whole change on
    /// its own. Android's note: an interrupted drag that never ended left the
    /// scrubber editing for good, and the controls over the video never hid.
    /// </summary>
    public static Slider Slider(double value, Action<double> changed, double minimum = 0, double maximum = 1, double step = 0, Brush? tint = null, string? label = null, Action<bool>? editing = null)
    {
        var slider = new Slider
        {
            Minimum = minimum,
            Maximum = Math.Max(minimum + 0.001, maximum),
            Value = value,
            StepFrequency = step > 0 ? step : (maximum - minimum) / 1000,
            SmallChange = step > 0 ? step : (maximum - minimum) / 20,
            LargeChange = (maximum - minimum) / 10,
            IsThumbToolTipEnabled = false,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var fill = tint ?? Ink.Foreground;
        foreach (var state in new[] { "", "PointerOver", "Pressed", "Disabled" })
        {
            slider.Resources["SliderTrackFill" + state] = Ink.White(0.14f);
            slider.Resources["SliderTrackValueFill" + state] = fill;
            slider.Resources["SliderThumbBackground" + state] = Ink.Brush(Shade.White);
            slider.Resources["SliderOuterThumbBackground" + state] = Ink.Brush(Shade.White);
        }
        if (label != null) slider.Named(label);
        var dragging = false;
        var setting = false;
        slider.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler((_, _) =>
        {
            if (dragging) return;
            dragging = true;
            editing?.Invoke(true);
        }), true);
        void Finish()
        {
            if (!dragging) return;
            dragging = false;
            editing?.Invoke(false);
        }
        slider.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler((_, _) => Finish()), true);
        slider.AddHandler(UIElement.PointerCaptureLostEvent, new PointerEventHandler((_, _) => Finish()), true);
        slider.Unloaded += (_, _) => Finish();
        slider.ValueChanged += (_, e) =>
        {
            if (setting) return;
            if (dragging)
            {
                changed(e.NewValue);
                return;
            }
            // From the keyboard or a screen reader: begun, moved and
            // finished at once.
            editing?.Invoke(true);
            changed(e.NewValue);
            editing?.Invoke(false);
        };
        slider.Tag = new Action<double>(to =>
        {
            setting = true;
            slider.Value = to;
            setting = false;
        });
        return slider;
    }

    /// <summary>Moves a media slider without it telling anybody, as a value read from the player rather than chosen.</summary>
    public static void Set(Slider slider, double value, double? maximum = null)
    {
        if (slider.Tag is not Action<double> set) return;
        if (maximum is { } most && Math.Abs(slider.Maximum - most) > 0.001)
        {
            set(Math.Min(slider.Value, most));
            slider.Maximum = Math.Max(slider.Minimum + 0.001, most);
        }
        if (Math.Abs(slider.Value - value) > 0.0005) set(value);
    }
}
