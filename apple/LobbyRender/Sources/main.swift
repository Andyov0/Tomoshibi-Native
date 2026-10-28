import AppKit
import SwiftUI
@testable import TomoshibiKit

// The Mac lobby drawn into pictures by a debug build of the app's own views,
// in a window that is never put on any display: for checking how the lobby's
// animals sit on its stage, and how they give way to the camera's picture,
// when there is no Mac to run the app on that somebody can look at (the
// owner's away, the test VM cannot be woken, 2026-10-28).
//
//     swift build
//     .build/debug/lobby-render <out.png> <width> <height> <light|dark> [seconds]
//         The window's whole content (AppRoot over the app's own AppModel),
//         once the model has had the given seconds to reach its lobby.
//     .build/debug/lobby-render transition <out prefix> <width> <height>
//             <light|dark> <on|off> <seconds...>
//         The lobby's stage while the camera comes on (on) or goes off (off),
//         that many seconds in, one picture each (Transition.swift).
//
// The model is the app's, so the environment decides what it talks to and
// shows, as for the app: TOMOSHIBI_BASE (a local test deployment, never the
// real one), TOMOSHIBI_SIGNIN to be signed in, TOMOSHIBI_DEBUG_RECENTS for
// rooms on the recent list, TOMOSHIBI_DEBUG_PORTRAIT=<animal>[@<seconds>] to
// hold one animal still; -AppleLanguages for the language.
//
// What a picture drawn this way cannot show: Liquid Glass, which draws
// nothing into it and blanks whatever is inside it (TOMOSHIBI_NO_GLASS=1
// draws the app's own stand-in instead, and the transition needs it); the
// sidebar's material and the list on it, which come out blank though the
// model holds them; the camera's picture, there being no camera; and how
// fast anything is drawn in a real window.

@MainActor
func render(_ args: [String]) {
    let out = args[1]
    let size = NSSize(width: Double(args[2])!, height: Double(args[3])!)
    let dark = args[4] == "dark"
    let settle = args.count > 5 ? Double(args[5]) ?? 8 : 8

    let application = NSApplication.shared
    application.setActivationPolicy(.prohibited)
    application.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)

    let model = AppModel()
    let hosting = NSHostingView(rootView: AppRoot(app: model).frame(minWidth: 760, minHeight: 520))
    let window = offscreenWindow(size, hosting)

    let until = Date().addingTimeInterval(settle)
    while Date() < until {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        hosting.layoutSubtreeIfNeeded()
    }
    write(hosting, window, to: out)
    print("rendered", out, "screen:", model.screen,
          "upcoming:", model.lobby.upcoming.map(\.room), "recent:", model.lobby.recentHere.map(\.room))
    exit(0)
}

/// A window far off every display, with the app's own title bar (hidden,
/// the content under it), holding `view` at `size`.
@MainActor
func offscreenWindow(_ size: NSSize, _ view: NSView) -> NSWindow {
    let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -20000, y: -20000), size: size),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                          backing: .buffered, defer: false)
    window.titlebarAppearsTransparent = true
    window.titleVisibility = .hidden
    window.appearance = NSApp.appearance
    window.contentView = view
    window.setContentSize(size)
    return window
}

@MainActor
func write(_ view: NSView, _ window: NSWindow, to out: String) {
    window.display()
    view.layoutSubtreeIfNeeded()
    guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else {
        print("no picture"); exit(1)
    }
    view.cacheDisplay(in: view.bounds, to: rep)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
}

MainActor.assumeIsolated {
    let args = CommandLine.arguments
    if args.count > 1, args[1] == "transition" { renderTransition(Array(args.dropFirst())) } else { render(args) }
}
