using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>The web's language rules and placeholder substitution, as Apple and Android keep them.</summary>
public class LocalizationTests
{
    [Fact]
    public void AScriptSubtagDecidesBeforeTheRegion()
    {
        // Simplified Chinese read in Hong Kong: the web's region rule alone
        // would open in traditional characters, against the script chosen.
        Assert.Equal(Language.ZhHans, Languages.Detect(new[] { "zh-Hans-HK" }));
        Assert.Equal(Language.ZhHant, Languages.Detect(new[] { "zh-Hant-CN" }));
        Assert.Equal(Language.ZhHant, Languages.Detect(new[] { "zh-TW" }));
        Assert.Equal(Language.ZhHans, Languages.Detect(new[] { "zh-CN" }));
    }

    [Fact]
    public void TheListIsReadInOrderPastLanguagesNotOffered()
    {
        Assert.Equal(Language.Ja, Languages.Detect(new[] { "fr-FR", "ja-JP", "en-US" }));
        Assert.Equal(Language.En, Languages.Detect(new[] { "de-DE" }));
    }

    [Fact]
    public void PlaceholdersAreFilledAndMissingOnesBecomeNothing()
    {
        Assert.Equal("Show Ann larger", Localization.Substitute("Show {name} larger", new Dictionary<string, object?> { ["name"] = "Ann" }));
        Assert.Equal("Show  larger", Localization.Substitute("Show {name} larger", new Dictionary<string, object?> { ["other"] = "x" }));
        Assert.Equal("3 people", Localization.Substitute("{count} people", new Dictionary<string, object?> { ["count"] = 3.0 }));
        // A brace that does not close a word is text.
        Assert.Equal("{not a name}", Localization.Substitute("{not a name}", new Dictionary<string, object?> { ["not"] = "x" }));
    }

    [Fact]
    public void AMissingTranslationFallsBackToTheEnglish()
    {
        var words = Localization.ForTesting(
            new Dictionary<Language, IReadOnlyDictionary<string, string>> { [Language.Ja] = new Dictionary<string, string> { ["Leave"] = "退出" } },
            Language.Ja);
        Assert.Equal("退出", words.T("Leave"));
        Assert.Equal("Stay", words.T("Stay"));
        Assert.Equal("退出", words.Say("leave"));
    }

    // What the Windows app adds: the choice is kept under the web's name,
    // and following the system again forgets it.

    [Fact]
    public void AChoiceIsKeptUnderTheWebsNameAndForgottenOnFollowingTheSystem()
    {
        var defaults = new MemoryDefaults();
        var words = new Localization(new Dictionary<Language, IReadOnlyDictionary<string, string>>(), defaults, () => new[] { "zh-CN" });
        Assert.Equal(Language.ZhHans, words.Language);
        Assert.True(words.FollowsSystem);
        var changes = 0;
        words.Changed += () => changes++;
        words.Choose(Language.Ja);
        Assert.Equal("ja", defaults.String(Localization.StorageKey));
        Assert.Equal(Language.Ja, words.Language);
        Assert.Equal(1, changes);
        words.FollowSystem();
        Assert.False(defaults.Has(Localization.StorageKey));
        Assert.Equal(Language.ZhHans, words.Language);
        Assert.Equal(2, changes);
        // Opened again, a kept choice wins over the system's language.
        defaults.Set(Localization.StorageKey, "en");
        Assert.Equal(Language.En, new Localization(new Dictionary<Language, IReadOnlyDictionary<string, string>>(), defaults, () => new[] { "zh-CN" }).Language);
    }

    [Fact]
    public void ATestKeepsNoChoice()
    {
        var defaults = new MemoryDefaults();
        var words = new Localization(new Dictionary<Language, IReadOnlyDictionary<string, string>>(), defaults, () => new[] { "en" }) { Keeps = false };
        words.Choose(Language.ZhHans);
        Assert.Equal(Language.ZhHans, words.Language);
        Assert.False(defaults.Has(Localization.StorageKey));
    }
}
