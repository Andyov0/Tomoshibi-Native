using System.Text.RegularExpressions;
using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The settings have the other apps' sections, in their order: Windows had
/// four of the six, without Watch together and Music.
/// </summary>
public class SettingsTopicsTests
{
    private static string Root()
    {
        var at = new DirectoryInfo(AppContext.BaseDirectory);
        while (at != null && !File.Exists(Path.Combine(at.FullName, "apple", "i18n", "native.json"))) at = at.Parent;
        return at?.FullName ?? throw new InvalidOperationException("the repository is not above " + AppContext.BaseDirectory);
    }

    [Fact]
    public void TheSettingsHaveTheOtherAppsSectionsInTheirOrder()
    {
        static List<string> Entries(string source, string pattern)
        {
            var body = Regex.Match(source, pattern, RegexOptions.Singleline);
            Assert.True(body.Success, pattern);
            return Regex.Matches(body.Groups[1].Value, @"\b([A-Z]\w*)\b(?:\(""[^""]*""\))?").Select(m => m.Groups[1].Value).ToList();
        }
        var android = Entries(File.ReadAllText(Path.Combine(Root(), "android", "app", "src", "main", "kotlin", "app", "tomoshibi", "ui", "settings", "SettingsView.kt")),
            @"enum class SettingsTopic\(val symbol: String\) \{(.*?);");
        var windows = Entries(File.ReadAllText(Path.Combine(Root(), "windows", "Tomoshibi", "UI", "Settings", "SettingsView.cs")),
            @"public enum Topic \{(.*?)\}");
        Assert.Equal(new[] { "General", "Media", "Watching", "Music", "Account", "About" }, android);
        Assert.Equal(android, windows);
        // And the watch ceiling is written by them, which nothing did.
        var pages = File.ReadAllText(Path.Combine(Root(), "windows", "Tomoshibi", "UI", "Settings", "CallSettings.cs"));
        Assert.Contains(".Remember(defaults)", pages);
    }
}
