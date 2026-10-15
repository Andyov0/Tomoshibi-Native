import CoreGraphics

/// Where every picture sits, as coordinates rather than as a tree.
///
/// Follows web/src/live/plan.ts and layout.ts line for line. The web learned
/// this the hard way: an arrangement expressed by which container a tile is in
/// destroys and rebuilds the tile whenever the arrangement changes, and a
/// picture that is rebuilt resubscribes and waits for a keyframe — the black
/// screen. A picture placed by coordinates is built once for as long as its
/// owner is in the call, and moving it to the stage is an animation rather
/// than an interruption. The same holds for SwiftUI views keyed by identity.
public enum Plan {
    /// Space between pictures, and between a picture and the edge.
    public static let gap: CGFloat = 8
    /// Nine keeps a grid readable on a laptop and caps how many streams the
    /// call asks for at once.
    public static let tilesPerPage = 9
    /// The controls' reserve at the foot when nothing has been measured.
    public static let island: CGFloat = 20 + 58 + gap
    /// Two people are one person being looked at and one being checked on.
    public static let aloneTogether = 2
    static let strip: CGFloat = 112
    static let headroom: CGFloat = 56
    static let landscape: CGFloat = 16 / 9
    static let uprightAspect: CGFloat = 3 / 4
    static let fill: CGFloat = 0.92

    /// A screen held upright, rather than a window that happens to be tall: a
    /// fifth taller than wide, so a window dragged through square does not
    /// reshape the call under the pointer.
    public static func upright(_ container: CGSize) -> Bool {
        container.height > container.width * 1.2
    }

    static func aspect(_ container: CGSize) -> CGFloat {
        upright(container) ? uprightAspect : landscape
    }

    struct Arrangement: Equatable {
        var columns: Int
        var rows: Int
        var width: CGFloat
        var height: CGFloat
    }

    /// The column count that gives each person the most picture (layout.ts).
    static func arrange(_ container: CGSize, count: Int, gap: CGFloat = gap, aspect: CGFloat = landscape) -> Arrangement? {
        let (width, height) = (container.width, container.height)
        guard count > 0, width > 0, height > 0 else { return nil }
        var best: Arrangement?
        var bestScore: CGFloat = 0
        for columns in 1...count {
            let rows = Int((Double(count) / Double(columns)).rounded(.up))
            let cellWidth = (width - CGFloat(columns - 1) * gap) / CGFloat(columns)
            let cellHeight = (height - CGFloat(rows - 1) * gap) / CGFloat(rows)
            guard cellWidth > 0, cellHeight > 0 else { continue }
            var tileWidth = min(cellWidth, cellHeight * aspect)
            var tileHeight = tileWidth / aspect
            let ceilingFill: CGFloat = container.height > container.width * 1.2 ? 1 : fill
            let ceiling = (height * ceilingFill - CGFloat(rows - 1) * gap) / CGFloat(rows)
            if tileHeight > ceiling {
                tileHeight = ceiling
                tileWidth = tileHeight * aspect
            }
            let score = tileWidth * tileHeight * (CGFloat(count) / CGFloat(columns * rows))
            if score > bestScore {
                bestScore = score
                best = Arrangement(columns: columns, rows: rows, width: tileWidth.rounded(.down), height: tileHeight.rounded(.down))
            }
        }
        return best
    }

    /// A grid, centred, each row centred on its own.
    public static func grid(_ container: CGSize, ids: [String], island: CGFloat = island) -> [String: CGRect] {
        var plan: [String: CGRect] = [:]
        guard let layout = arrange(CGSize(width: container.width, height: max(0, container.height - island)),
                                   count: ids.count, aspect: aspect(container)) else { return plan }
        let gridHeight = CGFloat(layout.rows) * layout.height + CGFloat(layout.rows - 1) * gap
        let top = (container.height - gridHeight) / 2
        for (index, id) in ids.enumerated() {
            let row = index / layout.columns
            let column = index % layout.columns
            let inRow = row == layout.rows - 1 ? ids.count - row * layout.columns : layout.columns
            let rowWidth = CGFloat(inRow) * layout.width + CGFloat(inRow - 1) * gap
            plan[id] = CGRect(x: (container.width - rowWidth) / 2 + CGFloat(column) * (layout.width + gap),
                              y: top + CGFloat(row) * (layout.height + gap),
                              width: layout.width, height: layout.height)
        }
        return plan
    }

    /// One picture on the stage, the rest in a filmstrip beneath.
    public static func focus(_ container: CGSize, stage: String, strip ids: [String], fullscreen: Bool = false, island: CGFloat = island) -> [String: CGRect] {
        var plan: [String: CGRect] = [:]
        guard container.width > 0, container.height > 0 else { return plan }
        if fullscreen {
            plan[stage] = CGRect(origin: .zero, size: container)
            return plan
        }
        let bottom = container.height - island
        let showStrip = !ids.isEmpty
        let stripTop = bottom - strip
        plan[stage] = CGRect(x: gap, y: gap, width: max(0, container.width - 2 * gap),
                             height: max(0, (showStrip ? stripTop - gap : bottom) - gap))
        guard showStrip else { return plan }
        let width = strip * aspect(container)
        let total = CGFloat(ids.count) * width + CGFloat(ids.count - 1) * gap
        let left = (container.width - total) / 2
        for (index, id) in ids.enumerated() {
            plan[id] = CGRect(x: left + CGFloat(index) * (width + gap), y: stripTop, width: width, height: strip)
        }
        return plan
    }

    /// How many the filmstrip holds at this width; never fewer than one.
    public static func stripCapacity(_ container: CGSize) -> Int {
        let width = strip * aspect(container)
        let usable = container.width - 2 * gap + gap
        return max(1, Int((usable / (width + gap)).rounded(.down)))
    }

    /// The other person filling the screen and the rest as cards down the
    /// trailing edge, the way every telephone draws a call of two.
    public static func close(_ container: CGSize, filling: String, cards: [String], island: CGFloat = island) -> [String: CGRect] {
        var plan: [String: CGRect] = [:]
        guard container.width > 0, container.height > 0 else { return plan }
        plan[filling] = CGRect(origin: .zero, size: container)
        let width = min(container.width * (upright(container) ? 0.26 : 0.18), (container.height - island - 2 * gap) / 2)
        let height = width / aspect(container)
        for (index, id) in cards.enumerated() {
            let top = headroom + CGFloat(index) * (height + gap)
            if top + height > container.height - island { continue }
            plan[id] = CGRect(x: container.width - width - gap, y: top, width: width, height: height)
        }
        return plan
    }

    /// How far above the bottom something drawn over the pictures must end to
    /// leave these ones uncovered.
    public static func clear(of plan: [String: CGRect], ids: [String], container: CGSize) -> CGFloat? {
        let tops = ids.compactMap { plan[$0]?.minY }
        guard let highest = tops.min() else { return nil }
        return container.height - highest + gap
    }
}
