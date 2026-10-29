using System.Numerics;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Shapes;
using Windows.UI.Text;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The pieces every screen is built of (Android's ui/kit, Apple's
/// TomoshibiKit/UI), made in code: text in the system's own face, the
/// symbols in Segoe Fluent Icons, plain cards for content and in-app
/// acrylic for the control layer. Lengths are device-independent pixels,
/// which are the other apps' points.
/// </summary>
internal static class K
{
    /// <summary>The system's face: Segoe UI Variable on Windows 11, Segoe UI before it.</summary>
    public static readonly FontFamily Face = new("Segoe UI Variable Text, Segoe UI");
    public static readonly FontFamily Display = new("Segoe UI Variable Display, Segoe UI");
    /// <summary>Room names and addresses, in the machine's monospace (the other apps use their system's).</summary>
    public static readonly FontFamily Mono = new("Cascadia Mono, Consolas");
    public static readonly FontFamily Icons = new("Segoe Fluent Icons, Segoe MDL2 Assets");

    public static readonly FontWeight Regular = FontWeights.Normal;
    public static readonly FontWeight Medium = FontWeights.Medium;
    public static readonly FontWeight Semibold = FontWeights.SemiBold;
    public static readonly FontWeight Bold = FontWeights.Bold;

    /// <summary>A run of words. <paramref name="lines"/> above nought cuts it there with an ellipsis; nought wraps freely.</summary>
    public static TextBlock Text(
        string text,
        double size = 13.5,
        FontWeight? weight = null,
        Brush? ink = null,
        bool mono = false,
        int lines = 0,
        TextAlignment align = TextAlignment.Left,
        bool tabular = false)
    {
        var block = new TextBlock
        {
            Text = text,
            FontSize = size,
            FontWeight = weight ?? Regular,
            FontFamily = mono ? Mono : size >= 20 ? Display : Face,
            Foreground = ink ?? Ink.Foreground,
            TextAlignment = align,
            TextWrapping = lines == 1 ? TextWrapping.NoWrap : TextWrapping.Wrap,
            TextTrimming = lines > 0 ? TextTrimming.CharacterEllipsis : TextTrimming.None,
            IsTextScaleFactorEnabled = true,
            LineHeight = size * 1.22,
            LineStackingStrategy = LineStackingStrategy.BlockLineHeight,
        };
        if (lines > 0) block.MaxLines = lines;
        if (align == TextAlignment.Center) block.HorizontalAlignment = HorizontalAlignment.Center;
        if (tabular) Typography.SetNumeralAlignment(block, FontNumeralAlignment.Tabular);
        return block;
    }

    /// <summary>
    /// Text that may have a flag in it, as a relay's label has: as Text where
    /// it has none, and on one line with its flags drawn where it has
    /// (Flags), since Windows has no glyphs for them.
    /// </summary>
    public static FrameworkElement Words(string text, double size = 13.5, FontWeight? weight = null, Brush? ink = null, int lines = 0, bool tabular = false) =>
        Flags.In(text)
            ? Flags.Line(text, size, run => Text(run, size, weight, ink, lines: 1, tabular: tabular))
            : Text(text, size, weight, ink, lines: lines, tabular: tabular);

    /// <summary>One of the apps' symbols (Symbols.cs maps their names to the Windows ones).</summary>
    public static FrameworkElement Symbol(string name, double size = 14, Brush? ink = null) => Symbols.Make(name, size, ink ?? Ink.Foreground);

    /// <summary>
    /// A column at most <paramref name="most"/> across, centred in a scroller
    /// that moves it up and down. A scroller places its content by the width
    /// the content asks for, not the width it is laid out at: a page whose
    /// rows asked for less than the column (the settings' About, at 391 of
    /// 620) was centred as if that narrow and drawn the column's full width,
    /// past the scroller's right edge, where it was cut off. Held to the
    /// scroller's width, to its most, it asks for what it is.
    /// </summary>
    public static ScrollViewer Scrolling(FrameworkElement column, double most)
    {
        column.MaxWidth = most;
        column.HorizontalAlignment = HorizontalAlignment.Center;
        var scroller = new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        scroller.SizeChanged += (_, e) => column.Width = Math.Min(most, e.NewSize.Width);
        return scroller;
    }

    public static StackPanel Column(double spacing, params UIElement[] children)
    {
        var panel = new StackPanel { Orientation = Orientation.Vertical, Spacing = spacing };
        foreach (var child in children) panel.Children.Add(child);
        return panel;
    }

    public static StackPanel Row(double spacing, params UIElement[] children)
    {
        var panel = new StackPanel { Orientation = Orientation.Horizontal, Spacing = spacing };
        foreach (var child in children) panel.Children.Add(child);
        return panel;
    }

    /// <summary>
    /// A column whose children slide to their new places when one comes or
    /// goes, rather than jumping: the appearing one fades in by its own
    /// motion, and the rest of the column makes room for it.
    /// </summary>
    public static StackPanel Flowing(double spacing, Orientation orientation = Orientation.Vertical)
    {
        var panel = new StackPanel { Orientation = orientation, Spacing = spacing };
        panel.ChildrenTransitions = new TransitionCollection { new RepositionThemeTransition { IsStaggeringEnabled = false } };
        return panel;
    }

    /// <summary>A row of fixed and flexible parts: <c>Grid</c> columns, auto unless given as stars.</summary>
    public static Grid Line(double spacing, params (UIElement Element, double Stars)[] parts)
    {
        var grid = new Grid { ColumnSpacing = spacing };
        for (var i = 0; i < parts.Length; i++)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = parts[i].Stars > 0 ? new GridLength(parts[i].Stars, GridUnitType.Star) : GridLength.Auto,
            });
            Grid.SetColumn((FrameworkElement)parts[i].Element, i);
            grid.Children.Add(parts[i].Element);
        }
        return grid;
    }

    public static (UIElement, double) Fixed(UIElement element) => (element, 0);
    public static (UIElement, double) Flex(UIElement element, double stars = 1) => (element, stars);

    /// <summary>Space that grows.</summary>
    public static Border Spacer(double width = 0, double height = 0) => new() { Width = width, Height = height };

    /// <summary>
    /// A plain card in the content: no material, the apps' white wash over
    /// whatever is behind (glass drawn plain, Android's <c>plainSurface</c>).
    /// In the light scheme the surface's white with the palette's border, so
    /// that a card stands above the page as the washed one does at night
    /// (daylight palette, addendum 2), where a black wash sank it.
    /// </summary>
    public static Border Card(UIElement? child = null, double radius = 16, float fill = 0.05f, float edge = 0.07f, Thickness? padding = null)
    {
        return new Border
        {
            Child = child,
            CornerRadius = new CornerRadius(radius),
            Background = Ink.Light ? Ink.Surface : Ink.Wash(fill),
            BorderBrush = edge > 0 ? (Ink.Light ? Ink.Border : Ink.Wash(edge)) : null,
            BorderThickness = new Thickness(edge > 0 ? 1 : 0),
            Padding = padding ?? new Thickness(0),
        };
    }

    /// <summary>An opaque surface, the screens' cards and the dialogs (Theme.surface).</summary>
    public static Border Solid(UIElement? child, double radius, Brush? fill = null, Thickness? padding = null) => new()
    {
        Child = child,
        CornerRadius = new CornerRadius(radius),
        Background = fill ?? Ink.Surface,
        Padding = padding ?? new Thickness(0),
    };

    /// <summary>
    /// The control layer's material: in-app acrylic over the call or the
    /// evening room behind it, tinted with the apps' surface grey, a
    /// hairline rim, and a shadow under it. Only the controls floating over
    /// the call or the room wear it (the bar, the sidebar, the stage's
    /// buttons, the pills over the pictures); what is read (a menu, a panel,
    /// a dialog) is opaque (<see cref="Floating"/>, <see cref="PanelSurface"/>),
    /// and what is inside the glass is plain, never material on material.
    /// </summary>
    public static Border Glass(UIElement? child = null, double radius = 22, Thickness? padding = null, bool shadow = true)
    {
        var border = new Border
        {
            Child = child,
            CornerRadius = new CornerRadius(radius),
            Background = Acrylic(),
            BorderBrush = Rim(),
            BorderThickness = new Thickness(1),
            Padding = padding ?? new Thickness(0),
        };
        if (shadow)
        {
            border.Shadow = new ThemeShadow();
            border.Translation = new Vector3(0, 0, 24);
        }
        return border;
    }

    /// <summary>
    /// What a glass comes and goes in when it moves as it does (Rises,
    /// Arrives, the overlay's panels): the glass is raised for its shadow
    /// through UIElement.Translation, which is the only raising a ThemeShadow
    /// is cast from, and an element so raised whose own visual is animated as
    /// well is drawn in one place and pressed in another. The lobby's sidebar
    /// took its rows' presses thirty-six points below where it drew them (its
    /// top margin, counted twice), so a person with the pointer on Servers
    /// opened Scheduled meetings. So a raised element's visual is never
    /// animated: what moves it moves this, which has no ground of its own and
    /// so takes no presses. It takes the glass's margin and alignment, to be
    /// as large as the glass and laid out where it was; call it once they are set.
    /// </summary>
    public static Border Holding(FrameworkElement glass)
    {
        var holder = new Border
        {
            Child = glass,
            Margin = glass.Margin,
            HorizontalAlignment = glass.HorizontalAlignment,
            VerticalAlignment = glass.VerticalAlignment,
        };
        glass.Margin = new Thickness(0);
        return holder;
    }

    /// <summary>
    /// The edge of a glass or a pane: the palette's border, the foreground
    /// at a tenth, flat in either scheme, as the web's glass draws it. At
    /// night it was a rim of light along the top fading down (white at 26
    /// per cent to 7), a gradient however thin, and the owner's rule is that
    /// there are none (2026-10-29).
    /// </summary>
    public static Brush Rim() => Ink.Border;

    /// <summary>
    /// What floats over the page or the call to be read (a menu, a popover,
    /// a toast): the surface, opaque in either scheme, with the hairline and
    /// a shadow. It was the glass at night, and words over a moving picture
    /// change their contrast while they are being read (the web's
    /// ui-polish: menus, dialogs and panels are opaque).
    /// </summary>
    public static Border Floating(UIElement? child = null, double radius = 22, Thickness? padding = null)
    {
        var card = Glass(child, radius, padding);
        card.Background = Ink.Surface;
        return card;
    }

    /// <summary>A panel beside the call (the messages, the sound, the devices, the host's, watching together and the music): opaque, as <see cref="Floating"/>.</summary>
    public static Border PanelSurface(UIElement? child = null, double radius = 22, Thickness? padding = null) => Floating(child, radius, padding);

    /// <summary>The acrylic itself, for a surface that is shaped some other way: tinted with the scheme's surface, which the system's material lightens or darkens round.</summary>
    public static AcrylicBrush Acrylic() => new()
    {
        TintColor = Ink.Of(Ink.Palette.Surface),
        TintOpacity = 0.62,
        TintLuminosityOpacity = 0.9,
        FallbackColor = Ink.Of(Ink.Palette.Surface.WithAlpha(0.92f)),
    };

    /// <summary>A hairline across a card, inset from its leading edge.</summary>
    public static Border Rule(double leading = 0, float alpha = 0.06f) => new()
    {
        Height = 1,
        Margin = new Thickness(leading, 0, 0, 0),
        Background = Ink.Wash(alpha),
    };

    /// <summary>The apps' spinner: a ring turning, in the colour it sits on.</summary>
    public static ProgressRing Spinner(double size = 16, Brush? ink = null) => new()
    {
        IsActive = true,
        Width = size,
        Height = size,
        Foreground = ink ?? Ink.Muted,
        Background = Ink.Transparent,
    };

    /// <summary>A round dot.</summary>
    public static Ellipse Dot(double size, Brush fill) => new() { Width = size, Height = size, Fill = fill };

    /// <summary>Names the element for screen readers and for the UI tests that find it by name.</summary>
    public static T Named<T>(this T element, string name, string? id = null) where T : FrameworkElement
    {
        AutomationProperties.SetName(element, name);
        if (id != null) AutomationProperties.SetAutomationId(element, id);
        return element;
    }

    /// <summary>A tooltip in the reader's language, the Windows way of naming a control that shows only a symbol.</summary>
    public static T Tip<T>(this T element, string tip) where T : FrameworkElement
    {
        ToolTipService.SetToolTip(element, tip);
        AutomationProperties.SetName(element, tip);
        return element;
    }

    public static T At<T>(this T element, HorizontalAlignment horizontal, VerticalAlignment vertical) where T : FrameworkElement
    {
        element.HorizontalAlignment = horizontal;
        element.VerticalAlignment = vertical;
        return element;
    }

    public static T Pad<T>(this T element, double left, double top, double right, double bottom) where T : FrameworkElement
    {
        element.Margin = new Thickness(left, top, right, bottom);
        return element;
    }

    public static T Pad<T>(this T element, double all) where T : FrameworkElement
    {
        element.Margin = new Thickness(all);
        return element;
    }

    /// <summary>Hidden from screen readers: a picture that says nothing a label beside it does not.</summary>
    public static T Quiet<T>(this T element) where T : UIElement
    {
        AutomationProperties.SetAccessibilityView(element, Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Raw);
        return element;
    }

    public static Grid Stack(params UIElement[] layers)
    {
        var grid = new Grid();
        foreach (var layer in layers) grid.Children.Add(layer);
        return grid;
    }

    public static Microsoft.UI.Xaml.Media.Brush Clear => new SolidColorBrush(Colors.Transparent);
}
