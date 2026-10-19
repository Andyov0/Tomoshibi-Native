using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The apps' symbols, named as the Apple apps name them (SF Symbols) and as
/// Android's ui/kit/Symbols.kt maps them to Material icons, drawn here from
/// Segoe Fluent Icons, Windows 11's own set, so that a control means on
/// Windows what it looks like on Windows. Where the set has no such glyph (a
/// camera turned off, a raised hand) the nearest is drawn with what it
/// lacks: a slash across the camera, the hand from Segoe UI Emoji in its own
/// colour.
/// </summary>
internal static class Symbols
{
    private static readonly Dictionary<string, string> Glyphs = new()
    {
        ["mic.fill"] = "",
        ["mic"] = "",
        ["mic.slash.fill"] = "",
        ["video.fill"] = "",
        ["video"] = "",
        ["camera.fill"] = "",
        ["calendar"] = "",
        ["calendar.badge.clock"] = "",
        ["calendar.badge.plus"] = "",
        ["gearshape"] = "",
        ["gearshape.fill"] = "",
        ["house"] = "",
        ["link"] = "",
        ["chevron.left"] = "",
        ["chevron.right"] = "",
        ["chevron.up"] = "",
        ["chevron.down"] = "",
        ["chevron.up.chevron.down"] = "",
        ["xmark"] = "",
        ["checkmark"] = "",
        ["checkmark.circle.fill"] = "",
        ["globe"] = "",
        ["person"] = "",
        ["person.fill"] = "",
        ["person.crop.circle.fill"] = "",
        ["person.fill.checkmark"] = "",
        ["key"] = "",
        ["key.fill"] = "",
        ["eye"] = "",
        ["eye.slash"] = "",
        ["eye.fill"] = "",
        ["arrow.up"] = "",
        ["arrow.right"] = "",
        ["bubble.left.fill"] = "",
        ["bubble.left.and.bubble.right.fill"] = "",
        ["bubble.left.and.text.bubble.right"] = "",
        ["phone.down.fill"] = "",
        ["server.rack"] = "",
        ["arrow.clockwise"] = "",
        ["ellipsis"] = "",
        ["arrow.up.left.and.arrow.down.right"] = "",
        ["arrow.down.right.and.arrow.up.left"] = "",
        ["speaker.wave.2.fill"] = "",
        ["speaker.wave.2"] = "",
        ["speaker.slash.fill"] = "",
        ["doc.on.doc"] = "",
        ["doc.on.clipboard"] = "",
        ["trash"] = "",
        ["info.circle"] = "",
        ["info.circle.fill"] = "",
        ["exclamationmark.circle.fill"] = "",
        ["exclamationmark.circle"] = "",
        ["exclamationmark.triangle"] = "",
        ["rectangle.portrait.and.arrow.right"] = "",
        ["door.left.hand.closed"] = "",
        ["person.crop.circle.badge.xmark"] = "",
        ["person.crop.circle.badge.exclamationmark"] = "",
        ["wifi.exclamationmark"] = "",
        ["clock"] = "",
        ["lock"] = "",
        ["lock.fill"] = "",
        ["rectangle.on.rectangle"] = "",
        ["play.fill"] = "",
        ["play.rectangle.fill"] = "",
        ["sparkles"] = "",
        ["wand.and.stars"] = "",
        ["slider.horizontal.3"] = "",
        ["waveform"] = "",
        ["waveform.path"] = "",
        ["photo"] = "",
        ["hourglass"] = "",
        ["app.badge"] = "",
        ["cpu"] = "",
        ["chevron.left.forwardslash.chevron.right"] = "",
        ["arrow.up.right"] = "",
        ["number"] = "#",
        ["return"] = "",
        ["music.note"] = "",
        ["person.and.background.dotted"] = "",
        ["film"] = "",
        ["door.left.hand.open"] = "",
        ["play.rectangle"] = "",
        ["signature"] = "",
    };

    public static FrameworkElement Make(string name, double size, Brush ink)
    {
        switch (name)
        {
            case "hand.raised.fill":
            case "hand.raised":
                // Segoe Fluent Icons has no raised hand, and the emoji font
                // has no outline of one to draw in a single colour (asked
                // for one, it drew a stroke): the hand is the emoji itself,
                // as the tiles' hand mark is on every platform.
                return new TextBlock
                {
                    Text = "✋",
                    FontFamily = new FontFamily("Segoe UI Emoji"),
                    FontSize = size,
                    Foreground = ink,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                    IsTextScaleFactorEnabled = false,
                }.Quiet();
            case "video.slash.fill":
            case "video.slash":
            {
                // Nor a camera turned off: the camera, struck through.
                var grid = new Grid { Width = size * 1.15, Height = size * 1.15, IsHitTestVisible = false };
                grid.Children.Add(Glyph("", size, ink));
                grid.Children.Add(new Line
                {
                    X1 = size * 0.12, Y1 = size * 0.12, X2 = size * 1.03, Y2 = size * 1.03,
                    Stroke = ink,
                    StrokeThickness = Math.Max(1.4, size / 9),
                    StrokeStartLineCap = PenLineCap.Round,
                    StrokeEndLineCap = PenLineCap.Round,
                });
                return grid.Quiet();
            }
            case "number":
                return new TextBlock
                {
                    Text = "#",
                    FontFamily = K.Mono,
                    FontSize = size,
                    Foreground = ink,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                }.Quiet();
        }
        return Glyph(Glyphs.TryGetValue(name, out var glyph) ? glyph : "", size, ink);
    }

    private static FontIcon Glyph(string glyph, double size, Brush ink) => new FontIcon
    {
        Glyph = glyph,
        FontFamily = K.Icons,
        FontSize = size,
        Foreground = ink,
        HorizontalAlignment = HorizontalAlignment.Center,
        VerticalAlignment = VerticalAlignment.Center,
        IsTextScaleFactorEnabled = false,
    }.Quiet();
}
