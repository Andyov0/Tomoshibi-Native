import Foundation

/*
 * What the interface says, in the language of whoever is reading it.
 *
 * The web client's i18n.ts, ported rather than replaced with Apple's machinery.
 * `String(localized:)` and `LocalizedStringKey` follow the system's language
 * and nothing else, and the web lets a person choose one in the call itself; it
 * also substitutes `{name}` placeholders where Apple expects `%@`, and every
 * translation is written in the web's form. So the catalog is used as storage
 * only: the build compiles it into one table per language, the tables are read
 * once, and lookup and substitution happen here exactly as the web does them.
 *
 * Every phrase is keyed by its English. A phrase with no translation falls back
 * to that key, which is a whole readable sentence rather than a dotted path, so
 * an incomplete dictionary degrades into a mixed interface instead of a broken
 * one.
 */

/// Values a phrase may carry, by the name inside the braces.
///
/// The web takes strings and numbers. Anything with a description is accepted
/// here; an Optional is not, so a missing name has to be decided by the caller
/// rather than appearing on screen as "Optional(…)".
public typealias PhraseValues = [String: any CustomStringConvertible]

/// A piece of a phrase: literal text, or a placeholder and the name inside it.
///
/// Some phrases carry something that is not text, a name in bold or a key
/// drawn as a key. Those cannot be assembled by concatenating translated
/// fragments, because word order is the first thing a translation changes:
/// "Joining as" and "with a signature" only go back together in English, and in
/// Japanese the name has moved before both. Cutting the finished translation at
/// its placeholders lets the value land wherever that language put it.
public struct PhraseSegment: Equatable, Sendable {
    /// The literal text, or the placeholder itself, braces included.
    public var text: String
    /// The name inside the braces, when this piece is a placeholder.
    public var value: String?

    public init(text: String, value: String? = nil) {
        self.text = text
        self.value = value
    }

    /// Cut a template at its placeholders: the web's `segments()`.
    ///
    /// A placeholder is what the web's `/\{(\w+)\}/g` matches, and JavaScript's
    /// `\w` is ASCII letters, digits and the underscore only. Swift's regex
    /// `\w` also matches 名, so a translator's `{名前}` would become a
    /// placeholder here while the web shows it as written; the scan below keeps
    /// to the web's definition, and works on Unicode scalars rather than
    /// characters so that a combining mark after a brace cannot hide it.
    public static func split(_ template: String) -> [PhraseSegment] {
        var parts: [PhraseSegment] = []
        let scalars = template.unicodeScalars
        var literal = scalars.startIndex
        var cursor = scalars.startIndex

        while cursor < scalars.endIndex {
            guard scalars[cursor] == "{" else {
                cursor = scalars.index(after: cursor)
                continue
            }

            let nameStart = scalars.index(after: cursor)
            var nameEnd = nameStart
            while nameEnd < scalars.endIndex, isWord(scalars[nameEnd]) {
                nameEnd = scalars.index(after: nameEnd)
            }

            guard nameEnd > nameStart, nameEnd < scalars.endIndex, scalars[nameEnd] == "}" else {
                cursor = nameStart
                continue
            }

            let close = scalars.index(after: nameEnd)
            if literal < cursor {
                parts.append(PhraseSegment(text: String(Substring(scalars[literal..<cursor]))))
            }
            parts.append(PhraseSegment(
                text: String(Substring(scalars[cursor..<close])),
                value: String(Substring(scalars[nameStart..<nameEnd]))
            ))
            literal = close
            cursor = close
        }

        if literal < scalars.endIndex {
            parts.append(PhraseSegment(text: String(Substring(scalars[literal...]))))
        }

        return parts
    }

    private static func isWord(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar {
        case "a"..."z", "A"..."Z", "0"..."9", "_": true
        default: false
        }
    }
}

/// The compiled catalog, one table per language.
///
/// Read once, on first use, from the tables the build makes of
/// Resources/Localizable.xcstrings. A table that cannot be read is empty rather
/// than fatal, so a broken build shows English everywhere instead of not
/// starting; LocalizationTests compares every table with the catalog itself so
/// that such a build does not get that far.
enum Phrasebook {
    static let tables: [Language: [String: String]] = Dictionary(
        uniqueKeysWithValues: Language.allCases.map { ($0, load($0)) }
    )

    /// Each table again, keyed by its phrases in lower case, for `say()`.
    ///
    /// Where two phrases differ only in case ("Rooms" and "rooms" both exist),
    /// the web returns whichever its dictionary file lists first; that order is
    /// not in the catalog, so here the one that sorts first wins. Both are
    /// reached exactly before folding is tried, so this only decides text typed
    /// in a third spelling, and deployments type place names, not these.
    static let folded: [Language: [String: String]] = tables.mapValues { table in
        var folded: [String: String] = [:]
        for key in table.keys.sorted() where folded[key.lowercased()] == nil {
            folded[key.lowercased()] = table[key]
        }
        return folded
    }

    static func template(_ phrase: String, in language: Language) -> String {
        tables[language]?[phrase] ?? phrase
    }

    private static func load(_ language: Language) -> [String: String] {
        guard
            let url = Bundle.module.url(
                forResource: "Localizable",
                withExtension: "strings",
                subdirectory: nil,
                localization: language.rawValue
            ),
            let data = try? Data(contentsOf: url),
            let table = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: String]
        else { return [:] }
        return table
    }
}

/// Put values where a template asks for them: the web's `t()` after lookup.
///
/// With no values at all the template is returned untouched, braces and all,
/// as the web does. With values, a placeholder whose value is missing becomes
/// nothing rather than staying as `{name}`, and a value nobody asked for is
/// ignored.
func substitute(_ template: String, _ values: PhraseValues?) -> String {
    guard let values else { return template }

    return PhraseSegment.split(template)
        .map { part in
            guard let name = part.value else { return part.text }
            return values[name].map(rendered) ?? ""
        }
        .joined()
}

/// A value as the web's `String(value)` writes it.
///
/// The difference that shows is whole numbers held as floating point: Swift
/// writes 50.0 where JavaScript, which has only the one kind of number, writes
/// 50, so "{percent}%" would read "50.0%" here and "50%" in a browser beside it.
private func rendered(_ value: any CustomStringConvertible) -> String {
    if let number = value as? any BinaryFloatingPoint {
        let double = Double(number)
        // Bounded at 2^53, where doubles stop holding every integer and the
        // conversion below could trap; nothing an interface counts gets near
        // it, and anything that does keeps Swift's own spelling.
        if double.rounded() == double, abs(double) < 9_007_199_254_740_992 {
            return String(Int64(double))
        }
    }
    return value.description
}
