import Foundation
import Observation
import SwiftUI
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

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

    /// A long press on the hand opened the tray, and the press's own release
    /// is still to come: the hand's button fires on that release too, and
    /// must not raise the hand as well.
    @ObservationIgnored fileprivate var held = false

    /// Whether a press on the hand was the end of a long press that opened
    /// the reactions instead, which then does nothing else. Asked by the
    /// hand's button before it does what it does.
    func pressWasHold() -> Bool {
        defer { held = false }
        return held
    }

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
    /// The bar's axis: down the side, the tray opens towards the pictures
    /// rather than over the buttons above it.
    var axis: Axis = .horizontal

    private var board: ReactionBoard { ReactionBoard.of(model.call) }
    private var open: Bool { board.trayOpen }

    var body: some View {
        HStack(spacing: 0) {
            Button {
                guard !board.pressWasHold() else { return }
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
            .opensReactions(board)
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
        .overlay(alignment: axis == .horizontal ? .bottom : .trailing) {
            if open {
                let anchor: UnitPoint = axis == .horizontal ? .bottom : .trailing
                tray
                    .fixedSize()
                    // Clear of the bar's own edge as well as the control's.
                    .offset(x: axis == .horizontal ? 0 : -78, y: axis == .horizontal ? -58 : 0)
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: anchor)).animation(Theme.settle),
                        removal: .opacity.combined(with: .scale(scale: 0.97, anchor: anchor)).animation(Theme.leave)
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

extension View {
    /// The reactions behind the hand, by the other way of pressing it: a
    /// right-click or a Control-click on the Mac, a long press on a touch
    /// screen (HandButton.tsx `onContextMenu`). The hand's own press stays
    /// the common act, raising it, and never opens anything; this is the way
    /// to the six for somebody who reaches for the hand, beside the narrow
    /// half that opens them too.
    ///
    /// Whatever draws the hand puts this on it, and its action begins with
    /// `guard !board.pressWasHold() else { return }`, so that the release
    /// of a long press does not raise the hand as well.
    func opensReactions(_ board: ReactionBoard) -> some View {
        modifier(OpensReactions(board: board))
    }
}

private struct OpensReactions: ViewModifier {
    let board: ReactionBoard
    @State private var opened = 0
    #if os(macOS)
    @State private var region = SecondaryClicks.Region()
    #endif

    func body(content: Content) -> some View {
        #if os(macOS)
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { region.frame = $0 }
            .onAppear { region.watch { toggle() } }
            .onDisappear { region.stop() }
        #else
        content
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.45).onEnded { _ in
                    board.held = true
                    toggle()
                }
            )
            // The finger lifting is when the hand's button would fire. A
            // finger that slid off the button first never fires it, and the
            // hold it ended must not swallow the next real press; so the mark
            // is cleared just after every lift, once the button has had its
            // chance to read it.
            .simultaneousGesture(
                DragGesture(minimumDistance: 0).onEnded { _ in
                    Task {
                        try? await Task.sleep(for: .milliseconds(80))
                        board.held = false
                    }
                }
            )
            .sensoryFeedback(.impact(weight: .light), trigger: opened)
        #endif
    }

    private func toggle() {
        withAnimation(Theme.settle) { board.trayOpen.toggle() }
        opened += 1
    }
}

#if os(macOS)
/// The other button, heard by the application before any view is: SwiftUI has
/// no way to hear it but a context menu, and the reactions are not a menu.
///
/// A view laid over the hand that took right-clicks in its hit test and let
/// everything else through was tried first; the hosting view decides for
/// itself which of its views a click is for, and the click went to the hand's
/// own button, which raised the hand. So the application's own stream of
/// events is watched instead, and a right-click or a Control-click inside the
/// region the hand occupies is taken out of it before it reaches any view.
enum SecondaryClicks {
    @MainActor
    final class Region {
        /// Where the hand is, in its window, from the top left (SwiftUI's
        /// global space, which is the window's content).
        var frame: CGRect = .zero
        private var monitor: Any?

        func watch(_ action: @escaping @MainActor () -> Void) {
            stop()
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { [weak self] event in
                // Read here, so that only plain values cross into the main
                // actor's isolation; the monitor is called on the main thread.
                guard Self.secondary(event) else { return event }
                let at = event.locationInWindow
                let window = event.windowNumber
                let taken = MainActor.assumeIsolated { () -> Bool in
                    guard let self, let content = NSApp.window(withWindowNumber: window)?.contentView else { return false }
                    guard self.frame.contains(CGPoint(x: at.x, y: content.bounds.height - at.y)) else { return false }
                    action()
                    return true
                }
                return taken ? nil : event
            }
        }

        func stop() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        nonisolated private static func secondary(_ event: NSEvent) -> Bool {
            event.type == .rightMouseDown || (event.type == .leftMouseDown && event.modifierFlags.contains(.control))
        }
    }
}
#endif
