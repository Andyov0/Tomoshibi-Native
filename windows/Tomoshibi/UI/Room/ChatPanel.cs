using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Windows.System;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>Pieces the call's panels share (kit/Notices.kt <c>PanelHeading</c>, <c>PanelRule</c>).</summary>
internal static class Panels
{
    public static Grid Heading(string title, string symbol, string closeLabel, Action close)
    {
        var tile = new Border
        {
            Width = 26,
            Height = 26,
            CornerRadius = new CornerRadius(8),
            Background = Ink.White(0.09f),
            Child = K.Symbol(symbol, 12, Ink.ForegroundAt(0.85f)),
        };
        var closeTap = Tap.Icon("xmark", closeLabel, Look.Clear, 26, 11);
        closeTap.Click += (_, _) => close();
        var line = K.Line(9, K.Fixed(tile), K.Flex(K.Text(title, 14, K.Semibold, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Fixed(closeTap));
        line.Padding = new Thickness(14, 10, 10, 10);
        return line;
    }

    /// <summary>A rule fading out at both ends.</summary>
    public static Border Rule()
    {
        var brush = new LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0, 0.5), EndPoint = new Windows.Foundation.Point(1, 0.5) };
        brush.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.White.WithAlpha(0f)), Offset = 0 });
        brush.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.White.WithAlpha(0.10f)), Offset = 0.5 });
        brush.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.White.WithAlpha(0f)), Offset = 1 });
        return new Border { Height = 1, Background = brush };
    }

    /// <summary>
    /// A message's words with its addresses as links (Linked.tsx): only
    /// complete http and https addresses, in the tally and underlined,
    /// opened in the browser.
    /// </summary>
    public static RichTextBlock Linked(string text, double size, int lines = 0)
    {
        var block = new RichTextBlock
        {
            FontSize = size,
            Foreground = Ink.Foreground,
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = lines == 0,
            FontFamily = K.Face,
        };
        if (lines > 0)
        {
            block.MaxLines = lines;
            block.TextTrimming = TextTrimming.CharacterEllipsis;
        }
        var paragraph = new Paragraph();
        foreach (var piece in Basics.Linked.Pieces(text))
        {
            switch (piece)
            {
                case Basics.Linked.Piece.Text words:
                    paragraph.Inlines.Add(new Run { Text = words.Words });
                    break;
                case Basics.Linked.Piece.Link link:
                    var hyperlink = new Hyperlink { Foreground = Ink.Tally, UnderlineStyle = UnderlineStyle.Single };
                    hyperlink.Inlines.Add(new Run { Text = link.Href });
                    if (Uri.TryCreate(link.Href, UriKind.Absolute, out var uri)) hyperlink.NavigateUri = uri;
                    paragraph.Inlines.Add(hyperlink);
                    break;
            }
        }
        block.Blocks.Add(paragraph);
        return block;
    }
}

/// <summary>
/// The call's messages (Android's ui/room/Chat.kt): what was said, a run by
/// one person under their name once, and the box. Enter sends and
/// Shift+Enter breaks the line, as in a browser's chat. What is typed is
/// kept with the call, not the panel, so closing the panel half way through
/// a sentence loses nothing; a message that did not leave is put back.
/// </summary>
internal sealed class ChatPanel : Grid
{
    private readonly CallModel _call;
    private readonly StackPanel _log = K.Flowing(0);
    private readonly ScrollViewer _scroller;
    private readonly Grid _empty;
    private readonly TextBox _box;
    private readonly Tap _send;
    private readonly StackPanel _failed;
    private readonly TextBlock _counter;
    private readonly Grid _under;
    private int _shown;

    public ChatPanel(CallModel call, Action close, double tallest)
    {
        _call = call;
        var glow = new Grid { Width = 80, Height = 80, HorizontalAlignment = HorizontalAlignment.Center };
        var radial = new RadialGradientBrush();
        radial.GradientStops.Add(new GradientStop { Color = Ink.Of(Ink.TallyShade.WithAlpha(0.16f)), Offset = 0 });
        radial.GradientStops.Add(new GradientStop { Color = Ink.Of(Ink.TallyShade.WithAlpha(0f)), Offset = 1 });
        glow.Children.Add(new Microsoft.UI.Xaml.Shapes.Ellipse { Fill = radial });
        glow.Children.Add(K.Symbol("bubble.left.and.text.bubble.right", 26, Ink.ForegroundAt(0.5f)));
        var emptyWords = K.Text(T("Messages disappear when the call ends."), 12, ink: Ink.Muted, align: TextAlignment.Center);
        emptyWords.MaxWidth = 200;
        _empty = new Grid { Children = { K.Column(12, glow, emptyWords) }, Padding = new Thickness(0, 26, 0, 26) };
        _empty.Arrives();

        _log.Padding = new Thickness(14, 12, 14, 12);
        _scroller = new ScrollViewer { Content = _log, MaxHeight = tallest, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        _scroller.Arrives();

        _box = new TextBox
        {
            Text = call.ChatDraft,
            PlaceholderText = call.State == uniffi.tomoshibi_core.CallState.Connected ? T("Say something") : T("Reconnecting…"),
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MaxHeight = 5 * 15 * 1.4 + 12,
            FontSize = 15,
            MaxLength = ChatRules.Longest,
            IsSpellCheckEnabled = true,
        };
        _box.Named(T("Say something"), "chat.box");
        // The box's own field drawn into the rounded box around it.
        foreach (var key in new[] { "TextControlBackground", "TextControlBackgroundPointerOver", "TextControlBackgroundFocused", "TextControlBorderBrush", "TextControlBorderBrushPointerOver", "TextControlBorderBrushFocused" })
        {
            _box.Resources[key] = K.Clear;
        }
        _box.BorderThickness = new Thickness(0);
        _box.TextChanged += (_, _) =>
        {
            var clamped = ChatRules.Clamp(_box.Text);
            if (clamped != _box.Text) _box.Text = clamped;
            _call.ChatDraft = _box.Text;
            Update();
        };
        _box.PreviewKeyDown += OnKey;
        _send = Tap.Icon("arrow.up", T("Send"), Look.Prominent, 28, 12.5);
        _send.VerticalAlignment = VerticalAlignment.Bottom;
        _send.Click += async (_, _) => await Send();
        var composer = new Border
        {
            Child = K.Line(8, K.Flex(_box), K.Fixed(_send)),
            Background = Ink.Black(0.22f),
            BorderBrush = Ink.White(0.09f),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(18),
            Padding = new Thickness(4, 5, 5, 5),
        };
        _box.GotFocus += (_, _) => composer.BorderBrush = Ink.TallyAt(0.55f);
        _box.LostFocus += (_, _) => composer.BorderBrush = Ink.White(0.09f);
        _counter = K.Text("", 10.5, ink: Ink.Muted, tabular: true);
        _failed = K.Row(5, K.Symbol("exclamationmark.circle.fill", 11, Ink.Danger), K.Text(T("That could not be sent."), 11, ink: Ink.Danger));
        _under = K.Line(0, K.Flex(_failed), K.Fixed(_counter));
        _under.Padding = new Thickness(8, 4, 8, 0);
        _under.Arrives();

        var column = K.Column(0,
            Panels.Heading(T("Messages"), "bubble.left.and.bubble.right.fill", T("Close messages"), close),
            Panels.Rule(),
            _empty,
            _scroller,
            K.Column(0, composer, _under).Pad(10, 4, 10, 10));
        Children.Add(K.Glass(column, 22));
        Update();
        // The box takes the keyboard a moment after the panel arrives.
        Loaded += async (_, _) =>
        {
            await Task.Delay(350);
            _box.Focus(FocusState.Programmatic);
        };
    }

    private async void OnKey(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter || e.Handled) return;
        var shift = Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift).HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
        if (shift) return;
        e.Handled = true;
        await Send();
    }

    private async Task Send()
    {
        if (!Sendable) return;
        _box.Text = "";
        await _call.SendDraft();
        // A message that did not leave is back in the box.
        if (_call.ChatDraft != _box.Text) _box.Text = _call.ChatDraft;
        _box.SelectionStart = _box.Text.Length;
        Update();
    }

    private bool Sendable => _call.State == uniffi.tomoshibi_core.CallState.Connected && !_call.Sending && !string.IsNullOrWhiteSpace(_call.ChatDraft);

    public void Update()
    {
        var offline = _call.State != uniffi.tomoshibi_core.CallState.Connected;
        _box.IsEnabled = !offline;
        _box.PlaceholderText = offline ? T("Reconnecting…") : T("Say something");
        _send.IsEnabled = Sendable;
        _failed.Visibility = _call.ChatFailed ? Visibility.Visible : Visibility.Collapsed;
        var length = _box.Text.Length;
        _counter.Text = length > 1800 ? $"{length}/{ChatRules.Longest}" : "";
        _counter.Foreground = length >= ChatRules.Longest ? Ink.Tally : Ink.Muted;
        // Under the box only while it has something to say.
        _under.Visibility = _call.ChatFailed || length > 1800 ? Visibility.Visible : Visibility.Collapsed;
        var messages = _call.Messages;
        _empty.Visibility = messages.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        _scroller.Visibility = messages.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
        if (messages.Count == _shown) return;
        if (messages.Count < _shown)
        {
            _log.Children.Clear();
            _shown = 0;
        }
        var speakers = messages.Select(m => m.Identity).ToList();
        for (var i = _shown; i < messages.Count; i++) _log.Children.Add(Said(messages[i], ChatRules.Continues(speakers, i), i == 0));
        _shown = messages.Count;
        // The newest in view.
        _scroller.UpdateLayout();
        _scroller.ChangeView(null, _scroller.ScrollableHeight, null, disableAnimation: false);
    }

    /// <summary>One message (Chat.kt <c>Said</c>): under its speaker's name and time at the start of a run, its words indented under them.</summary>
    private static StackPanel Said(ChatLine line, bool continues, bool first)
    {
        var column = K.Column(4);
        column.Margin = new Thickness(0, first ? 0 : continues ? 3 : 12, 0, 0);
        if (!continues)
        {
            var name = line.Mine ? T("{name} (you)", ("name", line.Name)) : Refusals.Named(line.Name, line.Identity);
            var header = K.Row(8,
                Faces.Monogram(line.Name, line.Identity, 20),
                K.Text(name, 12, K.Semibold, Ink.ForegroundAt(0.92f), lines: 1),
                K.Text(Clock(line.At), 10.5, ink: Ink.Muted, tabular: true));
            foreach (var child in header.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
            column.Children.Add(header);
        }
        var words = Panels.Linked(line.Text, 13);
        words.Margin = new Thickness(28, 0, 0, 0);
        column.Children.Add(words);
        column.Arrives();
        return column;
    }

    /// <summary>The time a message was said, as the reader's clock writes it.</summary>
    private static string Clock(DateTimeOffset at) => Times.Written(at, "shorttime");
}
