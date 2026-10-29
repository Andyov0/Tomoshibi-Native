import SwiftUI
import TomoshibiCore

/// The small console for whoever is running the meeting (components/room/HostPanel.tsx).
///
/// A link somebody can be sent, who is at the door, where the meeting is held,
/// and the people in it, each of whom can be quietened, made host or removed;
/// then ending it for everybody, asked twice, in the panel itself rather than
/// in a dialog over the call. Deliberately less than the management pages can
/// do: this reaches nobody outside this room.
///
/// The bar shows the crown that opens this only to whoever runs the room, as
/// the web does (ControlBar.swift, `Offers.host`). The room can still change
/// hands while the panel is open, by a handover from here or by the server
/// after the host left and came back, and then the panel says that somebody
/// else runs the room rather than keeping controls that would all answer
/// "not yours".
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
            PanelHeading(title: t("Running this room"), symbol: "crown.fill", symbolTint: Theme.tallyInk,
                         closeLabel: t("Close"), close: close)
            PanelRule()
            ViewThatFits(in: .vertical) {
                content
                ScrollView { content }.scrollIndicators(.hidden)
            }
        }
        // Opaque, as every panel is (Kit `panelSurface`).
        .panelSurface()
        .animation(Theme.settle, value: feature.standing.yours)
    }

    @ViewBuilder
    private var content: some View {
        VStack(alignment: .leading, spacing: 12) {
            if feature.standing.yours {
                inviting
                if !feature.waiting.isEmpty {
                    door
                        .transition(Theme.arrive)
                }
                if feature.usableRelays.count >= 2 {
                    moving
                        .transition(Theme.arrive)
                }
                people
                endingIt
                Text(feature.standing.admin
                    ? t("You can do this because you administer this server. Removing somebody ends their call; it does not stop them rejoining.")
                    : t("Removing somebody ends their call. It does not stop them rejoining with the same name."))
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.faint(0.85))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 6)
            } else {
                HStack(spacing: 10) {
                    Image(systemName: "crown")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.muted)
                    Text(t("Somebody else runs this room."))
                        .font(.system(size: 12.5))
                        .foregroundStyle(Theme.muted)
                }
                .padding(.vertical, 18)
                .frame(maxWidth: .infinity)
                .transition(Theme.arrive)
            }
        }
        .padding(12)
        .animation(Theme.settle, value: feature.link)
        .animation(Theme.settle, value: feature.usableRelays.count)
        .animation(Theme.settle, value: feature.waiting.isEmpty)
        .animation(Theme.settle, value: ending)
        .animation(Theme.settle, value: elsewhere)
    }

    // MARK: A link, and the door

    private var inviting: some View {
        VStack(alignment: .leading, spacing: 0) {
            MenuItem(title: feature.copied ? t("Link copied") : t("Invite somebody"),
                     symbol: feature.copied ? "checkmark" : "link",
                     role: .prominent,
                     busy: feature.busy == "invite") {
                Task { await feature.invite() }
            }
            .disabled(feature.busy == "invite")
            .animation(Theme.settle, value: feature.copied)

            if let link = feature.link {
                VStack(alignment: .leading, spacing: 8) {
                    Text(link)
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(Theme.foreground.opacity(0.8))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 10).padding(.vertical, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Theme.sunk(0.25)))
                        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Theme.ink.opacity(0.07)))
                    Text(t("Anybody with this link can join, until you end the meeting. No passphrase needed.") + " " + t("It stops working after a day either way."))
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    // A link goes out and cannot be taken back; this stops it
                    // without ending the meeting. Under a scope it also stops
                    // the standing links members made, so it says every link.
                    QuietButton(title: feature.scoped ? t("Stop every link to this room working") : t("Stop this link working"),
                                busy: feature.busy == "revoke") {
                        Task { await feature.revoke() }
                    }
                    .disabled(feature.busy == "revoke")
                }
                .padding(.horizontal, 8)
                .padding(.top, 4)
                .padding(.bottom, 8)
                .transition(Theme.arrive)
            }
        }
        .padding(4)
        .panelCard()
    }

    /// Who is outside, under the control for letting people in: the two are
    /// the same thought (HostPanel.tsx).
    private var door: some View {
        VStack(alignment: .leading, spacing: 4) {
            MenuHeading(title: t("Waiting to be let in"))
            DoorList(feature: feature)
                .padding(.horizontal, 10)
                .padding(.bottom, 10)
                .padding(.top, 4)
        }
        .panelCard(tint: Theme.tally)
    }

    // MARK: Where it is held

    /// The move under way takes the chevron's place with the app's spinner,
    /// each fading into the other: the menu row's own spinner is the
    /// system's, which is grey on this glass, and it arrived and left without
    /// a frame between (HostPanel.tsx `Elsewhere`, whose button swaps its
    /// icon for the spinner the same way and is not pressable meanwhile).
    private var moving: some View {
        let busy = feature.busy == "move"
        return VStack(alignment: .leading, spacing: 2) {
            MenuItem(title: t("Hold this call somewhere else"), symbol: "server.rack") {
                withAnimation(Theme.settle) { feature.choosingRelay.toggle() }
            }
            .disabled(busy)
            .overlay(alignment: .trailing) {
                ZStack {
                    if busy {
                        MediaSpinner(size: 14, tint: Theme.muted)
                            .transition(.opacity.combined(with: .scale(scale: 0.6)))
                    } else {
                        Image(systemName: "chevron.down")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(Theme.muted)
                            .rotationEffect(.degrees(elsewhere ? 180 : 0))
                            .transition(.opacity.combined(with: .scale(scale: 0.6)))
                    }
                }
                .padding(.trailing, 12)
                .allowsHitTesting(false)
                .animation(Theme.settle, value: busy)
            }
            if elsewhere {
                VStack(alignment: .leading, spacing: 2) {
                    Text(t("Everybody is reconnected. Nobody has to do anything."))
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 10).padding(.top, 2).padding(.bottom, 4)
                    ForEach(feature.usableRelays, id: \.name) { relay in
                        MenuItem(title: say(relay.label?.isEmpty == false ? relay.label! : relay.name), symbol: "arrow.right") {
                            feature.choosingRelay = false
                            Task { await feature.move(to: relay) }
                        }
                    }
                }
                .transition(Theme.arrive)
            }
        }
        .padding(4)
        .panelCard()
    }

    // MARK: The people

    @ViewBuilder
    private var people: some View {
        let others = feature.call.people
        VStack(alignment: .leading, spacing: 0) {
            if others.isEmpty {
                Text(t("Nobody else is here."))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .transition(Theme.arrive)
            } else {
                ForEach(others) { person in
                    PersonRow(feature: feature, person: person)
                        .transition(Theme.arrive)
                }
            }
        }
        .padding(4)
        .panelCard()
        .animation(Theme.settle, value: others.map(\.id))
    }

    // MARK: Ending it

    @ViewBuilder
    private var endingIt: some View {
        if ending {
            // The question names what will happen rather than asking whether
            // they are sure, which nobody has ever answered no to by reading
            // it (HostPanel.tsx `ending`).
            VStack(alignment: .leading, spacing: 12) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "door.left.hand.closed")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.dangerInk)
                        .frame(width: 18)
                    Text(feature.scoped
                        ? t("Everybody will be disconnected and told the room has closed. Links made in the call stop working; links made on an account page do not.")
                        : t("Everybody will be disconnected and told the room has closed. Links to it stop working."))
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.foreground)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 8) {
                    PillButton(title: t("Cancel"), height: 30) {
                        withAnimation(Theme.settle) { feature.ending = false }
                    }
                    // Red: ending it for everybody is leaving, for everybody.
                    PillButton(title: t("Close the room"), role: .destructive, busy: feature.busy == "close", height: 30, wide: true) {
                        Task { await feature.dissolve() }
                    }
                    .disabled(feature.busy == "close")
                }
            }
            .padding(12)
            .panelCard(tint: Theme.danger)
            .transition(.asymmetric(
                insertion: .opacity.combined(with: .scale(scale: 0.96, anchor: .top)).animation(Theme.settle),
                removal: .opacity.animation(Theme.leave)
            ))
        } else {
            MenuItem(title: t("End this meeting"), symbol: "door.left.hand.closed") {
                withAnimation(Theme.settle) { feature.ending = true }
            }
            .padding(4)
            .panelCard()
            .transition(.opacity.animation(Theme.settle))
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
        let name = person.name.isEmpty ? person.id : person.name
        // `isMicrophoneEnabled`: a microphone there and not already muted.
        let microphoneOn = person.track(.microphone).map { !$0.muted } ?? false
        HStack(spacing: 8) {
            Monogram(name: name, identity: person.id, size: 26)
            Text(name)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if feature.busy == person.id {
                MediaSpinner(size: 14, tint: Theme.muted)
                    .transition(.opacity.animation(Theme.settle))
            }
            RoundButton(symbol: "mic.slash.fill", label: t("Mute")) {
                Task { await feature.silence(person) }
            }
            .disabled(busy || !microphoneOn)
            // A room held under a scope has no host to hand over, and the
            // server refuses it outright; the button goes rather than
            // answering with a refusal.
            if !feature.scoped {
                // Faint beside a guest rather than taken away, and pressable,
                // so the reason can be said on a phone, where nobody hovers.
                let guest = feature.isGuest(person)
                RoundButton(symbol: "crown", label: t("Make host"), faint: guest) {
                    if guest {
                        feature.refuseHandOver()
                    } else {
                        Task { await feature.handOver(to: person) }
                    }
                }
                .disabled(busy)
                .help(guest ? t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one.") : t("Make host"))
            }
            RoundButton(symbol: "person.badge.minus", label: t("Remove from room"), role: .destructive) {
                Task { await feature.turnOut(person) }
            }
            .disabled(busy)
        }
        .padding(.leading, 8)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.ink.opacity(hovering ? 0.06 : 0)))
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: feature.busy)
    }
}

/// A small, quiet press for something that undoes rather than does: stopping
/// the link. It warms to red under the pointer, where it is about to happen.
private struct QuietButton: View {
    let title: String
    var busy = false
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if busy {
                    MediaSpinner(size: 10, tint: hovering ? Theme.foreground : Theme.muted)
                        .transition(.opacity.combined(with: .scale(scale: 0.6)))
                } else {
                    // A link struck through; there is no such symbol.
                    Image(systemName: "link")
                        .font(.system(size: 10, weight: .semibold))
                        .overlay(Capsule().frame(width: 1.4, height: 14).rotationEffect(.degrees(-45)))
                        .transition(.opacity.combined(with: .scale(scale: 0.6)))
                }
                Text(title)
                    .font(.system(size: 11.5, weight: .medium))
            }
            .foregroundStyle(hovering ? Theme.foreground : Theme.muted)
            .padding(.horizontal, 10).padding(.vertical, 5)
            .background(Capsule().fill(hovering ? Theme.danger.opacity(0.14) : Theme.ink.opacity(0.05)))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: busy)
    }
}
