import TomoshibiBasics
import XCTest

/// The web's own cases for the watch ceiling (web/src/live/preferences.test.ts),
/// with nil standing for its -1.
final class WatchCeilingTests: XCTestCase {
    private typealias Level = WatchCeiling.Level
    private let p360 = Level(width: 640, height: 360)
    private let p720 = Level(width: 1280, height: 720)
    private let p1080 = Level(width: 1920, height: 1080)
    private let p2160 = Level(width: 3840, height: 2160)
    private var ladder: [Level] { [p360, p720, p1080, p2160] }

    func testAllowsEveryLevelUpToTheCeilingAndNoneAboveIt() {
        XCTAssertNil(WatchCeiling.auto.cap(ladder))
        XCTAssertEqual(WatchCeiling.p1080.cap(ladder), 2)
        XCTAssertEqual(WatchCeiling.p720.cap(ladder), 1)
        // Nothing above it to keep out, so no ceiling at all.
        XCTAssertNil(WatchCeiling.p1080.cap(Array(ladder.prefix(3))))
        XCTAssertNil(WatchCeiling.p720.cap([]))
    }

    func testMeasuresTheShorterSideWithRoomForRounding() {
        XCTAssertEqual(WatchCeiling.p1080.cap([p720, Level(width: 1920, height: 1088), p2160]), 1)
        let scope = [Level(width: 1920, height: 800), Level(width: 3840, height: 1600)]
        XCTAssertEqual(WatchCeiling.p1080.cap(scope), 0)
        XCTAssertEqual(WatchCeiling.p720.cap(scope), 0)
        // A level that does not say its size is not refused for it.
        XCTAssertNil(WatchCeiling.p720.cap([Level(width: nil, height: 0), p720]))
        // A portrait 1080p picture is a 1080p picture.
        XCTAssertNil(WatchCeiling.p1080.cap([Level(width: 1080, height: 1920)]))
    }

    func testStopsBelowTheFirstLevelThatIsTooTall() {
        XCTAssertEqual(WatchCeiling.p1080.cap([p720, p2160, p1080]), 0)
    }

    func testLeavesTheLeastThereIs() {
        XCTAssertEqual(WatchCeiling.p720.cap([p1080, p2160]), 0)
    }

    func testOnlyEverLowersThePlayersOwnCeiling() {
        XCTAssertEqual(WatchCeiling.p720.capping(nil, ladder), 1)
        XCTAssertEqual(WatchCeiling.p720.capping(3, ladder), 1)
        XCTAssertEqual(WatchCeiling.p720.capping(0, ladder), 0)
        XCTAssertEqual(WatchCeiling.auto.capping(3, ladder), 3)
        XCTAssertNil(WatchCeiling.auto.capping(nil, ladder))
    }

    func testReadsOnlyTheWordsItWrites() {
        let defaults = UserDefaults(suiteName: "WatchCeilingTests")!
        defaults.removePersistentDomain(forName: "WatchCeilingTests")
        XCTAssertEqual(WatchCeiling.stored(in: defaults), .auto)
        for garbage in ["4k", "1080P", "", "auto ", "480p"] {
            defaults.set(garbage, forKey: WatchCeiling.storageKey)
            XCTAssertEqual(WatchCeiling.stored(in: defaults), .auto, garbage)
        }
        WatchCeiling.p1080.remember(in: defaults)
        XCTAssertEqual(defaults.string(forKey: WatchCeiling.storageKey), "1080p")
        XCTAssertEqual(WatchCeiling.stored(in: defaults), .p1080)
        WatchCeiling.auto.remember(in: defaults)
        XCTAssertNil(defaults.string(forKey: WatchCeiling.storageKey))
        defaults.removePersistentDomain(forName: "WatchCeilingTests")
    }
}
