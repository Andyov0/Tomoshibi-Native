import Foundation
import SwiftUI

/// Whether the lobby's animals are being drawn, and how far the drawing has
/// got: the round's clock, which runs only while somebody can see the
/// drawing and keeps its place while nobody can, so that coming back to the
/// window carries on from the stroke it stopped at rather than starting the
/// round over or jumping ahead by the time it was away.
///
/// Apart from the views (TomoshibiKit PreviewGlass) so that `swift test`
/// can hold it to that; the views only tell it what they can see.
public struct PortraitPlayback: Equatable, Sendable {
    /// When the round's second zero was, while it runs.
    public private(set) var origin: Date?
    /// How far it had got when it last stopped.
    public private(set) var stopped: Double = 0

    public init() {}

    public var running: Bool { origin != nil }

    /// Starts the clock, or carries it on from where it stopped. Running
    /// already, it is left as it is.
    public mutating func run(at now: Date) {
        guard origin == nil else { return }
        origin = now.addingTimeInterval(-stopped)
    }

    /// Stops the clock where it is. Stopped already, it is left as it is.
    public mutating func stop(at now: Date) {
        guard let origin else { return }
        stopped = now.timeIntervalSince(origin)
        self.origin = nil
    }

    /// Seconds into the round at `now`: moving while the clock runs, still
    /// while it does not.
    public func elapsed(at now: Date) -> Double {
        origin.map { now.timeIntervalSince($0) } ?? stopped
    }

    /// Whether the drawing should move: not while it is covered (a sheet over
    /// the phone's lobby, or the Mac's window not the one in front), not
    /// while it is out of sight (scrolled away), not while the scene is not
    /// the active one (the app in the background), and never with the
    /// system's Reduce Motion on.
    public static func runs(held: Bool, inSight: Bool, phase: ScenePhase, reduceMotion: Bool) -> Bool {
        !held && inSight && phase == .active && !reduceMotion
    }

    /// Whether a Mac window's drawing is seen: the window is the key window
    /// (the app in front, and in it this window rather than the Settings, a
    /// sheet over the lobby or a menu), not in the Dock, and not wholly
    /// covered. The scene's phase was all the drawing went by, and it speaks
    /// for the scene, not for whether the Settings window, a sheet or another
    /// app's window has come in front of the lobby's; whether the window is
    /// the key one does.
    public static func watched(key: Bool, miniaturized: Bool, visible: Bool) -> Bool {
        key && !miniaturized && visible
    }
}
