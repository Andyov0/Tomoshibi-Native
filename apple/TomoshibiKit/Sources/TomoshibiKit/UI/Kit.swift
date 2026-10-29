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

/// A pane anchored to a control, above or below it: the app's own popover.
/// It closes on Escape, when its id is toggled again, and when the screen's
/// `PopoverCatcher` is clicked. Opaque, as every menu is (`menuSurface`).
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
                    .menuSurface()
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

// MARK: - Surfaces that are content

extension View {
    /// What a menu, a popover and a list of choices are drawn on: an opaque
    /// surface, raised off what it hangs over, with a hairline edge and a
    /// shadow, and content inside.
    ///
    /// They were glass. Glass is for the controls that float over the
    /// content (Glass.swift), and a menu is something read: over a picture,
    /// the picture ran through its words, and the owner's rule for the
    /// clients is glass on the floating controls alone (2026-10-29, the
    /// web's "menus, dialogs and panels are opaque"). Fourteen at the corner,
    /// so a menu's rows of ten inside four of padding are concentric with it.
    func menuSurface(cornerRadius: CGFloat = Theme.Radius.medium) -> some View {
        modifier(OpaqueSurface(shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous), fill: Theme.floating))
    }

    /// A panel beside or over a call (the messages, the sound, the devices):
    /// opaque, for the reason `menuSurface` gives, in the surface's grey
    /// rather than a menu's raised one, at the panel's corner.
    func panelSurface(cornerRadius: CGFloat = Theme.Radius.panel) -> some View {
        modifier(OpaqueSurface(shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous), fill: Theme.surface))
    }
}

/// `menuSurface` and `panelSurface`: content inside, the fill and an edge
/// behind it, and a shadow that lifts it off a picture by night and off the
/// page by day.
private struct OpaqueSurface<S: Shape>: ViewModifier {
    let shape: S
    let fill: Color
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        content
            .contentLayer()
            .lifted()
            .background(shape.fill(fill))
            .overlay(shape.stroke(Theme.ink.opacity(0.10), lineWidth: 1))
            .clipShape(shape)
            .shadow(color: Theme.shadow(scheme == .dark ? 0.45 : 0.3), radius: 16, y: 6)
    }
}

/// How a control is pressed, one curve and one shrink everywhere (Theme.Press):
/// the press lands before whatever it starts has had time to answer. Not for
/// somebody who asked for less motion, for whom the colour the press also
/// changes says it.
struct Pressable: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        PressableBody(configuration: configuration)
    }

    private struct PressableBody: View {
        let configuration: Configuration
        @Environment(\.accessibilityReduceMotion) private var still

        var body: some View {
            configuration.label
                .scaleEffect(configuration.isPressed && !still ? Theme.Press.scale : 1)
                .animation(Theme.Press.animation, value: configuration.isPressed)
        }
    }
}

/// A row of a menu or a list pressed: lit a tenth towards the foreground
/// under the pointer and sixteen hundredths while a finger is on it, which
/// on a phone is the only answer a row gives (an iPhone's menu rows had
/// none: they lit under a pointer a finger does not have). Rows do not
/// shrink, as the system's own do not.
struct RowPress: ButtonStyle {
    var radius: CGFloat = Theme.Radius.small

    func makeBody(configuration: Configuration) -> some View {
        RowPressBody(configuration: configuration, radius: radius)
    }

    private struct RowPressBody: View {
        let configuration: Configuration
        let radius: CGFloat
        @State private var hovering = false
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            let lit = !enabled ? 0 : configuration.isPressed ? Theme.Press.quietHeld : hovering ? Theme.Press.quietHover : 0
            configuration.label
                .background(RoundedRectangle(cornerRadius: radius, style: .continuous).fill(Theme.ink.opacity(lit)))
                .onHover { hovering = $0 }
                .animation(Theme.Press.animation, value: configuration.isPressed)
                .animation(.easeOut(duration: 0.12), value: hovering)
        }
    }
}

/// A menu's row: a pointer's 32 on the Mac, a finger's 44 on a touch screen
/// (the web's 32 and 44, Android's 48 by its own rule).
var menuRowHeight: CGFloat {
    #if os(iOS)
    Theme.fingertip
    #else
    32
    #endif
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
/// Four in from a menu's corner of fourteen, so that its rows' ten are
/// concentric with it (Theme.Radius).
struct MenuCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) { content }
            .padding(4)
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
                // The spinner and the check come and go, as everything does;
                // the spinner popped in and out with the work it stood for.
                ZStack {
                    if busy {
                        ProgressView().controlSize(.small)
                            .transition(.opacity.combined(with: .scale(scale: 0.6)))
                    } else if checked {
                        Image(systemName: "checkmark")
                            .font(.system(size: 11.5, weight: .bold))
                            .foregroundStyle(Theme.tallyInk)
                            .transition(.scale.combined(with: .opacity))
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            // A row that explains itself (Leave's two) at least a fingertip
            // high everywhere.
            .frame(minHeight: detail == nil ? menuRowHeight : Theme.fingertip)
            .contentShape(RoundedRectangle(cornerRadius: Theme.Radius.small, style: .continuous))
        }
        .buttonStyle(RowPress())
        .opacity(enabled ? 1 : Theme.Press.disabled)
        .animation(Theme.settle, value: checked)
        .animation(Theme.settle, value: busy)
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
        .frame(minHeight: menuRowHeight)
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
