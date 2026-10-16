import Foundation
import Observation
import os
@testable import TomoshibiBasics
import XCTest

/*
 * How phrases are looked up, filled in and followed, ported case for case from
 * the web's web/src/live/i18n.test.ts where it has one, so that the same call
 * says the same thing in a browser and in the apps.
 *
 * Each test speaks from its own Localization over a defaults domain of its
 * own, emptied before and after, so that nothing here depends on, or changes,
 * the language of the machine running it. The domain's name is fixed: macOS
 * keeps a file for every domain ever named even after it is emptied, and a
 * fresh name per test left a hundred of them in ~/Library/Preferences.
 */
final class LocalizationTests: XCTestCase {
    private let suite = "TomoshibiBasicsTests.Localization"
    private var defaults: UserDefaults!

    override func setUp() {
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    private func speaking(_ language: Language) -> Localization {
        let localization = Localization(defaults: defaults, preferredLanguages: [])
        localization.language = language
        return localization
    }

    // MARK: t, as the web's tests have it

    func testFallsBackToAWholeSentenceRatherThanAKey() {
        XCTAssertEqual(speaking(.en).t("Nobody else is here."), "Nobody else is here.")
    }

    func testPutsValuesWhereThePhraseAsksForThem() {
        let en = speaking(.en)
        XCTAssertEqual(en.t("Show {name} larger", ["name": "Ada"]), "Show Ada larger")
        XCTAssertEqual(en.t("Device {number}", ["number": 2]), "Device 2")
    }

    func testLeavesAPhraseAloneWhenItCarriesNothing() {
        XCTAssertEqual(speaking(.en).t("Leave"), "Leave")
    }

    // MARK: t, where the web's behaviour has no test of its own

    func testAnUnknownPhraseIsItsOwnEnglishInEveryLanguage() {
        for language in Language.allCases {
            let localization = speaking(language)
            XCTAssertEqual(localization.t("A phrase nobody has written"), "A phrase nobody has written")
            XCTAssertEqual(localization.t("Nobody wrote {this}", ["this": "x"]), "Nobody wrote x")
        }
    }

    func testTranslatesAndPutsTheValueWhereTheTranslationPutsIt() {
        for language in Language.allCases where language != .en {
            let template = Phrasebook.tables[language]?["Show {name} larger"]
            XCTAssertNotNil(template, language.rawValue)
            XCTAssertNotEqual(template, "Show {name} larger", language.rawValue)

            let said = speaking(language).t("Show {name} larger", ["name": "Ada"])
            XCTAssertEqual(said, template?.replacingOccurrences(of: "{name}", with: "Ada"), language.rawValue)
            XCTAssertFalse(said.contains("{"), language.rawValue)
        }
    }

    func testWithoutValuesThePlaceholderStaysAsWritten() {
        // `if (!values) return template;`
        XCTAssertEqual(speaking(.en).t("Show {name} larger"), "Show {name} larger")
    }

    func testAValueThatWasNotGivenBecomesNothing() {
        // `String(values[part.value] ?? "")`
        XCTAssertEqual(speaking(.en).t("Show {name} larger", [:]), "Show  larger")
        XCTAssertEqual(speaking(.en).t("Show {name} larger", ["other": "Ada"]), "Show  larger")
    }

    func testOnlyAsciiWordsInBracesArePlaceholders() {
        // JavaScript's \w, which is narrower than Swift's.
        let en = speaking(.en)
        XCTAssertEqual(en.t("{名前} and {name}", ["名前": "A", "name": "B"]), "{名前} and B")
        XCTAssertEqual(en.t("{a b} {} {x}", ["a b": "A", "": "E", "x": "X"]), "{a b} {} X")
    }

    func testNumbersAreWrittenAsJavaScriptWritesThem() {
        let en = speaking(.en)
        XCTAssertEqual(en.t("Device {number}", ["number": 0]), "Device 0")
        XCTAssertEqual(en.t("Device {number}", ["number": 2.0]), "Device 2")
        XCTAssertEqual(en.t("Device {number}", ["number": -0.0]), "Device 0")
        XCTAssertEqual(en.t("Device {number}", ["number": 2.5]), "Device 2.5")
        XCTAssertEqual(en.t("Device {number}", ["number": Float(3)]), "Device 3")
    }

    func testTheFreeFunctionsSpeakFromTheSharedOne() {
        XCTAssertEqual(t("Leave"), Localization.shared.t("Leave"))
        XCTAssertEqual(t("Show {name} larger", ["name": "Ada"]), Localization.shared.t("Show {name} larger", ["name": "Ada"]))
        XCTAssertEqual(phraseSegments("Show {name} larger"), Localization.shared.phraseSegments("Show {name} larger"))
    }

    // MARK: segments, as the web's tests have it

    func testCutsAPhraseAtItsPlaceholdersAndKeepsTheRest() {
        let parts = PhraseSegment.split("Joining as {name} with a signature")
        XCTAssertEqual(parts.map(\.value), [nil, "name", nil])
        XCTAssertEqual(parts.map(\.text).joined(), "Joining as {name} with a signature")
    }

    func testHandlesAPhraseThatBeginsOrEndsWithAValue() {
        XCTAssertEqual(PhraseSegment.split("{name} joined").map(\.value), ["name", nil])
        XCTAssertEqual(PhraseSegment.split("Could not join {room}").map(\.value), [nil, "room"])
    }

    func testCutsWhereTheWebsPatternWouldMatch() {
        XCTAssertEqual(
            PhraseSegment.split("{{name}}"),
            [PhraseSegment(text: "{"), PhraseSegment(text: "{name}", value: "name"), PhraseSegment(text: "}")]
        )
        XCTAssertEqual(
            PhraseSegment.split("{a{b}"),
            [PhraseSegment(text: "{a"), PhraseSegment(text: "{b}", value: "b")]
        )
        XCTAssertEqual(PhraseSegment.split("{a}{b}").map(\.value), ["a", "b"])
        XCTAssertEqual(PhraseSegment.split(""), [])
        XCTAssertEqual(PhraseSegment.split("no braces"), [PhraseSegment(text: "no braces")])
    }

    func testPhraseSegmentsCutTheTranslationNotTheEnglish() {
        let ja = speaking(.ja)
        let parts = ja.phraseSegments("Only you can join as {name}")
        XCTAssertEqual(parts.map(\.text).joined(), Phrasebook.tables[.ja]?["Only you can join as {name}"])
        XCTAssertEqual(parts.compactMap(\.value), ["name"])
    }

    // MARK: say

    func testSayTranslatesWhatADeploymentTypesIgnoringCase() {
        for language in Language.allCases {
            let localization = speaking(language)
            let translated = Phrasebook.tables[language]?["Shanghai Telecom"]
            XCTAssertNotNil(translated, language.rawValue)
            XCTAssertEqual(localization.say("Shanghai Telecom"), translated, language.rawValue)
            XCTAssertEqual(localization.say("SHANGHAI TELECOM"), translated, language.rawValue)
            XCTAssertEqual(localization.say("Somewhere Else"), "Somewhere Else", language.rawValue)
        }
    }

    // MARK: Choosing a language

    func testDetectsFromThePreferenceListAsTheWebDoes() {
        let cases: [([String], Language)] = [
            (["en-US"], .en),
            (["en-GB", "ja-JP"], .en),
            (["ja"], .ja),
            (["ja-JP"], .ja),
            (["zh"], .zhHans),
            (["zh-CN"], .zhHans),
            (["zh-SG"], .zhHans),
            (["zh-Hans"], .zhHans),
            (["zh-Hans-CN"], .zhHans),
            (["zh-TW"], .zhHant),
            (["zh-HK"], .zhHant),
            (["zh-MO"], .zhHant),
            (["zh-Hant"], .zhHant),
            (["zh-Hant-TW"], .zhHant),
            (["zh-Hant-CN"], .zhHant),
            (["ZH-tw"], .zhHant),
            (["zh_TW"], .zhHant),
            // A language the interface is not written in is passed over, not
            // taken as a reason to settle on English.
            (["fr-FR", "ja-JP"], .ja),
            (["de-DE", "zh-Hant-HK", "en-US"], .zhHant),
            (["fr-FR"], .en),
            ([], .en),
        ]

        for (preferred, expected) in cases {
            XCTAssertEqual(Language.detect(from: preferred), expected, "\(preferred)")
        }
    }

    func testAnExplicitScriptOutranksTheRegion() {
        // The one place this differs from the web: Language.detect says why.
        XCTAssertEqual(Language.detect(from: ["zh-Hans-HK"]), .zhHans)
        XCTAssertEqual(Language.detect(from: ["zh-Hans-TW"]), .zhHans)
        XCTAssertEqual(Language.detect(from: ["zh-Hant-SG"]), .zhHant)
    }

    func testOpensInTheDetectedLanguageWhenNothingWasChosen() {
        XCTAssertEqual(Localization(defaults: defaults, preferredLanguages: ["fr", "ja-JP"]).language, .ja)
        XCTAssertNil(defaults.string(forKey: "meet-live.locale"))
    }

    func testAChoiceIsKeptUnderTheWebsKeyAndOutranksDetection() {
        let first = Localization(defaults: defaults, preferredLanguages: ["ja-JP"])
        first.language = .zhHant

        XCTAssertEqual(defaults.string(forKey: "meet-live.locale"), "zh-Hant")
        XCTAssertEqual(Localization(defaults: defaults, preferredLanguages: ["ja-JP"]).language, .zhHant)
    }

    func testChoosingTheLanguageAlreadyShowingIsStillAChoice() {
        let localization = Localization(defaults: defaults, preferredLanguages: ["en-US"])
        localization.language = .en

        XCTAssertEqual(defaults.string(forKey: "meet-live.locale"), "en")
        XCTAssertEqual(Localization(defaults: defaults, preferredLanguages: ["ja-JP"]).language, .en)
    }

    func testARunThatKeepsNothingShowsItsChoiceAndWritesNone() {
        // What a test driving the app gets (AppModel.rehearsal): the tile it
        // presses is the language it sees, and the owner's choice is still
        // the one written down when it has gone.
        defaults.set("en", forKey: "meet-live.locale")
        let localization = Localization(defaults: defaults, preferredLanguages: ["ja-JP"])
        localization.keeps = false

        localization.language = .zhHant
        XCTAssertEqual(localization.language, .zhHant)
        XCTAssertFalse(localization.followsSystem)
        XCTAssertEqual(defaults.string(forKey: "meet-live.locale"), "en")

        localization.followSystem()
        XCTAssertTrue(localization.followsSystem, "following the system is what this run chose, whatever is stored")
        XCTAssertEqual(defaults.string(forKey: "meet-live.locale"), "en")
        XCTAssertEqual(Localization(defaults: defaults, preferredLanguages: ["ja-JP"]).language, .en)
    }

    func testChoosingTheDetectedLanguageMovesTheTickAndIsWatched() {
        // Nothing chosen, English detected, English picked: the language
        // showing is the same, and the list's tick still has to move off
        // "Same as the system".
        let localization = Localization(defaults: defaults, preferredLanguages: ["en-US"])
        XCTAssertTrue(localization.followsSystem)
        let changed = OSAllocatedUnfairLock(initialState: 0)
        withObservationTracking {
            _ = localization.followsSystem
        } onChange: {
            changed.withLock { $0 += 1 }
        }

        localization.language = .en
        XCTAssertFalse(localization.followsSystem)
        XCTAssertEqual(changed.withLock { $0 }, 1)
    }

    func testAStoredValueThatIsNotExactlyACodeIsNoChoice() {
        for stored in ["fr", "zh-hans", "ZH-HANT", "japanese", ""] {
            defaults.set(stored, forKey: "meet-live.locale")
            XCTAssertEqual(Localization(defaults: defaults, preferredLanguages: ["ja"]).language, .ja, stored)
        }
    }

    // MARK: Following a change

    func testSayingAnythingIsWatchingTheLanguage() {
        // What SwiftUI relies on: a body that calls t() is re-run when the
        // language changes, without having watched for it explicitly.
        let localization = speaking(.en)
        let changed = OSAllocatedUnfairLock(initialState: 0)

        withObservationTracking {
            _ = localization.t("Leave")
        } onChange: {
            changed.withLock { $0 += 1 }
        }

        localization.language = .en
        XCTAssertEqual(changed.withLock { $0 }, 0, "choosing the language already showing changed nothing")

        localization.language = .ja
        XCTAssertEqual(changed.withLock { $0 }, 1)
        XCTAssertEqual(localization.t("Leave"), Phrasebook.tables[.ja]?["Leave"])
    }

    func testSayCutAndLanguageAreWatchedToo() {
        let localization = speaking(.en)
        let changed = OSAllocatedUnfairLock(initialState: 0)

        withObservationTracking {
            _ = localization.say("Shanghai Telecom")
        } onChange: {
            changed.withLock { $0 += 1 }
        }
        withObservationTracking {
            _ = localization.phraseSegments("Show {name} larger")
        } onChange: {
            changed.withLock { $0 += 1 }
        }

        localization.language = .zhHans
        XCTAssertEqual(changed.withLock { $0 }, 2)
    }

    func testCanBeReadFromAnyThreadWhileItChanges() {
        let localization = speaking(.en)
        let expected = Set(Language.allCases.map { Phrasebook.tables[$0]?["Leave"] ?? "" })

        DispatchQueue.concurrentPerform(iterations: 2000) { index in
            if index % 50 == 0 {
                localization.language = Language.allCases[(index / 50) % Language.allCases.count]
            } else {
                XCTAssertTrue(expected.contains(localization.t("Leave")))
            }
        }
    }

    // MARK: Names

    func testNamesEachLanguageInItself() {
        // The one list read by somebody who cannot read the language it is
        // currently displayed in.
        for language in Language.allCases {
            XCTAssertFalse(language.name.isEmpty)
        }
        XCTAssertNil(Language.ja.name.range(of: "[A-Za-z]", options: .regularExpression))
        XCTAssertNil(Language.zhHant.name.range(of: "[A-Za-z]", options: .regularExpression))
        XCTAssertNil(Language.zhHans.name.range(of: "[A-Za-z]", options: .regularExpression))
    }

    func testTheCodesAreTheWebs() {
        XCTAssertEqual(Language.allCases.map(\.rawValue), ["en", "zh-Hans", "zh-Hant", "ja"])
        XCTAssertEqual(Localization.storageKey, "meet-live.locale")
    }
}
