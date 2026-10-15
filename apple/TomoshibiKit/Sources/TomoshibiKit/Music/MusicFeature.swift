import Observation
import SwiftUI
import TomoshibiCore

/// The song desk, layered over a call (web/src/live/jukebox.ts, music.ts,
/// sound.ts, duck.ts, components/room/MusicPanel.tsx).
///
/// (Being written: the room view already places what this provides.)
@MainActor
@Observable
public final class MusicFeature {
    unowned let call: CallModel

    public init(call: CallModel) {
        self.call = call
    }
}

/// The desk: what is playing, the queue, searching and adding.
struct MusicPanelView: View {
    let feature: MusicFeature
    let close: () -> Void
    var body: some View { Theme.surface }
}
