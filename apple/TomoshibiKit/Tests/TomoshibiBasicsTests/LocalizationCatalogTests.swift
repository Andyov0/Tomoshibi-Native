import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * What these guard is the failure a translated interface has that an
 * untranslated one does not: words that quietly stop agreeing with each other.
 *
 * A phrase missing from one language shows English to a reader of that
 * language and fails nothing. A placeholder translated along with the sentence
 * around it, `{name}` becoming `{名前}`, puts the braces on screen in a language
 * whoever checks the build may not read. And a phrase written straight into a
 * view, or misspelt in a call to t(), is English for everybody, because t()
 * takes a string and has nothing to check it against. These are the web's
 * dictionary tests, made against the catalog the apps build from and against
 * the tables the build makes of it.
 */
final class LocalizationCatalogTests: XCTestCase {
    /// apple/, found from this file so that the tests read the repository and
    /// not whatever the build copied.
    private static let apple = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // TomoshibiBasicsTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // TomoshibiKit
        .deletingLastPathComponent() // apple

    private static let catalogURL = apple
        .appendingPathComponent("TomoshibiKit/Sources/TomoshibiBasics/Resources/Localizable.xcstrings")

    /// The catalog as written: phrase, then language code, then value.
    private func catalog() throws -> [String: [String: String]] {
        let data = try Data(contentsOf: Self.catalogURL)
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(root["sourceLanguage"] as? String, "en")
        let strings = try XCTUnwrap(root["strings"] as? [String: [String: Any]])

        return strings.mapValues { entry in
            let localizations = entry["localizations"] as? [String: [String: Any]] ?? [:]
            return localizations.compactMapValues { localization in
                (localization["stringUnit"] as? [String: Any])?["value"] as? String
            }
        }
    }

    /// The names inside the braces of a phrase, sorted, as the web's test has it.
    private func placeholders(_ template: String) -> [String] {
        PhraseSegment.split(template).compactMap(\.value).sorted()
    }

    // MARK: The catalog

    func testTheCatalogSaysEveryPhraseInEveryLanguageAndNothingElse() throws {
        let phrases = try catalog()
        XCTAssertGreaterThan(phrases.count, 0)

        let codes = Set(Language.allCases.map(\.rawValue))
        var problems: [String] = []
        for (phrase, translations) in phrases.sorted(by: { $0.key < $1.key }) {
            for missing in codes.subtracting(translations.keys).sorted() {
                problems.append("\(missing) is missing \(phrase.debugDescription)")
            }
            for extra in Set(translations.keys).subtracting(codes).sorted() {
                problems.append("\(phrase.debugDescription) is said in \(extra), which the apps do not speak")
            }
        }
        XCTAssertEqual(problems, [])
    }

    /*
     * The build compiles the catalog into one table per language, and t() reads
     * those. A table the build dropped, or a phrase it rewrote (a `%` it took
     * for a format, say), would show English or something else again, so the
     * tables are compared with the catalog both ways, value for value.
     */
    func testTheBuiltTablesAreTheCatalog() throws {
        let phrases = try catalog()
        let vocabulary = Set(phrases.keys)

        for language in Language.allCases {
            let table = try XCTUnwrap(Phrasebook.tables[language], language.rawValue)
            let said = Set(table.keys)

            XCTAssertEqual(vocabulary.subtracting(said).sorted(), [], "\(language.rawValue) is missing these")
            XCTAssertEqual(said.subtracting(vocabulary).sorted(), [], "\(language.rawValue) says these, which the catalog does not")

            let changed = phrases.keys.filter { table[$0] != nil && table[$0] != phrases[$0]?[language.rawValue] }
            XCTAssertEqual(changed.sorted(), [], "\(language.rawValue) differs from the catalog here")
        }
    }

    func testKeepsEveryPlaceholderExactlyAsItWas() {
        for language in Language.allCases {
            for (phrase, translated) in Phrasebook.tables[language] ?? [:] {
                XCTAssertEqual(placeholders(translated), placeholders(phrase), "\(language.rawValue): \(phrase)")
            }
        }
    }

    func testActuallyTranslatesRatherThanRepeatingTheEnglish() {
        // Proper nouns and single words can legitimately match. A table that
        // matched throughout would be a language somebody added and never
        // filled.
        for language in Language.allCases where language != .en {
            let table = Phrasebook.tables[language] ?? [:]
            let same = table.filter { $0.key == $0.value }.count
            XCTAssertGreaterThan(table.count, 0, language.rawValue)
            XCTAssertLessThan(same, table.count / 4, language.rawValue)
        }
    }

    // MARK: The source

    /*
     * Every phrase the apps say is a literal in a call to t(), and every one of
     * those must be in the catalog. On the web the compiler checks this, since
     * t() takes a Phrase, a type made of the English dictionary's keys; here
     * t() takes a String, so a phrase missing from native.json, or misspelt,
     * would show its English to every reader and fail nothing. This reads the
     * source for those calls instead. A phrase built by interpolation, rather
     * than with a {placeholder}, is reported too, since no catalog can hold it.
     */
    func testEveryPhraseTheSourceSaysIsInTheCatalog() throws {
        let vocabulary = Set(try catalog().keys)
        // The extensions too: the Messages extension's words are as much the
        // interface as the app's, and a phrase missing from the catalog there
        // would be English on every card.
        let roots = ["TomoshibiKit/Sources", "App", "Messages", "Broadcast"].map { Self.apple.appendingPathComponent($0) }
        let call = try NSRegularExpression(pattern: #"(?<![A-Za-z0-9_])t\(\s*"((?:[^"\\\n]|\\.)*)""#)

        var read = 0
        var unknown: [String] = []
        for root in roots {
            guard let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil) else { continue }
            for case let file as URL in walk where file.pathExtension == "swift" {
                read += 1
                let source = try String(contentsOf: file, encoding: .utf8)
                for (number, line) in source.components(separatedBy: "\n").enumerated() {
                    let trimmed = line.trimmingCharacters(in: .whitespaces)
                    if trimmed.hasPrefix("//") || trimmed.hasPrefix("*") || trimmed.hasPrefix("/*") { continue }

                    let range = NSRange(line.startIndex..., in: line)
                    for match in call.matches(in: line, range: range) {
                        guard let literal = Range(match.range(at: 1), in: line).map({ String(line[$0]) }) else { continue }
                        let phrase = unescaped(literal)
                        if phrase == nil || !vocabulary.contains(phrase!) {
                            unknown.append("\(file.lastPathComponent):\(number + 1): \(literal)")
                        }
                    }
                }
            }
        }

        XCTAssertGreaterThan(read, 0, "found no Swift source under \(Self.apple.path)")
        XCTAssertEqual(unknown, [], "add these to apple/i18n/native.json and regenerate")
    }

    /// A Swift string literal's contents as the string it makes, or nil when it
    /// interpolates.
    private func unescaped(_ literal: String) -> String? {
        var result = ""
        var characters = literal.makeIterator()
        while let character = characters.next() {
            guard character == "\\" else {
                result.append(character)
                continue
            }
            switch characters.next() {
            case "n": result.append("\n")
            case "t": result.append("\t")
            case "r": result.append("\r")
            case "0": result.append("\0")
            case "\"": result.append("\"")
            case "'": result.append("'")
            case "\\": result.append("\\")
            case "u":
                guard characters.next() == "{" else { return nil }
                var hex = ""
                while let digit = characters.next(), digit != "}" { hex.append(digit) }
                guard let value = UInt32(hex, radix: 16), let scalar = Unicode.Scalar(value) else { return nil }
                result.unicodeScalars.append(scalar)
            default:
                return nil
            }
        }
        return result
    }
}
