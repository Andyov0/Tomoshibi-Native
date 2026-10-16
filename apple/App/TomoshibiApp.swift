import SwiftUI
import TomoshibiKit

@main
struct TomoshibiApp: App {
    @State private var app = AppModel()

    var body: some Scene {
        WindowGroup {
            AppRoot(app: app)
                #if os(macOS)
                .frame(minWidth: 480, minHeight: 360)
                #endif
        }
        #if os(macOS)
        .windowStyle(.hiddenTitleBar)
        .commands {
            // The two switches people reach for without looking, on the keys
            // other call apps on the Mac use for them.
            CommandMenu(t("Call")) {
                Button(app.call?.microphoneOn == true ? t("Mute microphone") : t("Unmute microphone")) {
                    guard let call = app.call else { return }
                    Task { try? await call.setMicrophone(!call.microphoneOn) }
                }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(app.call == nil)
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
