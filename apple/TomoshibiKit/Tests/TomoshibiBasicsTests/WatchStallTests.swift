import TomoshibiBasics
import XCTest

/// The stall rule of the video watched together, read on a clock whose
/// readings within one beat are not in order.
final class WatchStallTests: XCTestCase {
    /// The crash: a player built during a beat began after the beat read the
    /// clock, and its first look at the stall is always due.
    func testASourceThatBeganAfterTheBeatIsNotLateAndDoesNotTrap() {
        var stall = WatchStall(started: 1_000_002)
        XCTAssertTrue(stall.due(1_000_000))
        XCTAssertFalse(stall.late(1_000_000))
        XCTAssertEqual(WatchStall.since(1_000_002, 1_000_000), 0)
    }

    func testIsLateOnlyOnceTheWaitHasPassed() {
        let stall = WatchStall(started: 10_000)
        XCTAssertFalse(stall.late(18_000))
        XCTAssertTrue(stall.late(18_001))
    }

    func testLooksOnceASecondEvenWhenAReadingRunsBackwards() {
        var stall = WatchStall(started: 0)
        XCTAssertTrue(stall.due(5_000))
        XCTAssertFalse(stall.due(4_999))
        XCTAssertFalse(stall.due(5_999))
        XCTAssertTrue(stall.due(6_000))
    }

    /// A new source restarts the wait and not the once-a-second clock.
    func testRestartingKeepsTheLastLook() {
        var stall = WatchStall(started: 0)
        XCTAssertTrue(stall.due(9_000))
        XCTAssertTrue(stall.late(9_000))
        stall.restart(at: 9_400)
        XCTAssertFalse(stall.late(9_300))
        XCTAssertFalse(stall.due(9_500))
        XCTAssertFalse(stall.late(17_400))
        XCTAssertTrue(stall.late(17_401))
    }
}
