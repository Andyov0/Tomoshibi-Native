import CoreGraphics
import XCTest
@testable import TomoshibiBasics

/// A call on an iPhone Duo held half open: nothing of the room lies across the
/// fold. The sizes are the Duo's inner display, 951 by 669 points held as a
/// book and 669 by 951 standing on a table, with a fold 24 points wide down or
/// across the middle.
final class FoldedPlanTests: XCTestCase {
    let book = CGSize(width: 951, height: 669)
    let table = CGSize(width: 669, height: 951)
    let band: CGFloat = 24

    var bookHalves: (Plan.Halves, crease: CGRect) {
        let crease = CGRect(x: book.width / 2 - band / 2, y: 0, width: band, height: book.height)
        let first = CGRect(x: 0, y: 0, width: crease.minX, height: book.height)
        let second = CGRect(x: crease.maxX, y: 0, width: book.width - crease.maxX, height: book.height)
        return (.book(first: first, second: second), crease)
    }

    var tableHalves: (Plan.Halves, crease: CGRect) {
        let crease = CGRect(x: 0, y: table.height / 2 - band / 2, width: table.width, height: band)
        let upper = CGRect(x: 0, y: 0, width: table.width, height: crease.minY)
        let lower = CGRect(x: 0, y: crease.maxY, width: table.width, height: table.height - crease.maxY)
        return (.table(upper: upper, lower: lower), crease)
    }

    func people(_ count: Int) -> [String] { ["me"] + (1..<max(1, count)).map { "p\($0)" } }

    /// The ways a room is in: everybody in a grid, somebody on the stage, the
    /// stage filling the screen, a panel open.
    func cases(_ count: Int) -> [(stage: String?, fullscreen: Bool, panel: Bool)] {
        let other = count > 1 ? "p1" : "me"
        return [(nil, false, false), (nil, false, true), (other, false, false), (other, false, true), (other, true, false)]
    }

    func assertClear(_ folded: Plan.Folded, of crease: CGRect, halves: Plan.Halves, _ what: String) {
        let (a, b): (CGRect, CGRect) = switch halves {
        case let .book(first, second): (first, second)
        case let .table(upper, lower): (upper, lower)
        }
        for (id, spot) in folded.room.spots {
            XCTAssertFalse(spot.insetBy(dx: 0.5, dy: 0.5).intersects(crease), "\(id) across the fold: \(what)")
            XCTAssertTrue(a.insetBy(dx: -0.5, dy: -0.5).contains(spot) || b.insetBy(dx: -0.5, dy: -0.5).contains(spot),
                          "\(id) not on one side: \(what)")
        }
        XCTAssertFalse(folded.controls.insetBy(dx: 0.5, dy: 0.5).intersects(crease), "the controls across the fold: \(what)")
        XCTAssertFalse(folded.panel.insetBy(dx: 0.5, dy: 0.5).intersects(crease), "the panel across the fold: \(what)")
        XCTAssertFalse(folded.centre.insetBy(dx: 0.5, dy: 0.5).intersects(crease), "the centre across the fold: \(what)")
    }

    func testNothingLiesAcrossTheFoldOfABook() {
        let (halves, crease) = bookHalves
        for count in 1...16 {
            for one in cases(count) {
                let folded = Plan.folded(halves, pictures: people(count), mine: "me", stage: one.stage,
                                         fullscreen: one.fullscreen, panelOpen: one.panel)
                assertClear(folded, of: crease, halves: halves, "book, \(count) people, \(one)")
                // Filling the screen shows the stage alone, flat or folded.
                if !one.fullscreen {
                    XCTAssertNotNil(folded.room.spots["me"], "yourself, never paged away: book, \(count), \(one)")
                }
            }
        }
    }

    func testNothingLiesAcrossTheFoldOfAPhoneOnATable() {
        let (halves, crease) = tableHalves
        for count in 1...16 {
            for one in cases(count) {
                let folded = Plan.folded(halves, pictures: people(count), mine: "me", stage: one.stage,
                                         fullscreen: one.fullscreen, panelOpen: one.panel)
                assertClear(folded, of: crease, halves: halves, "table, \(count) people, \(one)")
            }
        }
    }

    /// Standing on a table, what is looked at is up top and what is touched
    /// lies on the table: every picture above the fold, the controls and the
    /// panel below it.
    func testOnATableThePicturesStandAndTheControlsLie() {
        let (halves, crease) = tableHalves
        for count in [1, 2, 5, 12] {
            let folded = Plan.folded(halves, pictures: people(count), mine: "me", stage: nil, panelOpen: true)
            for spot in folded.room.spots.values { XCTAssertLessThanOrEqual(spot.maxY, crease.minY + 0.5) }
            XCTAssertGreaterThanOrEqual(folded.controls.minY, crease.maxY - 0.5)
            XCTAssertGreaterThanOrEqual(folded.panel.minY, crease.maxY - 0.5)
        }
    }

    /// Held as a book, a call of two is a person on each page, and the
    /// person talked to on the first.
    func testACallOfTwoIsOnePersonAPage() {
        let (halves, crease) = bookHalves
        let folded = Plan.folded(halves, pictures: ["me", "p1"], mine: "me", stage: nil)
        XCTAssertLessThanOrEqual(folded.room.spots["p1"]!.maxX, crease.minX + 0.5)
        XCTAssertGreaterThanOrEqual(folded.room.spots["me"]!.minX, crease.maxX - 0.5)
    }

    /// Shared between the pages, the pictures are one size, as one grid would
    /// draw them: a lone picture on a page drawn larger than the rest read as
    /// somebody singled out.
    func testAGridSharedBetweenPagesIsOneSize() {
        let (halves, _) = bookHalves
        for count in 3...9 {
            let sizes = Set(Plan.folded(halves, pictures: people(count), mine: "me", stage: nil)
                .room.spots.values.map { "\(Int($0.width))x\(Int($0.height))" })
            XCTAssertEqual(sizes.count, 1, "\(count) people: \(sizes)")
        }
    }

    /// And on one line across the fold, clear of the controls: each grid
    /// centred on its own page, the lone picture on the first page of a call
    /// of three stood higher than the two on the second.
    func testAGridSharedBetweenPagesIsOnOneLine() {
        let (halves, crease) = bookHalves
        for count in 3...9 {
            let spots = Plan.folded(halves, pictures: people(count), mine: "me", stage: nil).room.spots.values
            let first = spots.filter { $0.maxX <= crease.minX + 0.5 }.reduce(CGRect.null) { $0.union($1) }
            let second = spots.filter { $0.minX >= crease.maxX - 0.5 }.reduce(CGRect.null) { $0.union($1) }
            XCTAssertEqual(first.midY, second.midY, accuracy: 0.5, "\(count) people: \(first) and \(second)")
            XCTAssertLessThanOrEqual(max(first.maxY, second.maxY), book.height - Plan.island + 0.5, "\(count) people: under the controls")
        }
    }
}
