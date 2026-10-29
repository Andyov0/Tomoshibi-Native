import os
import SwiftUI
import TomoshibiBasics
#if os(iOS)
import UIKit
#endif

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
//
// The inner display held on its side lays the system's bars down the same
// edge, and keeps no column outside the safe area for them; there a screen
// keeps a column of its own, as wide as the outer display's, and lays its
// content out beside it (VerticalBar, in TomoshibiBasics). The screens there
// are the outer display's, the lobby without its sidebar and the settings and
// the schedule over the whole screen, and the lobby draws its account in the
// column's head itself rather than leaving it to a navigation bar the system
// may not stand there (PhoneLobby `sideways`). Upright, the inner display has
// its bars across the top and the foot, and the screens are as they are on
// any iPhone.

enum SystemColumn {
    /// The width of the capsule the screens' controls stand on in the
    /// column (`ColumnCapsule`), which the column has to give.
    static let leastWidth: CGFloat = VerticalBar.leastWidth

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
    ///
    /// `kept` is a column of the app's own on the inner display (VerticalBar
    /// `kept`): the last points of the view on that edge, with whatever the
    /// safe area leaves beyond them. `own` says the column is the app's,
    /// kept inside the view or added to its safe area by the screen
    /// (`systemColumn`): its head starts at the view's own top, which is
    /// already below the status bar across the top of the inner display,
    /// where the outer display's starts at the top of its safe area.
    @MainActor
    static func frame(_ proxy: GeometryProxy, edge: HorizontalEdge?, least: CGSize,
                      kept: CGFloat = 0, own: Bool = false) -> CGRect? {
        guard let edge else { return nil }
        let insets = proxy.safeAreaInsets
        let inset = edge == .trailing ? insets.trailing : insets.leading
        let width = inset + kept
        guard width >= least.width else { return nil }
        let x = edge == .trailing ? proxy.size.width - kept : -inset
        var occlusions: [CGRect] = []
        var creases: [CGRect] = []
        if #available(iOS 27.1, *) {
            occlusions = proxy.reservedRegions(kind: .occlusion).filter(\.isActive).map(\.frame)
            creases = proxy.reservedRegions(kind: .division).filter(\.isActive).map(\.frame)
        }
        let ours = own || kept > 0
        return VerticalBar.column(x: x, width: width, height: proxy.size.height, top: ours ? 0 : insets.top,
                                  occlusions: occlusions, creases: creases, least: least.height,
                                  headroom: ours ? 0 : VerticalBar.systemHeadroom)
    }

    /// Whether the window `proxy` measures holds a division region, active
    /// or not: the inner display of an iPhone Duo, whose fold the system
    /// reports while the phone lies flat as well when asked for the
    /// inactive ones (`ReservedRegion.QueryOptions.includeInactive`, iOS
    /// 27.1). The outer display reports none (iPhone Duo simulator).
    static func divided(_ proxy: GeometryProxy) -> Bool {
        if #available(iOS 27.1, *) {
            return !proxy.reservedRegions(kind: .division, options: .includeInactive).isEmpty
        }
        return false
    }

    /// The size of the app's window, keyboard or not. The root's own measure
    /// is made shorter by the keyboard (an iPad on its side measured 1133 by
    /// 284 with it up), and the inner display upright, 669 by 951, would
    /// measure wider than tall under a tall enough keyboard and stand its
    /// column for as long as somebody typed; the window's bounds are the
    /// window's whatever is over it. `measured` where there is no window to
    /// ask.
    @MainActor
    static func windowSize(measured: CGSize) -> CGSize {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        guard let window = windows.first(where: \.isKeyWindow) ?? windows.first else { return measured }
        return window.bounds.size
    }
    #endif
}

#if os(iOS)
/// What the app's window says about the system's vertical bar, read once at
/// the root (AppRoot) for every screen to ask: its size, the edge the system
/// reports there, and whether a fold divides it. The screens ask the system
/// for the edge themselves as well (`readsVerticalBarEdge`), and its word
/// where it reaches them comes first; this is what stands where it does not,
/// a sheet among them, and what says whether the window is on its side.
@MainActor
@Observable
final class VerticalBarWindow {
    static let shared = VerticalBarWindow()

    var size: CGSize = .zero
    var reported: HorizontalEdge?
    var divided = false

    /// The edge the system lays its bars down in this window, by its own
    /// word, a test's stand-in for it, or the fold of the inner display on
    /// its side (VerticalBar `edge`).
    func edge(rightToLeft: Bool) -> HorizontalEdge? {
        VerticalBar.edge(reported: reported, rehearsed: FoldDebug.verticalEdge, divided: divided,
                         window: size, rightToLeft: rightToLeft)
    }

    /// Whether the screens are laid out as the outer display's, with no
    /// sidebar (VerticalBar `sideways`), at a `regular` width or not.
    ///
    /// The window's shape is asked of the window as it is now rather than
    /// taken from `size`, which the root measures only after the frame it
    /// changed in. In that frame the size class of a phone being opened is
    /// already regular, and judged by the outer display's size, upright, the
    /// lobby would start fading its sidebar in only to fade it out again.
    func sideways(regular: Bool, rightToLeft: Bool) -> Bool {
        let now = SystemColumn.windowSize(measured: size)
        let edge = VerticalBar.edge(reported: reported, rehearsed: FoldDebug.verticalEdge, divided: divided,
                                    window: now, rightToLeft: rightToLeft)
        return VerticalBar.sideways(edge: edge, regular: regular, window: now)
    }
}

/// The window as the app's root measures it (AppRoot).
struct WindowShape: Equatable {
    var size: CGSize
    var divided: Bool
}

/// A screen's frame in the window and the safe area beyond it on either
/// side, measured outside the column the screen keeps for itself: inside
/// it, the column is part of the safe area, and a screen that judged its
/// column by the safe area it had made would find the system had made one
/// and give its own up.
struct ColumnOutside: Equatable {
    var frame: CGRect = .zero
    var leading: CGFloat = 0
    var trailing: CGFloat = 0

    init() {}

    init(_ proxy: GeometryProxy) {
        frame = proxy.frame(in: .global)
        leading = proxy.safeAreaInsets.leading
        trailing = proxy.safeAreaInsets.trailing
    }

    func inset(_ edge: HorizontalEdge?) -> CGFloat { edge == .leading ? leading : trailing }

    /// The width the screen keeps for a column of its own on `edge` in a
    /// window `window` big (VerticalBar `kept`).
    func kept(_ edge: HorizontalEdge?, window: CGSize, rightToLeft: Bool) -> CGFloat {
        guard let edge, frame.width > 0 else { return 0 }
        let reaches = VerticalBar.reaches(edge, frame: frame, inset: inset(edge), window: window, rightToLeft: rightToLeft)
        return VerticalBar.kept(edge: edge, inset: inset(edge), reaches: reaches, window: window)
    }
}
#endif

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
    ///
    /// `kept`, where it is given, is told the column the screen keeps for
    /// itself instead of the column being added to this view's safe area,
    /// for a screen whose content does not take its safe area from this
    /// view: a navigation stack lays its pages out by the safe area of the
    /// window, and the phone's lobby on the inner display on its side ran
    /// its lists under the column with the column added to the stack's.
    func systemColumn<Head: View, Foot: View>(_ shown: Binding<Bool>, least: CGFloat = 200,
                                              kept: Binding<KeptColumn>? = nil,
                                              @ViewBuilder head: () -> Head,
                                              @ViewBuilder foot: () -> Foot) -> some View {
        #if os(iOS)
        modifier(SystemColumnBar(shown: shown, least: least, keptOut: kept, head: head(), foot: foot()))
        #else
        self
        #endif
    }

    /// Lays this view out beside the column a screen keeps for itself
    /// (`systemColumn`'s `kept`): that much more safe area on its side.
    func besideKeptColumn(_ kept: KeptColumn) -> some View {
        safeAreaPadding(kept.edge == .leading ? .leading : .trailing, kept.width)
    }
}

/// The column a screen keeps for itself, where the system keeps none: the
/// edge it is on and how much of the screen it takes; nothing elsewhere.
struct KeptColumn: Equatable {
    var edge: HorizontalEdge = .trailing
    var width: CGFloat = 0
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
    /// Told the column kept rather than adding it to the safe area here.
    let keptOut: Binding<KeptColumn>?
    let head: Head
    let foot: Foot
    @State private var sides = ColumnSides()
    /// The edge the system reports to this view (`toolbarVerticalEdge`).
    @State private var edge: HorizontalEdge?
    /// Whether the column has been looked for once: the first answer is how
    /// the screen arrives, and is given without moving anything.
    @State private var settled = false
    /// The screen's frame and the safe area beyond it, outside the column it
    /// keeps for itself; nothing until it has been measured once.
    @State private var outside = ColumnOutside()
    @State private var measured = false
    /// The column the screen keeps for itself was first found as it
    /// arrived: the controls are put there as the screen arrives, as they
    /// are into the system's column, rather than sliding over from the
    /// content a moment after it.
    @State private var keptOnArrival = false
    @Environment(\.layoutDirection) private var direction

    func body(content: Content) -> some View {
        let rightToLeft = direction == .rightToLeft
        let window = VerticalBarWindow.shared
        // The system's word to this view first, then the window's (a sheet
        // the word does not reach, the fold of the inner display on its
        // side, a test's stand-in).
        let barEdge = edge ?? window.edge(rightToLeft: rightToLeft)
        let kept = outside.kept(barEdge, window: window.size, rightToLeft: rightToLeft)
        let keptEdge: HorizontalEdge? = kept > 0 ? barEdge : nil
        let inside = keptOut == nil ? 0 : kept
        let column = sides.reach(barEdge) >= least ? sides.on(barEdge) : nil
        content
            .overlay(alignment: .topLeading) {
                Color.clear
                    .onGeometryChange(for: ColumnSides.self) { [keptEdge, inside] proxy in
                        // Any height: the height asked for is judged
                        // against the tallest the column has been.
                        let wide = CGSize(width: SystemColumn.leastWidth, height: 0)
                        // The column kept, inside this view where it is
                        // not part of its safe area (`keptOut`).
                        return ColumnSides(leading: SystemColumn.frame(proxy, edge: .leading, least: wide,
                                                                       kept: keptEdge == .leading ? inside : 0,
                                                                       own: keptEdge == .leading),
                                           trailing: SystemColumn.frame(proxy, edge: .trailing, least: wide,
                                                                        kept: keptEdge == .trailing ? inside : 0,
                                                                        own: keptEdge == .trailing))
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
                                    insertion: .move(edge: barEdge == .leading ? .leading : .trailing).combined(with: .opacity).animation(Theme.settle),
                                    removal: .move(edge: barEdge == .leading ? .leading : .trailing).combined(with: .opacity).animation(Theme.leave)))
                            }
                        }
                        .animation(Theme.settle, value: column)
                    }
            }
            // The column the screen keeps for itself where the system keeps
            // none (the inner display on its side): part of the content's
            // safe area, so that the content is laid out beside it, as it is
            // beside the outer display's, and whatever reaches under the
            // safe area (a backdrop, the picture of yourself) reaches under
            // this one too. Nothing anywhere else.
            .safeAreaPadding(keptEdge == .leading ? .leading : .trailing, keptOut == nil ? kept : 0)
            .animation(Theme.settle, value: kept)
            .onChange(of: KeptColumn(edge: keptEdge ?? .trailing, width: kept), initial: true) { _, now in
                guard let keptOut, keptOut.wrappedValue != now else { return }
                // A column kept as the screen arrives is how it arrives;
                // one kept or given up afterwards comes and goes with the
                // column, as the padding here would.
                if keptOnArrival || !measured {
                    var arriving = Transaction()
                    arriving.disablesAnimations = true
                    withTransaction(arriving) { keptOut.wrappedValue = now }
                } else {
                    withAnimation(Theme.settle) { keptOut.wrappedValue = now }
                }
            }
            .background {
                Color.clear
                    .onGeometryChange(for: ColumnOutside.self) { proxy in
                        ColumnOutside(proxy)
                    } action: { now in
                        if measured {
                            outside = now
                        } else {
                            // The first measure, as the screen arrives: a
                            // column kept now is how the screen arrives.
                            var arriving = Transaction()
                            arriving.disablesAnimations = true
                            withTransaction(arriving) {
                                outside = now
                                measured = true
                                keptOnArrival = now.kept(barEdge, window: window.size, rightToLeft: rightToLeft) > 0
                            }
                        }
                        if FoldDebug.logsGeometry {
                            Logger(subsystem: "app.tomoshibi", category: "geometry").notice(
                                "GEOMETRY column outside frame=\(now.frame.debugDescription, privacy: .public) leading=\(now.leading, privacy: .public) trailing=\(now.trailing, privacy: .public) edge=\(String(describing: barEdge), privacy: .public) window=\(window.size.debugDescription, privacy: .public) kept=\(now.kept(barEdge, window: window.size, rightToLeft: rightToLeft), privacy: .public)")
                        }
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
                guard shown != now || !settled || keptOnArrival else { return }
                if settled && !keptOnArrival {
                    withAnimation(Theme.settle) { shown = now }
                } else {
                    var arriving = Transaction()
                    arriving.disablesAnimations = true
                    withTransaction(arriving) { shown = now }
                }
                if now { keptOnArrival = false }
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
