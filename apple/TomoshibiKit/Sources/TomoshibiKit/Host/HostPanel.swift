import SwiftUI
import TomoshibiCore

/// The small console for whoever is running the meeting (components/room/HostPanel.tsx).
///
/// A link somebody can be sent, who is at the door, where the meeting is held,
/// and the people in it, each of whom can be quietened, made host or removed;
/// then ending it for everybody, asked twice. Deliberately less than the
/// management pages can do: this reaches nobody outside this room.
///
/// The web shows the crown that opens this only to whoever runs the room. The
/// bar here always has the button, so somebody who does not run the room is
/// told so rather than shown controls that would all answer "not yours".
struct HostPanelView: View {
    let feature: HostFeature
    let close: () -> Void

    private var ending: Bool { feature.ending }
    private var elsewhere: Bool { feature.choosingRelay }

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 0)
            panel
        }
        .onAppear { feature.panelOpen = true }
        .onDisappear { feature.panelOpen = false }
        .task { await feature.loadRelays() }
    }

    private var panel: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "crown.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.tally)
                Text(t("Running this room"))
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close"))
            }
            .padding(12)
            Divider().overlay(Theme.border)
            ViewThatFits(in: .vertical) {
                content
                ScrollView { content }
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
        .shadow(color: .black.opacity(0.4), radius: 20, y: 8)
        .animation(Theme.settle, value: feature.standing.yours)
    }

    @ViewBuilder
    private var content: some View {
        VStack(alignment: .leading, spacing: 10) {
            if feature.standing.yours {
                inviting
                Rule()
                if feature.usableRelays.count >= 2 {
                    moving
                        .transition(Theme.arrive)
                    Rule()
                }
                people
                Rule()
                endingIt
                Text(feature.standing.admin
                    ? t("You can do this because you administer this server. Removing somebody ends their call; it does not stop them rejoining.")
                    : t("Removing somebody ends their call. It does not stop them rejoining with the same name."))
                    .font(.system(size: 10.5))
                    .foregroundStyle(Theme.muted.opacity(0.85))
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                Text(t("Somebody else runs this room."))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
                    .transition(Theme.arrive)
            }
        }
        .padding(12)
        .animation(Theme.settle, value: feature.link)
        .animation(Theme.settle, value: feature.usableRelays.count)
        .animation(Theme.settle, value: ending)
        .animation(Theme.settle, value: elsewhere)
    }

    // MARK: A link, and the door

    @ViewBuilder
    private var inviting: some View {
        VStack(alignment: .leading, spacing: 6) {
            PanelButton(
                symbol: feature.copied ? "checkmark" : "link",
                tint: feature.copied ? Theme.tally : Theme.foreground,
                busy: feature.busy == "invite",
                title: feature.copied ? t("Link copied") : t("Invite somebody")
            ) {
                Task { await feature.invite() }
            }

            // Who is outside, under the control for letting people in: the two
            // are the same thought (HostPanel.tsx).
            if !feature.waiting.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Rule()
                    Text(t("Waiting to be let in"))
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.muted)
                    DoorList(feature: feature)
                }
                .transition(Theme.arrive)
            }

            if let link = feature.link {
                VStack(alignment: .leading, spacing: 6) {
                    Text(link)
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(Theme.muted)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 8).padding(.vertical, 6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
                    Text(t("Anybody with this link can join, until you end the meeting. No passphrase needed.") + " " + t("It stops working after a day either way."))
                        .font(.system(size: 10.5))
                        .foregroundStyle(Theme.muted.opacity(0.85))
                        .fixedSize(horizontal: false, vertical: true)
                    // A link goes out and cannot be taken back; this stops it
                    // without ending the meeting. Under a scope it also stops
                    // the standing links members made, so it says every link.
                    Button {
                        Task { await feature.revoke() }
                    } label: {
                        HStack(spacing: 6) {
                            if feature.busy == "revoke" {
                                ProgressView().controlSize(.mini)
                            } else {
                                // A link struck through; there is no such symbol.
                                Image(systemName: "link")
                                    .font(.system(size: 10))
                                    .overlay(Rectangle().frame(width: 1.2, height: 14).rotationEffect(.degrees(-45)))
                            }
                            Text(feature.scoped ? t("Stop every link to this room working") : t("Stop this link working"))
                                .font(.system(size: 11))
                        }
                        .foregroundStyle(Theme.muted)
                        .padding(.horizontal, 6).padding(.vertical, 4)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(feature.busy == "revoke")
                }
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: feature.waiting.isEmpty)
    }

    // MARK: Where it is held

    private var moving: some View {
        VStack(alignment: .leading, spacing: 6) {
            PanelButton(symbol: "server.rack", busy: feature.busy == "move", title: t("Hold this call somewhere else")) {
                feature.choosingRelay.toggle()
            }
            if elsewhere {
                VStack(alignment: .leading, spacing: 2) {
                    Text(t("Everybody is reconnected. Nobody has to do anything."))
                        .font(.system(size: 10.5))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 8).padding(.vertical, 4)
                    ForEach(feature.usableRelays, id: \.name) { relay in
                        HoverRow {
                            feature.choosingRelay = false
                            Task { await feature.move(to: relay) }
                        } label: {
                            Text(say(relay.label?.isEmpty == false ? relay.label! : relay.name))
                                .font(.system(size: 12))
                                .foregroundStyle(Theme.foreground)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
                .padding(4)
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.border))
                .transition(Theme.arrive)
            }
        }
    }

    // MARK: The people

    @ViewBuilder
    private var people: some View {
        let others = feature.call.people
        if others.isEmpty {
            Text(t("Nobody else is here."))
                .font(.system(size: 11.5))
                .foregroundStyle(Theme.muted)
                .transition(Theme.arrive)
        } else {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(others) { person in
                    PersonRow(feature: feature, person: person)
                        .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: others.map(\.id))
        }
    }

    // MARK: Ending it

    @ViewBuilder
    private var endingIt: some View {
        if ending {
            VStack(alignment: .leading, spacing: 8) {
                Text(feature.scoped
                    ? t("Everybody will be disconnected and told the room has closed. Links made in the call stop working; links made on an account page do not.")
                    : t("Everybody will be disconnected and told the room has closed. Links to it stop working."))
                    .font(.system(size: 11.5))
                    .foregroundStyle(Theme.foreground)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    // Red: ending it for everybody is leaving, for everybody.
                    Button {
                        Task { await feature.dissolve() }
                    } label: {
                        HStack(spacing: 6) {
                            if feature.busy == "close" { ProgressView().controlSize(.mini).tint(Theme.dangerForeground) }
                            Text(t("Close the room")).font(.system(size: 12, weight: .medium))
                        }
                        .foregroundStyle(Theme.dangerForeground)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                        .background(RoundedRectangle(cornerRadius: 6).fill(Theme.danger))
                    }
                    .buttonStyle(.plain)
                    .disabled(feature.busy == "close")
                    Button {
                        feature.ending = false
                    } label: {
                        Text(t("Cancel"))
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.muted)
                            .padding(.horizontal, 10).padding(.vertical, 6)
                            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 8).fill(Theme.danger.opacity(0.1)))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.danger.opacity(0.4)))
            .transition(Theme.arrive)
        } else {
            PanelButton(symbol: "door.left.hand.closed", tint: Theme.muted, title: t("End this meeting")) {
                feature.ending = true
            }
            .transition(Theme.arrive)
        }
    }
}

/// One person, and what can be done about them (HostPanel.tsx's list).
private struct PersonRow: View {
    let feature: HostFeature
    let person: Person
    @State private var hovering = false

    var body: some View {
        let busy = feature.busy != nil
        // `isMicrophoneEnabled`: a microphone there and not already muted.
        let microphoneOn = person.track(.microphone).map { !$0.muted } ?? false
        HStack(spacing: 4) {
            Text(person.name.isEmpty ? person.id : person.name)
                .font(.system(size: 12.5))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if feature.busy == person.id {
                ProgressView().controlSize(.mini)
                    .transition(.opacity.animation(Theme.settle))
            }
            Small(symbol: "mic.slash", label: t("Mute"), disabled: busy || !microphoneOn) {
                Task { await feature.silence(person) }
            }
            // A room held under a scope has no host to hand over, and the
            // server refuses it outright; the button goes rather than answering
            // with a refusal.
            if !feature.scoped {
                // Dimmed beside a guest rather than taken away, and pressable,
                // so the reason can be said on a phone, where nobody hovers.
                let guest = feature.isGuest(person)
                Small(symbol: "crown", label: t("Make host"), disabled: busy, dim: guest) {
                    if guest {
                        feature.refuseHandOver()
                    } else {
                        Task { await feature.handOver(to: person) }
                    }
                }
                .help(guest ? t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one.") : t("Make host"))
            }
            Small(symbol: "person.badge.minus", label: t("Remove from room"), disabled: busy) {
                Task { await feature.turnOut(person) }
            }
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 4)
        .background(RoundedRectangle(cornerRadius: 6).fill(hovering ? Theme.surfaceHigh : .clear))
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: hovering)
        .animation(Theme.settle, value: feature.busy)
    }

    private struct Small: View {
        let symbol: String
        let label: String
        var disabled = false
        var dim = false
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: symbol)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled || dim ? 0.3 : 1)
            .accessibilityLabel(label)
            .help(label)
        }
    }
}

/// A bordered row-wide button with a symbol, the panel's one kind of button.
private struct PanelButton: View {
    let symbol: String
    var tint: Color = Theme.foreground
    var busy = false
    let title: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                ZStack {
                    if busy {
                        ProgressView().controlSize(.mini)
                    } else {
                        Image(systemName: symbol)
                            .font(.system(size: 12))
                            .foregroundStyle(tint)
                            .contentTransition(.symbolEffect(.replace))
                    }
                }
                .frame(width: 16)
                Text(title)
                    .font(.system(size: 12.5))
                    .foregroundStyle(Theme.foreground)
                    .contentTransition(.opacity)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(RoundedRectangle(cornerRadius: 8).fill(hovering ? Theme.surfaceHigh : .clear))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.border))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: hovering)
        .animation(Theme.settle, value: title)
        .animation(Theme.settle, value: busy)
    }
}

/// A plain row that lights on hover, for a list of choices.
private struct HoverRow<Label: View>: View {
    let action: () -> Void
    @ViewBuilder let label: Label
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            label
                .padding(.horizontal, 8).padding(.vertical, 5)
                .background(RoundedRectangle(cornerRadius: 5).fill(hovering ? Theme.surfaceHigh : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: hovering)
    }
}

/// A hairline between the panel's sections.
private struct Rule: View {
    var body: some View {
        Rectangle().fill(Theme.border).frame(height: 1)
    }
}
