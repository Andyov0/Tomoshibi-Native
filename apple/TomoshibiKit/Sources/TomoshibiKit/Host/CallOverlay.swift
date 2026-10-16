import SwiftUI
import TomoshibiCore

/// Where the host's feature runs from, and nothing it draws any more: what a
/// press did and who is at the door are said in the corner with everything
/// else the call says (Toasts.swift), as the web says them (notices.ts).
///
/// The room view places this once, with the one `HostFeature` SwiftUI kept,
/// so its task is the feature's lifetime: it starts when the call is on
/// screen and stops when the call view goes.
struct CallOverlayView: View {
    let feature: HostFeature

    var body: some View {
        Color.clear
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            .task { await feature.run() }
            // App.tsx: a hard limit on how long the move is shown. The rejoin
            // takes two or three seconds; a move that never ends the call (the
            // announcement went out and the close did not) would otherwise
            // leave "you will be back in a moment" over the controls for good.
            .task(id: feature.call.moving?.at) {
                guard let at = feature.call.moving?.at else { return }
                try? await Task.sleep(for: .seconds(max(0, 20 - Date().timeIntervalSince(at))))
                if !Task.isCancelled { feature.giveUpMoving(at: at) }
            }
    }
}

/// Who is at the door, each with the two answers (AtTheDoor.tsx): what they
/// typed to be called, which is a claim, and where they knocked from, which is
/// not. Drawn in the door's notice and inside the host panel alike.
struct DoorList: View {
    let feature: HostFeature

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(feature.waiting, id: \.id) { one in
                HStack(spacing: 9) {
                    Monogram(name: one.name.isEmpty ? "?" : one.name, identity: one.id, size: 26)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(one.name.isEmpty ? t("Somebody") : one.name)
                            .font(.system(size: 12.5, weight: .medium))
                            .foregroundStyle(Theme.foreground)
                            .lineLimit(1)
                        Text(one.address)
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundStyle(Theme.muted)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: 4)
                    PillButton(title: t("Not now"), busy: false) {
                        Task { await feature.answer(one, admit: false) }
                    }
                    PillButton(title: t("Let in"), role: .prominent, busy: feature.answering == one.id) {
                        Task { await feature.answer(one, admit: true) }
                    }
                }
                .disabled(feature.answering == one.id)
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: feature.waiting.map(\.id))
        .animation(Theme.settle, value: feature.answering)
        .accessibilityElement(children: .contain)
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
                MediaSpinner(size: 18, tint: Theme.muted)
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
