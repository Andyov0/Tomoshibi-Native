import CoreGraphics
import XCTest
@testable import TomoshibiBasics

/// The web's own cases for where pictures go (web/src/live/plan.test.ts),
/// against the Swift port. The faults they guard are the ones the web found:
/// a filmstrip passing under the controls, a phone showing one person in a
/// letterbox, two strangers side by side where a conversation should be.
final class PlanTests: XCTestCase {
    let window = CGSize(width: 1600, height: 900)
    let phone = CGSize(width: 393, height: 852)

    func ids(_ count: Int) -> [String] { (0..<count).map { "p\($0)" } }

    func overlaps(_ a: CGRect, _ b: CGRect) -> Bool {
        let tolerance: CGFloat = 0.5
        return a.maxX - tolerance > b.minX && b.maxX - tolerance > a.minX && a.maxY - tolerance > b.minY && b.maxY - tolerance > a.minY
    }

    func testGridPlacesEverybodyOnceWithoutOverlap() {
        for count in 1...9 {
            let plan = Plan.grid(window, ids: ids(count))
            XCTAssertEqual(plan.count, count)
            let placed = Array(plan.values)
            for i in placed.indices {
                for j in placed.indices where j > i {
                    XCTAssertFalse(overlaps(placed[i], placed[j]), "\(count) people")
                }
            }
        }
    }

    func testGridKeepsClearOfTheControlsAndInsideTheWindow() {
        for count in 1...9 {
            for spot in Plan.grid(window, ids: ids(count)).values {
                XCTAssertLessThanOrEqual(spot.maxY, window.height - Plan.island / 2)
                XCTAssertGreaterThanOrEqual(spot.minX, 0)
                XCTAssertGreaterThanOrEqual(spot.minY, 0)
                XCTAssertLessThanOrEqual(spot.maxX, window.width)
            }
        }
    }

    func testGridCentresAShortLastRow() {
        let plan = Plan.grid(CGSize(width: 1200, height: 900), ids: ids(5))
        let lastRow = plan.values.filter { $0.minY == plan.values.map(\.minY).max() }
        let left = lastRow.map(\.minX).min()!, right = lastRow.map(\.maxX).max()!
        XCTAssertEqual(left, 1200 - right, accuracy: 0.001)
    }

    func testGridPlansNothingUnmeasured() {
        XCTAssertTrue(Plan.grid(.zero, ids: ids(4)).isEmpty)
        XCTAssertTrue(Plan.grid(window, ids: []).isEmpty)
    }

    func testFocusPutsTheStageAboveTheStripAndBothClearOfTheControls() {
        let plan = Plan.focus(window, stage: "stage", strip: ids(4))
        let stage = plan["stage"]!
        for id in ids(4) {
            let spot = plan[id]!
            XCTAssertGreaterThanOrEqual(spot.minY, stage.maxY)
            XCTAssertLessThanOrEqual(spot.maxY, window.height - Plan.island + 0.5)
        }
    }

    func testFocusFillsTheScreenWhenAsked() {
        let plan = Plan.focus(window, stage: "stage", strip: ids(3), fullscreen: true)
        XCTAssertEqual(plan.count, 1)
        XCTAssertEqual(plan["stage"], CGRect(origin: .zero, size: window))
    }

    func testStripCapacityHoldsSomebodyAlways() {
        XCTAssertEqual(Plan.stripCapacity(CGSize(width: 10, height: 900)), 1)
        XCTAssertGreaterThan(Plan.stripCapacity(CGSize(width: 1600, height: 900)), Plan.stripCapacity(CGSize(width: 800, height: 900)))
    }

    func testAPhoneIsFilledRatherThanBanded() {
        // The web measured 0.28 before it learned to turn tiles upright.
        for count in [3, 4, 6] {
            let used = Plan.grid(phone, ids: ids(count)).values.reduce(0) { $0 + $1.width * $1.height }
            XCTAssertGreaterThan(used / (phone.width * (phone.height - Plan.island)), 0.45, "\(count) people")
        }
    }

    func testUprightPicturesAreUpright() {
        let spot = Plan.grid(phone, ids: ids(4)).values.first!
        XCTAssertGreaterThan(spot.height, spot.width)
        let wide = Plan.grid(window, ids: ids(4)).values.first!
        XCTAssertGreaterThan(wide.width, wide.height)
    }

    func testTwoPeopleAreAConversation() {
        let plan = Plan.close(phone, filling: "them", cards: ["me"])
        XCTAssertEqual(plan["them"], CGRect(origin: .zero, size: phone))
        let me = plan["me"]!
        XCTAssertLessThanOrEqual(me.maxX, phone.width)
        XCTAssertLessThanOrEqual(me.maxY, phone.height - Plan.island)
        XCTAssertLessThan(me.width, phone.width / 2)
    }

    func testACardThatWouldNotFitIsDropped() {
        let short = CGSize(width: 393, height: 300)
        let plan = Plan.close(short, filling: "them", cards: ids(6))
        for (id, spot) in plan where id != "them" {
            XCTAssertLessThanOrEqual(spot.maxY, short.height - Plan.island)
        }
    }
}
