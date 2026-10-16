import SwiftUI
import TomoshibiCore

/// The room's name, at the head of the call.
///
/// It carried a connection light as well, beside the signal badge that says
/// the same thing better: two lights for one connection, which could disagree
/// for the seconds between the server's grade and the first measurement. The
/// badge is the web's (Signal.tsx), so the name stands alone.
struct RoomTitle: View {
    @Bindable var model: CallModel

    var body: some View {
        Text(model.room)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Theme.foreground)
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, 12).frame(height: 30)
            .glassSurface(Capsule())
    }
}

/// Alone in the room: the link that brings somebody else in, or, for somebody
/// who does not run the room, who can (components/room/EmptyRoom.tsx).
///
/// The address is only a way in where anybody may use a room that exists
/// (`linkWorks`). Where the door asks for an invitation, copying it and
/// sending it invites somebody to be turned away, so there it mints an
/// invitation instead, and only for whoever runs the room, since the server
/// refuses anybody else; somebody who does not is told who can.
///
/// Which of the two is shown is the room's standing and nothing else, as on
/// the web. An invitation that fails says why in the corner and leaves the
/// button there to press again (EmptyRoom.tsx `share`, `actionFailed`); only
/// the server's "not yours" says the standing is out of date, and then the
/// standing is asked again, which is what turns the button into the
/// sentence. Any failure at all used to turn it into the sentence for the
/// rest of the call, a dropped connection included, telling the person who
/// runs the room that only whoever runs it could let anybody in.
///
/// A small capsule above the controls that can be put away, as the web's is a
/// single quiet button. It used to be a pane of glass with a heading laid over
/// the middle of your own picture, with no way to close it, and the owner
/// found it jarring (2026-10-16).
struct EmptyRoomCard: View {
    @Bindable var model: CallModel
    /// A plain link to this room lets people in.
    let linkWorks: Bool
    /// This participant runs the room, so may mint an invitation.
    let host: Bool
    /// The room's plain address.
    let link: String
    /// Asks the server again who runs the room.
    let recheck: () async -> Void
    let dismiss: () -> Void
    @State private var copied = false
    @State private var minting = false

    private var notHost: Bool { !linkWorks && !host }

    var body: some View {
        HStack(spacing: 0) {
            if notHost {
                Text(t("Only whoever runs this room can let somebody else in."))
                    .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    .lineLimit(2)
                    .padding(.leading, 16).padding(.trailing, 6)
                    .transition(Theme.arrive)
            } else {
                Button(action: linkWorks ? copy : invite) {
                    HStack(spacing: 7) {
                        ZStack {
                            if minting {
                                ProgressView().controlSize(.mini)
                            } else {
                                Image(systemName: copied ? "checkmark" : linkWorks ? "doc.on.doc" : "link")
                                    .foregroundStyle(copied ? Theme.tally : Theme.foreground)
                                    .transition(.scale.combined(with: .opacity))
                            }
                        }
                        .frame(width: 14)
                        Text(copied ? t("Link copied") : linkWorks ? t("Copy link") : t("Invite somebody"))
                            .contentTransition(.opacity)
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
        .animation(Theme.settle, value: linkWorks)
    }

    private func copy() {
        Pasteboard.copy(link)
        copied = true
        Task {
            try? await Task.sleep(for: .seconds(1.6))
            copied = false
        }
    }

    private func invite() {
        guard !minting else { return }
        minting = true
        Task {
            defer { minting = false }
            do {
                let made = try await model.deployment.createInvite(room: model.room, bearer: model.call.token())
                let link = model.deployment.inviteLink(token: made.token, room: model.room)
                Pasteboard.copy(link)
                minting = false
                copied = true
                try? await Task.sleep(for: .seconds(1.6))
                copied = false
            } catch {
                minting = false
                model.actionFailed(HostFeature.explain(error))
                if case let .Refused(_, code)? = error as? ProtocolError, code == "not_yours" {
                    await recheck()
                }
            }
        }
    }
}
