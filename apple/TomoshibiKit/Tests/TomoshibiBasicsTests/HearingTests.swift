import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The sound panel's record, held to what the web writes and reads under the
 * same name (web/src/live/hearing.ts and hearing.test.ts). What these guard is
 * a setting quietly not being what somebody chose: a guest's block surviving
 * onto whoever is issued that identity next, a hand-edited volume of 3 played
 * three times too loud, a book the web wrote that the app cannot read.
 */
final class HearingTests: XCTestCase {
    private let proven = HearingFiling(key: "ABCDEFGHIJ", keeps: true)
    private let guest = HearingFiling(key: "g0123456789abcdef0123456789abcdef", keeps: false)

    func testEverybodyIsAsSentUntilSomebodySaysOtherwise() {
        let book = HearingBook()
        for sound in Sound.allCases {
            XCTAssertEqual(book.setting(proven, sound), .asSent)
        }
        XCTAssertNil(book.stored)
    }

    func testAProvenSettingIsKeptInTheWebsShape() throws {
        var book = HearingBook()
        XCTAssertTrue(book.set(proven, .music, HearingSetting(volume: 0.25, blocked: false)))
        XCTAssertTrue(book.set(proven, .voice, HearingSetting(volume: 1, blocked: true)))

        let stored = try XCTUnwrap(book.stored)
        // Byte for byte what the web's JSON.stringify writes for the same book.
        XCTAssertEqual(stored, #"{"ABCDEFGHIJ/music":{"volume":0.25,"blocked":false},"ABCDEFGHIJ/voice":{"volume":1,"blocked":true}}"#)
        let parsed = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(stored.utf8)) as? [String: [String: Any]])
        XCTAssertEqual(Set(parsed.keys), ["ABCDEFGHIJ/music", "ABCDEFGHIJ/voice"])
        XCTAssertEqual(parsed["ABCDEFGHIJ/music"]?["volume"] as? Double, 0.25)
        XCTAssertEqual(parsed["ABCDEFGHIJ/voice"]?["blocked"] as? Bool, true)

        // And read back as it was written, by the next launch.
        let again = HearingBook(stored: stored)
        XCTAssertEqual(again.setting(proven, .music), HearingSetting(volume: 0.25, blocked: false))
        XCTAssertEqual(again.setting(proven, .voice), HearingSetting(volume: 1, blocked: true))
        XCTAssertEqual(again.setting(proven, .screen), .asSent)
    }

    func testAVolumeIsWrittenInItsShortestForm() {
        var book = HearingBook()
        book.set(proven, .voice, HearingSetting(volume: 0.35, blocked: false))
        XCTAssertEqual(book.stored, #"{"ABCDEFGHIJ/voice":{"volume":0.35,"blocked":false}}"#)
        book.set(proven, .voice, HearingSetting(volume: 0, blocked: false))
        XCTAssertEqual(book.stored, #"{"ABCDEFGHIJ/voice":{"volume":0,"blocked":false}}"#)
    }

    func testAGuestsSettingLastsTheCallAndIsNeverWritten() {
        var book = HearingBook()
        XCTAssertFalse(book.set(guest, .voice, HearingSetting(volume: 0, blocked: true)))
        XCTAssertEqual(book.setting(guest, .voice), HearingSetting(volume: 0, blocked: true))
        XCTAssertNil(book.stored)
    }

    func testTurningSomebodyBackUpRemovesThemFromStorage() {
        var book = HearingBook()
        book.set(proven, .voice, HearingSetting(volume: 0.5, blocked: false))
        XCTAssertNotNil(book.stored)
        XCTAssertTrue(book.set(proven, .voice, .asSent))
        XCTAssertNil(book.stored)
    }

    func testWhatTheWebWroteIsReadAndWhatWasEditedByHandIsClamped() {
        let book = HearingBook(stored: """
        {"AAAAAAAAAA/voice":{"volume":3,"blocked":false},
         "BBBBBBBBBB/voice":{"volume":-1},
         "CCCCCCCCCC/voice":{"volume":"loud","blocked":1},
         "DDDDDDDDDD/voice":{"volume":false,"blocked":true},
         "EEEEEEEEEE/voice":{"volume":0.4,"blocked":true}}
        """)
        func setting(_ key: String) -> HearingSetting { book.setting(HearingFiling(key: key, keeps: true), .voice) }
        XCTAssertEqual(setting("AAAAAAAAAA"), HearingSetting(volume: 1, blocked: false))
        XCTAssertEqual(setting("BBBBBBBBBB"), HearingSetting(volume: 0, blocked: false))
        // A string is not a volume and a 1 is not a block, as `=== true` has it.
        XCTAssertEqual(setting("CCCCCCCCCC"), HearingSetting(volume: 1, blocked: false))
        // JSON's false is not a volume of nought either, though it arrives as a number.
        XCTAssertEqual(setting("DDDDDDDDDD"), HearingSetting(volume: 1, blocked: true))
        XCTAssertEqual(setting("EEEEEEEEEE"), HearingSetting(volume: 0.4, blocked: true))
    }

    func testUnreadableStorageIsNothingDecided() {
        XCTAssertEqual(HearingBook(stored: "not json"), HearingBook())
        XCTAssertEqual(HearingBook(stored: "[1,2]"), HearingBook())
        XCTAssertEqual(HearingBook(stored: nil), HearingBook())
    }

    func testAVolumeOutsideTheRangeIsClampedOnTheWayIn() {
        var book = HearingBook()
        book.set(proven, .screen, HearingSetting(volume: 7, blocked: false))
        // Clamped to 1 and not blocked: as sent, so nothing is kept at all.
        XCTAssertEqual(book.setting(proven, .screen), .asSent)
        XCTAssertNil(book.stored)
    }

    func testSilencedIsABlockOrNothingToHear() {
        XCTAssertTrue(HearingSetting(volume: 0, blocked: false).silenced)
        XCTAssertTrue(HearingSetting(volume: 1, blocked: true).silenced)
        XCTAssertFalse(HearingSetting(volume: 0.05, blocked: false).silenced)
    }
}
