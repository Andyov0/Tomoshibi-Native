import SwiftUI
import TomoshibiBasics

/// The call's notices, in the corner above the controls (the web draws
/// sonner at the bottom right, Notices.tsx).
///
/// Piled as sonner piles them: the newest in front, at most two more peeking
/// above it, so that three people arriving at once is one card's worth of the
/// picture covered rather than a column of them. Reading the corner spreads
/// the pile into a column and stops every countdown, so a notice is never
/// taken away from under the pointer or the finger that is on it; letting go
/// lets them run on with whatever they had left. A notice is swiped to the
/// right to be rid of it, and one that stays until it is dealt with also has
/// its close button.
///
/// Where the messages in the other corner would meet these, on a phone or in
/// a narrow window, these rise above them (CornerLanes).
struct ToastStack: View {
    @Bindable var model: CallModel
    /// The corner spread into a column: under the pointer on a Mac, or after
    /// a tap on a touch screen.
    @State private var spread = false
    @State private var heights: [UUID: CGFloat] = [:]
    /// The leading edge of this column, in the window, to tell whether it
    /// meets the messages.
    @State private var leading: CGFloat = .infinity
    @State private var collapsing: Task<Void, Never>?
    /// The pile closing after the pointer left it, a moment later, so that
    /// crossing the gap between two spread cards does not close it under the
    /// pointer on the way.
    @State private var leaving: Task<Void, Never>?

    /// The widest a notice is drawn, which is the web's eighteen rems and a
    /// little: wide enough for two lines of a refusal, narrow enough to leave
    /// a picture's face alone.
    static let width: CGFloat = 300

    var body: some View {
        // Newest first: the front of the pile is the last thing said.
        let shown = Array(model.toasts.reversed())
        let measured = shown.map { heights[$0.id] ?? 44 }
        let open = spread || Self.forcedSpread
        let lifts = NoticePile.lifts(measured, spread: open)
        let front = measured.first ?? 0
        ZStack(alignment: .bottomTrailing) {
            ForEach(Array(shown.enumerated()), id: \.element.id) { place, toast in
                ToastCard(
                    toast: toast,
                    attachment: toast.attachment.flatMap { model.attachments[$0] },
                    // Behind the front one, while piled, a card is the front
                    // one's height and shows nothing of its own: only its
                    // edge, peeking above.
                    pinnedHeight: !open && place > 0 ? front : nil,
                    act: { model.act(on: toast.id) },
                    close: { model.dismiss(toast.id) },
                    interacting: { touching in hold(touching) },
                    measured: { height in
                        if heights[toast.id] != height { heights[toast.id] = height }
                    }
                )
                .scaleEffect(NoticePile.scale(place: place, spread: open), anchor: .top)
                .offset(y: -(lifts.indices.contains(place) ? lifts[place] : 0))
                .zIndex(Double(shown.count - place))
                .allowsHitTesting(open || place == 0)
                .transition(.asymmetric(
                    insertion: .offset(y: 18).combined(with: .opacity).combined(with: .scale(scale: 0.96, anchor: .bottom))
                        .animation(Theme.settle),
                    removal: .opacity.combined(with: .scale(scale: 0.96)).animation(Theme.leave)
                ))
            }
        }
        .frame(height: NoticePile.height(measured, spread: open), alignment: .bottomTrailing)
        // No shape of its own: only the cards take a press, as only sonner's
        // toasts do on the web (its list has no height). The column used to
        // be a rectangle the full width of the widest notice and the height
        // of the pile, drawn over the filmstrip, and a press on a picture
        // anywhere in it beside a short notice went to nothing.
        .frame(maxWidth: Self.width, alignment: .bottomTrailing)
        #if os(macOS)
        .onHover { inside in hovered(inside) }
        #else
        // No pointer to rest on a touch screen: a tap spreads the pile and
        // holds it a few seconds, as long as it takes to read three lines.
        .onTapGesture {
            guard !model.toasts.isEmpty else { return }
            withAnimation(Theme.settle) { spread = true }
            model.holdToasts(true)
            collapsing?.cancel()
            collapsing = Task {
                try? await Task.sleep(for: .seconds(4))
                guard !Task.isCancelled else { return }
                withAnimation(Theme.settle) { spread = false }
                model.holdToasts(Self.forcedHold)
            }
        }
        #endif
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minX } action: { leading = $0 }
        .padding(.bottom, CornerLanes.lift(notices: leading, said: model.saidFrame))
        .animation(Theme.settle, value: shown.map(\.id))
        .animation(Theme.settle, value: measured)
        .animation(Theme.settle, value: model.saidFrame)
        .onChange(of: model.toasts.isEmpty) { _, empty in
            guard empty else { return }
            spread = false
            collapsing?.cancel()
            model.holdToasts(Self.forcedHold)
            heights = [:]
        }
        .onChange(of: model.toasts.last?.id) { _, id in
            // Said aloud to a screen reader as it arrives, as sonner's live
            // region does: a notice nobody can see is one nobody was given.
            guard let id, let toast = model.toasts.first(where: { $0.id == id }) else { return }
            AccessibilityNotification.Announcement([toast.title, toast.detail].compactMap { $0 }.joined(separator: ". ")).post()
        }
        .onAppear { if Self.forcedHold { model.holdToasts(true) } }
        .accessibilityElement(children: .contain)
    }

    #if os(macOS)
    private func hovered(_ inside: Bool) {
        leaving?.cancel()
        guard !inside else {
            withAnimation(Theme.settle) { spread = !model.toasts.isEmpty }
            model.holdToasts(!model.toasts.isEmpty || Self.forcedHold)
            return
        }
        leaving = Task {
            try? await Task.sleep(for: .milliseconds(160))
            guard !Task.isCancelled else { return }
            withAnimation(Theme.settle) { spread = false }
            model.holdToasts(Self.forcedHold)
        }
    }
    #endif

    /// A finger on one card (dragging it away) holds the corner as reading
    /// it does.
    private func hold(_ touching: Bool) {
        if touching { model.holdToasts(true) } else if !spread && !Self.forcedHold { model.holdToasts(false) }
    }

    /// TOMOSHIBI_DEBUG_TOASTS=spread, in a debug build: the corner as it is
    /// while being read, spread and held, so that a script can photograph a
    /// column of notices without a pointer; =hold keeps them piled but
    /// stops their countdowns, for a photograph of the pile.
    static let forcedSpread: Bool = debugToasts == "spread"
    static let forcedHold: Bool = debugToasts == "spread" || debugToasts == "hold"

    private static var debugToasts: String? {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_TOASTS"]
        #else
        nil
        #endif
    }
}

/// One notice: a pane of glass with the words, an edge that says what kind of
/// thing it is, and whatever it offers to do.
private struct ToastCard: View {
    let toast: CallToast
    let attachment: (@MainActor () -> AnyView)?
    let pinnedHeight: CGFloat?
    let act: () -> Void
    let close: () -> Void
    let interacting: (Bool) -> Void
    let measured: (CGFloat) -> Void
    @State private var dragged: CGFloat = 0
    @State private var dragging = false

    var body: some View {
        content
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { measured($0) }
            .opacity(pinnedHeight == nil ? 1 : 0)
            .frame(height: pinnedHeight, alignment: .top)
            .clipped()
            .glassPane(cornerRadius: 14)
            .offset(x: dragged)
            .opacity(1 - min(0.7, Double(max(0, dragged)) / 260))
            .gesture(swipe)
            .accessibilityElement(children: .contain)
            .accessibilityAction(named: Text(t("Dismiss"))) { close() }
    }

    private var content: some View {
        // A notice is no wider than its words (Notices.tsx `sm:w-fit`), up to
        // the column's width; one carrying the door's list takes all of it.
        FitWidth(minimum: toast.attachment == nil ? 170 : ToastStack.width, maximum: ToastStack.width) {
            line
        }
    }

    private var line: some View {
        HStack(alignment: .top, spacing: 10) {
            // notices.ts: the edge is the whole of the severity system. A
            // wash of colour over a card this small says less than a rail
            // beside it.
            Capsule()
                .fill(edge)
                .frame(width: 3)
                .frame(maxHeight: .infinity)
                .padding(.vertical, 1)
                .shadow(color: toast.kind == .info ? .clear : edge.opacity(0.6), radius: 4)
            VStack(alignment: .leading, spacing: 3) {
                Text(toast.title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .fixedSize(horizontal: false, vertical: true)
                if let detail = toast.detail {
                    Text(detail)
                        .font(.system(size: 11.5))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let attachment {
                    attachment()
                        .padding(.top, 5)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let actionTitle = toast.actionTitle {
                ActionPill(title: actionTitle, action: act)
                    .frame(maxHeight: .infinity)
            }
            if toast.closable {
                CloseDot(action: close)
            }
        }
        .padding(.leading, 9)
        .padding(.trailing, toast.closable ? 7 : 12)
        .padding(.vertical, 10)
    }

    /// Plain for something that happened, the signal colour for somebody else
    /// taking the stage or anything still happening, red for a failure.
    private var edge: Color {
        switch toast.kind {
        case .info: Color.white.opacity(0.22)
        case .signal, .warning: Theme.tally
        case .failure: Theme.danger
        }
    }

    /// To the right and away; a short drag springs back. Leftwards it hardly
    /// moves, so that it feels held rather than broken.
    private var swipe: some Gesture {
        DragGesture(minimumDistance: 8)
            .onChanged { value in
                if !dragging {
                    dragging = true
                    interacting(true)
                }
                let x = value.translation.width
                dragged = x < 0 ? x / 6 : x
            }
            .onEnded { value in
                dragging = false
                if value.translation.width > 70 || value.predictedEndTranslation.width > 160 {
                    withAnimation(.timingCurve(0.4, 0, 1, 1, duration: 0.18)) { dragged = ToastStack.width + 60 }
                    Task {
                        try? await Task.sleep(for: .milliseconds(170))
                        close()
                        interacting(false)
                    }
                } else {
                    withAnimation(Theme.settle) { dragged = 0 }
                    interacting(false)
                }
            }
    }
}

/// A notice's one action, in the foreground colour rather than the signal
/// one (Notices.tsx `actionButton`): the notice it sits on is usually saying
/// something cannot be done, and the signal colour means something is
/// happening.
private struct ActionPill: View {
    let title: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(Theme.background)
                .lineLimit(1)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(Capsule().fill(Theme.foreground.opacity(hovering ? 0.88 : 1)))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
    }
}

/// The close button, for a notice about something still true.
private struct CloseDot: View {
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 8.5, weight: .bold))
                .foregroundStyle(hovering ? Theme.foreground : Theme.muted)
                .frame(width: 20, height: 20)
                .background(Circle().fill(Color.white.opacity(hovering ? 0.12 : 0)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityLabel(t("Dismiss"))
        .help(t("Dismiss"))
    }
}

#if DEBUG
extension CallModel {
    /// TOMOSHIBI_DEBUG_NOTICES=<file>, in a debug build: every notice raised
    /// is appended to <file> as it is raised, one line each, so that a script
    /// driving a browser in the same room can read what the app said without
    /// catching it on screen in the seconds it lasts.
    static func noticeLog(_ toast: CallToast) {
        guard let path = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_NOTICES"], !path.isEmpty else { return }
        let line = "\(Date().timeIntervalSince1970) \(toast.kind) \(toast.lasts.map { String($0) } ?? "stays")"
            + "\(toast.closable ? " closable" : "") | \(toast.title)\(toast.detail.map { " | \($0)" } ?? "")\n"
        let data = Data(line.utf8)
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }
}
#endif
