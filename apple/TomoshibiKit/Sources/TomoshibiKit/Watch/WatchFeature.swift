import Observation
import SwiftUI
import TomoshibiCore

/// Watching together, layered over a call (web/src/live/watch.ts,
/// components/room/WatchScreen.tsx, WatchPanel.tsx, WatchTab.tsx).
///
/// (Being written: the room view already places what this provides.)
@MainActor
@Observable
public final class WatchFeature {
    unowned let call: CallModel

    /// Whether a show is running and its picture belongs on the stage.
    public private(set) var onStage = false

    public init(call: CallModel) {
        self.call = call
    }
}

/// The video everybody is watching, drawn on the stage.
struct WatchStageView: View {
    let feature: WatchFeature
    var body: some View { Theme.surface }
}

/// The queue, the box a link is pasted into, the media library.
struct WatchPanelView: View {
    let feature: WatchFeature
    let close: () -> Void
    var body: some View { Theme.surface }
}
