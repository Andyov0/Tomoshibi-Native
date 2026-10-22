import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * A name the Messages extension makes for a meeting it starts is read by the
 * core and the web, which tell a generated name from a chosen one by finding
 * its words in their own lists (names.ts and identity.rs `looks_generated`).
 * What these guard is the lists drifting apart: a word added to the core's and
 * not here, or one list reordered, and names made here would stop looking
 * generated to everything else, and be warned about as guessable.
 */
final class RoomNamesTests: XCTestCase {
    /// The core's identity.rs, found from this file so the test reads the
    /// repository rather than a copy.
    private static let identity = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // TomoshibiBasicsTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // TomoshibiKit
        .deletingLastPathComponent() // apple
        .deletingLastPathComponent() // the repository
        .appendingPathComponent("core/protocol/src/identity.rs")

    private func coreList(_ name: String) throws -> [String] {
        let source = try String(contentsOf: Self.identity, encoding: .utf8)
        let start = try XCTUnwrap(source.range(of: "const \(name): [&str; 256] = ["), "\(name) in identity.rs")
        let end = try XCTUnwrap(source.range(of: "];", range: start.upperBound ..< source.endIndex))
        let body = source[start.upperBound ..< end.lowerBound]
        return body.split(separator: "\"").enumerated().filter { $0.offset % 2 == 1 }.map { String($0.element) }
    }

    func testTheListsAreTheCoresInItsOrder() throws {
        XCTAssertEqual(RoomNames.adjectives, try coreList("ADJECTIVES"))
        XCTAssertEqual(RoomNames.nouns, try coreList("NOUNS"))
        XCTAssertEqual(RoomNames.verbs, try coreList("VERBS"))
        for list in [RoomNames.adjectives, RoomNames.nouns, RoomNames.verbs] {
            XCTAssertEqual(list.count, 256, "one byte picks a word only from a list of 256")
            XCTAssertEqual(Set(list).count, 256)
        }
    }

    /// The core's own arithmetic: a byte a word, the last two bytes big-endian
    /// modulo ten thousand, padded to four.
    func testFiveBytesMakeTheNameTheCoreMakesOfThem() {
        XCTAssertEqual(RoomNames.name(from: [0, 0, 0, 0, 7]), "amber-abbey-abide-0007")
        XCTAssertEqual(RoomNames.name(from: [255, 255, 255, 0xFF, 0xFF]),
                       "\(RoomNames.adjectives[255])-\(RoomNames.nouns[255])-\(RoomNames.verbs[255])-5535")
        XCTAssertEqual(RoomNames.name(from: [1, 2, 3, 0x27, 0x10]), "ancient-alcove-anchor-0000")
    }

    /// The shape the server accepts and `looks_generated` recognises, and
    /// different every time.
    func testGeneratedNamesAreValidAndApart() {
        let pattern = try! NSRegularExpression(pattern: "^[a-z]+-[a-z]+-[a-z]+-[0-9]{4}$")
        var seen = Set<String>()
        for _ in 0 ..< 500 {
            let name = RoomNames.generated()
            XCTAssertNotNil(pattern.firstMatch(in: name, range: NSRange(name.startIndex..., in: name)), name)
            let parts = name.split(separator: "-").map(String.init)
            XCTAssertTrue(RoomNames.adjectives.contains(parts[0]) && RoomNames.nouns.contains(parts[1])
                && RoomNames.verbs.contains(parts[2]), name)
            XCTAssertLessThanOrEqual(name.count, 64, "the server's longest room name")
            seen.insert(name)
        }
        XCTAssertGreaterThan(seen.count, 495, "five hundred names drawn from 37 bits hardly ever meet")
    }
}
