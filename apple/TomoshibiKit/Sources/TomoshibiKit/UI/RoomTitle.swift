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
///
/// A small capsule above the controls that can be put away, as the web's is a
/// single quiet button. It used to be a pane of glass with a heading laid over
/// the middle of your own picture, with no way to close it, and the owner
/// found it jarring (2026-10-16).
struct EmptyRoomCard: View {
    @Bindable var model: CallModel
    let dismiss: () -> Void
    @State private var copied = false
    @State private var notHost = false
    @State private var minting = false

    var body: some View {
        HStack(spacing: 0) {
            if notHost {
                Text(t("Only whoever runs this room can let somebody else in."))
                    .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    .lineLimit(2)
                    .padding(.leading, 16).padding(.trailing, 6)
                    .transition(Theme.arrive)
            } else {
                Button(action: invite) {
                    HStack(spacing: 7) {
                        ZStack {
                            if minting {
                                ProgressView().controlSize(.mini)
                            } else {
                                Image(systemName: copied ? "checkmark" : "link")
                                    .foregroundStyle(copied ? Theme.tally : Theme.foreground)
                                    .transition(.scale.combined(with: .opacity))
                            }
                        }
                        .frame(width: 14)
                        Text(copied ? t("Link copied") : t("Invite somebody"))
                    }
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .padding(.leading, 14).padding(.trailing, 10)
                    .frame(height: 34)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(minting)
                .transition(Theme.arrive)
            }
            Rectangle().fill(Color.white.opacity(0.12)).frame(width: 1, height: 16)
            Button(action: dismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 10.5, weight: .bold))
                    .foregroundStyle(Theme.muted)
                    .frame(width: 34, height: 34)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(t("Close"))
            .help(t("Close"))
        }
        .glassSurface(Capsule())
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: 420)
        .animation(Theme.settle, value: copied)
        .animation(Theme.settle, value: notHost)
        .animation(Theme.settle, value: minting)
    }

    private func invite() {
        guard !minting else { return }
        minting = true
        Task {
            defer { minting = false }
            do {
                let made = try await model.deployment.createInvite(room: model.room, bearer: model.call.token())
                let link = model.deployment.inviteLink(token: made.token, room: model.room)
                #if os(macOS)
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(link, forType: .string)
                #else
                UIPasteboard.general.string = link
                #endif
                minting = false
                copied = true
                try? await Task.sleep(for: .seconds(1.6))
                copied = false
            } catch {
                notHost = true
            }
        }
    }
}
