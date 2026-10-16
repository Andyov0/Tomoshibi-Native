import SwiftUI
import TomoshibiBasics
#if os(macOS)
import AppKit
#endif

// What the call's controls add to the app's own pieces (Kit.swift): a
// popover hung off a control of the bar, above it or beside it; a button for
// the bar; a row to pick one of a list from.

// MARK: - The bar's popovers

/// The app's popover, hung off one of the bar's controls: above it while the
/// bar runs along the foot of the call, to its side while the bar runs down
/// the side ("At the side", controls.ts), where above a button is another
/// button.
///
/// Hung from a line of no thickness along the control's edge rather than
/// placed with an alignment guide. Inside the bar, Kit's `glassPopover`,
/// which places itself with one, came out over the control and the buttons
/// after it, running off the foot of the window: the guide that should have
/// lifted it above the control was not applied there. A popover that hangs
/// from the edge needs no guide and cannot cover what it belongs to.
struct ControlPopover<Panel: View>: ViewModifier {
    enum Edge { case top, leading }

    let id: String
    let popovers: Popovers
    var edge: Edge = .top
    var place: BarPopoverPlace = .centre
    var width: CGFloat = 280
    @ViewBuilder var panel: () -> Panel

    func body(content: Content) -> some View {
        let open = popovers.open == id
        content
            .overlay(alignment: edge == .top ? .top : .leading) {
                if edge == .top {
                    Color.clear.frame(height: 0)
                        .overlay(alignment: place == .end ? .bottomTrailing : .bottom) { pane(open) }
                } else {
                    Color.clear.frame(width: 0)
                        .overlay(alignment: place == .end ? .bottomTrailing : .trailing) { pane(open) }
                }
            }
            // Above the buttons that follow it in the bar, which are drawn
            // after it and would otherwise be drawn over its popover.
            .zIndex(open ? 10 : 0)
    }

    @ViewBuilder
    private func pane(_ open: Bool) -> some View {
        if open {
            let anchor: UnitPoint = edge == .top ? (place == .end ? .bottomTrailing : .bottom) : (place == .end ? .bottomTrailing : .trailing)
            panel()
                .frame(width: width)
                .glassPane(cornerRadius: 18)
                .fixedSize(horizontal: false, vertical: true)
                // Clear of the bar's own edge as well as the control's: the
                // bar's padding is seven points.
                .padding(edge == .top ? .bottom : .trailing, 19)
                .transition(.asymmetric(
                    insertion: .opacity.combined(with: .scale(scale: 0.92, anchor: anchor)).animation(Theme.settle),
                    removal: .opacity.combined(with: .scale(scale: 0.96, anchor: anchor)).animation(Theme.leave)
                ))
                .escapeCloses(popovers)
        }
    }
}

/// Where along its control a bar's popover sits: centred on it, or flush
/// with the end of the bar (the trailing edge along the foot, the bottom
/// down the side), so that the last buttons' popovers do not hang off the
/// window.
enum BarPopoverPlace {
    case centre, end
}

extension View {
    /// One of the bar's popovers: above its control along the foot of the
    /// call, beside it down the side.
    func barPopover<Panel: View>(_ id: String, in popovers: Popovers, axis: Axis, place: BarPopoverPlace = .centre,
                                 width: CGFloat = 280, @ViewBuilder panel: @escaping () -> Panel) -> some View {
        modifier(ControlPopover(id: id, popovers: popovers, edge: axis == .horizontal ? .top : .leading,
                                place: place, width: width, panel: panel))
    }

    /// Escape closes the open popover: the Mac's exit command, a hardware
    /// keyboard's Escape on the iPad (as Kit.swift's own popover does).
    @ViewBuilder
    fileprivate func escapeCloses(_ popovers: Popovers) -> some View {
        #if os(macOS)
        onExitCommand { popovers.close() }
        #else
        onKeyPress(.escape) {
            popovers.close()
            return .handled
        }
        #endif
    }
}

#if os(macOS)
/// Escape closes whichever of the screen's popovers is open, wherever the
/// keyboard is.
///
/// A popover's own exit command only fires while something inside it has the
/// keyboard, and a popover opened with the pointer has nothing focused: the
/// leaving popover, which the web closes on Escape because "a dialog that can
/// only be dismissed by choosing something is a dialog somebody chooses
/// something in by accident" (Leaving.tsx), stayed open. So while a popover
/// is open the window's key presses are looked at first, and Escape is taken.
struct EscapeClosesPopovers: ViewModifier {
    let popovers: Popovers?
    @State private var monitor: Any?

    func body(content: Content) -> some View {
        content
            .onChange(of: popovers?.open != nil, initial: true) { _, open in
                if open { watch() } else { unwatch() }
            }
            .onDisappear { unwatch() }
    }

    private func watch() {
        guard monitor == nil, let popovers else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            // 53 is Escape on every keyboard layout.
            guard event.keyCode == 53, popovers.open != nil else { return event }
            MainActor.assumeIsolated { popovers.close() }
            return nil
        }
    }

    private func unwatch() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
}
#endif

// MARK: - The bar's buttons

/// One round button of the control bar: a lit plate in the chosen colour
/// while what it stands for is on, a plain one otherwise, a little brighter
/// under the pointer and while its popover is open.
///
/// Inside the bar's glass the buttons are lit plates rather than glass of
/// their own: glass on glass reads as smudge.
struct BarButton: View {
    let symbol: String
    let label: String
    var lit = false
    /// Off, and drawn quieter for it: a muted microphone is a choice, not a
    /// fault, so it is unlit rather than red (ControlBar.tsx `Toggle`).
    var quiet = false
    /// Its popover is open.
    var pressed = false
    var size: CGFloat = 44
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size * 0.36, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(lit ? Theme.tallyForeground : (quiet ? Theme.muted : Theme.foreground))
                .frame(width: size, height: size)
                .background(Circle().fill(lit ? Theme.tally : Color.white.opacity(pressed ? 0.24 : hovering && enabled ? 0.18 : 0.12)))
                .overlay(Circle().stroke(Color.white.opacity(lit ? 0.3 : pressed ? 0.2 : 0.08), lineWidth: 1))
                .contentShape(Circle())
        }
        .buttonStyle(BarPress())
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: lit)
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: pressed)
        .accessibilityLabel(label)
        .accessibilityAddTraits(lit || pressed ? .isSelected : [])
        .help(label)
    }
}

/// A press that answers: the button gives a little under the finger.
struct BarPress: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.93 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

/// The dot a bar button wears when something it stands for is happening
/// out of sight (ControlBar.tsx ~383): in the signal colour, ringed in the
/// bar's own grey.
struct BarDot: View {
    var body: some View {
        Circle().fill(Theme.tally).frame(width: 9, height: 9)
            .overlay(Circle().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
            .offset(x: -2, y: 2)
            .transition(Theme.arrive)
    }
}

// MARK: - Choosing one of a list

/// One of a list to pick from, inside a settings card: a device, a language.
/// The chosen one is ticked in the signal colour; the row lights under the
/// pointer, as a menu's rows do.
struct ChoiceRow: View {
    let title: String
    var symbol: String?
    var checked = false
    var last = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        VStack(spacing: 0) {
            Button(action: action) {
                HStack(spacing: 12) {
                    if let symbol {
                        Image(systemName: symbol)
                            .font(.system(size: 12.5, weight: .semibold))
                            .foregroundStyle(checked ? Theme.tally : Theme.foreground.opacity(0.75))
                            // As wide as a settings row's symbol tile, so the
                            // words of both kinds of row start in one line.
                            .frame(width: 26)
                    }
                    Text(title)
                        .font(.system(size: 13.5, weight: checked ? .medium : .regular))
                        .foregroundStyle(Theme.foreground)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 8)
                    if checked {
                        Image(systemName: "checkmark")
                            .font(.system(size: 11.5, weight: .bold))
                            .foregroundStyle(Theme.tally)
                            .transition(.scale(scale: 0.5).combined(with: .opacity))
                    }
                }
                .padding(.horizontal, 14)
                .frame(minHeight: 40)
                .background(Color.white.opacity(hovering && enabled ? 0.06 : 0))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(enabled ? 1 : 0.45)
            .onHover { hovering = $0 }
            .animation(.easeOut(duration: 0.12), value: hovering)
            .animation(Theme.settle, value: checked)
            .accessibilityAddTraits(checked ? .isSelected : [])
            if !last {
                Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
                    .padding(.leading, symbol == nil ? 14 : 52)
            }
        }
    }
}
