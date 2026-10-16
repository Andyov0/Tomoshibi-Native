import SwiftUI
import TomoshibiKit
#if os(macOS)
import AppKit
#endif

@main
struct TomoshibiApp: App {
    @State private var app = AppModel()

    var body: some Scene {
        WindowGroup {
            AppRoot(app: app)
                #if os(macOS)
                .frame(minWidth: 480, minHeight: 360)
                // A link opened with the app goes to the window already open
                // rather than to a new one: there is one call at a time, and
                // a second window drew the same call twice.
                .handlesExternalEvents(preferring: ["*"], allowing: ["*"])
                #endif
        }
        #if os(macOS)
        .windowStyle(.hiddenTitleBar)
        .commands {
            // No second window. Every window is the same app on the same
            // call, and ⌘N opened one that put the join screen over a call
            // still live; the window closed and reopened from the Dock comes
            // back on whatever screen the app is on. Taking New Window away
            // took the whole File menu with it, Close and its ⌘W included,
            // so Close is put back in its place, in place of the system's
            // Close and Close All, which then came back beside it twice over.
            // A single Window scene instead has no File menu at all, and the
            // app went away, call and all, when its window was closed.
            CommandGroup(replacing: .saveItem) {}
            CommandGroup(replacing: .newItem) {
                Button(t("Close")) {
                    (NSApp.keyWindow ?? NSApp.orderedWindows.first { $0.isVisible && $0.canBecomeKey })?.performClose(nil)
                }
                .keyboardShortcut("w", modifiers: .command)
            }
            // The two switches people reach for without looking, on the keys
            // other call apps on the Mac use for them.
            CommandMenu(t("Call")) {
                // Through the app rather than straight to the call, so that a
                // press while the join is still bringing the microphone up,
                // or while the permission prompt is open, waits rather than
                // publishing a second microphone (AppModel `toggleMicrophone`).
                Button(app.call?.microphoneOn == true ? t("Mute microphone") : t("Unmute microphone")) {
                    Task { await app.toggleMicrophone() }
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(app.call == nil || app.switchingMicrophone)
                Button(app.call?.cameraOn == true ? t("Turn camera off") : t("Turn camera on")) {
                    guard let call = app.call else { return }
                    Task { try? await call.setCamera(!call.cameraOn) }
                }
                .keyboardShortcut("e", modifiers: .command)
                .disabled(app.call == nil)
                Divider()
                Button(t("Leave")) { Task { await app.leave() } }
                    .disabled(app.call == nil)
            }
        }
        #endif
        #if os(macOS)
        // The app's own Settings window, on ⌘, as every Mac app has it, drawn
        // as the app's own: a glass sidebar of sections over the evening
        // room, resizable down to what its two columns need.
        Settings {
            SettingsView(app: app)
        }
        .windowResizability(.contentMinSize)
        #endif
    }
}

