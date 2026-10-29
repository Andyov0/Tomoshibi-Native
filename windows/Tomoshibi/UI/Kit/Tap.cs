using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// A button as the apps draw one: a capsule or a circle of a given height,
/// its fill moving towards the foreground under the pointer and further
/// when pressed, a little smaller while pressed, and dimmed when it cannot
/// be pressed, by the one rule every look follows (Basics.Looks). A
/// Windows Button underneath, so that it takes the keyboard's focus, shows
/// the system's focus ring, answers Enter and Space, and is a button to
/// Narrator and to UI Automation.
/// </summary>
internal class Tap : Button
{
    private Look _look;
    private bool _lit;
    private bool _held;
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
        // Painted again once shown: a button turned off before it reached
        // the screen hears no IsEnabledChanged, and stayed lit.
        Loaded += (_, _) => Paint();
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

    protected override void OnContentChanged(object oldContent, object newContent)
    {
        base.OnContentChanged(oldContent, newContent);
        // New content is drawn in its maker's ink; a greyed prominent
        // button's is greyed again (Paint).
        _greyed = false;
        Paint();
    }

    private double _faint = 1;

    /// <summary>
    /// Drawn faint while still pressable (Notices.kt <c>RoundButton</c>
    /// <c>faint</c>): a press that will be refused, and is kept so the
    /// reason can be said. Set on the content here because Paint sets the
    /// content's opacity on every change and would undo it set from outside.
    /// </summary>
    public double Faint
    {
        get => _faint;
        set
        {
            _faint = value;
            Paint();
        }
    }

    public Look Look
    {
        get => _look;
        set
        {
            if (_look == value) return;
            _look = value;
            Paint();
        }
    }

    /// <summary>On: a bar button whose thing is happening (a share going out, a hand up) wears the tally.</summary>
    public bool Lit
    {
        get => _lit;
        set
        {
            if (_lit == value) return;
            _lit = value;
            Paint();
        }
    }

    /// <summary>
    /// Held down: a button whose panel is open, drawn pressed in the
    /// neutral grey rather than the tally (Looks.Paint).
    /// </summary>
    public bool Held
    {
        get => _held;
        set
        {
            if (_held == value) return;
            _held = value;
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

    /// <summary>The ink a look draws its content in, in the scheme being drawn.</summary>
    public static Brush InkFor(Look look, bool lit = false) =>
        Ink.Brush(Looks.Paint(look, Ink.Palette, Ink.TallyShade, Appearance.Shared.TallyForeground, Touch.Rest, lit: lit).Ink);

    /// <summary>Whether the content is drawn in the greyed prominent's ink at present.</summary>
    private bool _greyed;

    private void Paint()
    {
        var touch = _down ? Touch.Down : _over ? Touch.Over : Touch.Rest;
        var enabled = IsEnabled;
        var painted = Looks.Paint(_look, Ink.Palette, Ink.TallyShade, Appearance.Shared.TallyForeground, touch, enabled, _lit, _held, _resting);
        // The fill, its edge and what is in it are dimmed here rather than
        // by the button's own opacity: that was set before the button
        // reached the screen and lost there, the composition visual the
        // press scales being the one XAML writes the element's opacity to
        // when it is shown.
        Background = Ink.Brush(painted.Fill);
        BorderBrush = painted.Edge is { } edge ? Ink.Brush(edge) : null;
        BorderThickness = new Thickness(painted.Edge != null ? 1 : 0);
        if (Content is UIElement inside)
        {
            inside.Opacity = enabled ? _faint * painted.ContentOpacity : painted.ContentOpacity;
            // A prominent button that cannot be pressed is a grey plate with
            // grey words, its content drawn again in the muted ink, and in
            // the tally's ink again once it can be pressed.
            var grey = _look == Look.Prominent && !enabled && _resting == null;
            if (grey != _greyed)
            {
                _greyed = grey;
                Recolour(inside, Ink.Brush(painted.Ink));
            }
        }
        Motion.ScaleTo(this, painted.Scale);
    }

    /// <summary>The words, symbols and spinners in a button's content, drawn in another ink.</summary>
    private static void Recolour(UIElement element, Brush ink)
    {
        switch (element)
        {
            case TextBlock text:
                text.Foreground = ink;
                break;
            case FontIcon icon:
                icon.Foreground = ink;
                break;
            case ProgressRing ring:
                ring.Foreground = ink;
                break;
            case Microsoft.UI.Xaml.Shapes.Path path:
                if (path.Fill != null) path.Fill = ink;
                if (path.Stroke != null) path.Stroke = ink;
                break;
            case Line line:
                line.Stroke = ink;
                break;
            case Border border when border.Child != null:
                Recolour(border.Child, ink);
                break;
            case Panel panel:
                foreach (var child in panel.Children) Recolour(child, ink);
                break;
        }
    }
}
