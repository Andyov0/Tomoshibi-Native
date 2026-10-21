using System.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// Flags, drawn rather than typed (Flag.tsx on the web, which does the same).
///
/// The relays' labels lean on flag emoji, and Windows has none: Segoe UI
/// Emoji has no glyph for a pair of regional indicators, so the pair fell back
/// to what it literally is and "🇨🇳Shanghai CT" read "CNShanghai CT" in the
/// server menu, the list of servers and a call's connection details. The
/// pairs are drawn with Twemoji's artwork, the web's own files, shipped with
/// the app (Assets\Flags) rather than fetched, so that a flag is there with
/// no network to ask. The text either side is left as it was, the missing
/// space after the flag included: that is how the label was typed.
///
/// A pair, never one alone: one is a letter somebody may have typed; two
/// together are a country. A pair the app has no picture for stays as it was.
/// </summary>
internal static class Flags
{
    private static readonly string Folder = Path.Combine(AppContext.BaseDirectory, "Assets", "Flags");
    private static readonly Dictionary<string, SvgImageSource?> Pictures = new();

    private static bool Indicator(Rune rune) => rune.Value is >= 0x1F1E6 and <= 0x1F1FF;

    /// <summary>Whether the text has a flag in it to draw.</summary>
    public static bool In(string text)
    {
        var previous = false;
        foreach (var rune in text.EnumerateRunes())
        {
            var now = Indicator(rune);
            if (now && previous) return true;
            previous = now;
        }
        return false;
    }

    /// <summary>
    /// The text as a line with its flags drawn in (K.Words): its runs of text,
    /// each as <paramref name="words"/> draws it, and each pair as its picture
    /// between them, as tall as the text and a third again as wide, which a
    /// TextBlock cannot hold (an InlineUIContainer is the RichTextBlock's).
    /// The last run is cut short where the line has no room for all of it.
    /// The pictures say nothing to a screen reader, since the name beside each
    /// already says the place, and read aloud it would be the country twice.
    /// </summary>
    public static FrameworkElement Line(string text, double size, Func<string, TextBlock> words)
    {
        var line = new Grid();
        var parts = Parts(text);
        var last = parts.FindLastIndex(part => part.Words != null);
        for (var i = 0; i < parts.Count; i++)
        {
            line.ColumnDefinitions.Add(new ColumnDefinition { Width = i == last ? new GridLength(1, GridUnitType.Star) : GridLength.Auto });
            FrameworkElement piece;
            if (parts[i].Words is { } run)
            {
                piece = words(run);
            }
            else
            {
                piece = new Image
                {
                    Source = parts[i].Picture,
                    Width = size * 1.3,
                    Height = size,
                    Stretch = Stretch.Uniform,
                    VerticalAlignment = VerticalAlignment.Center,
                    IsHitTestVisible = false,
                };
                AutomationProperties.SetAccessibilityView(piece, AccessibilityView.Raw);
            }
            Grid.SetColumn(piece, i);
            line.Children.Add(piece);
        }
        return line;
    }

    /// <summary>The text as runs of words and pictures, a pair with no picture left as it was typed.</summary>
    private static List<(string? Words, SvgImageSource? Picture)> Parts(string text)
    {
        var parts = new List<(string? Words, SvgImageSource? Picture)>();
        var words = new StringBuilder();
        var runes = text.EnumerateRunes().ToList();
        for (var i = 0; i < runes.Count; i++)
        {
            if (i + 1 < runes.Count && Indicator(runes[i]) && Indicator(runes[i + 1])
                && Picture(Code(runes[i], runes[i + 1])) is { } picture)
            {
                if (words.Length > 0) parts.Add((words.ToString(), null));
                words.Clear();
                parts.Add((null, picture));
                i++;
                continue;
            }
            words.Append(runes[i].ToString());
        }
        if (words.Length > 0) parts.Add((words.ToString(), null));
        return parts;
    }

    /// <summary>The text with its flags taken out, as a screen reader is to read it.</summary>
    public static string Plain(string text)
    {
        var plain = new StringBuilder();
        var runes = text.EnumerateRunes().ToList();
        for (var i = 0; i < runes.Count; i++)
        {
            if (i + 1 < runes.Count && Indicator(runes[i]) && Indicator(runes[i + 1])) { i++; continue; }
            plain.Append(runes[i].ToString());
        }
        return plain.ToString().Trim();
    }

    /// <summary>The two letters a pair stands for, lower case, as the files are named.</summary>
    private static string Code(Rune first, Rune second) =>
        new string(new[] { (char)('a' + first.Value - 0x1F1E6), (char)('a' + second.Value - 0x1F1E6) });

    /// <summary>The picture for a country, read once and shared; none where the app has no file for it.</summary>
    private static SvgImageSource? Picture(string code)
    {
        if (Pictures.TryGetValue(code, out var known)) return known;
        var file = Path.Combine(Folder, code + ".svg");
        var picture = File.Exists(file) ? new SvgImageSource(new Uri(file)) : null;
        Pictures[code] = picture;
        return picture;
    }
}
