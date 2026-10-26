import CoreGraphics
import SwiftUI

/// Where the system lays its bars down one side of the screen rather than
/// across its top and its foot (iOS 27.1, the iPhone Duo), and the column the
/// screens stand their controls in there: worked out from what the system
/// says, apart from the views, so that it can be tested.
///
/// The Duo's outer display keeps a column 84 points wide down its trailing
/// edge, outside the safe area, with the camera, the clock and the signal at
/// its head, and the screens stand their controls under them (SystemColumn in
/// the kit). Apple has the inner display held on its side lay the bars down
/// the same edge, "their position remains consistent when you open iPhone
/// Duo to the inner display in landscape", and upright go back to an
/// ordinary iPhone's bars across the top and the foot. On the inner display
/// there is no column outside the safe area to stand them in: the controls
/// stayed where an iPad has them, the microphone and the camera at the foot
/// of the stage and the server in its corner, and the owner asked why the
/// inner display did not use the right side as the outer one does
/// (2026-10-26). Where the system lays its bars down the side but leaves no
/// column there, the screens keep one of their own, as wide as the outer
/// display's, and lay their content out beside it.
public enum VerticalBar {
    /// The column a screen keeps for itself where the system keeps none: the
    /// width of the outer display's, so that the controls stand the same
    /// distance from the edge whichever way the phone is opened.
    public static let width: CGFloat = 84
    /// The least a column has to give: the capsule the screens' controls
    /// stand on (the kit's ColumnCapsule), forty-four point plates in seven
    /// points of glass either side.
    public static let leastWidth: CGFloat = 58
    /// Where the head of the system's column starts at the least, when the
    /// system says nothing covers it (`column`): below the clock and the
    /// signal the inner display on its side stands there.
    public static let systemHeadroom: CGFloat = 100

    /// The edge the system lays its bars down, for a window `window` big.
    ///
    /// `reported` is the system's word for it (SwiftUI's
    /// `toolbarVerticalEdge`, iOS 27.1), and it stands wherever it is given.
    /// Where it is not, a window that holds a division region (the fold of
    /// the Duo's inner display, which the system reports even while the
    /// phone lies flat when inactive regions are asked for) and is wider
    /// than tall is taken as the inner display on its side, whose bars stand
    /// down the edge the hardware keeps for them: the right, which in a
    /// language read from right to left is the leading edge, since the
    /// column follows the hardware rather than the reading. That is for a
    /// phone on which the system's word does not reach the view asking; it
    /// was not seen on the simulator, whose stand-in for the inner display
    /// reports neither. `rehearsed` is a test's stand-in for the system's
    /// word, taken only where the system's would be given, on a window wider
    /// than tall.
    public static func edge(reported: HorizontalEdge?, rehearsed: HorizontalEdge?, divided: Bool,
                            window: CGSize, rightToLeft: Bool) -> HorizontalEdge? {
        if let reported { return reported }
        guard window.width > window.height else { return nil }
        if let rehearsed { return rehearsed }
        return divided ? (rightToLeft ? .leading : .trailing) : nil
    }

    /// Whether the screens lay themselves out as the outer display's are, a
    /// column down the side and no sidebar: a window `window` big at a
    /// `regular` width, wider than tall, whose system lays its bars down
    /// `edge`. That is the inner display on its side.
    ///
    /// The inner display on its side was given the lobby an iPad has, a
    /// sidebar holding Start, Join, the server and the account, with the
    /// column beside its stage holding only the microphone and the camera;
    /// the owner found it did not look right beside the outer display, whose
    /// column holds everything, and chose the outer display's screens for it
    /// (2026-10-26). A compact width is the outer display whichever way it is
    /// held, which has those screens already, and taller than wide is the
    /// inner display upright, whose bars are an ordinary iPhone's.
    public static func sideways(edge: HorizontalEdge?, regular: Bool, window: CGSize) -> Bool {
        regular && edge != nil && window.width > window.height
    }

    /// Whether a view whose frame in its window is `frame`, `inset` short of
    /// the window's side on `edge` by the safe area, reaches that side: a
    /// screen does, and so does the page beside a sidebar. The sidebar does
    /// not, and Apple keeps its bars across it (only a split view's detail
    /// has its bars down the side); nor does a sheet in the middle of the
    /// inner display, which keeps its controls on itself and stops some 150
    /// points short of either side.
    ///
    /// A screen on its way in is drawn at 97 percent of its size
    /// (Theme.arrive), and its frame in the window, which carries the
    /// scaling, stops some 14 points short of the side of an inner display:
    /// judged to the point, the call arrived with its bar across the foot
    /// and moved it into the column a third of a second later. Short by up
    /// to a twenty-fifth of the window still reaches.
    public static func reaches(_ edge: HorizontalEdge, frame: CGRect, inset: CGFloat, window: CGSize,
                               rightToLeft: Bool) -> Bool {
        let right = (edge == .trailing) != rightToLeft
        let slack = max(1, window.width / 25)
        return right ? frame.maxX + inset >= window.width - slack : frame.minX - inset <= slack
    }

    /// How much of a view a screen keeps for a column of its own on `edge`:
    /// nothing where the system lays no bars down the side, where it keeps a
    /// column of its own there already (`inset` is what the safe area leaves
    /// on that side, 84 points on the outer display), where the view does
    /// not reach the side, or on a window taller than wide, where the bars
    /// go back across the top and the foot; otherwise what a column as wide
    /// as the outer display's needs beyond the inset.
    ///
    /// Taller than wide is asked of the window whatever the system says:
    /// the outer display stands upright with its column, and a passing
    /// moment in which it reported the edge before the column (a scene
    /// moving between displays) is not a reason to make it narrower.
    public static func kept(edge: HorizontalEdge?, inset: CGFloat, reaches: Bool, window: CGSize) -> CGFloat {
        guard edge != nil, inset < leastWidth, reaches, window.width > window.height else { return 0 }
        return width - max(0, inset)
    }

    /// The column, in the coordinates of a view `height` tall: `x` and
    /// `width` across, from `top` down to its foot, clear of what occludes
    /// it and of a fold across it.
    ///
    /// An occlusion in the upper half of the view pushes the column's head
    /// below it (the outer display's camera, clock and signal, the inner
    /// display's camera while it is on), and one in the lower half lifts its
    /// foot above it. A fold across the column leaves it the larger of the
    /// two parts, so that nothing in it is pressed in the curve; the column
    /// of a phone on its side runs beside the fold of a book rather than
    /// across it, and this is for whatever else a fold may do. Nil where
    /// what is left is not `least` tall. The column starts eight points
    /// below its head, as the call's bar does.
    ///
    /// `headroom` is how far down the head starts at the least where nothing
    /// is said to occlude it: the system's own column on the inner display
    /// on its side has its clock and signal at the head, some hundred points
    /// of it (the owner's photograph of a phone, 2026-10-26), and a column
    /// whose head the system did not report covered would have stood the
    /// first control over the clock. Nothing where something is reported
    /// there, which then says where the head is.
    public static func column(x: CGFloat, width: CGFloat, height: CGFloat, top: CGFloat,
                              occlusions: [CGRect], creases: [CGRect], least: CGFloat,
                              headroom: CGFloat = 0) -> CGRect? {
        var head = top
        var foot = height
        var covered = false
        for region in occlusions where region.maxX > x && region.minX < x + width {
            if region.midY < height / 2 {
                head = max(head, region.maxY)
                covered = true
            } else {
                foot = min(foot, region.minY)
            }
        }
        if !covered { head = max(head, headroom) }
        var column = CGRect(x: x, y: head + 8, width: width, height: max(0, foot - head - 8))
        for crease in creases where crease.intersects(column) {
            if crease.width >= crease.height {
                let above = CGRect(x: column.minX, y: column.minY, width: column.width,
                                   height: max(0, crease.minY - column.minY))
                let below = CGRect(x: column.minX, y: crease.maxY, width: column.width,
                                   height: max(0, column.maxY - crease.maxY))
                column = above.height >= below.height ? above : below
            } else {
                let left = CGRect(x: column.minX, y: column.minY, width: max(0, crease.minX - column.minX),
                                  height: column.height)
                let right = CGRect(x: crease.maxX, y: column.minY, width: max(0, column.maxX - crease.maxX),
                                   height: column.height)
                column = left.width >= right.width ? left : right
            }
        }
        return column.height >= least ? column : nil
    }
}
