import CoreGraphics
import SwiftUI
import XCTest
@testable import TomoshibiBasics

/// The column down the side of an iPhone Duo's screens (VerticalBar): which
/// edge the system's bars stand on, where a screen keeps a column of its own,
/// and where in the column the controls stand. The sizes are the Duo's: the
/// outer display 466 by 678 points upright with its column 84 wide, the inner
/// display 951 by 669 on its side and 669 by 951 upright.
final class VerticalBarTests: XCTestCase {
    let outer = CGSize(width: 466, height: 678)
    let side = CGSize(width: 951, height: 669)
    let upright = CGSize(width: 669, height: 951)

    // MARK: The edge

    func testTheSystemsWordStandsWhereverItIsGiven() {
        XCTAssertEqual(VerticalBar.edge(reported: .trailing, rehearsed: nil, divided: false, window: outer, rightToLeft: false), .trailing)
        XCTAssertEqual(VerticalBar.edge(reported: .leading, rehearsed: .trailing, divided: true, window: side, rightToLeft: false), .leading)
    }

    func testTheFoldOfAWindowOnItsSideStandsInForTheSystemsWord() {
        XCTAssertEqual(VerticalBar.edge(reported: nil, rehearsed: nil, divided: true, window: side, rightToLeft: false), .trailing,
                       "the inner display on its side, its bars down the right")
        XCTAssertNil(VerticalBar.edge(reported: nil, rehearsed: nil, divided: true, window: upright, rightToLeft: false),
                     "upright, the bars go back across the top and the foot")
        XCTAssertNil(VerticalBar.edge(reported: nil, rehearsed: nil, divided: false, window: side, rightToLeft: false),
                     "a phone on its side with no fold is any phone")
    }

    func testTheColumnFollowsTheHardwareNotTheReading() {
        XCTAssertEqual(VerticalBar.edge(reported: nil, rehearsed: nil, divided: true, window: side, rightToLeft: true), .leading,
                       "the right is the leading edge of a language read from right to left")
    }

    func testARehearsalStandsInOnlyWhereTheSystemWouldSpeak() {
        XCTAssertEqual(VerticalBar.edge(reported: nil, rehearsed: .trailing, divided: false, window: side, rightToLeft: false), .trailing)
        XCTAssertNil(VerticalBar.edge(reported: nil, rehearsed: .trailing, divided: false, window: upright, rightToLeft: false))
        XCTAssertNil(VerticalBar.edge(reported: nil, rehearsed: .trailing, divided: false, window: .zero, rightToLeft: false),
                     "nothing before the window is measured")
    }

    // MARK: The outer display's screens on the inner display

    /// The inner display on its side is laid out as the outer display is,
    /// with no sidebar; upright, and the outer display either way, are as
    /// they were.
    func testOnlyTheInnerDisplayOnItsSideTakesTheOuterDisplaysScreens() {
        XCTAssertTrue(VerticalBar.sideways(edge: .trailing, regular: true, window: side), "the inner display on its side")
        XCTAssertFalse(VerticalBar.sideways(edge: nil, regular: true, window: side),
                       "a wide window whose bars are across the top, an iPad's")
        XCTAssertFalse(VerticalBar.sideways(edge: .trailing, regular: true, window: upright), "the inner display upright")
        XCTAssertFalse(VerticalBar.sideways(edge: .trailing, regular: false, window: outer), "the outer display upright")
        XCTAssertFalse(VerticalBar.sideways(edge: .trailing, regular: false, window: CGSize(width: 678, height: 466)),
                       "the outer display on its side, which has the phone's screens already")
        XCTAssertFalse(VerticalBar.sideways(edge: .trailing, regular: true, window: .zero), "before the window is measured")
    }

    // MARK: A column of the screen's own

    func testNoColumnIsKeptWhereTheSystemKeepsOne() {
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 84, reaches: true, window: CGSize(width: 678, height: 466)), 0,
                       "the outer display's own column, even on a window wider than tall")
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 84, reaches: true, window: outer), 0)
    }

    func testTheInnerDisplayOnItsSideKeepsTheOuterDisplaysWidth() {
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 0, reaches: true, window: side), 84)
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 20, reaches: true, window: side), 64,
                       "a narrow inset is part of the column, which is 84 wide in all")
    }

    func testNoColumnIsKeptUprightBesideASidebarOrWithoutTheSystemsWord() {
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 0, reaches: true, window: upright), 0, "upright")
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 0, reaches: false, window: side), 0,
                       "a sidebar, or a sheet in the middle of the screen")
        XCTAssertEqual(VerticalBar.kept(edge: nil, inset: 0, reaches: true, window: side), 0, "no bars down the side")
        XCTAssertEqual(VerticalBar.kept(edge: .trailing, inset: 0, reaches: true, window: outer), 0,
                       "the outer display upright, whatever a passing moment reports")
    }

    func testWhatReachesTheSide() {
        // The page beside the sidebar of the lobby reaches the right; the
        // sidebar does not; a sheet in the middle reaches neither side.
        XCTAssertTrue(VerticalBar.reaches(.trailing, frame: CGRect(x: 284, y: 20, width: 667, height: 629), inset: 0,
                                          window: side, rightToLeft: false))
        XCTAssertFalse(VerticalBar.reaches(.trailing, frame: CGRect(x: 0, y: 20, width: 284, height: 629), inset: 0,
                                           window: side, rightToLeft: false))
        XCTAssertFalse(VerticalBar.reaches(.trailing, frame: CGRect(x: 150, y: 30, width: 651, height: 639), inset: 0,
                                           window: side, rightToLeft: false))
        XCTAssertTrue(VerticalBar.reaches(.trailing, frame: CGRect(x: 0, y: 0, width: 931, height: 669), inset: 20,
                                          window: side, rightToLeft: false), "inside a safe area that reaches it")
        XCTAssertTrue(VerticalBar.reaches(.leading, frame: CGRect(x: 0, y: 0, width: 400, height: 669), inset: 0,
                                          window: side, rightToLeft: false))
        XCTAssertTrue(VerticalBar.reaches(.leading, frame: CGRect(x: 551, y: 0, width: 400, height: 669), inset: 0,
                                          window: side, rightToLeft: true), "the leading edge of a right-to-left window")
    }

    /// A screen arriving is drawn at 97 percent of its size, its frame
    /// short of the side by a point and a half in a hundred; it reaches the
    /// side all the same, and the call arrives with its bar in the column.
    func testAScreenOnItsWayInReachesTheSide() {
        let whole = CGRect(x: 0, y: 20, width: 951, height: 629)
        let arriving = whole.insetBy(dx: whole.width * 0.015, dy: whole.height * 0.015)
        XCTAssertTrue(VerticalBar.reaches(.trailing, frame: arriving, inset: 0, window: side, rightToLeft: false))
        XCTAssertFalse(VerticalBar.reaches(.trailing, frame: CGRect(x: 150, y: 30, width: 651, height: 639), inset: 0,
                                           window: side, rightToLeft: false), "a sheet in the middle still does not")
    }

    // MARK: Where the controls stand in it

    /// The outer display's column as the screens found it before the inner
    /// display had one: below the camera's and the clock's occlusions at its
    /// head, to the foot of the view.
    func testTheOuterDisplaysColumnIsWhereItWas() {
        let occlusions = [CGRect(x: 399.67, y: 29.33, width: 37, height: 37), CGRect(x: 382, y: 0, width: 84, height: 174)]
        let column = VerticalBar.column(x: 382, width: 84, height: 644, top: 0, occlusions: occlusions, creases: [], least: 0)
        XCTAssertEqual(column, CGRect(x: 382, y: 182, width: 84, height: 462))
    }

    func testTheInnerDisplaysCameraPushesTheHeadDown() {
        // The camera at the top right of the inner display on its side,
        // while it is on, in a room 951 wide whose last 84 points are the
        // column.
        let camera = CGRect(x: 884.67, y: 9.33, width: 37, height: 37)
        let column = VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [camera], creases: [], least: 0)
        XCTAssertEqual(column?.minY ?? 0, camera.maxY + 8, accuracy: 0.01)
        XCTAssertEqual(column?.maxY ?? 0, 629, accuracy: 0.01)
        let off = VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [], creases: [], least: 0)
        XCTAssertEqual(off?.minY, 8, "with the camera off, from the head of the view")
    }

    /// The system's column on the inner display on its side has its clock
    /// at the head: where nothing is reported over it the head starts below
    /// where the clock stands, and where something is, that says where.
    func testTheSystemsColumnKeepsClearOfAClockItDoesNotReport() {
        let bare = VerticalBar.column(x: 867, width: 84, height: 649, top: 0, occlusions: [], creases: [], least: 0,
                                      headroom: VerticalBar.systemHeadroom)
        XCTAssertEqual(bare?.minY, VerticalBar.systemHeadroom + 8)
        let clock = CGRect(x: 867, y: 0, width: 84, height: 132)
        let reported = VerticalBar.column(x: 867, width: 84, height: 649, top: 0, occlusions: [clock], creases: [], least: 0,
                                          headroom: VerticalBar.systemHeadroom)
        XCTAssertEqual(reported?.minY, clock.maxY + 8, "what the system reports, even nearer the head than the least")
        let low = CGRect(x: 880, y: 560, width: 40, height: 40)
        XCTAssertEqual(VerticalBar.column(x: 867, width: 84, height: 649, top: 0, occlusions: [low], creases: [], least: 0,
                                          headroom: VerticalBar.systemHeadroom)?.minY,
                       VerticalBar.systemHeadroom + 8, "something at the foot says nothing of the head")
        // The outer display's head, which the system reports, is where it was.
        let occlusions = [CGRect(x: 399.67, y: 29.33, width: 37, height: 37), CGRect(x: 382, y: 0, width: 84, height: 174)]
        XCTAssertEqual(VerticalBar.column(x: 382, width: 84, height: 644, top: 0, occlusions: occlusions, creases: [], least: 0,
                                          headroom: VerticalBar.systemHeadroom),
                       CGRect(x: 382, y: 182, width: 84, height: 462))
    }

    func testAnOcclusionAtTheFootLiftsIt() {
        let low = CGRect(x: 880, y: 560, width: 40, height: 40)
        let column = VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [low], creases: [], least: 0)
        XCTAssertEqual(column?.maxY, 560)
        let beside = CGRect(x: 600, y: 560, width: 40, height: 40)
        XCTAssertEqual(VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [beside], creases: [], least: 0)?.maxY,
                       629, "one that misses the column changes nothing")
    }

    func testAFoldAcrossTheColumnLeavesItTheLargerPart() {
        let across = CGRect(x: 0, y: 400, width: 951, height: 24)
        let column = VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [], creases: [across], least: 0)
        XCTAssertEqual(column, CGRect(x: 867, y: 8, width: 84, height: 392))
        XCTAssertFalse(column.map { $0.intersects(across) } ?? true)
        // The fold of a book runs down the middle of the inner display on
        // its side, beside the column rather than across it.
        let down = CGRect(x: 463.5, y: 0, width: 24, height: 669)
        XCTAssertEqual(VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [], creases: [down], least: 0),
                       CGRect(x: 867, y: 8, width: 84, height: 621))
    }

    func testAColumnTooShortIsNone() {
        XCTAssertNil(VerticalBar.column(x: 867, width: 84, height: 300, top: 0, occlusions: [], creases: [], least: 340))
        XCTAssertNotNil(VerticalBar.column(x: 867, width: 84, height: 629, top: 0, occlusions: [], creases: [], least: 340))
    }
}
