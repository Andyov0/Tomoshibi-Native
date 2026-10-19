import os
import SwiftUI
#if os(iOS)
import UIKit
#endif

// Where a fold crosses a screen, and the system's furniture a screen shares
// its edges with, for the iPhone Duo (iOS 27.1).
//
// The Duo's inner display divides in two while it is held half open, as a
// book or standing on a table, and the system says where through a reserved
// region of the division kind (GeometryProxy `reservedRegions(kind:)`), active
// only while the hinge is part way. Everything the system draws keeps out of
// it by itself, its sheets, alerts, menus and split views; what the app lays
// out by hand does not, and the call's pictures, its bar and the cards in
// front of a call sat across the fold, a face bent down the middle of a book.
// Apple's guidance is the Android lobby's for a Fold (ui/Fold.kt): keep what is
// read and pressed to one side, move only what has to move, and nothing
// interactive in the curve; scrolling content may run across it.
//
// The hinge's own angle (`onHingeChange`) is not used for any of this. Apple
// offers it for effects and points layout at the regions, which are what
// moves when the fold does and are zero wide when the phone lies flat.

/// The fold across a view, in that view's own coordinates.
struct Fold: Equatable {
    /// The band the fold takes, its margins included; nil while the view is
    /// flat or the fold misses it.
    var crease: CGRect?

    static let flat = Fold(crease: nil)

    /// Down the view, the phone held as a book; across it, standing on a
    /// table as a laptop does.
    enum Posture: Equatable { case flat, book, table }

    /// The least a side has to be, along the axis the fold divides, to be
    /// laid out as a screen of its own; less than this (a window in Split
    /// View that only just reaches over the fold) and the view is taken as
    /// flat, since a strip of a pane is somewhere nothing fits (Fold.kt
    /// `LEAST_HALF`).
    static let leastSide: CGFloat = 200

    func posture(in size: CGSize) -> Posture {
        guard let crease else { return .flat }
        if crease.height >= crease.width {
            return crease.minX >= Self.leastSide && size.width - crease.maxX >= Self.leastSide ? .book : .flat
        }
        return crease.minY >= Self.leastSide && size.height - crease.maxY >= Self.leastSide ? .table : .flat
    }

    /// Which side of the fold a part of a screen keeps to.
    enum Side {
        /// What is read and typed into: the upper half on a table, where the
        /// keyboard does not come up over it, and the trailing page of a
        /// book, where the system moves its own alerts (Apple, "Strike a
        /// pose with adaptive layouts").
        case reading
        /// The upper half on a table, the leading page of a book.
        case first
        /// The lower half on a table, the trailing page of a book.
        case second
    }

    /// The part of a view `size` big that `side` is while the fold divides
    /// it; the whole view while it does not.
    func pane(_ side: Side, in size: CGSize, direction: LayoutDirection = .leftToRight) -> CGRect {
        let whole = CGRect(origin: .zero, size: size)
        guard let crease else { return whole }
        switch posture(in: size) {
        case .flat:
            return whole
        case .book:
            let left = CGRect(x: 0, y: 0, width: crease.minX, height: size.height)
            let right = CGRect(x: crease.maxX, y: 0, width: size.width - crease.maxX, height: size.height)
            let leading = direction == .leftToRight ? left : right
            let trailing = direction == .leftToRight ? right : left
            return side == .first ? leading : trailing
        case .table:
            let upper = CGRect(x: 0, y: 0, width: size.width, height: crease.minY)
            let lower = CGRect(x: 0, y: crease.maxY, width: size.width, height: size.height - crease.maxY)
            return side == .second ? lower : upper
        }
    }

    /// `pane`, and where the fold crosses the view too near one edge for
    /// both sides to be screens, the side that is not: the lobby's page
    /// beside its sidebar starts some 140 points before the fold of a book,
    /// too little to be laid out as a page of its own, and the greeting
    /// started there and ran over the fold. Whatever `side` asks for, that
    /// side is the one there is.
    func roomy(_ side: Side, in size: CGSize, direction: LayoutDirection = .leftToRight) -> CGRect {
        guard let crease, posture(in: size) == .flat else { return pane(side, in: size, direction: direction) }
        if crease.height >= crease.width {
            let left = CGRect(x: 0, y: 0, width: max(0, crease.minX), height: size.height)
            let right = CGRect(x: crease.maxX, y: 0, width: max(0, size.width - crease.maxX), height: size.height)
            return left.width >= right.width ? left : right
        }
        let upper = CGRect(x: 0, y: 0, width: size.width, height: max(0, crease.minY))
        let lower = CGRect(x: 0, y: crease.maxY, width: size.width, height: max(0, size.height - crease.maxY))
        return upper.height >= lower.height ? upper : lower
    }

    /// Whether the fold runs down the view, as a book's does.
    var down: Bool { crease.map { $0.height >= $0.width } ?? false }
}

extension Fold {
    /// The fold crossing the view `proxy` measures, from the system's
    /// division regions where it has them (iOS 27.1), or a rehearsal's.
    @MainActor
    static func of(_ proxy: GeometryProxy) -> Fold {
        if let rehearsed = FoldDebug.crease(in: proxy) { return Fold(crease: rehearsed) }
        #if os(iOS)
        if #available(iOS 27.1, *) {
            // The geometry is the view's own, mirrored for a right-to-left
            // language as the view's own layout is, so a side read from it
            // is the side the layout will put things.
            let active = proxy.reservedRegions(kind: .division).filter(\.isActive)
            let crossing = active.map(\.frame)
                .map { $0.intersection(CGRect(origin: .zero, size: proxy.size)) }
                .filter { !$0.isNull && ($0.width > 0 || $0.height > 0) }
            if let first = crossing.first {
                return Fold(crease: crossing.dropFirst().reduce(first) { $0.union($1) })
            }
        }
        #endif
        return .flat
    }
}

/// TOMOSHIBI_DEBUG_FOLD=book|table, honoured only in a debug build driven by a
/// test (TOMOSHIBI_BASE set), stands in a fold down or across the middle of
/// every screen, 24 points wide, for laying the screens out half open where
/// the simulator cannot be folded from a script: the Duo simulator folds from
/// Device Hub's buttons and SpringBoard refuses a pose from anything else on a
/// build that is not Apple's own. TOMOSHIBI_DEBUG_GEOMETRY=1 logs what the
/// system says about the room's edges under the app's `geometry` category.
enum FoldDebug {
    static let posture: String? = {
        #if DEBUG
        let all = ProcessInfo.processInfo.environment
        return all["TOMOSHIBI_BASE"] == nil ? nil : all["TOMOSHIBI_DEBUG_FOLD"]
        #else
        return nil
        #endif
    }()

    static let logsGeometry: Bool = {
        #if DEBUG
        let all = ProcessInfo.processInfo.environment
        return all["TOMOSHIBI_BASE"] != nil && all["TOMOSHIBI_DEBUG_GEOMETRY"] != nil
        #else
        return false
        #endif
    }()

    /// The window the stand-in fold is drawn down the middle of, as the
    /// app's root measures it: the fold is the screen's, wherever on the
    /// screen the view asking about it is.
    @MainActor static var window: CGSize = .zero

    @MainActor
    static func crease(in proxy: GeometryProxy) -> CGRect? {
        guard posture != nil, window.width > 0 else { return nil }
        let band: CGFloat = 24
        let screen: CGRect
        switch posture {
        case "book": screen = CGRect(x: window.width / 2 - band / 2, y: 0, width: band, height: window.height)
        case "table": screen = CGRect(x: 0, y: window.height / 2 - band / 2, width: window.width, height: band)
        default: return nil
        }
        let frame = proxy.frame(in: .global)
        let crossing = screen.intersection(frame)
        guard !crossing.isNull, crossing.width > 0, crossing.height > 0 else { return nil }
        return crossing.offsetBy(dx: -frame.minX, dy: -frame.minY)
    }

    @MainActor
    static func log(_ what: String, _ proxy: GeometryProxy) {
        guard logsGeometry else { return }
        let insets = proxy.safeAreaInsets
        var regions = ""
        #if os(iOS)
        if #available(iOS 27.1, *) {
            let occlusion = proxy.reservedRegions(kind: .occlusion, options: .includeInactive)
            let division = proxy.reservedRegions(kind: .division, options: .includeInactive)
            regions = "occlusion=\(occlusion.map { "\($0.frame) active=\($0.isActive) margins=\($0.margins)" }) "
                + "division=\(division.map { "\($0.frame) active=\($0.isActive)" })"
        }
        #endif
        Logger(subsystem: "app.tomoshibi", category: "geometry").notice(
            "GEOMETRY \(what, privacy: .public) size=\(proxy.size.debugDescription, privacy: .public) safe=(t \(insets.top), l \(insets.leading), b \(insets.bottom), r \(insets.trailing)) \(regions, privacy: .public)")
    }
}

extension View {
    /// Reports the edge the system puts its own bars on where it lays them
    /// down the side of the screen (`toolbarVerticalEdge`, iOS 27.1): the
    /// trailing edge of an iPhone Duo's outer display, under its status bar,
    /// and of the inner display held on its side; the leading edge of an app
    /// in the left half of Split View. Nil where the system keeps its bars
    /// across the top and the foot, which is everywhere before iOS 27.1.
    @ViewBuilder
    func readsVerticalBarEdge(_ report: @escaping (HorizontalEdge?) -> Void) -> some View {
        #if os(iOS)
        if #available(iOS 27.1, *) {
            modifier(VerticalBarEdgeReader(report: report))
        } else {
            self
        }
        #else
        self
        #endif
    }

    /// Keeps this view to one side of a fold across the space it is given,
    /// or to the larger side of one near its edge, moving there and back with
    /// the arrival curve as the phone is folded or opened out, so that a card
    /// is never laid across the curve. The whole space, and no change at all,
    /// while nothing divides it.
    func keptToOneSide(_ side: Fold.Side = .reading) -> some View {
        modifier(KeptToOneSide(side: side))
    }
}

#if os(iOS)
@available(iOS 27.1, *)
private struct VerticalBarEdgeReader: ViewModifier {
    @Environment(\.toolbarVerticalEdge) private var edge
    let report: (HorizontalEdge?) -> Void

    func body(content: Content) -> some View {
        content.onChange(of: edge, initial: true) { report(edge) }
    }
}
#endif

extension View {
    /// Keeps this view to one page of a book across the width it is given,
    /// or to the wider side of a fold down it near one edge, and leaves it
    /// where it is otherwise: for what sits at the foot or the head of a
    /// screen, a notice or a row of buttons, which a fold across the screen
    /// already leaves on one half and a fold down it would cut in two. Takes
    /// no more height than the view does.
    func keptToOnePage(_ side: Fold.Side = .reading) -> some View {
        modifier(KeptToOnePage(side: side))
    }
}

private struct KeptToOnePage: ViewModifier {
    let side: Fold.Side
    @State private var page: CGRect?
    @Environment(\.layoutDirection) private var direction

    func body(content: Content) -> some View {
        content
            .frame(width: page?.width)
            .offset(x: page?.minX ?? 0)
            .frame(maxWidth: .infinity, alignment: page == nil ? .center : .leading)
            .onGeometryChange(for: CGRect?.self) { [side, direction] proxy in
                MainActor.assumeIsolated {
                    let fold = Fold.of(proxy)
                    guard fold.down else { return nil }
                    return fold.roomy(side, in: proxy.size, direction: direction)
                }
            } action: { page = $0 }
            .animation(Theme.settle, value: page)
    }
}

private struct KeptToOneSide: ViewModifier {
    let side: Fold.Side
    @Environment(\.layoutDirection) private var direction

    func body(content: Content) -> some View {
        GeometryReader { proxy in
            let pane = Fold.of(proxy).roomy(side, in: proxy.size, direction: direction)
            content
                .frame(width: pane.width, height: pane.height)
                .offset(x: pane.minX, y: pane.minY)
                .animation(Theme.settle, value: pane)
        }
    }
}

extension View {
    /// A pane of glass over a call whose corners follow the screen's own
    /// where it sits near them (`ConcentricRectangle`, iOS 26 and macOS 26),
    /// never tighter than `minimum`; a plain rounded rectangle before those
    /// systems. A panel in the corner of an iPhone Duo's outer display sits
    /// under corners of eight points on one side and fifty-nine on the other,
    /// and one radius for all four looked pinched against the large ones and
    /// loose against the small. The shape is passed on as itself, not wrapped,
    /// so that the system can still resolve it against the screen.
    @ViewBuilder
    func concentricGlassShell(minimum: CGFloat) -> some View {
        if #available(macOS 26.0, iOS 26.0, *) {
            contentLayer()
                .glassSurface(ConcentricRectangle(corners: .concentric(minimum: .fixed(minimum)), isUniform: false))
                .controlLayer()
        } else {
            glassShell(cornerRadius: minimum)
        }
    }
}
