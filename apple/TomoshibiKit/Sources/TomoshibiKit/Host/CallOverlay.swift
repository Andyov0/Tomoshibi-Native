import SwiftUI
import TomoshibiCore

/// What is drawn over the whole call: somebody waiting at the door, what a
/// press did, and the meeting being moved.
///
/// Also where this feature runs from. The room view places this once, with the
/// one `HostFeature` SwiftUI kept, so its task is the feature's lifetime: it
/// starts when the call is on screen and stops when the call view goes.
struct CallOverlayView: View {
    let feature: HostFeature

    /// The widest a panel is drawn beside the call, with its margin (RoomView).
    private static let panelWidth: CGFloat = 360 + 24

    var body: some View {
        ZStack {
            GeometryReader { geometry in
                // Beside an open panel rather than under it: the panels take
                // the trailing edge, and most of what is said here answers a
                // press made in one of them.
                let beside = feature.panelOpen || feature.soundOpen
                let room = geometry.size.width - Self.panelWidth - 32
                VStack(spacing: 8) {
                    Spacer(minLength: 0)
                    if !feature.waiting.isEmpty && !feature.panelOpen {
                        DoorNotice(feature: feature)
                            .transition(Theme.slideUp)
                    }
                    ForEach(feature.notices) { notice in
                        NoticeLine(notice: notice)
                            .transition(Theme.arrive)
                    }
                }
                .frame(maxWidth: beside && room >= 240 ? min(400, room) : 400)
                .padding(.horizontal, 16)
                // Just above the island, where a notice covers no picture's
                // name and no control (Notices.tsx `mobileOffset`), and above
                // the reactions while they are open over the bar.
                .padding(.bottom, Plan.island + 4 + (feature.reactions.trayOpen ? 52 : 0))
                .frame(maxWidth: .infinity, maxHeight: .infinity,
                       alignment: beside && room >= 240 ? .bottomLeading : .bottom)
                .animation(Theme.settle, value: feature.notices.map(\.id))
                .animation(Theme.settle, value: feature.waiting.isEmpty || feature.panelOpen)
                .animation(Theme.settle, value: beside)
                .animation(Theme.settle, value: feature.reactions.trayOpen)
            }

            if let to = feature.movingTo {
                MovingOverlay(to: to)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: feature.movingTo)
        .task { await feature.run() }
        // App.tsx: a hard limit on how long the move is shown. The rejoin
        // takes two or three seconds; a move that never ends the call (the
        // announcement went out and the close did not) would otherwise leave
        // "you will be back in a moment" over the controls for good.
        .task(id: feature.call.moving?.at) {
            guard let at = feature.call.moving?.at else { return }
            try? await Task.sleep(for: .seconds(max(0, 20 - Date().timeIntervalSince(at))))
            if !Task.isCancelled { feature.giveUpMoving(at: at) }
        }
    }
}

/// Somebody waiting to be let in, said where the host will see it
/// (notices.ts `atTheDoor`, AtTheDoor.tsx).
///
/// The web's notice only opens the panel, because a decision made from a
/// toast is one made without reading the name or where it came from. This
/// one carries both, the name somebody typed and the address they knocked
/// from, so the decision can be made where it is read; it stays for as long
/// as anybody is outside, and goes the moment the door is empty.
struct DoorNotice: View {
    let feature: HostFeature

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(feature.waiting.count == 1
                ? t("Somebody is waiting to be let in")
                : t("{count} people are waiting to be let in", ["count": String(feature.waiting.count)]))
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(Theme.foreground)
            DoorList(feature: feature)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surfaceHigh)
        .overlay(alignment: .leading) {
            // The signal colour's rail: somebody is outside right now.
            Rectangle().fill(Theme.tally).frame(width: 2)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.border))
        .shadow(color: .black.opacity(0.4), radius: 14, y: 4)
        .accessibilityElement(children: .contain)
    }
}

/// Who is at the door, each with the two answers (AtTheDoor.tsx): what they
/// typed to be called, which is a claim, and where they knocked from, which is
/// not. Used by the notice and inside the host panel alike.
struct DoorList: View {
    let feature: HostFeature

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(feature.waiting, id: \.id) { one in
                HStack(spacing: 8) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(one.name.isEmpty ? t("Somebody") : one.name)
                            .font(.system(size: 12.5))
                            .foregroundStyle(Theme.foreground)
                            .lineLimit(1)
                        Text(one.address)
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundStyle(Theme.muted)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 4)
                    Button {
                        Task { await feature.answer(one, admit: true) }
                    } label: {
                        Text(t("Let in"))
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(Theme.tallyForeground)
                            .padding(.horizontal, 8).padding(.vertical, 4)
                            .background(RoundedRectangle(cornerRadius: 6).fill(Theme.tally))
                    }
                    .buttonStyle(.plain)
                    Button {
                        Task { await feature.answer(one, admit: false) }
                    } label: {
                        Text(t("Not now"))
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.foreground)
                            .padding(.horizontal, 8).padding(.vertical, 4)
                            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
                    }
                    .buttonStyle(.plain)
                }
                .disabled(feature.answering == one.id)
                .opacity(feature.answering == one.id ? 0.4 : 1)
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: feature.waiting.map(\.id))
        .animation(Theme.settle, value: feature.answering)
    }
}

/// One line about a press: a red rail for one that did not take, a plain one
/// for one that did. Fades after ten seconds, and has nothing to close it
/// with: what it reports is over, and a target that lives that briefly is one
/// nobody hits (notices.ts, the rule about lifetimes).
struct NoticeLine: View {
    let notice: HostNotice

    var body: some View {
        Text(notice.text)
            .font(.system(size: 12.5))
            .foregroundStyle(Theme.foreground)
            .fixedSize(horizontal: false, vertical: true)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surfaceHigh)
        .overlay(alignment: .leading) {
            // notices.ts: the edge is the whole of the severity system.
            Rectangle().fill(notice.danger ? Theme.danger : Theme.border).frame(width: 2)
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.border))
        .shadow(color: .black.opacity(0.35), radius: 10, y: 3)
    }
}

/// A call being moved to another machine, from a chair (components/room/Moving.tsx).
///
/// A move ends the call and everybody comes straight back; what that looked
/// like was being thrown out. So this is drawn over the call for the few
/// seconds it takes, says where the meeting is going, and offers nothing to
/// press. The destination is what the server announced, put through `say()`
/// as the web puts it.
struct MovingOverlay: View {
    let to: String

    var body: some View {
        let named = say(to.trimmingCharacters(in: .whitespacesAndNewlines))
        ZStack {
            Theme.background.opacity(0.8).ignoresSafeArea()
            Rectangle().fill(.ultraThinMaterial).opacity(0.5).ignoresSafeArea()
            VStack(spacing: 12) {
                ProgressView().controlSize(.small).tint(Theme.muted)
                Text(named.isEmpty ? t("Moving this meeting") : t("Moving this meeting to {name}", ["name": named]))
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .multilineTextAlignment(.center)
                // The one thing worth saying: nobody has to do anything.
                Text(t("You will be back in a moment."))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
            }
            .padding(.horizontal, 24)
        }
        .allowsHitTesting(false)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.updatesFrequently)
    }
}
