using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;

namespace Tomoshibi.UI.Kit;

/// <summary>What a button wears (Android's GlassButton, prominent and destructive; the bar's BarButton; a quiet one in a list).</summary>
internal enum Look
{
    /// <summary>The apps' white wash over whatever is behind, with a hairline edge.</summary>
    Plain,
    /// <summary>The tally, with its own ink: the one thing on the screen to press.</summary>
    Prominent,
    /// <summary>Red, for leaving and for a device turned off.</summary>
    Destructive,
    /// <summary>Nothing until the pointer is over it: a row, a link, a close mark.</summary>
    Clear,
    /// <summary>The call's bar: a stronger wash, lit in the tally when on.</summary>
    Bar,
}

/// <summary>
/// A button as the apps draw one: a capsule or a circle of a given height,
/// its fill brightening under the pointer and further when pressed, a
/// little smaller while pressed, and dimmed when it cannot be pressed. A
/// Windows Button underneath, so that it takes the keyboard's focus, shows
/// the system's focus ring, answers Enter and Space, and is a button to
/// Narrator and to UI Automation.
/// </summary>
internal class Tap : Button
{
    private Look _look;
    private bool _lit;
    private bool _over;
    private bool _down;
    private readonly bool _circle;
    private readonly double _height;

    public Tap(UIElement content, Look look = Look.Plain, double height = 36, bool circle = false, string? label = null, double? padding = null)
    {
        Style = (Style)Application.Current.Resources["Tap"];
        _look = look;
        _circle = circle;
        _height = height;
        Content = content;
        Height = height;
        MinWidth = height;
        if (circle) Width = height;
        CornerRadius = new CornerRadius(height / 2);
        Padding = circle ? new Thickness(0) : new Thickness(padding ?? height * 0.45, 0, padding ?? height * 0.45, 0);
        if (label != null) this.Tip(label);
        PointerEntered += (_, _) => { _over = true; Paint(); };
        PointerExited += (_, _) => { _over = false; _down = false; Paint(); };
        AddHandler(PointerPressedEvent, new PointerEventHandler((_, _) => { _down = true; Paint(); }), true);
        AddHandler(PointerReleasedEvent, new PointerEventHandler((_, _) => { _down = false; Paint(); }), true);
        PointerCaptureLost += (_, _) => { _down = false; Paint(); };
        IsEnabledChanged += (_, _) => Paint();
        Paint();
    }

    /// <summary>A capsule holding words, with a symbol before them when given.</summary>
    public static Tap Words(string words, string? symbol = null, Look look = Look.Plain, double height = 36, double size = 13.5, double? padding = null)
    {
        var ink = InkFor(look);
        var row = K.Row(6);
        row.VerticalAlignment = VerticalAlignment.Center;
        if (symbol != null) row.Children.Add(K.Symbol(symbol, size * 0.95, ink));
        row.Children.Add(K.Text(words, size, K.Semibold, ink, lines: 1));
        var tap = new Tap(row, look, height, padding: padding);
        tap.Named(words);
        return tap;
    }

    /// <summary>A circle holding a symbol, named for the screen reader and the pointer's tooltip.</summary>
    public static Tap Icon(string symbol, string label, Look look = Look.Plain, double size = 36, double? glyph = null)
    {
        var tap = new Tap(K.Symbol(symbol, glyph ?? size * 0.4, InkFor(look)), look, size, circle: true, label: label);
        return tap;
    }

    public Look Look
    {
        get => _look;
        set
        {
            _look = value;
            Paint();
        }
    }

    /// <summary>On: a bar button whose device or panel is on wears the tally.</summary>
    public bool Lit
    {
        get => _lit;
        set
        {
            _lit = value;
            Paint();
        }
    }

    private Shade? _resting;

    /// <summary>A fill of its own at rest, in place of the look's (a chosen row, a selected section).</summary>
    public Shade? Resting
    {
        get => _resting;
        set
        {
            _resting = value;
            Paint();
        }
    }

    public static Brush InkFor(Look look, bool lit = false) => look switch
    {
        Look.Prominent => Ink.TallyForeground,
        Look.Destructive => Ink.Brush(Theme.DangerForeground),
        Look.Bar when lit => Ink.TallyForeground,
        _ => Ink.Foreground,
    };

    private void Paint()
    {
        var (rest, over, down, edge) = _look switch
        {
            Look.Prominent => (Ink.TallyShade, Mix(Ink.TallyShade, 0.10f), Mix(Ink.TallyShade, -0.10f), (Shade?)null),
            Look.Destructive => (Theme.Danger, Mix(Theme.Danger, 0.08f), Theme.Danger.WithAlpha(0.86f), (Shade?)null),
            Look.Clear => (Shade.White.WithAlpha(0f), Shade.White.WithAlpha(0.08f), Shade.White.WithAlpha(0.14f), (Shade?)null),
            Look.Bar when _lit => (Ink.TallyShade, Mix(Ink.TallyShade, 0.10f), Mix(Ink.TallyShade, -0.10f), Shade.White.WithAlpha(0.3f)),
            Look.Bar => (Shade.White.WithAlpha(0.12f), Shade.White.WithAlpha(0.18f), Shade.White.WithAlpha(0.24f), Shade.White.WithAlpha(0.08f)),
            _ => (Shade.White.WithAlpha(0.07f), Shade.White.WithAlpha(0.13f), Shade.White.WithAlpha(0.16f), Shade.White.WithAlpha(0.09f)),
        };
        if (_resting is { } resting)
        {
            // A row lit for being the chosen one keeps its light under the pointer, a little brighter.
            rest = resting;
            over = new Shade(resting.Red, resting.Green, resting.Blue, Math.Min(1f, resting.Alpha + 0.06f));
            down = new Shade(resting.Red, resting.Green, resting.Blue, Math.Min(1f, resting.Alpha + 0.12f));
        }
        var enabled = IsEnabled;
        Background = Ink.Brush(!enabled ? rest : _down ? down : _over ? over : rest);
        BorderBrush = edge is { } e ? Ink.Brush(e) : null;
        BorderThickness = new Thickness(edge != null ? 1 : 0);
        // On the composition visual, which the press's scale is on too:
        // once the visual is in use, XAML no longer carries UIElement.Opacity to it.
        Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(this).Opacity = enabled ? 1f : 0.45f;
        Motion.ScaleTo(this, _down && enabled ? 0.96f : 1f);
    }

    /// <summary>A colour lightened (positive) or darkened (negative) by an amount, its alpha kept.</summary>
    private static Shade Mix(Shade shade, float by)
    {
        float Move(float channel) => by >= 0 ? channel + (1 - channel) * by : channel * (1 + by);
        return new Shade(Move(shade.Red), Move(shade.Green), Move(shade.Blue), shade.Alpha);
    }
}
