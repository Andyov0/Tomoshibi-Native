#if os(macOS)
import AppKit
@testable import TomoshibiBasics
import XCTest

/// The Mac's window, its close button pointed at the app, closed afterwards
/// as the button would have closed it (CloseButtons). It used to be closed
/// with `performClose`, which pressed the pointed button again, until the
/// stack ran out: the owner's Mac reported a crash every time the window was
/// closed (0.1.7 (1), 2026-11-05).
@MainActor
final class CloseButtonsTests: XCTestCase {
    /// The app as MacWindows is to the button: asked on a press, and closing
    /// the window through CloseButtons. A press that comes back is the fault
    /// itself, counted and not followed, so that the test fails rather than
    /// running the stack out.
    @MainActor private final class Asker: NSObject {
        let buttons: CloseButtons
        var presses = 0
        init(_ buttons: CloseButtons) { self.buttons = buttons }
        @objc func pressed(_ sender: NSButton) {
            presses += 1
            guard presses == 1, let window = sender.window else { return }
            buttons.close(window)
        }
    }

    /// The window's delegate, standing for SwiftUI's: asked whether the
    /// window may close, and told when it does.
    @MainActor private final class Delegate: NSObject, NSWindowDelegate {
        var allows = true
        var asked = 0
        var closed = 0
        func windowShouldClose(_ sender: NSWindow) -> Bool {
            asked += 1
            return allows
        }
        func windowWillClose(_ notification: Notification) { closed += 1 }
    }

    private func window(_ delegate: Delegate) -> NSWindow {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: -4000, y: -4000, width: 200, height: 100),
                              styleMask: [.titled, .closable], backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        window.delegate = delegate
        return window
    }

    func testAPressOfThePointedButtonClosesTheWindowOnce() {
        let delegate = Delegate()
        let window = window(delegate)
        let buttons = CloseButtons()
        let asker = Asker(buttons)
        buttons.point(window, at: asker, action: #selector(Asker.pressed(_:)))
        // Pointed again when the window next becomes key: what is kept must
        // still be the button's own close, not the pointing.
        buttons.point(window, at: asker, action: #selector(Asker.pressed(_:)))
        window.standardWindowButton(.closeButton)?.performClick(nil)
        XCTAssertEqual(asker.presses, 1, "closing the window pressed its close button again")
        XCTAssertEqual(delegate.asked, 1, "the window's delegate was not asked")
        XCTAssertEqual(delegate.closed, 1, "the window did not close")
    }

    func testAWindowItsDelegateKeepsOpenStaysOpen() {
        let delegate = Delegate()
        delegate.allows = false
        let window = window(delegate)
        let buttons = CloseButtons()
        let asker = Asker(buttons)
        buttons.point(window, at: asker, action: #selector(Asker.pressed(_:)))
        window.standardWindowButton(.closeButton)?.performClick(nil)
        XCTAssertEqual(asker.presses, 1)
        XCTAssertEqual(delegate.asked, 1)
        XCTAssertEqual(delegate.closed, 0)
    }

    func testAWindowWhoseButtonWasNeverPointedClosesAsAnyOther() {
        let delegate = Delegate()
        let window = window(delegate)
        CloseButtons().close(window)
        XCTAssertEqual(delegate.asked, 1)
        XCTAssertEqual(delegate.closed, 1)
    }
}
#endif
