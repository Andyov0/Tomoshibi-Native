import SwiftUI
import TomoshibiCore

/// The room's name and how this connection is doing, at the head of the call
/// (components/room/RoomTitle.tsx, Signal.tsx). The light is green, amber or
/// red because a connection light is read at a glance and everybody already
/// knows which colour means which; it is the one place a second signal
/// colour appears.
struct RoomTitle: View {
    @Bindable var model: CallModel

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(colour)
                .frame(width: 8, height: 8)
                .accessibilityLabel(label)
                .help(label)
            Text(model.room)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
        }
        .padding(.horizontal, 12).frame(height: 30)
        .glassSurface(Capsule())
        .animation(Theme.settle, value: model.localQuality)
    }

    private var colour: Color {
        switch model.localQuality {
        case .excellent, .good: return Theme.good
        case .poor: return Theme.tally
        case .lost: return Theme.danger
        }
    }

    private var label: String {
        switch model.localQuality {
        case .excellent, .good: return t("Good connection")
        case .poor: return t("Poor connection")
        case .lost: return t("Connection lost")
        }
    }
}

/// Alone in the room: the link that brings somebody else in, or, for somebody
/// who does not run the room, who can (components/room/EmptyRoom.tsx).
struct EmptyRoomCard: View {
    @Bindable var model: CallModel
    @State private var copied = false
    @State private var notHost = false

    var body: some View {
        VStack(spacing: 12) {
            Text(t("Nobody else is here.")).font(.system(size: 15, weight: .medium)).foregroundStyle(Theme.foreground)
            if notHost {
                Text(t("Only whoever runs this room can let somebody else in."))
                    .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    .multilineTextAlignment(.center)
                    .transition(Theme.arrive)
            } else {
                Button(action: invite) {
                    Text(copied ? t("Link copied") : t("Invite somebody"))
                        .font(.system(size: 13.5, weight: .semibold))
                        .foregroundStyle(Theme.tallyForeground)
                        .padding(.horizontal, 16).padding(.vertical, 9)
                        .background(Capsule().fill(Theme.tally))
                }
                .buttonStyle(.plain)
                .transition(Theme.arrive)
            }
        }
        .padding(24)
        .frame(maxWidth: 360)
        .glassPane(cornerRadius: 26)
        .animation(Theme.settle, value: copied)
        .animation(Theme.settle, value: notHost)
    }

    private func invite() {
        Task {
            do {
                let made = try await model.deployment.createInvite(room: model.room, bearer: model.call.token())
                let link = model.deployment.inviteLink(token: made.token, room: model.room)
                #if os(macOS)
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(link, forType: .string)
                #else
                UIPasteboard.general.string = link
                #endif
                copied = true
                try? await Task.sleep(for: .seconds(3))
                copied = false
            } catch {
                notHost = true
            }
        }
    }
}
