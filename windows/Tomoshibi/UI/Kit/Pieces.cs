using System.Globalization;
using System.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;
using Windows.Foundation;
using Windows.UI;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The ground the apps draw wherever there is no picture (Android's
/// ui/kit/Glass.kt <c>Backdrop</c>): the theme's background, flat. It was a
/// dusk gradient under three coloured glows until the owner found the
/// gradients cheap and asked for fewer (2026-10-20); a place that laid it at
/// part opacity over something else still does.
/// </summary>
internal sealed class Backdrop : Grid
{
    public Backdrop()
    {
        IsHitTestVisible = false;
        Background = Ink.Background;
    }
}

/// <summary>
/// The product's mark (Android's ui/lobby/Pieces.kt <c>Mark</c>): four tiles
/// of a call on a dark ground, the last one lit, on a 32-unit grid.
/// </summary>
internal sealed class Mark : Canvas
{
    public Mark(double size = 20)
    {
        Width = size;
        Height = size;
        IsHitTestVisible = false;
        var unit = size / 32;
        Children.Add(new Rectangle
        {
            Width = size, Height = size, RadiusX = 8 * unit, RadiusY = 8 * unit,
            Fill = new SolidColorBrush(Color.FromArgb(255, 0x0F, 0x0E, 0x0D)),
        });
        var inner = unit * 0.86;
        var offset = (size - 32 * inner) / 2;
        var places = new[] { (6, 8), (17, 8), (6, 17), (17, 17) };
        for (var i = 0; i < places.Length; i++)
        {
            var tile = new Rectangle
            {
                Width = 9 * inner, Height = 7 * inner, RadiusX = 2 * inner, RadiusY = 2 * inner,
                Fill = new SolidColorBrush(i == 3 ? Color.FromArgb(255, 0xFF, 0xA2, 0x3B) : Color.FromArgb(255, 0x55, 0x52, 0x4E)),
            };
            SetLeft(tile, offset + places[i].Item1 * inner);
            SetTop(tile, offset + places[i].Item2 * inner);
            Children.Add(tile);
        }
        this.Quiet();
    }
}

/// <summary>The faces drawn for somebody with no picture: an initial on a colour of their own.</summary>
internal static class Faces
{
    private static readonly Color[] InitialPalette =
    {
        Color.FromArgb(255, 0x5E, 0x5C, 0xE6), Color.FromArgb(255, 0x2E, 0x8B, 0x9A), Color.FromArgb(255, 0x9A, 0x5C, 0xC8),
        Color.FromArgb(255, 0x34, 0x78, 0xF6), Color.FromArgb(255, 0xC6, 0x6B, 0x3F), Color.FromArgb(255, 0x3F, 0x98, 0x63),
    };

    private static readonly Color[] MonogramPalette =
    {
        Color.FromArgb(255, 0x8A, 0x6A, 0x45), Color.FromArgb(255, 0xA1, 0x58, 0x4A), Color.FromArgb(255, 0x5D, 0x6B, 0x58),
        Color.FromArgb(255, 0x4B, 0x5A, 0x6B), Color.FromArgb(255, 0x96, 0x81, 0x4F), Color.FromArgb(255, 0x6B, 0x55, 0x66),
    };

    private static readonly Color[] AvatarPalette =
    {
        Rgb(0.55f, 0.33f, 0.24f), Rgb(0.42f, 0.36f, 0.27f), Rgb(0.33f, 0.38f, 0.33f),
        Rgb(0.48f, 0.30f, 0.33f), Rgb(0.36f, 0.33f, 0.40f), Rgb(0.50f, 0.42f, 0.25f),
    };

    private static Color Rgb(float r, float g, float b) => Ink.Of(new Shade(r, g, b));

    /// <summary>FNV-1a over the UTF-8 bytes, 64 bits, as Notices.kt and Tile.kt hash a name to a colour.</summary>
    public static ulong Hash(string text) => Monograms.Hash(text);

    /// <summary>The first letter of the first two words, upper case; a question mark for nobody.</summary>
    public static string Initials(string name, int words = 2)
    {
        var parts = name.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries).Take(words)
            .Select(p => StringInfo.GetNextTextElement(p).ToUpperInvariant());
        var joined = string.Concat(parts);
        return joined.Length == 0 ? "?" : joined;
    }

    /// <summary>
    /// Android's kit/People.kt <c>Initial</c>: a circle in one of six colours
    /// chosen by the name's code points, its first letter in white.
    /// </summary>
    public static Grid Initial(string name, double size)
    {
        var sum = name.EnumerateRunes().Sum(r => (long)r.Value);
        var grid = new Grid { Width = size, Height = size };
        grid.Children.Add(new Ellipse { Fill = new SolidColorBrush(InitialPalette[(int)(sum % InitialPalette.Length)]) });
        grid.Children.Add(K.Text(Initials(name, 1), size * 0.42, K.Semibold, Ink.Brush(Shade.White), align: TextAlignment.Center).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        return grid.Quiet();
    }

    /// <summary>
    /// The call's monogram (kit/Notices.kt <c>Monogram</c>): a circle in one
    /// of six colours chosen by the identity (Monograms.Index, the same
    /// answers as the iPhone's and Android's), a fine edge, and up to two
    /// initials of the name. One colour, where it was a blend of two: the
    /// first of the two, so nobody changes colour.
    /// </summary>
    public static Grid Monogram(string name, string identity, double size)
    {
        var grid = new Grid { Width = size, Height = size };
        grid.Children.Add(new Ellipse { Fill = new SolidColorBrush(MonogramPalette[Monograms.Index(identity, MonogramPalette.Length)]), Stroke = Ink.White(0.14f), StrokeThickness = 0.75 });
        var initials = Initials(string.IsNullOrEmpty(name) ? identity : name);
        grid.Children.Add(K.Text(initials, size * (initials.Length > 1 ? 0.36 : 0.40), K.Semibold, Ink.White(0.94f), align: TextAlignment.Center).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        return grid.Quiet();
    }

    /// <summary>A camera that is off (Tile.kt <c>Avatar</c>): initials in a circle a third of the tile's shorter side.</summary>
    public static Color AvatarColour(string name) => AvatarPalette[(int)(Hash(name) % 6)];

    /// <summary>The account's picture over its initial, faded in once it has loaded (kit/People.kt <c>AccountAvatar</c>).</summary>
    public static Grid AccountAvatar(string name, string? address, double size)
    {
        var grid = new Grid { Width = size, Height = size };
        grid.Children.Add(Initial(name, size));
        if (address != null)
        {
            var picture = new Ellipse();
            Motion.Hide(picture);
            var image = new BitmapImage { DecodePixelWidth = (int)(size * 2), DecodePixelType = DecodePixelType.Logical };
            image.ImageOpened += (_, _) => Motion.FadeTo(picture, 1);
            image.UriSource = new Uri(address);
            picture.Fill = new ImageBrush { ImageSource = image, Stretch = Stretch.UniformToFill };
            grid.Children.Add(picture);
        }
        grid.Children.Add(new Ellipse { Stroke = Ink.White(0.12f), StrokeThickness = 1 });
        return grid.Quiet();
    }
}
