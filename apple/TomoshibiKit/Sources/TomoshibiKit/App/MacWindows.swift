#if os(macOS)
import AppKit
import Observation
import TomoshibiBasics

/// What closing the window does on the Mac (owner, 2026-10-22). Out of a
/// call, closing the window quits Tomoshibi: nothing is left behind in the
/// Dock or the menu bar. In a call, the app stays in the background with
/// the call going on, and its Dock icon brings the window back on the same
/// call; and once that call ends with no window open, the app quits as it
/// would have when the window closed. Sharing the screen, closing the
/// window first says that Tomoshibi is still looking at the screen, and
/// asks whether to go on sharing in the background, stop the share and
/// close, or keep the window.
///
/// The window's close button and ⌘W are what ask (`guardClose`). SwiftUI
/// owns the window's delegate, so rather than standing in front of it the
/// close button is pointed here, and the Close command asks the same.
@MainActor
public final class MacWindows: NSObject, NSApplicationDelegate {
    public private(set) weak var app: AppModel?
    private var watching = false

    public static weak var shared: MacWindows?

    override public init() {
        super.init()
        Self.shared = self
    }

    /// The model the window shows, told once the window is up.
    public func attach(_ app: AppModel) {
        self.app = app
        guardButtons()
        watchCall()
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        NotificationCenter.default.addObserver(forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.guardButtons() }
        }
    }

    /// Quits with the last window out of a call; keeps going in one.
    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        !inCall
    }

    private var inCall: Bool {
        guard let app else { return false }
        return app.call != nil || app.movingTo != nil
    }

    /// The window the call is in: not the settings, not a panel.
    private var callWindows: [NSWindow] {
        NSApp.windows.filter { window in
            !(window is NSPanel) && window.canBecomeMain
                && !(window.identifier?.rawValue.localizedCaseInsensitiveContains("settings") ?? false)
        }
    }

    /// Points each window's close button at `closePressed`, once.
    private func guardButtons() {
        for window in callWindows {
            guard let button = window.standardWindowButton(.closeButton), button.action != #selector(closePressed(_:)) else { continue }
            button.target = self
            button.action = #selector(closePressed(_:))
        }
    }

    @objc private func closePressed(_ sender: NSButton) {
        guard let window = sender.window else { return }
        guardClose(window)
    }

    /// Closes the window, having asked first if the screen is being shared.
    public func guardClose(_ window: NSWindow) {
        guard let call = app?.call, call.sharing.active else {
            window.performClose(nil)
            return
        }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = t("Tomoshibi is still accessing your screen")
        alert.informativeText = t("You are sharing your screen in this call. If you close the window, the call and the share go on in the background.")
        alert.addButton(withTitle: t("Keep sharing"))
        alert.addButton(withTitle: t("Stop sharing and close"))
        alert.addButton(withTitle: t("Cancel"))
        alert.beginSheetModal(for: window) { answer in
            MainActor.assumeIsolated {
                switch answer {
                case .alertFirstButtonReturn:
                    window.performClose(nil)
                case .alertSecondButtonReturn:
                    Task { @MainActor in
                        await call.sharing.stopScreen()
                        window.performClose(nil)
                    }
                default:
                    break
                }
            }
        }
    }

    /// Quits when a call ends with no window open: it was only the call that
    /// kept the app going once its window was closed.
    private func watchCall() {
        guard !watching else { return }
        watching = true
        rearm()
    }

    private func rearm() {
        withObservationTracking {
            _ = app?.call
            _ = app?.movingTo
        } onChange: { [weak self] in
            Task { @MainActor in
                guard let self else { return }
                self.callChanged()
                self.rearm()
            }
        }
    }

    /// A window in the Dock still shows the call's end when it comes back,
    /// so the app stays for it. With none, the app goes, and the window it
    /// would open next is the lobby's rather than an end nobody saw.
    private func callChanged() {
        guard !inCall, !callWindows.contains(where: { $0.isVisible || $0.isMiniaturized }) else { return }
        app?.backToLobby()
        NSApp.terminate(nil)
    }
}
#endif
