using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Tomoshibi.Basics;

/// <summary>
/// A language the interface is written in: the web client's four
/// (web/src/live/i18n.ts <c>LOCALES</c>), with the codes as stored values, as
/// on Apple (Language.swift) and Android (basics/Localization.kt).
/// </summary>
public enum Language
{
    En,
    ZhHans,
    ZhHant,
    Ja,
}

public static class Languages
{
    public static readonly IReadOnlyList<Language> All = new[] { Language.En, Language.ZhHans, Language.ZhHant, Language.Ja };

    public static string Code(this Language language) => language switch
    {
        Language.En => "en",
        Language.ZhHans => "zh-Hans",
        Language.ZhHant => "zh-Hant",
        Language.Ja => "ja",
        _ => "en",
    };

    /// <summary>
    /// What the language calls itself. Never translated: a language picker is
    /// read by somebody who cannot yet read the language on screen.
    /// </summary>
    public static string DisplayName(this Language language) => language switch
    {
        Language.En => "English",
        Language.ZhHans => "简体中文",
        Language.ZhHant => "繁體中文",
        Language.Ja => "日本語",
        _ => "English",
    };

    public static CultureInfo Culture(this Language language) => CultureInfo.GetCultureInfo(language.Code());

    public static Language? Of(string? code) => All.Cast<Language?>().FirstOrDefault(l => l!.Value.Code() == code);

    /// <summary>
    /// Which language to open in, given the system's preference list: the
    /// web's <c>detect()</c> rule for rule, read in order, with an explicit
    /// script subtag deciding before a region (Language.swift says why:
    /// simplified Chinese with the region set to Hong Kong arrives as
    /// <c>zh-Hans-HK</c>, which the web's region rule would open in
    /// traditional).
    /// </summary>
    public static Language Detect(IEnumerable<string> preferred)
    {
        foreach (var tag in preferred)
        {
            var lower = tag.ToLowerInvariant();
            if (lower.StartsWith("ja")) return Language.Ja;
            if (lower.StartsWith("zh"))
            {
                var subtags = lower.Split('-', '_');
                if (subtags.Contains("hant")) return Language.ZhHant;
                if (subtags.Contains("hans")) return Language.ZhHans;
                var traditional = new[] { "hant", "tw", "hk", "mo" }.Any(lower.Contains);
                return traditional ? Language.ZhHant : Language.ZhHans;
            }
            if (lower.StartsWith("en")) return Language.En;
        }
        return Language.En;
    }
}

/// <summary>
/// What the interface says, in the language of whoever is reading it: the
/// web's i18n.ts, as on Apple (Phrases.swift) and Android, rather than
/// Windows' resource files, which follow the system's language alone while
/// the web lets a person choose one in the call itself, and expect
/// positional arguments where every translation is written with
/// <c>{name}</c>.
///
/// Every phrase is keyed by its English, from the same catalog the Apple
/// apps build from (apple/i18n/generate.mjs, then android/i18n.mjs, which
/// writes the phrases.json this reads). A phrase with no translation falls
/// back to its key, a whole readable sentence.
/// </summary>
public sealed class Localization
{
    /// <summary>The web's own storage name for the choice.</summary>
    public const string StorageKey = "meet-live.locale";

    private readonly IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> _tables;
    private readonly IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> _folded;
    private readonly IDefaults? _defaults;
    private readonly Func<IEnumerable<string>> _system;

    /// <summary>The language on screen changed; everything that said something says it again.</summary>
    public event Action? Changed;

    public Localization(
        IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> tables,
        IDefaults? defaults,
        Func<IEnumerable<string>> system)
    {
        _tables = tables;
        _defaults = defaults;
        _system = system;
        _folded = tables.ToDictionary(pair => pair.Key, pair =>
        {
            var folded = new Dictionary<string, string>();
            foreach (var key in pair.Value.Keys.OrderBy(k => k, StringComparer.Ordinal))
            {
                folded.TryAdd(key.ToLowerInvariant(), pair.Value[key]);
            }
            return (IReadOnlyDictionary<string, string>)folded;
        });
        var kept = Languages.Of(defaults?.String(StorageKey));
        Chosen = kept != null;
        Language = kept ?? Languages.Detect(system());
    }

    /// <summary>The app's, once the app has made one; English keys before that, and in the tests.</summary>
    public static Localization Shared { get; set; } = new(
        new Dictionary<Language, IReadOnlyDictionary<string, string>>(), null, () => new[] { "en" });

    /// <summary>For tests: the tables as given, the language fixed, nothing kept.</summary>
    public static Localization ForTesting(IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> tables, Language language)
    {
        var made = new Localization(tables, null, () => new[] { language.Code() });
        made.Language = language;
        made.Chosen = true;
        return made;
    }

    /// <summary>Whether the person chose a language rather than following the system's.</summary>
    public bool Chosen { get; private set; }

    public bool FollowsSystem => !Chosen;

    /// <summary>The language on screen.</summary>
    public Language Language { get; private set; }

    /// <summary>
    /// Whether a choice is written down: a test driving the app shows its
    /// choice while it runs and keeps none, since what it would write over is
    /// the owner's.
    /// </summary>
    public bool Keeps { get; set; } = true;

    public void Choose(Language language)
    {
        if (Keeps) _defaults?.Set(StorageKey, language.Code());
        Chosen = true;
        Set(language);
    }

    public void FollowSystem()
    {
        if (Keeps) _defaults?.Remove(StorageKey);
        Chosen = false;
        Set(Languages.Detect(_system()));
    }

    /// <summary>The system's languages changed; followed unless one was chosen.</summary>
    public void SystemChanged()
    {
        if (!Chosen) Set(Languages.Detect(_system()));
    }

    private void Set(Language language)
    {
        var changed = language != Language;
        Language = language;
        if (changed) Changed?.Invoke();
    }

    public string Template(string phrase) =>
        _tables.TryGetValue(Language, out var table) && table.TryGetValue(phrase, out var said) ? said : phrase;

    public string T(string phrase, IReadOnlyDictionary<string, object?>? values = null) => Substitute(Template(phrase), values);

    /// <summary>
    /// Text the deployment supplies rather than the interface, such as a
    /// relay's label: translated if it matches a phrase, exactly or ignoring
    /// case, and shown as typed otherwise.
    /// </summary>
    public string Say(string text)
    {
        if (_tables.TryGetValue(Language, out var table) && table.TryGetValue(text, out var exact)) return exact;
        if (_folded.TryGetValue(Language, out var folded) && folded.TryGetValue(text.ToLowerInvariant(), out var loose)) return loose;
        return text;
    }

    public IReadOnlyList<PhraseSegment> Segments(string phrase) => PhraseSegment.Split(Template(phrase));

    /// <summary>The phrase table as android/i18n.mjs writes it: one object per language code, keyed by the English.</summary>
    public static IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> Parse(string json)
    {
        using var document = JsonDocument.Parse(json);
        var tables = new Dictionary<Language, IReadOnlyDictionary<string, string>>();
        foreach (var language in Languages.All)
        {
            var table = new Dictionary<string, string>();
            if (document.RootElement.TryGetProperty(language.Code(), out var entries))
            {
                foreach (var entry in entries.EnumerateObject()) table[entry.Name] = entry.Value.GetString() ?? entry.Name;
            }
            tables[language] = table;
        }
        return tables;
    }

    /// <summary>
    /// <c>{name}</c> replaced by its value; a placeholder with no value becomes
    /// nothing, as on the web. Whole doubles are written without a point, as
    /// Swift's <c>rendered</c> does, so a count reads "3" rather than "3.0".
    /// </summary>
    public static string Substitute(string template, IReadOnlyDictionary<string, object?>? values)
    {
        if (values == null) return template;
        var built = new StringBuilder();
        foreach (var part in PhraseSegment.Split(template))
        {
            if (part.Value is not { } name)
            {
                built.Append(part.Text);
                continue;
            }
            built.Append(Rendered(values.TryGetValue(name, out var value) ? value : null));
        }
        return built.ToString();
    }

    private static string Rendered(object? value) => value switch
    {
        null => "",
        double d when d == Math.Round(d) && Math.Abs(d) < 9.007199254740992E15 => ((long)d).ToString(CultureInfo.InvariantCulture),
        double d => d.ToString(CultureInfo.InvariantCulture),
        float f => Rendered((double)f),
        IFormattable formattable => formattable.ToString(null, CultureInfo.InvariantCulture),
        _ => value.ToString() ?? "",
    };
}

/// <summary>A run of a template: literal text, or a <c>{placeholder}</c> and its name.</summary>
public sealed record PhraseSegment(string Text, string? Value = null)
{
    public static IReadOnlyList<PhraseSegment> Split(string template)
    {
        var parts = new List<PhraseSegment>();
        var literal = 0;
        var cursor = 0;
        while (cursor < template.Length)
        {
            if (template[cursor] != '{')
            {
                cursor++;
                continue;
            }
            var nameStart = cursor + 1;
            var nameEnd = nameStart;
            while (nameEnd < template.Length && IsWordChar(template[nameEnd])) nameEnd++;
            if (nameEnd == nameStart || nameEnd >= template.Length || template[nameEnd] != '}')
            {
                cursor = nameStart;
                continue;
            }
            if (literal < cursor) parts.Add(new PhraseSegment(template[literal..cursor]));
            parts.Add(new PhraseSegment(template[cursor..(nameEnd + 1)], template[nameStart..nameEnd]));
            literal = nameEnd + 1;
            cursor = literal;
        }
        if (literal < template.Length) parts.Add(new PhraseSegment(template[literal..]));
        return parts;
    }

    private static bool IsWordChar(char c) => c is >= 'a' and <= 'z' or >= 'A' and <= 'Z' or >= '0' and <= '9' or '_';
}

/// <summary>Everything the interface says goes through these, keyed by its English.</summary>
public static class Words
{
    public static string T(string phrase) => Localization.Shared.T(phrase);

    public static string T(string phrase, params (string Name, object? Value)[] values) =>
        Localization.Shared.T(phrase, values.ToDictionary(v => v.Name, v => v.Value));

    /// <summary>Text from the deployment: translated where it matches a phrase.</summary>
    public static string Say(string text) => Localization.Shared.Say(text);

    /// <summary>
    /// The culture of the language on screen, for writing dates and numbers
    /// in it: the web sets the document's <c>lang</c> as the language
    /// changes, and a time written in English inside a Japanese sentence
    /// reads wrongly.
    /// </summary>
    public static CultureInfo Reading => Localization.Shared.Language.Culture();
}
