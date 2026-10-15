import Foundation

/// A language the interface is written in.
///
/// The same four as the web client's `LOCALES` (web/src/live/i18n.ts), with the
/// same codes as raw values, because the code is what is stored as a person's
/// choice and what names each column of the String Catalog. apple/i18n/generate.mjs
/// compares its own list with the web's and stops if the web gains a fifth, so
/// a language cannot arrive in one client and be silently missing from the
/// other.
public enum Language: String, CaseIterable, Identifiable, Sendable {
    case en
    case zhHans = "zh-Hans"
    case zhHant = "zh-Hant"
    case ja

    public var id: String { rawValue }

    /// What the language calls itself, as the web's `LOCALE_NAMES`.
    ///
    /// Never translated: a language picker is read by somebody who cannot yet
    /// read the language currently on screen, and has to find their own.
    public var name: String {
        switch self {
        case .en: "English"
        case .zhHans: "简体中文"
        case .zhHant: "繁體中文"
        case .ja: "日本語"
        }
    }

    /// The language as a Locale, for `.environment(\.locale, …)`.
    ///
    /// The web sets the document's `lang` when the language changes because a
    /// screen reader picks its voice from it and line breaking follows it; text
    /// in Japanese labelled as English is read aloud wrongly. The environment's
    /// locale is the nearest native equivalent.
    public var locale: Locale { Locale(identifier: rawValue) }

    /// Which language to open in, given a preference list such as
    /// `Locale.preferredLanguages`.
    ///
    /// The web's `detect()`, rule for rule. The list is read in order, because
    /// somebody who lists Japanese before English means it, and a tag in a
    /// language the interface is not written in is passed over in favour of the
    /// next rather than settling on English at once: French then Japanese opens
    /// in Japanese. Matching is by prefix, so `zh-CN`, `zh-SG` and `zh` reach
    /// simplified Chinese while `zh-TW`, `zh-HK` and `zh-MO` reach traditional.
    ///
    /// One difference, which a browser rarely meets and an Apple device meets
    /// routinely. Apple writes the script into the tag and the region after it,
    /// so somebody reading simplified Chinese with their region set to Hong
    /// Kong arrives as `zh-Hans-HK`. The web's pattern sees `hk` and opens in
    /// traditional characters, against the script the person chose. Here an
    /// explicit script subtag decides first, and the region rule only applies
    /// to a tag without one, which is every tag a browser normally sends.
    public static func detect(from preferred: [String]) -> Language {
        for tag in preferred {
            let lower = tag.lowercased()

            if lower.hasPrefix("ja") { return .ja }
            if lower.hasPrefix("zh") {
                let subtags = lower.split(whereSeparator: { $0 == "-" || $0 == "_" })
                if subtags.contains("hant") { return .zhHant }
                if subtags.contains("hans") { return .zhHans }
                let traditional = ["hant", "tw", "hk", "mo"].contains { lower.contains($0) }
                return traditional ? .zhHant : .zhHans
            }
            if lower.hasPrefix("en") { return .en }
        }

        return .en
    }
}
