import os
import SwiftUI
@testable import TomoshibiBasics
import XCTest

/*
 * Which appearance the iPhone's screens around a call are drawn in, and what
 * the window is told (WindowLook), through the reports a window and its
 * scene give in the order the simulator gave them (iOS 27.1): the scene's
 * change first and the window's a moment later, the scene alone turned to
 * the other appearance while the app is photographed on its way into the
 * background, and the window alone dark while a call holds it.
 */
@MainActor
final class WindowLookTests: XCTestCase {
    /// The window is told the call's appearance while the call holds it and
    /// nothing at all otherwise: told the system's appearance by name, it
    /// would keep that one when the system changed.
    func testTheWindowIsToldTheCallsAppearanceAndThenNothing() {
        XCTAssertEqual(WindowLook.window(holdsCall: true, meeting: .dark), .dark)
        XCTAssertNil(WindowLook.window(holdsCall: false, meeting: .dark), "after the call the window follows the system")
        XCTAssertNil(WindowLook.window(holdsCall: true, meeting: nil), "a call that follows the system tells the window nothing")
    }

    /// Before the window has said anything, the screens take what the
    /// window was drawn in; after, what it said.
    func testBeforeAnyReportTheScreensTakeTheWindows() {
        let look = WindowLook()
        XCTAssertNil(look.system)
        XCTAssertEqual(look.outside(window: .light), .light)
        XCTAssertEqual(look.outside(window: .dark), .dark)
        look.report(window: .light, scene: .light, windowTold: false)
        XCTAssertEqual(look.outside(window: .dark), .light, "a window still dark from a call is not the system")
    }

    /// The system changed with the app in front, behind it, and while a call
    /// held the window.
    func testTheScreensFollowTheSystemAndNotTheCall() {
        let look = WindowLook()
        look.report(window: .light, scene: .light, windowTold: false)
        XCTAssertEqual(look.system, .light)

        // Photographed in the other appearance on the way into the
        // background: the scene first, then the window; the screens are
        // drawn for the photograph in what the window says.
        look.report(window: .light, scene: .dark, windowTold: false)
        XCTAssertEqual(look.system, .light, "the window still drawn light")
        look.report(window: .dark, scene: .dark, windowTold: false)
        XCTAssertEqual(look.system, .dark, "the photograph in the dark had light screens")
        look.report(window: .light, scene: .light, windowTold: false)
        XCTAssertEqual(look.system, .light)

        // A call takes the window dark; the system is still light.
        look.report(window: .dark, scene: .light, windowTold: true)
        XCTAssertEqual(look.system, .light, "the call's dark taken for the system's")
        // The system turns dark during the call, and light again.
        look.report(window: .dark, scene: .dark, windowTold: true)
        XCTAssertEqual(look.system, .dark, "a change during the call was missed")
        look.report(window: .dark, scene: .light, windowTold: true)
        XCTAssertEqual(look.system, .light)
        // The call over, the window follows the system again.
        look.report(window: .light, scene: .light, windowTold: false)
        XCTAssertEqual(look.system, .light)
        look.report(window: .dark, scene: .dark, windowTold: false)
        XCTAssertEqual(look.system, .dark, "after the call the system was no longer followed")
    }

    /// A style that says nothing leaves the last word standing; where only
    /// the scene says, its word is taken.
    func testAnUnspecifiedReportChangesNothing() {
        let look = WindowLook()
        look.report(window: nil, scene: nil, windowTold: false)
        XCTAssertNil(look.system)
        look.report(window: .light, scene: .light, windowTold: false)
        look.report(window: nil, scene: nil, windowTold: false)
        XCTAssertEqual(look.system, .light, "an unspecified style taken as dark")
        look.report(window: nil, scene: .dark, windowTold: false)
        XCTAssertEqual(look.system, .dark)
        look.report(window: .light, scene: nil, windowTold: true)
        XCTAssertEqual(look.system, .dark, "the window's word taken while the call held it")
    }

    /// A change redraws whatever read the appearance, at once; a report of
    /// the same appearance redraws nothing.
    func testAChangeRedrawsTheScreens() {
        let look = WindowLook()
        look.report(window: .light, scene: .light, windowTold: false)
        let changed = OSAllocatedUnfairLock(initialState: 0)
        withObservationTracking {
            _ = look.outside(window: .light)
        } onChange: {
            changed.withLock { $0 += 1 }
        }
        look.report(window: .light, scene: .light, windowTold: false)
        XCTAssertEqual(changed.withLock { $0 }, 0, "the same appearance redrew the screens")
        look.report(window: .dark, scene: .dark, windowTold: false)
        XCTAssertEqual(changed.withLock { $0 }, 1, "the screens were not redrawn when the system changed")
        XCTAssertEqual(look.outside(window: .light), .dark)
    }
}
