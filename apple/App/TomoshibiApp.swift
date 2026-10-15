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
        #endif
    }
}
