@testable import TomoshibiBasics
import XCTest

/*
 * When the call's audio engine is started again after the hardware changed
 * under it, and when that is given up (CallAudio, EngineRecovery). What these
 * guard is the call that went silent both ways with the microphone's button
 * on: the fourth change in ten seconds left the engine stopped for good, and
 * nothing said so.
 */
final class EngineRecoveryTests: XCTestCase {
    func testRestartsAtOnceThreeTimesThenWaitsForQuietAndTriesOnceMore() {
        var recovery = EngineRecovery()
        XCTAssertEqual(recovery.changed(at: 0), .restart)
        XCTAssertEqual(recovery.changed(at: 1), .restart)
        XCTAssertEqual(recovery.changed(at: 2), .restart)
        // The fourth within ten seconds: still settling, so not into it again,
        // but not left stopped either.
        XCTAssertEqual(recovery.changed(at: 3), .wait(EngineRecovery.quietFor))
        XCTAssertTrue(recovery.waiting)
        XCTAssertEqual(recovery.quiet(at: 6), .restart)
        XCTAssertFalse(recovery.waiting)
        XCTAssertFalse(recovery.gaveUp)
    }

    func testOnlyTheLastTenSecondsCount() {
        var recovery = EngineRecovery()
        for at in [0.0, 1, 2] { XCTAssertEqual(recovery.changed(at: at), .restart) }
        // A headset plugged in a while later is a fresh start, not the fourth.
        XCTAssertEqual(recovery.changed(at: 12.5), .restart)
    }

    func testAChangeWhileWaitingPushesTheTryBack() {
        var recovery = EngineRecovery()
        for at in [0.0, 1, 2] { _ = recovery.changed(at: at) }
        XCTAssertEqual(recovery.changed(at: 3), .wait(3))
        XCTAssertEqual(recovery.changed(at: 4), .wait(3))
        XCTAssertTrue(recovery.waiting)
        // A failure reported while it waits changes nothing: the wait stands.
        XCTAssertEqual(recovery.failed(at: 4.5), .none)
        XCTAssertTrue(recovery.waiting)
    }

    func testAStartThatThrowsWaitsRatherThanGivingUpOrLooping() {
        var recovery = EngineRecovery()
        XCTAssertEqual(recovery.changed(at: 0), .restart)
        XCTAssertEqual(recovery.failed(at: 0.1), .wait(3))
        XCTAssertEqual(recovery.quiet(at: 3.1), .restart)
    }

    func testGivesUpWhenTheTryAfterTheQuietFailsToo() {
        var recovery = EngineRecovery()
        XCTAssertEqual(recovery.changed(at: 0), .restart)
        XCTAssertEqual(recovery.failed(at: 0.1), .wait(3))
        XCTAssertEqual(recovery.quiet(at: 3.1), .restart)
        XCTAssertEqual(recovery.failed(at: 3.2), .giveUp)
        XCTAssertTrue(recovery.gaveUp)
        // Given up, it waits for something else to start the engine.
        XCTAssertEqual(recovery.changed(at: 4), .none)
        XCTAssertEqual(recovery.failed(at: 5), .none)
    }

    func testGivesUpWhenItChurnsPastTheBudgetAgainSoonAfterTheTry() {
        var recovery = EngineRecovery()
        for at in [0.0, 1, 2] { _ = recovery.changed(at: at) }
        XCTAssertEqual(recovery.changed(at: 3), .wait(3))
        XCTAssertEqual(recovery.quiet(at: 6), .restart)
        recovery.started()
        // The try worked, and the hardware went straight back to churning.
        XCTAssertEqual(recovery.changed(at: 7), .giveUp)
    }

    func testChurningLongAfterASettledTryWaitsAgainRatherThanGivingUp() {
        var recovery = EngineRecovery()
        for at in [0.0, 1, 2] { _ = recovery.changed(at: at) }
        _ = recovery.changed(at: 3)
        _ = recovery.quiet(at: 6)
        recovery.started()
        for at in [100.0, 101, 102] { XCTAssertEqual(recovery.changed(at: at), .restart) }
        XCTAssertEqual(recovery.changed(at: 103), .wait(3))
    }

    func testAnythingThatStartsTheEngineEndsAWaitAndAGiveUp() {
        var recovery = EngineRecovery()
        _ = recovery.changed(at: 0)
        _ = recovery.failed(at: 0)
        XCTAssertTrue(recovery.waiting)
        recovery.started()
        XCTAssertFalse(recovery.waiting)
        XCTAssertEqual(recovery.quiet(at: 3), .none, "a wait that was overtaken has nothing left to try")

        _ = recovery.failed(at: 10)
        _ = recovery.quiet(at: 13)
        XCTAssertEqual(recovery.failed(at: 13.1), .giveUp)
        recovery.started()
        XCTAssertFalse(recovery.gaveUp)
        XCTAssertEqual(recovery.changed(at: 200), .restart)
    }

    /// Whatever the hardware does, the engine is never left stopped with
    /// nothing either trying it again or saying so: every stop ends in a
    /// restart, a wait whose try is still to come, or a give-up.
    func testNeverLeavesTheEngineStoppedWithNothingComing() {
        var recovery = EngineRecovery()
        var now = 0.0
        for _ in 0..<200 {
            now += 0.4
            let step = recovery.changed(at: now)
            switch step {
            case .restart, .giveUp: break
            case .wait: XCTAssertTrue(recovery.waiting)
            case .none: XCTAssertTrue(recovery.waiting || recovery.gaveUp, "stopped with nothing coming at \(now)")
            }
        }
    }
}
