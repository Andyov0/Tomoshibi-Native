@testable import TomoshibiBasics
import XCTest

/*
 * The update bars move at one speed (SteadyFill; the owner, 2026-10-22: "make
 * it move evenly"): a whole bar in 1.2 s however fast the bytes come, never ahead of
 * them, straight back where a download begins again, and the step after it
 * waiting until it is drawn full. Moved here by hand (`drives` false), a
 * hundredth of a second at a time.
 */
@MainActor
final class SteadyFillTests: XCTestCase {
    let tick: Duration = .milliseconds(10)

    func testABarThatArrivesAtOnceIsDrawnAtOneSpeedAndTakesItsTime() {
        let bar = SteadyFill(drives: false)
        bar.follow(1)
        var last = bar.shown
        var steps: [Double] = []
        for _ in 0..<119 {
            bar.advance(by: tick)
            steps.append(bar.shown - last)
            last = bar.shown
        }
        XCTAssertLessThan(bar.shown, 1, "full before 1.2 s")
        let step = 0.01 * SteadyFill.perSecond
        XCTAssertTrue(steps.allSatisfy { abs($0 - step) < 1e-9 }, "every tick the same step: \(Set(steps.map { ($0 * 1e6).rounded() }))")
        bar.advance(by: tick)
        bar.advance(by: tick)
        XCTAssertEqual(bar.shown, 1)
    }

    func testABarIsNeverAheadOfWhatHasArrived() {
        let bar = SteadyFill(drives: false)
        bar.follow(0.3)
        for _ in 0..<500 { bar.advance(by: tick) }
        XCTAssertEqual(bar.shown, 0.3, accuracy: 1e-12)
        bar.follow(0.31)
        bar.advance(by: .seconds(5))
        XCTAssertEqual(bar.shown, 0.31, accuracy: 1e-12)
    }

    func testADownloadBegunAgainIsShownAtOnceAndNotDrawnBackwards() {
        let bar = SteadyFill(drives: false)
        bar.follow(0.8)
        bar.advance(by: .seconds(2))
        bar.follow(0.2)
        XCTAssertEqual(bar.shown, 0.2, accuracy: 1e-12)
        bar.follow(.nan)
        XCTAssertEqual(bar.shown, 0)
    }

    func testWhatWaitsForTheBarGoesOnOnlyOnceItIsFull() async {
        let bar = SteadyFill(drives: false)
        bar.follow(1)
        var through = false
        let waiting = Task { await bar.filled(); through = true }
        for _ in 0..<100 { bar.advance(by: tick) }
        for _ in 0..<20 { await Task.yield() }
        XCTAssertFalse(through, "let go with the bar at \(bar.shown)")
        for _ in 0..<30 { bar.advance(by: tick) }
        await waiting.value
        XCTAssertTrue(through)
    }

    func testAResetLetsGoOfWhoeverWasWaiting() async {
        let bar = SteadyFill(drives: false)
        bar.follow(1)
        let waiting = Task { await bar.filled() }
        for _ in 0..<10 { await Task.yield() }
        bar.reset()
        await waiting.value
        XCTAssertEqual(bar.shown, 0)
    }
}
