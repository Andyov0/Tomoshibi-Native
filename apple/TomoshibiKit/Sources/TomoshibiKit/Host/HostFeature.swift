import Observation
import SwiftUI
import TomoshibiCore

/// Running a room and hearing it: the host's controls, the door, invitations,
/// every person's volume, reactions (web/src/live/host.ts, knock.ts,
/// hearing.ts, hands.ts; components/room/HostPanel.tsx, Hosting.tsx,
/// AtTheDoor.tsx, SoundPanel.tsx, HandButton.tsx, Reacting.tsx).
///
/// (Being written: the room view already places what this provides.)
@MainActor
@Observable
public final class HostFeature {
    unowned let call: CallModel

    public init(call: CallModel) {
        self.call = call
    }
}

/// Mute, remove, hand over, move, invite, and the door.
struct HostPanelView: View {
    let feature: HostFeature
    let close: () -> Void
    var body: some View { Theme.surface }
}

/// Every person's voice, screen sound and music, each turned up, down or off.
struct SoundPanelView: View {
    let feature: HostFeature
    let close: () -> Void
    var body: some View { Theme.surface }
}

/// What is drawn over the whole call: floating reactions, somebody waiting at
/// the door, the move overlay.
struct CallOverlayView: View {
    let feature: HostFeature
    var body: some View { EmptyView() }
}
