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
/// camera turned off) the nearest is drawn with what it lacks, a slash
/// across the camera; where it has nothing near (a crown, a raised hand),
/// the symbol is drawn as a path, in the ink as every glyph is.
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
        ["megaphone.fill"] = "",
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
        ["rectangle.inset.filled.on.rectangle"] = "\uE7F4",
        ["rectangle.inset.filled.and.person.filled"] = "\uE7F4",
        ["waveform.circle.fill"] = "\uE8D6",
        ["stop.circle"] = "\uE71A",
        ["hifispeaker"] = "\uE7F5",
        ["person.badge.minus"] = "\uE8F8",
        ["gauge.with.dots.needle.33percent"] = "\uEB5E",
        ["arrow.down.right.and.arrow.up.left.rectangle"] = "\uE73F",
        ["arrow.up.left.and.arrow.down.right.rectangle"] = "\uE740",
        ["music.note.list"] = "\uE8D6",
        ["forward.fill"] = "\uE893",
        ["pause.fill"] = "\uE769",
        ["plus"] = "\uE710",
        ["magnifyingglass"] = "\uE721",
        ["arrow.uturn.backward"] = "\uE72B",
        ["folder"] = "\uE8B7",
        // Watching together's (WatchStage.kt, MediaBrowser.kt): the nearest
        // Segoe has to each, a globe for a video the connection cannot reach.
        ["folder.fill"] = "\uE8B7",
        ["play.circle.fill"] = "\uE768",
        ["forward.end.fill"] = "\uE893",
        ["list.bullet.rectangle"] = "\uE8FD",
        ["film.stack"] = "\uE8B2",
        ["icloud.slash"] = "\uE774",
        ["arrow.up.right.square"] = "\uE8A7",
        ["externaldrive.badge.exclamationmark"] = "\uE7BA",
        // The song desk's (MusicPanel.kt): a person for "lower the music while
        // anybody talks", upload's arrow to a line for "play next".
        ["person.wave.2.fill"] = "\uE77B",
        ["arrow.up.to.line"] = "\uE898",
        ["music.magnifyingglass"] = "\uE721",
        ["plus.circle.fill"] = "\uECC8",
        ["tv"] = "\uE7F4",
        // A film's sharpness (the watch settings' picture quality): the
        // monitor, as for the screen.
        ["sparkles.tv"] = "\uE7F4",
        ["antenna.radiowaves.left.and.right"] = "\uEC05",
        ["hand.thumbsup.fill"] = "\uE8E1",
        ["xmark.circle.fill"] = "\uEB90",
        ["speaker.wave.1.fill"] = "\uE993",
        ["square.and.arrow.up"] = "\uE72D",
        ["signature"] = "",
    };

    /// <summary>Material's rounded back_hand, the raised hand Android draws (Icons.Rounded.BackHand), as path markup on its 24-unit grid.</summary>
    private const string Hand = "M21,16 c0,4.42 -3.58,8 -8,8 c-3.26,0 -6.19,-1.99 -7.4,-5.02 l-3.03,-7.61 C2.26,10.58 3,9.79 3.81,10.05 l0.79,0.26 c0.56,0.18 1.02,0.61 1.24,1.16 l1.28,3.21 C7.2,14.88 7.38,15 7.58,15 H8 V3.25 C8,2.56 8.56,2 9.25,2 s1.25,0.56 1.25,1.25 v8.25 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V1.25 C11.5,0.56 12.06,0 12.75,0 S14,0.56 14,1.25 V11.5 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V2.75 c0,-0.69 0.56,-1.25 1.25,-1.25 c0.69,0 1.25,0.56 1.25,1.25 v8.75 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V5.75 c0,-0.69 0.56,-1.25 1.25,-1.25 S21,5.06 21,5.75 V16 Z";

    public static FrameworkElement Make(string name, double size, Brush ink)
    {
        switch (name)
        {
            case "hand.raised.fill":
            case "hand.raised":
            {
                // Nor a raised hand. It was the emoji, in its own yellow,
                // the one coloured thing on a bar of single-colour glyphs,
                // and it stayed yellow whatever the bar's state said; the
                // emoji font has no outline of one to draw in the ink. So
                // it is drawn: the hand Android's bar shows, Material's
                // rounded back_hand (Apache 2.0), on its 24-unit grid, in
                // the ink like every other symbol. The tiles' hand mark is
                // still the emoji, as it is on every platform and the web.
                var filled = name == "hand.raised.fill";
                var hand = new Microsoft.UI.Xaml.Shapes.Path
                {
                    Data = (Geometry)Microsoft.UI.Xaml.Markup.XamlBindingHelper.ConvertValue(typeof(Geometry), Hand),
                    Fill = filled ? ink : null,
                    Stroke = filled ? null : ink,
                    StrokeThickness = filled ? 0 : 1.3,
                    StrokeLineJoin = PenLineJoin.Round,
                    Stretch = Stretch.Uniform,
                    Width = size * 0.9,
                    Height = size * 1.12,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                };
                return hand.Quiet();
            }
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
            case "crown":
            case "crown.fill":
            {
                // Nor a crown, which is who runs the room on every
                // platform: three points on a band, drawn.
                var crown = new Microsoft.UI.Xaml.Shapes.Path
                {
                    Data = (Geometry)Microsoft.UI.Xaml.Markup.XamlBindingHelper.ConvertValue(typeof(Geometry), "M 1,9 L 2,3 L 5,6 L 8,1 L 11,6 L 14,3 L 15,9 Z M 2,10.5 L 14,10.5 L 14,12.5 L 2,12.5 Z"),
                    Fill = name == "crown.fill" ? ink : null,
                    Stroke = ink,
                    StrokeThickness = name == "crown.fill" ? 0 : 1.3,
                    StrokeLineJoin = PenLineJoin.Round,
                    Stretch = Stretch.Uniform,
                    Width = size * 1.05,
                    Height = size * 0.9,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                };
                return crown.Quiet();
            }
            case "arrow.right.to.line":
            {
                // A way in, for joining a meeting: an arrow going in at a
                // door, the web's sign-in symbol (lucide's log-in, ISC), on
                // its 24-unit grid. Segoe has no such glyph; its nearest were
                // a link, which says "copy this" more than "come in", and the
                // eye the web had, which is the passphrase's.
                var way = new Microsoft.UI.Xaml.Shapes.Path
                {
                    Data = (Geometry)Microsoft.UI.Xaml.Markup.XamlBindingHelper.ConvertValue(typeof(Geometry), "M 15,3 L 19,3 A 2,2 0 0 1 21,5 L 21,19 A 2,2 0 0 1 19,21 L 15,21 M 10,17 L 15,12 L 10,7 M 15,12 L 3,12"),
                    Stroke = ink,
                    StrokeThickness = Math.Max(1.2, size / 11),
                    StrokeLineJoin = PenLineJoin.Round,
                    StrokeStartLineCap = PenLineCap.Round,
                    StrokeEndLineCap = PenLineCap.Round,
                    Stretch = Stretch.Uniform,
                    Width = size * 0.92,
                    Height = size * 0.92,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                };
                return way.Quiet();
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
