import Foundation
import Observation
import SwiftUI
import TomoshibiCore

/// The reactions on screen in one call (hands.ts `useReactions`).
///
/// A reaction is an event rather than a state: it happened, it is seen for
/// four seconds, and it is gone, so nobody arriving later sees a thumbs-up from
/// before they came. Held per call and found from the core's `Call`, because
/// the three places that need it are given nothing else in common: the tile a
/// reaction floats over, the control that sends one, and the feature that
/// hears one arrive.
@MainActor
@Observable
final class ReactionBoard {
    struct Shown: Identifiable, Equatable {
        let id = UUID()
        /// Whose it is, as the identity their token was signed with.
        let from: String
        let what: String
    }

    private(set) var shown: [Shown] = []

    /// Whether the six are open above the hand. Kept here rather than in the
    /// control, so that a test can open them (TOMOSHIBI_DEBUG_HOST).
    var trayOpen = false

    private struct Entry {
        weak var call: Call?
        let board: ReactionBoard
    }

    private static var boards: [ObjectIdentifier: Entry] = [:]

    /// The board for this call, made the first time anybody asks.
    ///
    /// Checked against the call itself and not only its address, because an
    /// address is reused once a call is gone and the next call would inherit
    /// the last one's reactions.
    static func of(_ call: Call) -> ReactionBoard {
        let key = ObjectIdentifier(call)
        if let entry = boards[key], entry.call === call { return entry.board }
        boards = boards.filter { $0.value.call != nil }
        let board = ReactionBoard()
        boards[key] = Entry(call: call, board: board)
        return board
    }

    /// Shows one, and takes it down again after the web's four seconds by the
    /// timer that put it up rather than by a sweep (hands.ts `REACTION_FOR`).
    func show(_ what: String, from identity: String) {
        let one = Shown(from: identity, what: what)
        shown.append(one)
        Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(reactionShownForMs())))
            self?.shown.removeAll { $0.id == one.id }
        }
    }

    func on(_ identity: String) -> [Shown] {
        shown.filter { $0.from == identity }
    }

    /// Sends a reaction to everybody and shows it on this participant's own
    /// picture at once: data does not come back to whoever sent it, so the
    /// sender would otherwise never see their own (hands.ts `react`).
    ///
    /// The topic is the web's `REACTION` (hands.ts; hands.rs `REACTION_TOPIC`),
    /// sent reliably as the web sends it; the payload is the reaction's own
    /// text, which the protocol crate refuses to make for anything but the
    /// six a browser would show.
    func send(_ what: String, in model: CallModel) {
        guard let payload = try? reactionPayload(what: what) else { return }
        show(what, from: model.localIdentity)
        Task { try? await model.send(topic: "reaction", payload: payload, reliable: true) }
    }
}

/// Whatever somebody just reacted with, rising off their picture
/// (components/room/Reacting.tsx): low on the picture, centred, each new one
/// beside the last rather than on top of it, rising and gone in four seconds.
struct FloatingReactions: View {
    let call: Call
    let identity: String
    var compact = false

    var body: some View {
        let mine = ReactionBoard.of(call).on(identity)
        GeometryReader { geometry in
            ZStack {
                ForEach(Array(mine.enumerated()), id: \.element.id) { index, one in
                    Floating(what: one.what, size: compact ? 22 : 30)
                        // Reacting.tsx: `(at % 3) * 28 - 28` pixels from the
                        // middle, so two of the same at once are two things.
                        .offset(x: CGFloat(index % 3) * 28 - 28)
                        .transition(.opacity.animation(Theme.leave))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            // Above the island where the picture runs under it. The web's
            // forty pixels are from the foot of a tile that ends above the
            // controls; here the person filling a call of two is drawn under
            // them, and a reaction forty points up was hidden behind the bar.
            .padding(.bottom, min(Plan.island + 12, geometry.size.height * 0.3))
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// One reaction's flight: the web's `float` keyframes, in from a little
    /// below at sixty per cent of its size over the first eighth, still until
    /// seven tenths, then up sixty-four points and out.
    private struct Floating: View {
        let what: String
        let size: CGFloat
        @State private var phase = 0
        @Environment(\.accessibilityReduceMotion) private var reduceMotion

        var body: some View {
            Text(what)
                .font(.system(size: size))
                .scaleEffect(phase == 0 && !reduceMotion ? 0.6 : 1)
                .offset(y: reduceMotion ? 0 : (phase == 0 ? 6 : phase == 1 ? 0 : -64))
                .opacity(phase == 1 ? 1 : 0)
                .task {
                    let total = Double(reactionShownForMs()) / 1000
                    withAnimation(Theme.settle.speed(0.32 / (total * 0.12))) { phase = 1 }
                    try? await Task.sleep(for: .seconds(total * 0.7))
                    withAnimation(.easeIn(duration: total * 0.3)) { phase = 2 }
                }
        }
    }
}

/// Putting a hand up, and the reactions behind it (components/room/HandButton.tsx).
///
/// One control for two things, because they are one intention: saying
/// something without interrupting whoever is talking. A press on the hand puts
/// it up or down and never opens anything; the narrow half beside it opens the
/// six reactions above the bar, and choosing one sends it and closes them.
struct HandControl: View {
    @Bindable var model: CallModel
    /// The bar's own way of doing something that may fail.
    let act: (@escaping () async throws -> Void) -> Void
    /// The room's panels, which this control only ever opens for
    /// TOMOSHIBI_DEBUG_HOST: a test cannot reach the bar's buttons on a
    /// machine whose accessibility answers with the application for every
    /// element, and the panels are the room view's to open.
    @Binding var panel: RoomView.Panel?

    private var board: ReactionBoard { ReactionBoard.of(model.call) }
    private var open: Bool { board.trayOpen }

    var body: some View {
        HStack(spacing: 0) {
            Button {
                act { try await model.setHand(!model.handUp) }
            } label: {
                Image(systemName: "hand.raised.fill")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(model.handUp ? Theme.tallyForeground : Theme.foreground)
                    .frame(width: 40, height: 44)
                    .background(UnevenRoundedRectangle(topLeadingRadius: 22, bottomLeadingRadius: 22)
                        .fill(model.handUp ? Theme.tally : Color.white.opacity(0.12)))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(model.handUp ? t("Lower your hand") : t("Raise your hand"))
            .help(model.handUp ? t("Lower your hand") : t("Raise your hand"))

            Button {
                board.trayOpen.toggle()
            } label: {
                Image(systemName: "chevron.up")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(open ? Theme.foreground : Theme.muted)
                    .rotationEffect(.degrees(open ? 180 : 0))
                    .frame(width: 20, height: 44)
                    .background(UnevenRoundedRectangle(bottomTrailingRadius: 22, topTrailingRadius: 22)
                        .fill(open ? Color.white.opacity(0.2) : Color.white.opacity(0.12)))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(t("Reactions"))
            .accessibilityAddTraits(open ? .isSelected : [])
            .help(t("Reactions"))
        }
        .animation(Theme.settle, value: model.handUp)
        .overlay(alignment: .bottom) {
            if open {
                tray
                    .fixedSize()
                    // Clear of the bar's own edge as well as the control's.
                    .offset(y: -58)
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: .bottom)).animation(Theme.settle),
                        removal: .opacity.combined(with: .scale(scale: 0.97, anchor: .bottom)).animation(Theme.leave)
                    ))
            }
        }
        .animation(Theme.settle, value: open)
        .onChange(of: HostDebug.panelWanted(for: model.call)) { _, wanted in
            if let wanted { panel = wanted.panel }
        }
    }

    private var tray: some View {
        HStack(spacing: 2) {
            ForEach(reactionChoices(), id: \.self) { one in
                ReactionChoice(what: one) {
                    board.send(one, in: model)
                    board.trayOpen = false
                }
            }
        }
        .padding(4)
        .glassSurface(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private struct ReactionChoice: View {
        let what: String
        let action: () -> Void
        @State private var hovering = false

        var body: some View {
            Button(action: action) {
                Text(what)
                    .font(.system(size: 20))
                    .frame(width: 36, height: 34)
                    .background(RoundedRectangle(cornerRadius: 8).fill(hovering ? Theme.surface : .clear))
                    .scaleEffect(hovering ? 1.25 : 1)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { hovering = $0 }
            .animation(Theme.settle, value: hovering)
            // The reaction itself, as the web labels it: it is its own name.
            .accessibilityLabel(what)
        }
    }
}
