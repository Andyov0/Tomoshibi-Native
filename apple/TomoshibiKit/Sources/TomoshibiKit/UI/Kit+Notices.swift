import SwiftUI
import TomoshibiBasics

// More of the app's own controls (see Kit.swift), for the panels beside a
// call and the corner of it: a volume slider made of glass, small round
// buttons, a panel's heading, a person's monogram, and the layout a notice
// uses to be no wider than its words.

// MARK: - Sliders

/// How loud, from nothing to full: a track of glass that fills in the
/// foreground colour, and a thumb that grows under the pointer.
///
/// The system's slider was what the sound panel used, and it is the one
/// stock piece left that looked like a preferences pane in a call (the owner
/// asked for none, 2026-10-17). The web's is the browser's own range input,
/// tinted in the foreground colour (SoundPanel.tsx `accent-fg`), and so is
/// this: a volume is not something switched on, and the signal colour is kept
/// for what is.
struct GlassSlider: View {
    @Binding var value: Double
    /// What a press or a key moves it to the nearest of: the web's range
    /// steps in twentieths.
    var step: Double = 0.05
    let label: String
    @Environment(\.isEnabled) private var enabled
    @State private var hovering = false
    @State private var dragging = false

    private var lit: Bool { (hovering || dragging) && enabled }

    var body: some View {
        GeometryReader { geometry in
            let thumb: CGFloat = lit ? 17 : 14
            let width = geometry.size.width
            let travel = max(1, width - thumb)
            let x = thumb / 2 + CGFloat(min(1, max(0, value))) * travel
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.black.opacity(0.28))
                    .overlay(Capsule().stroke(Color.white.opacity(0.10), lineWidth: 1))
                    .frame(height: lit ? 6 : 5)
                Capsule()
                    .fill(LinearGradient(colors: [Theme.foreground.opacity(0.62), Theme.foreground.opacity(0.95)],
                                         startPoint: .leading, endPoint: .trailing))
                    .frame(width: max(lit ? 6 : 5, x), height: lit ? 6 : 5)
                Circle()
                    .fill(Color.white)
                    .overlay(Circle().stroke(Color.black.opacity(0.12), lineWidth: 0.5))
                    .shadow(color: .black.opacity(0.35), radius: dragging ? 5 : 3, y: 1)
                    .frame(width: thumb, height: thumb)
                    .offset(x: x - thumb / 2)
            }
            .frame(width: width, height: geometry.size.height)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        guard enabled else { return }
                        dragging = true
                        set(Double((drag.location.x - thumb / 2) / travel))
                    }
                    .onEnded { _ in dragging = false }
            )
        }
        .frame(height: 22)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: lit)
        .animation(.easeOut(duration: 0.08), value: value)
        .accessibilityElement()
        .accessibilityLabel(label)
        .accessibilityValue("\(Int((value * 100).rounded()))%")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: set(value + step)
            case .decrement: set(value - step)
            @unknown default: break
            }
        }
        #if os(macOS)
        .focusable(enabled)
        .onKeyPress(.leftArrow) { set(value - step); return .handled }
        .onKeyPress(.rightArrow) { set(value + step); return .handled }
        #endif
    }

    /// Rounded to the step rather than given one: a stepped system slider
    /// draws a tick for every twentieth.
    private func set(_ raw: Double) {
        let snapped = (min(1, max(0, raw)) / step).rounded() * step
        if abs(snapped - value) > 0.0001 { value = min(1, max(0, snapped)) }
    }
}

// MARK: - Small buttons

/// A small round button with a symbol: a panel's close, a row's actions.
/// Lit under the pointer; a destructive one reddens there, so the colour
/// that means leaving is only on what is about to be pressed.
struct RoundButton: View {
    enum Role { case normal, destructive }

    let symbol: String
    let label: String
    var size: CGFloat = 28
    var role: Role = .normal
    /// Pressable but drawn faint, for a press that will only be refused with
    /// a reason (making a guest host).
    var faint = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size * 0.43, weight: .semibold))
                .foregroundStyle(ink)
                .frame(width: size, height: size)
                .background(Circle().fill(fill))
                .overlay(Circle().stroke(Color.white.opacity(hovering && enabled ? 0.12 : 0), lineWidth: 1))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .opacity(!enabled ? 0.3 : faint ? 0.45 : 1)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityLabel(label)
        .help(label)
    }

    private var ink: Color {
        guard hovering && enabled else { return Theme.muted }
        return role == .destructive ? Theme.danger : Theme.foreground
    }

    private var fill: Color {
        guard hovering && enabled else { return Color.white.opacity(0.0) }
        return role == .destructive ? Theme.danger.opacity(0.16) : Color.white.opacity(0.10)
    }
}

/// Words in a small filled capsule, for a row's answer inside a pane that is
/// already glass (glass on glass reads as smudge): the person's colour for
/// the one that says yes, a lit plate for the other, red for the one that
/// cannot be undone.
struct PillButton: View {
    enum Role { case normal, prominent, destructive }

    let title: String
    var role: Role = .normal
    var busy = false
    var height: CGFloat = 26
    /// Stretched across what it is given, for a pair that shares a row.
    var wide = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if busy {
                    ProgressView().controlSize(.mini).tint(ink)
                        .transition(.opacity.combined(with: .scale(scale: 0.6)))
                }
                Text(title)
                    .font(.system(size: height < 28 ? 11.5 : 12.5, weight: role == .normal ? .medium : .semibold))
                    .lineLimit(1)
            }
            .foregroundStyle(ink)
            .padding(.horizontal, height * 0.42)
            .frame(maxWidth: wide ? .infinity : nil, minHeight: height)
            .background(Capsule().fill(fill))
            .overlay(Capsule().stroke(Color.white.opacity(role == .normal ? 0.08 : 0.18), lineWidth: 1))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: busy)
    }

    private var ink: Color {
        switch role {
        case .normal: Theme.foreground
        case .prominent: Theme.tallyForeground
        case .destructive: Theme.dangerForeground
        }
    }

    private var fill: Color {
        let lift = hovering && enabled
        switch role {
        case .normal: return Color.white.opacity(lift ? 0.16 : 0.10)
        case .prominent: return Theme.tally.opacity(lift ? 0.88 : 1)
        case .destructive: return Theme.danger.opacity(lift ? 0.88 : 1)
        }
    }
}

// MARK: - A panel's heading

/// The top of a panel beside the call: a symbol, its name, and the close.
struct PanelHeading<Trailing: View>: View {
    let title: String
    var symbol: String?
    var symbolTint: Color = Theme.foreground.opacity(0.85)
    let closeLabel: String
    let close: () -> Void
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 9) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(symbolTint)
                    .frame(width: 26, height: 26)
                    .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.white.opacity(0.09)))
            }
            Text(title)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            Spacer(minLength: 8)
            trailing
            RoundButton(symbol: "xmark", label: closeLabel, size: 26, action: close)
        }
        .padding(.leading, 14)
        .padding(.trailing, 10)
        .padding(.vertical, 10)
    }
}

extension PanelHeading where Trailing == EmptyView {
    init(title: String, symbol: String? = nil, symbolTint: Color = Theme.foreground.opacity(0.85),
         closeLabel: String, close: @escaping () -> Void) {
        self.init(title: title, symbol: symbol, symbolTint: symbolTint, closeLabel: closeLabel, close: close) { EmptyView() }
    }
}

/// The hairline under a panel's heading, fading at its ends.
struct PanelRule: View {
    var body: some View {
        LinearGradient(colors: [Color.white.opacity(0), Color.white.opacity(0.10), Color.white.opacity(0)],
                       startPoint: .leading, endPoint: .trailing)
            .frame(height: 1)
    }
}

// MARK: - Cards

extension View {
    /// A group of rows inside a panel: a faint plate of its own on the
    /// panel's glass, edged in `tint` where the group is about something
    /// happening (the door) or about to (ending the meeting).
    func panelCard(tint: Color? = nil) -> some View {
        background(RoundedRectangle(cornerRadius: 16, style: .continuous)
            .fill(tint.map { $0.opacity(0.10) } ?? Color.white.opacity(0.05)))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(tint.map { $0.opacity(0.35) } ?? Color.white.opacity(0.07), lineWidth: 1))
    }
}

// MARK: - People

/// Somebody, small: their initials on a blend of two of the web's avatar
/// colours (live/avatar.ts `AVATAR_COLOURS`), chosen by their identity so the
/// same person is the same colours in every corner. The tile's own avatar is
/// drawn for a tile and is a speck at this size.
struct Monogram: View {
    let name: String
    let identity: String
    var size: CGFloat = 24

    /// "#8a6a45", "#a1584a", "#5d6b58", "#4b5a6b", "#96814f", "#6b5566".
    private static let colours: [Color] = [
        Color(red: 0x8a / 255, green: 0x6a / 255, blue: 0x45 / 255),
        Color(red: 0xa1 / 255, green: 0x58 / 255, blue: 0x4a / 255),
        Color(red: 0x5d / 255, green: 0x6b / 255, blue: 0x58 / 255),
        Color(red: 0x4b / 255, green: 0x5a / 255, blue: 0x6b / 255),
        Color(red: 0x96 / 255, green: 0x81 / 255, blue: 0x4f / 255),
        Color(red: 0x6b / 255, green: 0x55 / 255, blue: 0x66 / 255),
    ]

    var body: some View {
        let (first, second) = Self.pair(identity)
        ZStack {
            Circle().fill(LinearGradient(colors: [first, second], startPoint: .topLeading, endPoint: .bottomTrailing))
            Circle().stroke(Color.white.opacity(0.14), lineWidth: 0.75)
            Text(initials)
                .font(.system(size: size * 0.40, weight: .semibold, design: .rounded))
                .foregroundStyle(Color.white.opacity(0.94))
                .minimumScaleFactor(0.6)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private var initials: String {
        let words = name.split(whereSeparator: \.isWhitespace)
        let letters = words.prefix(2).compactMap(\.first)
        return letters.isEmpty ? "?" : String(letters).uppercased()
    }

    /// FNV-1a over the identity, as the tile's avatar hashes it.
    private static func pair(_ identity: String) -> (Color, Color) {
        var hash: UInt64 = 1469598103934665603
        for byte in identity.utf8 { hash = (hash ^ UInt64(byte)) &* 1099511628211 }
        let first = Int(hash % UInt64(colours.count))
        let second = (first + 1 + Int((hash / 7) % UInt64(colours.count - 1))) % colours.count
        return (colours[first], colours[second])
    }
}

// MARK: - Fitting

/// As wide as what it holds wants, within `minimum` and `maximum`, and as tall
/// as that width makes it: a notice no wider than its words, and words that
/// wrap at the maximum rather than running out of the card.
///
/// A `frame(maxWidth:)` under `fixedSize` measures the height at the ideal,
/// one-line width and then lays the words out narrower, so a notice that
/// wrapped ran out of the bottom of its own glass.
struct FitWidth: Layout {
    var minimum: CGFloat = 0
    var maximum: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let only = subviews.first else { return .zero }
        let width = fitted(only, proposal)
        let height = only.sizeThatFits(ProposedViewSize(width: width, height: nil)).height
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let only = subviews.first else { return }
        only.place(at: bounds.origin, proposal: ProposedViewSize(width: bounds.width, height: bounds.height))
    }

    private func fitted(_ view: LayoutSubview, _ proposal: ProposedViewSize) -> CGFloat {
        let ceiling = min(maximum, proposal.width ?? maximum)
        let ideal = view.sizeThatFits(.unspecified).width
        return max(min(minimum, ceiling), min(ideal, ceiling))
    }
}
