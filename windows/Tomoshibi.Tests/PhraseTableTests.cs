using System.Text.Json;
using System.Text.RegularExpressions;
using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The phrases the Windows app says are the other apps' phrases. Every
/// <c>T("…")</c> in the Windows sources must be a key of the catalog the
/// Apple apps build from (apple/i18n/generate.mjs), which is where the web's
/// dictionaries and native.json meet; in C#, as in Swift, <c>T()</c> takes a
/// string, so a phrase misspelt, reworded or missing from native.json would
/// otherwise show English to everybody, which is what the Apple apps'
/// LocalizationCatalogTests catch there.
/// </summary>
public class PhraseTableTests
{
    private static string Root()
    {
        var at = new DirectoryInfo(AppContext.BaseDirectory);
        while (at != null && !File.Exists(Path.Combine(at.FullName, "apple", "i18n", "native.json"))) at = at.Parent;
        return at?.FullName ?? throw new InvalidOperationException("the repository is not above " + AppContext.BaseDirectory);
    }

    private static HashSet<string> CatalogKeys()
    {
        var path = Path.Combine(Root(), "apple", "TomoshibiKit", "Sources", "TomoshibiBasics", "Resources", "Localizable.xcstrings");
        using var catalog = JsonDocument.Parse(File.ReadAllText(path));
        return catalog.RootElement.GetProperty("strings").EnumerateObject().Select(p => p.Name).ToHashSet(StringComparer.Ordinal);
    }

    private static readonly Regex Said = new(@"(?<![A-Za-z0-9_])T\(\s*""((?:[^""\\]|\\.)*)""", RegexOptions.Compiled);
    private static readonly Regex Interpolated = new(@"(?<![A-Za-z0-9_])T\(\s*\$", RegexOptions.Compiled);

    private static string Unescape(string literal) =>
        Regex.Replace(literal, @"\\(u[0-9A-Fa-f]{4}|.)", m => m.Groups[1].Value switch
        {
            "n" => "\n",
            "t" => "\t",
            "\\" => "\\",
            "\"" => "\"",
            var u when u.StartsWith('u') => ((char)Convert.ToInt32(u[1..], 16)).ToString(),
            var other => other,
        });

    private static IEnumerable<string> Sources()
    {
        var windows = Path.Combine(Root(), "windows");
        foreach (var project in new[] { "Tomoshibi", "Tomoshibi.Basics" })
        {
            var folder = Path.Combine(windows, project);
            foreach (var file in Directory.EnumerateFiles(folder, "*.cs", SearchOption.AllDirectories))
            {
                var relative = Path.GetRelativePath(folder, file);
                // Generated bindings and build output are not the app's words.
                if (relative.StartsWith("Generated") || relative.StartsWith("obj") || relative.StartsWith("bin")) continue;
                yield return file;
            }
        }
    }

    [Fact]
    public void EveryPhraseTheAppSaysIsInTheCatalog()
    {
        var keys = CatalogKeys();
        var missing = new List<string>();
        var count = 0;
        foreach (var file in Sources())
        {
            var text = File.ReadAllText(file);
            foreach (Match match in Said.Matches(text))
            {
                count++;
                var phrase = Unescape(match.Groups[1].Value);
                if (!keys.Contains(phrase)) missing.Add($"{Path.GetFileName(file)}: \"{phrase}\"");
            }
            foreach (Match match in Interpolated.Matches(text)) missing.Add($"{Path.GetFileName(file)}: an interpolated phrase at {match.Index}");
        }
        Assert.True(missing.Count == 0, "not in the catalog:\n" + string.Join("\n", missing));
        // A pattern that matched nothing would pass whatever the sources said.
        Assert.True(count > 50, $"only {count} phrases found in the Windows sources");
    }

    [Fact]
    public void TheTableTheAppReadsHasTheFourLanguagesAndTheirPlaceholders()
    {
        var path = Path.Combine(Root(), "android", "app", "src", "main", "assets", "phrases.json");
        var tables = Localization.Parse(File.ReadAllText(path));
        Assert.Equal(Languages.All.ToHashSet(), tables.Keys.ToHashSet());
        Assert.True(tables[Language.ZhHans].Count > 500);
        // A placeholder translated with its sentence would put braces on the screen.
        foreach (var (language, table) in tables)
        {
            foreach (var (phrase, said) in table)
            {
                var expected = PhraseSegment.Split(phrase).Select(s => s.Value).Where(v => v != null).OrderBy(v => v);
                var found = PhraseSegment.Split(said).Select(s => s.Value).Where(v => v != null).OrderBy(v => v);
                Assert.True(expected.SequenceEqual(found), $"{language.Code()}: {phrase} -> {said}");
            }
        }
        var words = Localization.ForTesting(tables, Language.ZhHans);
        Assert.NotEqual("Join", words.T("Join"));
    }
}
