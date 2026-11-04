#if os(macOS)
import AppKit

/// Windows whose close button is pointed somewhere else first, and the way
/// to close one of them afterwards as its close button would have.
///
/// The Mac's window asks before it closes while the screen is shared
/// (MacWindows in the kit): SwiftUI owns the window's delegate, so the
/// close button is pointed at the app instead. The app then closed the
/// window with `performClose`, which closes a window by pressing its close
/// button, and the button pressed was the one pointed at the app: the press
/// came straight back, and again, until the stack ran out. Every close of
/// the window out of a share, with the button or with ⌘W, ended in a crash
/// report rather than a quit (the owner's Mac, 0.1.7 (1), 2026-11-05: 9,324
/// presses deep). With the screen shared, Keep Sharing asked again instead
/// of closing, and Stop Sharing and Close crashed the same way.
///
/// What the button did before is kept here and done as it would have done
/// it: the window's own close (`_close:` on macOS 26 and 27), which asks
/// the window's delegate and quits the app with its last window, as a
/// press of the system's button does.
@MainActor
public final class CloseButtons {
    private struct Own {
        weak var target: AnyObject?
        let action: Selector?
    }

    private var own: [ObjectIdentifier: Own] = [:]

    public init() {}

    /// Points `window`'s close button at `action` on `target`, remembering
    /// what it did; a button pointed there already is left alone, so that
    /// what is remembered is never the pointing itself.
    public func point(_ window: NSWindow, at target: AnyObject, action: Selector) {
        guard let button = window.standardWindowButton(.closeButton),
              !(button.action == action && button.target === target) else { return }
        own[ObjectIdentifier(button)] = Own(target: button.target as AnyObject?, action: button.action)
        button.target = target
        button.action = action
    }

    /// Closes `window` as its close button did before it was pointed
    /// elsewhere, without pressing the button. A window whose button was
    /// never pointed is closed as any other; one whose button's own target
    /// has gone is asked through its delegate and closed.
    public func close(_ window: NSWindow) {
        guard let button = window.standardWindowButton(.closeButton),
              let kept = own[ObjectIdentifier(button)] else {
            window.performClose(nil)
            return
        }
        if let target = kept.target, let action = kept.action,
           NSApp.sendAction(action, to: target, from: button) { return }
        if window.delegate?.windowShouldClose?(window) ?? true { window.close() }
    }
}
#endif
