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

    /// Clear of the controls whole, and of the window's edges by the gap:
    /// the web lets a full grid's last row run half under the bar, and six
    /// people in a wide window touch both sides of it.
    func testGridKeepsClearOfTheControlsAndInsideTheWindow() {
        for size in [window, CGSize(width: 1280, height: 772), phone] {
            for count in 1...9 {
                for spot in Plan.grid(size, ids: ids(count)).values {
                    XCTAssertLessThanOrEqual(spot.maxY, size.height - Plan.island + 0.5, "\(count) in \(size)")
                    XCTAssertGreaterThanOrEqual(spot.minX, Plan.gap - 0.5, "\(count) in \(size)")
                    XCTAssertGreaterThanOrEqual(spot.minY, 0)
                    XCTAssertLessThanOrEqual(spot.maxX, size.width - Plan.gap + 0.5, "\(count) in \(size)")
                }
            }
        }
    }

    /// The paging arrows sit beside the pictures, not over the first and last.
    func testARoomThatPagesLeavesItsArrowsRoom() {
        let grid = Plan.room(window, pictures: ["me"] + ids(20), mine: "me", stage: nil)
        for spot in grid.spots.values {
            XCTAssertGreaterThanOrEqual(spot.minX, Plan.pagerReserve - 0.5)
            XCTAssertLessThanOrEqual(spot.maxX, window.width - Plan.pagerReserve + 0.5)
        }
        let strip = Plan.room(window, pictures: ["me", "stage"] + ids(20), mine: "me", stage: "stage")
        for id in strip.shown {
            XCTAssertGreaterThanOrEqual(strip.spots[id]!.minX, Plan.pagerReserve - 0.5)
            XCTAssertLessThanOrEqual(strip.spots[id]!.maxX, window.width - Plan.pagerReserve + 0.5)
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

    // MARK: The whole room (Room.tsx)

    /// Room.tsx `mine`: whichever page is showing, your own picture is on it,
    /// and it takes one of the nine places rather than making a tenth.
    func testYourOwnPictureNeverPagesAway() {
        let everybody = ["me"] + ids(20)
        var seen: Set<String> = []
        let first = Plan.room(window, pictures: everybody, mine: "me", stage: nil)
        XCTAssertEqual(first.pages, 3)
        for page in 0..<first.pages {
            let room = Plan.room(window, pictures: everybody, mine: "me", stage: nil, page: page)
            XCTAssertNotNil(room.spots["me"], "page \(page)")
            XCTAssertEqual(room.shown.first, "me")
            XCTAssertLessThanOrEqual(room.spots.count, Plan.tilesPerPage)
            seen.formUnion(room.shown)
        }
        XCTAssertEqual(seen, Set(everybody), "every picture is on some page")
    }

    func testYourOwnPictureStaysInTheStripToo() {
        let everybody = ["me", "stage"] + ids(30)
        let room = Plan.room(window, pictures: everybody, mine: "me", stage: "stage", page: 2)
        XCTAssertNotNil(room.spots["me"])
        XCTAssertNotNil(room.spots["stage"])
        let places = Plan.stripCapacity(CGSize(width: window.width - 2 * Plan.pagerReserve, height: window.height))
        XCTAssertEqual(room.spots.count, places + 1)
    }

    func testAPageThatNoLongerExistsShowsTheLastOne() {
        let room = Plan.room(window, pictures: ["me"] + ids(10), mine: "me", stage: nil, page: 7)
        XCTAssertEqual(room.page, room.pages - 1)
        XCTAssertFalse(room.shown.isEmpty)
    }

    func testTwoPeopleAreDrawnAsACallOfTwo() {
        let room = Plan.room(phone, pictures: ["me", "them"], mine: "me", stage: nil)
        XCTAssertEqual(room.spots["them"], CGRect(origin: .zero, size: phone))
        XCTAssertLessThan(room.spots["me"]!.width, phone.width / 2)
    }

    func testFillingTheScreenDrawsTheStageAlone() {
        let room = Plan.room(window, pictures: ["me", "stage"] + ids(4), mine: "me", stage: "stage", fullscreen: true)
        XCTAssertEqual(room.spots, ["stage": CGRect(origin: .zero, size: window)])
    }

    func testAStageThatIsNotInTheRoomIsTheGrid() {
        let room = Plan.room(window, pictures: ["me"] + ids(4), mine: "me", stage: "gone")
        XCTAssertEqual(room.spots.count, 5)
        XCTAssertNil(room.spots["gone"])
    }

    /// The strain card, stacked above the notices, is lifted clear of the
    /// filmstrip's pictures and of nothing else (plan.test.ts `clearOf`).
    func testTheStrainCardEndsAboveTheFilmstrip() {
        let room = Plan.room(window, pictures: ["me", "stage"] + ids(3), mine: "me", stage: "stage")
        let clear = Plan.clear(of: room.spots, ids: room.shown, container: window)
        let stripTop = room.shown.compactMap { room.spots[$0]?.minY }.min()!
        XCTAssertEqual(clear, window.height - stripTop + Plan.gap)
        // Sitting just above the controls, as it did, it ends inside the
        // strip, and goes up by the rest.
        let ends = Plan.island + 8 + 10
        let lift = Plan.lift(toClear: clear, ends: ends)
        XCTAssertGreaterThan(lift, 0)
        XCTAssertLessThanOrEqual(window.height - (ends + lift), stripTop - Plan.gap)
        // Already above it, under a pile of notices: where it is.
        XCTAssertEqual(Plan.lift(toClear: clear, ends: clear! + 40), 0)
        // No strip, nothing to clear.
        let grid = Plan.room(window, pictures: ["me"] + ids(3), mine: "me", stage: nil)
        XCTAssertNil(Plan.clear(of: grid.spots, ids: [], container: window))
        XCTAssertEqual(Plan.lift(toClear: nil, ends: ends), 0)
    }

    // MARK: Who has the stage (usePin.test.tsx)

    func testAShareTakesTheStageWhenItStartsAndGivesItBack() {
        var pin = StagePin()
        pin.update(pictures: ["a", "b"], shares: [])
        XCTAssertNil(pin.pinned)
        pin.update(pictures: ["a", "b", "a/screen"], shares: ["a/screen"])
        XCTAssertEqual(pin.pinned, "a/screen")
        pin.update(pictures: ["a", "b"], shares: [])
        XCTAssertNil(pin.pinned)
    }

    func testAShareAlreadyRunningOnArrivalTakesTheStage() {
        var pin = StagePin()
        pin.update(pictures: ["a", "a/screen"], shares: ["a/screen"])
        XCTAssertEqual(pin.pinned, "a/screen")
    }

    /// The way back to the grid: pressing the share on the stage shows
    /// everybody, and the share does not climb straight back on.
    func testTheGridCanBeChosenWhileSomebodyShares() {
        var pin = StagePin()
        pin.update(pictures: ["a", "b", "a/screen"], shares: ["a/screen"])
        pin.toggle("a/screen")
        pin.update(pictures: ["a", "b", "a/screen"], shares: ["a/screen"])
        XCTAssertNil(pin.pinned)
        // Until a share that is new starts.
        pin.update(pictures: ["a", "b", "a/screen", "b/screen"], shares: ["a/screen", "b/screen"])
        XCTAssertEqual(pin.pinned, "b/screen")
    }

    func testAShareDoesNotTakeAStagePinnedByHand() {
        var pin = StagePin()
        pin.update(pictures: ["a", "b"], shares: [])
        pin.pin("b")
        pin.update(pictures: ["a", "b", "a/screen"], shares: ["a/screen"])
        XCTAssertEqual(pin.pinned, "b")
        pin.update(pictures: ["a", "b"], shares: [])
        XCTAssertEqual(pin.pinned, "b", "an unrelated share ending keeps a pin made by hand")
    }

    func testAPinnedPersonLeavingEmptiesTheStage() {
        var pin = StagePin()
        pin.update(pictures: ["a", "b"], shares: [])
        pin.pin("b")
        pin.update(pictures: ["a"], shares: [])
        XCTAssertNil(pin.pinned)
    }

    func testTheSameTileTwiceTogglesOff() {
        var pin = StagePin()
        pin.toggle("a")
        XCTAssertEqual(pin.pinned, "a")
        pin.toggle("a")
        XCTAssertNil(pin.pinned)
    }

    func testTheFirstShareKeepsTheStageAndHandsItOn() {
        var pin = StagePin()
        pin.update(pictures: ["a", "b", "a/screen"], shares: ["a/screen"])
        pin.update(pictures: ["a", "b", "a/screen", "b/screen"], shares: ["a/screen", "b/screen"])
        XCTAssertEqual(pin.pinned, "a/screen", "a second share does not fight the first for the stage")
        pin.update(pictures: ["a", "b", "b/screen"], shares: ["b/screen"])
        XCTAssertEqual(pin.pinned, "b/screen", "the remaining share takes over rather than the grid")
    }

    // MARK: Who spoke last (useSpeakingOrder.test.tsx)

    func testSomebodyTalkingComesForwardAndStaysThroughAPause() {
        var record = SpeakingRecord()
        let room = ["a", "b", "c"]
        record.hear(speaking: [], present: room, at: 1)
        XCTAssertEqual(record.order(room), ["a", "b", "c"])
        record.hear(speaking: ["c"], present: room, at: 2)
        XCTAssertEqual(record.order(room), ["c", "a", "b"])
        record.hear(speaking: [], present: room, at: 3)
        XCTAssertEqual(record.order(room), ["c", "a", "b"])
        record.hear(speaking: ["b"], present: room, at: 4)
        XCTAssertEqual(record.order(room), ["b", "c", "a"])
    }

    func testSomebodyWhoLeftIsForgotten() {
        var record = SpeakingRecord()
        record.hear(speaking: ["c"], present: ["a", "c"], at: 1)
        record.hear(speaking: [], present: ["a"], at: 2)
        XCTAssertEqual(record.order(["a", "c"]), ["a", "c"])
    }
}
