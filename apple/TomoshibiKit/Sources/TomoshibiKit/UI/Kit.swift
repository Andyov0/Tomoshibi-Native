import SwiftUI
import TomoshibiBasics

// The app's own controls, drawn rather than borrowed.
//
// The owner did not want the system's stock pieces (2026-10-17): a grouped
// Form, a pull-down Menu and a confirmation dialog look like a preferences
// pane bolted onto a call, and the web client, which draws its own, looked
// better than the native one. Everything here is glass over the call or the
// backdrop, wears the person's chosen colour only where something is on
// or primary, and arrives and leaves with the same two curves as every other
// view (Theme.arrive). Screens build their menus, popovers and settings out
// of these so that they look like one app.

// MARK: - Popovers

/// Which of the app's popovers is open: at most one, as with the system's
/// own, so opening one closes another.
@MainActor
@Observable
final class Popovers {
    private(set) var open: String?

    func toggle(_ id: String) {
        withAnimation(Theme.settle) { open = open == id ? nil : id }
    }

    func show(_ id: String) {
        withAnimation(Theme.settle) { open = id }
    }

    func close() {
        guard open != nil else { return }
        withAnimation(Theme.leave) { open = nil }
    }
}

extension EnvironmentValues {
    /// The screen's popovers; nil outside a screen that keeps any.
    @Entry var popovers: Popovers?
}

/// A pane of glass anchored to a control, above or below it: the app's own
/// popover. It closes on Escape, when its id is toggled again, and when the
/// screen's `PopoverCatcher` is clicked.
struct GlassPopover<Panel: View>: ViewModifier {
    let id: String
    let popovers: Popovers
    var edge: VerticalEdge = .top
    var alignment: HorizontalAlignment = .center
    var width: CGFloat = 280
    @ViewBuilder var panel: () -> Panel

    func body(content: Content) -> some View {
        content.overlay(alignment: Alignment(horizontal: alignment, vertical: edge == .top ? .top : .bottom)) {
            if popovers.open == id {
                panel()
                    .frame(width: width)
                    .glassShell(cornerRadius: 18)
                    .fixedSize(horizontal: false, vertical: true)
                    .alignmentGuide(edge == .top ? VerticalAlignment.top : VerticalAlignment.bottom) { d in
                        edge == .top ? d[.bottom] + 12 : d[.top] - 12
                    }
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .scale(scale: 0.92, anchor: edge == .top ? .bottom : .top)).animation(Theme.settle),
                        removal: .opacity.combined(with: .scale(scale: 0.96, anchor: edge == .top ? .bottom : .top)).animation(Theme.leave)
                    ))
                    .closesOnEscape(popovers)
                    .zIndex(20)
            }
        }
    }
}

extension View {
    func glassPopover<Panel: View>(_ id: String, in popovers: Popovers, edge: VerticalEdge = .top,
                                   alignment: HorizontalAlignment = .center, width: CGFloat = 280,
                                   @ViewBuilder panel: @escaping () -> Panel) -> some View {
        modifier(GlassPopover(id: id, popovers: popovers, edge: edge, alignment: alignment, width: width, panel: panel))
    }
}

extension View {
    /// Escape closes the open popover: the Mac's exit command, a hardware
    /// keyboard's Escape on the iPad.
    @ViewBuilder
    func closesOnEscape(_ popovers: Popovers) -> some View {
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

/// Behind a screen's popovers: a click anywhere else closes the open one, as
/// it does for the system's. Placed under the layer the popovers are drawn
/// in, it takes the click before whatever is under it does.
struct PopoverCatcher: View {
    let popovers: Popovers

    var body: some View {
        if popovers.open != nil {
            Color.black.opacity(0.001)
                .contentShape(Rectangle())
                .onTapGesture { popovers.close() }
                .ignoresSafeArea()
        }
    }
}

// MARK: - The keyboard

extension View {
    /// On an iPhone, puts the keyboard away on a tap on nothing in particular
    /// and on a scroll: tapped into the lobby's join bar or the link field,
    /// the keyboard stayed over half the screen with no way to put it away
    /// but pressing Join. Taken by a catcher behind the content rather than a
    /// gesture over it, so that a tap on a field or a button is still theirs:
    /// a tap laid over everything put the keyboard away again as a field was
    /// pressed to bring it up.
    @ViewBuilder
    func putsKeyboardAway() -> some View {
        #if os(iOS)
        scrollDismissesKeyboard(.immediately)
            .background {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { putKeyboardAway() }
            }
        #else
        self
        #endif
    }
}

#if os(iOS)
/// Whatever is typing gives the keyboard up.
@MainActor
func putKeyboardAway() {
    UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
}
#endif

// MARK: - Menus

/// What a popover menu is made of: rows of items, small headings, rules.
struct MenuCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) { content }
            .padding(6)
    }
}

struct MenuHeading: View {
    let title: String

    var body: some View {
        Text(title.uppercased())
            .font(.system(size: 10.5, weight: .semibold))
            .tracking(0.6)
            .foregroundStyle(Theme.muted)
            .padding(.horizontal, 10)
            .padding(.top, 8)
            .padding(.bottom, 2)
    }
}

struct MenuRule: View {
    var body: some View {
        Rectangle().fill(Theme.ink.opacity(0.08)).frame(height: 1)
            .padding(.horizontal, 8).padding(.vertical, 4)
    }
}

/// One row of a menu: a symbol, words, perhaps a detail or a check, lit under
/// the pointer.
struct MenuItem: View {
    enum Role { case normal, destructive, prominent }

    let title: String
    var symbol: String?
    var detail: String?
    var role: Role = .normal
    var checked = false
    var busy = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 18)
                        .foregroundStyle(role == .destructive ? Theme.dangerInk : role == .prominent ? Theme.tallyInk : Theme.foreground.opacity(0.85))
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(title)
                        .font(.system(size: 13.5, weight: role == .normal ? .regular : .semibold))
                        .foregroundStyle(role == .destructive ? Theme.dangerInk : Theme.foreground)
                    if let detail {
                        Text(detail).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 8)
                if busy {
                    ProgressView().controlSize(.small)
                } else if checked {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11.5, weight: .bold))
                        .foregroundStyle(Theme.tallyInk)
                        .transition(.scale.combined(with: .opacity))
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, detail == nil ? 7 : 6)
            .frame(minHeight: 34)
            .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Theme.ink.opacity(hovering && enabled ? 0.09 : 0)))
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: checked)
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// A menu row that is a switch.
struct MenuSwitch: View {
    let title: String
    var symbol: String?
    var detail: String?
    @Binding var isOn: Bool

    var body: some View {
        HStack(spacing: 10) {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 13, weight: .medium)).frame(width: 18)
                    .foregroundStyle(Theme.foreground.opacity(0.85))
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.system(size: 13.5))
                if let detail {
                    Text(detail).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 8)
            Toggle("", isOn: $isOn).labelsHidden().toggleStyle(.glassSwitch)
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 36)
    }
}

// MARK: - Choosing

/// The app's switch: a capsule that fills with the chosen colour when on.
struct GlassSwitchStyle: ToggleStyle {
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 8) {
                configuration.label
                ZStack(alignment: configuration.isOn ? .trailing : .leading) {
                    Capsule()
                        .fill(configuration.isOn ? Theme.tally : Theme.ink.opacity(0.14))
                        .overlay(Capsule().stroke(Theme.ink.opacity(configuration.isOn ? 0 : 0.1)))
                    Circle()
                        .fill(Color.white)
                        .shadow(color: .black.opacity(0.25), radius: 2, y: 1)
                        .padding(2.5)
                }
                .frame(width: 38, height: 23)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .animation(Theme.settle, value: configuration.isOn)
        .accessibilityValue(configuration.isOn ? Text("1") : Text("0"))
    }
}

extension ToggleStyle where Self == GlassSwitchStyle {
    static var glassSwitch: GlassSwitchStyle { GlassSwitchStyle() }
}

/// A few choices side by side, the chosen one under a pill that slides to it.
struct SegmentedChoice<Value: Hashable>: View {
    let options: [(value: Value, label: String)]
    @Binding var selection: Value
    var height: CGFloat = 30
    @Namespace private var pill

    var body: some View {
        HStack(spacing: 2) {
            ForEach(options.indices, id: \.self) { index in
                let option = options[index]
                let chosen = option.value == selection
                Button {
                    withAnimation(Theme.settle) { selection = option.value }
                } label: {
                    Text(option.label)
                        .font(.system(size: 12.5, weight: chosen ? .semibold : .regular))
                        .foregroundStyle(chosen ? Theme.foreground : Theme.muted)
                        .lineLimit(1)
                        .padding(.horizontal, 10)
                        .frame(maxWidth: .infinity, minHeight: height - 4)
                        .background {
                            if chosen {
                                // By day the chosen one is white on the
                                // track's grey, as the system's own are.
                                Capsule().fill(Theme.lifted(0.16))
                                    .overlay(Capsule().stroke(Theme.ink.opacity(0.10)))
                                    .matchedGeometryEffect(id: "pill", in: pill)
                            }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chosen ? .isSelected : [])
            }
        }
        .padding(2)
        .background(Capsule().fill(Theme.sunk(0.22)))
        .overlay(Capsule().stroke(Theme.ink.opacity(0.07)))
    }
}

// MARK: - Settings

/// A card of settings rows under a heading.
struct SettingsSection<Content: View>: View {
    let title: String
    var footer: String?
    @ViewBuilder var content: Content
    @Environment(\.onLifted) private var nested

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.muted)
                .padding(.leading, 6)
            VStack(spacing: 0) { content }
                .lifted()
                .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.lifted(0.05, nested: nested)))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.ink.opacity(0.07)))
            if let footer {
                Text(footer).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                    .padding(.horizontal, 6)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// One row of a settings card: a symbol in a tile, words, and the control.
struct SettingsRow<Control: View>: View {
    let title: String
    var detail: String?
    var symbol: String?
    var last = false
    @ViewBuilder var control: Control

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(Theme.foreground)
                        .frame(width: 26, height: 26)
                        .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Theme.ink.opacity(0.10)))
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13.5))
                    if let detail {
                        Text(detail).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 10)
                control
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .frame(minHeight: 48)
            if !last {
                Rectangle().fill(Theme.ink.opacity(0.06)).frame(height: 1)
                    .padding(.leading, symbol == nil ? 14 : 52)
            }
        }
    }
}

// MARK: - Levels

/// How loud something is, 0 to 1, as a row of segments that light in turn:
/// the "Hearing you" meter of the web's join screen (MicLevel.tsx).
struct LevelMeter: View {
    let level: Double
    var segments = 14

    var body: some View {
        HStack(spacing: 2.5) {
            ForEach(0..<segments, id: \.self) { index in
                let lit = Double(index) / Double(segments) < level
                Capsule()
                    .fill(lit ? Theme.tally : Theme.ink.opacity(0.12))
                    .frame(width: 4, height: 14)
            }
        }
        .animation(.easeOut(duration: 0.08), value: level)
        .accessibilityHidden(true)
    }
}
