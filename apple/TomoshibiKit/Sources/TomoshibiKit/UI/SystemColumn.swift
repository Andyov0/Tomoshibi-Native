import os
import SwiftUI
import TomoshibiBasics

// The column the system keeps down one side of the screen for its status bar
// and its own bars, where it lays them down the side rather than across the
// top and the foot (`toolbarVerticalEdge`, iOS 27.1): on an iPhone Duo's outer
// display, 84 points down the trailing edge with the camera, the clock and
// the signal at its head and nothing below them. The system's own bars stand
// in it: the items of a navigation bar under the clock, a back button among
// them, and a bottom bar as a capsule at its foot. The app's screens drew
// their controls inside the safe area beside it instead, a quarter of a
// screen 466 points wide left empty below the clock while the buttons
// crowded the 382 points beside it. The owner asked for the bars to go into
// that column, "the space under the camera", on every screen (2026-10-26).
//
// The app draws its controls there itself, as the call's bar already does,
// rather than handing them to the system's bars. The system lays a toolbar
// down the column only for a navigation stack or a sheet holding one: a
// bottom bar on a screen without one was laid across the foot of the screen
// even with the column beside it (checked on the Duo simulator, iOS 27.1). A
// toolbar item is drawn apart from the window's content as well, where the
// app's own popovers cannot reach out of it (HomeStage), and the language
// menu and the bar's popovers are those. The one exception is the lobby's
// account button, which is already a toolbar item of a navigation stack and
// opens nothing beside itself: the system stands it under the clock as it
// stands the server page's back button there (PhoneLobby).

enum SystemColumn {
    /// The width of the capsule the screens' controls stand on in the
    /// column (`ColumnCapsule`), which the column has to give.
    static let leastWidth: CGFloat = 58

    #if os(iOS)
    /// The column the system lays its own bars down, outside the safe area of
    /// the view `proxy` measures on `edge` and below whatever occludes its
    /// head (the status bar and the camera of an iPhone Duo's outer display,
    /// which the system reports as occlusion regions), in that view's
    /// coordinates; nil where the system lays no bars down the side, or
    /// leaves less than `least` there (the app's languages are all read left
    /// to right, so the column's side is the side the edge names). The view
    /// has to be laid out inside the safe area: one that reaches past it has
    /// no inset on that side and finds no column, and keeps its controls
    /// where they were.
    ///
    /// Laid inside the safe area instead, the call's bar took the foot of a
    /// screen that the Duo's outer display has little of, and the column
    /// beside it, a quarter of the width, stood empty below the clock.
    @MainActor
    static func frame(_ proxy: GeometryProxy, edge: HorizontalEdge?, least: CGSize) -> CGRect? {
        guard let edge else { return nil }
        let insets = proxy.safeAreaInsets
        let width = edge == .trailing ? insets.trailing : insets.leading
        guard width >= least.width else { return nil }
        let x = edge == .trailing ? proxy.size.width : -width
        var top = insets.top
        if #available(iOS 27.1, *) {
            for region in proxy.reservedRegions(kind: .occlusion) where region.isActive {
                let frame = region.frame
                if frame.maxX > x, frame.minX < x + width, frame.midY < proxy.size.height / 2 {
                    top = max(top, frame.maxY)
                }
            }
        }
        let column = CGRect(x: x, y: top + 8, width: width, height: max(0, proxy.size.height - top - 8))
        return column.height >= least.height ? column : nil
    }
    #endif
}

extension View {
    /// Stands `head` under the system's clock and `foot` at the foot of the
    /// column the system keeps down the side of the screen, where it keeps
    /// one, and says through `shown` whether it did, for the screen to take
    /// its own copies of those controls out of the content: the same
    /// controls in one place or the other, never both. Where the system
    /// keeps no column nothing is drawn and the screen is as it was: every
    /// other iPhone, the Mac, and the Duo opened out, whose inner display's
    /// stand-in on the simulator reports none either way up.
    ///
    /// The controls slide into the column from its edge as the column comes,
    /// a phone being closed or turned, and out with it, while the screen's
    /// own copies arrive and leave with the kit's curves; nothing changes
    /// place between two frames. `least` is the height the controls need: a
    /// column that has never been that tall is not used, and one the keyboard
    /// has shortened since still is (`ColumnSides`). Apply it to a view laid
    /// out inside the safe area, under the screen's popover layer, so that a
    /// popover opened from the column is drawn by it.
    func systemColumn<Head: View, Foot: View>(_ shown: Binding<Bool>, least: CGFloat = 200,
                                              @ViewBuilder head: () -> Head,
                                              @ViewBuilder foot: () -> Foot) -> some View {
        #if os(iOS)
        modifier(SystemColumnBar(shown: shown, least: least, head: head(), foot: foot()))
        #else
        self
        #endif
    }
}

#if os(iOS)
/// The columns on either side of a view, as `SystemColumn.frame` finds them,
/// measured once for both so that the edge can be read apart from the
/// geometry, with the tallest each has been since it last changed in any
/// other way.
///
/// The keyboard shortens a column from below: it is part of the safe area
/// at its foot, and a view that keeps clear of it is that much shorter. A
/// column judged by its height at the time went below what the start sheet's
/// controls need as soon as its room field took the keyboard, and the close
/// and the microphone and camera left the column for the card while
/// somebody typed, and came back as the keyboard went. Judged by its tallest
/// it stays, and its foot stands above the keyboard. Measuring the column
/// through the keyboard instead (`ignoresSafeArea(.keyboard)` on the
/// measuring layer) changed nothing: the layer is the content's overlay, and
/// the content has already been made shorter.
private struct ColumnSides: Equatable {
    var leading: CGRect?
    var trailing: CGRect?
    var leadingReach: CGFloat = 0
    var trailingReach: CGFloat = 0

    func on(_ edge: HorizontalEdge?) -> CGRect? {
        switch edge {
        case .leading: leading
        case .trailing: trailing
        case nil: nil
        }
    }

    /// The tallest the column on `edge` has been.
    func reach(_ edge: HorizontalEdge?) -> CGFloat {
        switch edge {
        case .leading: leadingReach
        case .trailing: trailingReach
        case nil: 0
        }
    }

    /// These columns, measured again: each keeps the tallest it has been
    /// while it is the same column, its side, width and head where they
    /// were, and starts again from its own height otherwise.
    func remeasured(leading: CGRect?, trailing: CGRect?) -> ColumnSides {
        func reach(_ old: CGRect?, _ oldReach: CGFloat, _ new: CGRect?) -> CGFloat {
            guard let new else { return 0 }
            guard let old, abs(old.minX - new.minX) < 1, abs(old.width - new.width) < 1, abs(old.minY - new.minY) < 1 else {
                return new.height
            }
            return max(oldReach, new.height)
        }
        return ColumnSides(leading: leading, trailing: trailing,
                           leadingReach: reach(self.leading, leadingReach, leading),
                           trailingReach: reach(self.trailing, trailingReach, trailing))
    }
}

private struct SystemColumnBar<Head: View, Foot: View>: ViewModifier {
    @Binding var shown: Bool
    let least: CGFloat
    let head: Head
    let foot: Foot
    @State private var sides = ColumnSides()
    @State private var edge: HorizontalEdge?
    /// Whether the column has been looked for once: the first answer is how
    /// the screen arrives, and is given without moving anything.
    @State private var settled = false

    func body(content: Content) -> some View {
        let column = sides.reach(edge) >= least ? sides.on(edge) : nil
        content
            .overlay(alignment: .topLeading) {
                Color.clear
                    .onGeometryChange(for: ColumnSides.self) { proxy in
                        MainActor.assumeIsolated {
                            // Any height: the height asked for is judged
                            // against the tallest the column has been.
                            let wide = CGSize(width: SystemColumn.leastWidth, height: 0)
                            return ColumnSides(leading: SystemColumn.frame(proxy, edge: .leading, least: wide),
                                               trailing: SystemColumn.frame(proxy, edge: .trailing, least: wide))
                        }
                    } action: { now in
                        sides = sides.remeasured(leading: now.leading, trailing: now.trailing)
                        if FoldDebug.logsGeometry {
                            Logger(subsystem: "app.tomoshibi", category: "geometry").notice(
                                "GEOMETRY column leading=\(String(describing: now.leading), privacy: .public) trailing=\(String(describing: now.trailing), privacy: .public) reach=\(sides.trailingReach, privacy: .public)")
                        }
                    }
                    .overlay(alignment: .topLeading) {
                        ZStack(alignment: .topLeading) {
                            if let column {
                                VStack(spacing: 12) {
                                    head
                                    Spacer(minLength: 0)
                                    foot
                                }
                                // Above the column's foot by what the call's bar
                                // leaves there (RoomView `barPlace`).
                                .padding(.bottom, 8)
                                .frame(width: column.width, height: column.height, alignment: .top)
                                .offset(x: column.minX, y: column.minY)
                                .controlLayer()
                                .transition(.asymmetric(
                                    insertion: .move(edge: edge == .leading ? .leading : .trailing).combined(with: .opacity).animation(Theme.settle),
                                    removal: .move(edge: edge == .leading ? .leading : .trailing).combined(with: .opacity).animation(Theme.leave)))
                            }
                        }
                        .animation(Theme.settle, value: column)
                    }
            }
            .readsVerticalBarEdge { now in
                edge = now
                if FoldDebug.logsGeometry {
                    Logger(subsystem: "app.tomoshibi", category: "geometry").notice(
                        "GEOMETRY column edge \(String(describing: now), privacy: .public)")
                }
            }
            .onChange(of: column != nil, initial: true) { _, now in
                guard shown != now || !settled else { return }
                if settled {
                    withAnimation(Theme.settle) { shown = now }
                } else {
                    var arriving = Transaction()
                    arriving.disablesAnimations = true
                    withTransaction(arriving) { shown = now }
                }
                settled = true
            }
    }
}
#endif

/// The column's controls on one capsule of glass, as the call's bar stands
/// in the column: forty-four point plates (`BarButton`) rather than glass
/// buttons of their own, since glass on glass reads as smudge.
struct ColumnCapsule<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 6) { content }
            .padding(7)
            .glassBacking(Capsule())
            .fixedSize()
    }
}

/// The rule between groups on a column's capsule, as the call's bar has
/// before Leave.
struct ColumnRule: View {
    var body: some View {
        Rectangle().fill(Theme.border).frame(width: 26, height: 1).padding(.vertical, 2)
    }
}

/// A single control under the system's clock, a circle of glass the size of
/// the system's own there (a back button, a navigation bar's item).
struct ColumnHeadButton: View {
    let symbol: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 16, weight: .semibold))
        }
        .buttonStyle(.glassCircle(48))
        .accessibilityLabel(label)
        .help(label)
    }
}

/// The microphone and the camera as they will be on joining, down the
/// column: red while off, as `DeviceSwitches` draws them, since this is the
/// last moment to notice before everybody else does. The same labels, so
/// that whatever presses one presses the other.
struct ColumnDeviceSwitches: View {
    @Bindable var app: AppModel

    var body: some View {
        BarButton(symbol: app.startWithMicrophone ? "mic.fill" : "mic.slash.fill",
                  label: app.startWithMicrophone ? t("Mute microphone") : t("Unmute microphone"),
                  alarm: !app.startWithMicrophone) {
            app.startWithMicrophone.toggle()
        }
        BarButton(symbol: app.startWithCamera ? "video.fill" : "video.slash.fill",
                  label: app.startWithCamera ? t("Turn camera off") : t("Turn camera on"),
                  alarm: !app.startWithCamera) {
            app.startWithCamera.toggle()
        }
    }
}
