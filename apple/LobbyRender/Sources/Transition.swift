import AppKit
import SwiftUI
@testable import TomoshibiKit
import TomoshibiBasics

// The lobby's stage part of the way through the camera coming on or going
// off, with the time held still: each layer drawn at the opacity its fade
// has reached by then, on the curve HomeStage gives it.

/// A cubic-bezier timing curve: how much of the change is done when `u` of
/// the time has gone, as SwiftUI's `timingCurve` animations have it.
func timing(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double, _ u: Double) -> Double {
    let u = min(max(u, 0), 1)
    func bez(_ a: Double, _ b: Double, _ t: Double) -> Double {
        3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t
    }
    var lo = 0.0, hi = 1.0
    for _ in 0..<60 {
        let mid = (lo + hi) / 2
        if bez(x1, x2, mid) < u { lo = mid } else { hi = mid }
    }
    return bez(y1, y2, (lo + hi) / 2)
}

/// Theme `settle` and `leave`, `s` seconds in.
func settle(_ s: Double) -> Double { timing(0.2, 0.9, 0.3, 1, s / Theme.settleSeconds) }
func leave(_ s: Double) -> Double { timing(0.4, 0, 1, 1, s / Theme.leaveSeconds) }

/// The stage as HomeStage stacks it, each layer at the opacity given: the
/// ground, the camera's picture (a flat stand-in, there being no camera to
/// draw offscreen), the animals (StageDrawing, as the app draws them), the
/// greeting, and the switches in their capsule.
struct TransitionStage: View {
    let app: AppModel
    let picture: Double
    let drawing: Double

    var body: some View {
        ZStack(alignment: .topLeading) {
            Backdrop()
            ZStack {
                Color(red: 0.36, green: 0.39, blue: 0.43)
                Text("Camera picture (a stand-in: there is no camera offscreen)")
                    .font(.system(size: 15)).foregroundStyle(.white.opacity(0.8))
            }
            .opacity(picture)
            StageDrawing().opacity(drawing)
            VStack(alignment: .leading, spacing: 6) {
                Text(t("Hello, {name}", ["name": app.account?.name ?? "uitest"])).font(.system(size: 34, weight: .bold))
                Text(t("Start one from the sidebar when you are ready.")).font(.system(size: 15)).foregroundStyle(Theme.ink.opacity(0.78))
            }
            .shadow(color: Theme.shadow(0.45), radius: 16, y: 2)
            .padding(.leading, 36).padding(.top, 56)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay(alignment: .bottom) {
            DeviceSwitches(app: app)
                .padding(8)
                .glassSurface(Capsule())
                .padding(.bottom, 28)
        }
    }
}

/// transition <out prefix> <width> <height> <light|dark> <on|off> <seconds...>
@MainActor
func renderTransition(_ args: [String]) {
    let prefix = args[1]
    let size = NSSize(width: Double(args[2])!, height: Double(args[3])!)
    let dark = args[4] == "dark"
    let comingOn = args[5] == "on"
    let moments = args.dropFirst(6).compactMap(Double.init)

    let application = NSApplication.shared
    application.setActivationPolicy(.prohibited)
    application.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    let model = AppModel()
    // LEAVE=1 has the animals go on the leaving curve, the one HomeStage
    // does not use, to set the two side by side.
    let leaving = ProcessInfo.processInfo.environment["LEAVE"] != nil

    for s in moments {
        // Both layers on the settling curve: off to on, the picture comes in
        // as the animals go; on to off, the picture goes as the animals come
        // back, a new round still in its first empty moment.
        let picture = comingOn ? settle(s) : 1 - settle(s)
        let drawing = comingOn ? 1 - (leaving ? leave(s) : settle(s)) : settle(s)
        let hosting = NSHostingView(rootView: TransitionStage(app: model, picture: picture, drawing: drawing))
        let window = offscreenWindow(size, hosting)
        for _ in 0..<20 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
            hosting.layoutSubtreeIfNeeded()
        }
        let out = "\(prefix)-\(String(format: "%.2f", s)).png"
        write(hosting, window, to: out)
        print("frame", String(format: "%.2f", s), "picture", String(format: "%.2f", picture),
              "drawing", String(format: "%.2f", drawing), out)
    }
    exit(0)
}
