using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Kit;

/// <summary>The rows of a popover's menu (Android's kit/Menus.kt).</summary>
internal static class Menu
{
    public enum Role { Normal, Prominent, Destructive }

    /// <summary>A column of items, six in from the edge.</summary>
    public static StackPanel Card(params UIElement[] items)
    {
        var column = K.Column(2, items);
        column.Padding = new Thickness(6);
        return column;
    }

    /// <summary>
    /// One choice: an optional symbol, its title and a line under it, and a
    /// tick when it is the one chosen. Lit under the pointer; a Windows
    /// button underneath, so the keyboard reaches it.
    /// </summary>
    public static Tap Item(string title, Action act, string? symbol = null, string? detail = null, bool check = false, Role role = Role.Normal, bool enabled = true, bool busy = false)
    {
        var tint = role switch
        {
            Role.Destructive => Ink.Danger,
            Role.Prominent => Ink.Tally,
            _ => Ink.ForegroundAt(0.85f),
        };
        var words = K.Column(1, K.Text(title, 13.5, role == Role.Normal ? K.Regular : K.Semibold, role == Role.Destructive ? Ink.Danger : Ink.Foreground, lines: 2));
        if (detail != null) words.Children.Add(K.Text(detail, 11.5, ink: Ink.Muted, lines: 2));
        var parts = new List<(UIElement, double)>();
        if (symbol != null) parts.Add(K.Fixed(new Border { Width = 18, Child = K.Symbol(symbol, 13, tint) }));
        parts.Add(K.Flex(words));
        if (busy) parts.Add(K.Fixed(K.Spinner(14)));
        else if (check) parts.Add(K.Fixed(K.Symbol("checkmark", 11.5, Ink.Tally).Pad(8, 0, 0, 0)));
        var line = K.Line(10, parts.ToArray());
        line.VerticalAlignment = VerticalAlignment.Center;
        var tap = new Tap(line, Look.Clear, height: 34, padding: 10)
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            CornerRadius = new CornerRadius(10),
            IsEnabled = enabled,
        };
        tap.Height = double.NaN;
        tap.MinHeight = 34;
        tap.Padding = new Thickness(10, detail != null ? 6 : 7, 10, detail != null ? 6 : 7);
        tap.Named(title);
        tap.Click += (_, _) => act();
        return tap;
    }

    public static Border Rule() => new() { Height = 1, Margin = new Thickness(8, 4, 8, 4), Background = Ink.White(0.08f) };

    /// <summary>A heading over a group of items, in capitals as the other apps draw it.</summary>
    public static TextBlock Heading(string text)
    {
        var block = K.Text(text.ToUpper(Reading), 10.5, K.Semibold, Ink.Muted, lines: 1);
        block.CharacterSpacing = 60;
        block.Margin = new Thickness(10, 8, 10, 2);
        return block;
    }
}

/// <summary>
/// The apps' segmented choice (kit/Choosing.kt <c>SegmentedChoice</c>): a
/// dark track and a lighter pill that slides to the chosen segment. Windows
/// has no control of this shape; the segments are buttons, so the
/// keyboard and Narrator reach each.
/// </summary>
internal sealed class Segmented : Grid
{
    private readonly Border _pill;
    private readonly List<Tap> _segments = new();
    private readonly List<TextBlock> _labels = new();
    private int _chosen;

    public event Action<int>? Chose;

    /// <summary>With <paramref name="fill"/>, the choices share the width it is given, as the phone's sheet has them beside its close button.</summary>
    public Segmented(IReadOnlyList<string> options, int chosen, double height = 30, bool fill = false)
    {
        _chosen = chosen;
        Height = height;
        CornerRadius = new CornerRadius(height / 2);
        Background = Ink.Black(0.22f);
        BorderBrush = Ink.White(0.07f);
        BorderThickness = new Thickness(1);
        Padding = new Thickness(2);
        _pill = new Border
        {
            CornerRadius = new CornerRadius((height - 4) / 2),
            Background = Ink.White(0.16f),
            BorderBrush = Ink.White(0.10f),
            BorderThickness = new Thickness(1),
            HorizontalAlignment = HorizontalAlignment.Left,
        };
        Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.SetIsTranslationEnabled(_pill, true);
        var row = new Grid { ColumnSpacing = 2 };
        for (var i = 0; i < options.Count; i++)
        {
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = fill ? new GridLength(1, GridUnitType.Star) : GridLength.Auto });
            var label = K.Text(options[i], 12.5, i == chosen ? K.Semibold : K.Regular, i == chosen ? Ink.Foreground : Ink.Muted, lines: 1);
            var index = i;
            var segment = new Tap(label, Look.Clear, height - 4, padding: 10) { CornerRadius = new CornerRadius((height - 4) / 2) };
            segment.Named(options[i]);
            if (fill) segment.HorizontalAlignment = HorizontalAlignment.Stretch;
            segment.Click += (_, _) => Choose(index);
            Grid.SetColumn(segment, i);
            row.Children.Add(segment);
            _segments.Add(segment);
            _labels.Add(label);
        }
        Children.Add(_pill);
        Children.Add(row);
        row.LayoutUpdated += (_, _) => PlacePill(animated: false);
    }

    private bool _placed;
    private double _placedX = double.NaN, _placedWidth = double.NaN;

    private void PlacePill(bool animated)
    {
        if (_chosen < 0 || _chosen >= _segments.Count) return;
        var segment = _segments[_chosen];
        if (segment.ActualWidth <= 0) return;
        var x = segment.TransformToVisual(this).TransformPoint(new Windows.Foundation.Point(0, 0)).X - Padding.Left - BorderThickness.Left;
        // Placed again only where the chosen segment moved or changed width
        // (one that shares a width that changed), not at every layout pass.
        if (_placed && !animated && Math.Abs(x - _placedX) < 0.5 && Math.Abs(segment.ActualWidth - _placedWidth) < 0.5) return;
        _placedX = x;
        _placedWidth = segment.ActualWidth;
        _pill.Width = segment.ActualWidth;
        _pill.Height = segment.ActualHeight;
        var visual = Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(_pill);
        if (!animated)
        {
            _placed = true;
            visual.Properties.InsertVector3("Translation", new System.Numerics.Vector3((float)x, 0, 0));
            return;
        }
        var compositor = visual.Compositor;
        var move = compositor.CreateVector3KeyFrameAnimation();
        move.InsertKeyFrame(1f, new System.Numerics.Vector3((float)x, 0, 0), Motion.SettleCurve(compositor));
        move.Duration = Motion.Settle;
        visual.StartAnimation("Translation", move);
    }

    public void Choose(int index, bool tell = true)
    {
        if (index == _chosen) return;
        _chosen = index;
        for (var i = 0; i < _labels.Count; i++)
        {
            _labels[i].FontWeight = i == index ? K.Semibold : K.Regular;
            _labels[i].Foreground = i == index ? Ink.Foreground : Ink.Muted;
        }
        PlacePill(animated: true);
        if (tell) Chose?.Invoke(index);
    }
}

/// <summary>The fields the apps type into, as Windows controls in the apps' colours.</summary>
internal static class Fields
{
    /// <summary>
    /// A text field with a symbol before it (kit/Fields.kt <c>GlassField</c>):
    /// no capital, no correction and no suggestion, since what is typed is a
    /// name, a room or a passphrase, and a keyboard that "corrects" a name
    /// signs in as somebody else.
    /// </summary>
    public static TextBox Text(string prompt, string text, Action<string> changed, bool mono = false, double height = 40, Action? submit = null)
    {
        var box = new TextBox
        {
            Text = text,
            PlaceholderText = prompt,
            IsSpellCheckEnabled = false,
            IsTextPredictionEnabled = false,
            FontSize = 14,
            MinHeight = height,
            VerticalContentAlignment = VerticalAlignment.Center,
            FontFamily = mono && text.Length > 0 ? K.Mono : K.Face,
        };
        box.Named(prompt);
        box.TextChanged += (_, _) =>
        {
            if (mono) box.FontFamily = box.Text.Length > 0 ? K.Mono : K.Face;
            changed(box.Text);
        };
        if (submit != null) box.KeyDown += (_, e) =>
        {
            if (e.Key == Windows.System.VirtualKey.Enter)
            {
                e.Handled = true;
                submit();
            }
        };
        return box;
    }

    /// <summary>
    /// A passphrase field: the system's own, with the Windows reveal button
    /// standing in for the other apps' eye ("Show passphrase").
    /// </summary>
    public static PasswordBox Secret(string prompt, string text, Action<string> changed, double height = 40, Action? submit = null)
    {
        var box = new PasswordBox
        {
            Password = text,
            PlaceholderText = prompt,
            FontSize = 14,
            MinHeight = height,
            VerticalContentAlignment = VerticalAlignment.Center,
            PasswordRevealMode = PasswordRevealMode.Peek,
        };
        box.Named(prompt);
        box.PasswordChanged += (_, _) => changed(box.Password);
        if (submit != null) box.KeyDown += (_, e) =>
        {
            if (e.Key == Windows.System.VirtualKey.Enter)
            {
                e.Handled = true;
                submit();
            }
        };
        return box;
    }

    /// <summary>A switch: Windows' ToggleSwitch with nothing written beside it, lit in the tally (Ink.ApplyAccent).</summary>
    public static ToggleSwitch Switch(bool on, Action<bool> changed, string label)
    {
        var toggle = new ToggleSwitch
        {
            IsOn = on,
            OnContent = "",
            OffContent = "",
            MinWidth = 0,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        toggle.Named(label);
        toggle.Toggled += (_, _) => changed(toggle.IsOn);
        return toggle;
    }
}

/// <summary>The settings' sections and rows (kit/Settings.kt), used wherever a card holds a list of choices.</summary>
internal static class Rows
{
    /// <summary>A titled card of rows, and a line of small print under it.</summary>
    public static StackPanel Section(string? title, UIElement card, string? footer = null)
    {
        var column = K.Column(8);
        if (title != null) column.Children.Add(K.Text(title, 12, K.Semibold, Ink.Muted).Pad(6, 0, 0, 0));
        column.Children.Add(card);
        if (footer != null) column.Children.Add(K.Text(footer, 11.5, ink: Ink.Muted).Pad(6, 0, 6, 0));
        return column;
    }

    /// <summary>The card a section's rows sit on.</summary>
    public static Border Card(params UIElement[] rows) => K.Card(K.Column(0, rows), 16);

    /// <summary>A symbol on its own tile, at the head of a row.</summary>
    public static Border Tile(string symbol)
    {
        return new Border
        {
            Width = 26,
            Height = 26,
            CornerRadius = new CornerRadius(7),
            Background = Ink.White(0.10f),
            Child = K.Symbol(symbol, 12.5, Ink.Foreground),
        };
    }

    /// <summary>One row: an optional symbol tile, a title and a line under it, and a control at the end; a hairline under it unless last.</summary>
    public static StackPanel Row(string title, UIElement? control = null, string? detail = null, string? symbol = null, bool last = false)
    {
        var words = K.Column(2, K.Text(title, 13.5));
        if (detail != null) words.Children.Add(K.Text(detail, 11.5, ink: Ink.Muted));
        words.VerticalAlignment = VerticalAlignment.Center;
        var parts = new List<(UIElement, double)>();
        if (symbol != null) parts.Add(K.Fixed(Tile(symbol)));
        parts.Add(K.Flex(words));
        if (control != null)
        {
            if (control is FrameworkElement framework) framework.VerticalAlignment = VerticalAlignment.Center;
            parts.Add(K.Fixed(new Border { Child = control, Margin = new Thickness(22, 0, 0, 0) }));
        }
        var line = K.Line(12, parts.ToArray());
        line.MinHeight = 48;
        line.Padding = new Thickness(14, 9, 14, 9);
        var column = K.Column(0, line);
        if (!last) column.Children.Add(K.Rule(symbol != null ? 52 : 14));
        return column;
    }
}
