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
    ///
    /// Two departures from plan.ts, both about the edges. The grid keeps
    /// `sides` clear at the left and right, the gap by default, which the
    /// stage and the strip always kept and the grid did not: six people in a
    /// wide window were drawn edge to edge, against the window's frame. And
    /// it is centred in the window but never lower than the controls allow:
    /// centred on the whole height, a full grid's last row ran a third of the
    /// way under the bar, which plan.test.ts tolerates and the eye does not.
    public static func grid(_ container: CGSize, ids: [String], island: CGFloat = island, sides: CGFloat = gap) -> [String: CGRect] {
        var plan: [String: CGRect] = [:]
        guard let layout = arrange(CGSize(width: max(0, container.width - 2 * sides), height: max(0, container.height - island)),
                                   count: ids.count, aspect: aspect(container)) else { return plan }
        let gridHeight = CGFloat(layout.rows) * layout.height + CGFloat(layout.rows - 1) * gap
        let top = max(0, min((container.height - gridHeight) / 2, container.height - island - gridHeight))
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
    /// leave these ones uncovered (plan.ts `clearOf`).
    public static func clear(of plan: [String: CGRect], ids: [String], container: CGSize) -> CGFloat? {
        let tops = ids.compactMap { plan[$0]?.minY }
        guard let highest = tops.min() else { return nil }
        return container.height - highest + gap
    }

    /// How much further up a card stacked on what is below it in its corner
    /// has to go to end `clear` above the bottom, where it now ends `ends`
    /// above it: nothing where there is nothing to clear or it is already
    /// clear.
    ///
    /// For the card telling somebody sharing that their line cannot carry
    /// the share, which sat just above the controls, where the filmstrip
    /// begins, and covered the pictures there: a press on one landed on the
    /// card (Room.tsx `aboveStrip`).
    public static func lift(toClear clear: CGFloat?, ends: CGFloat) -> CGFloat {
        guard let clear else { return 0 }
        return max(0, clear - ends)
    }

    // MARK: The whole room

    /// How tall a window has to be before the controls fit down its side
    /// (hooks/useRoomFor.ts): the column is a little under four hundred
    /// points, and at a phone's landscape height both of its ends, mute and
    /// leave, were off the screen.
    public static let sideNeeds: CGFloat = 480
    /// The lane the controls take down the side when nothing has been
    /// measured: the web's `mr-[5.75rem]`.
    public static let sideLane: CGFloat = 92
    /// How long a room is left alone before controls that may step aside do
    /// (hooks/useIdle.ts `AFTER`).
    public static let idleAfter: Double = 3
    /// What a room that pages keeps clear either side for its arrows, so that
    /// they sit beside the pictures they turn rather than over the first and
    /// the last of them (Plane.tsx draws them over the pictures).
    public static let pagerReserve: CGFloat = 52

    /// Everything the room view needs from one arrangement: where each
    /// picture is, and which page of how many is showing.
    public struct Room: Equatable {
        public var spots: [String: CGRect]
        /// The page showing, already brought back inside the pages there are.
        public var page: Int
        public var pages: Int
        /// The pictures drawn below or around the stage, in order: the strip,
        /// the grid, or the card in a call of two.
        public var shown: [String]
    }

    /// Where every picture goes, as Room.tsx's `Stage` works it out
    /// (~408-437).
    ///
    /// `pictures` is everybody, in the order they are drawn; `mine` is this
    /// participant's own camera, which never pages away: losing sight of
    /// yourself because the room got busy is disorienting, and it costs
    /// nothing, since it is drawn from the local capture. It takes one of the
    /// page's places rather than being added beyond them, so a page is never
    /// more than nine. `stage` is whatever is pinned.
    public static func room(_ container: CGSize, pictures: [String], mine: String?, stage: String?,
                            fullscreen: Bool = false, page: Int = 0, island: CGFloat = island) -> Room {
        let stage = stage.flatMap { pictures.contains($0) ? $0 : nil }
        let me = mine.flatMap { pictures.contains($0) ? $0 : nil }
        let kept = me.flatMap { $0 == stage ? nil : [$0] } ?? []
        let rest = pictures.filter { $0 != me && $0 != stage }
        func places(_ width: CGFloat) -> Int {
            max(1, (stage != nil ? stripCapacity(CGSize(width: width, height: container.height)) : tilesPerPage) - kept.count)
        }
        // A room that will page gives its arrows room, and the strip holds
        // what fits beside them.
        let paged = rest.count > places(container.width)
        let per = places(paged ? container.width - 2 * pagerReserve : container.width)
        let pages = max(1, (rest.count + per - 1) / per)
        let showing = min(max(0, page), pages - 1)
        let start = showing * per
        let shown = kept + Array(rest[min(start, rest.count)..<min(rest.count, start + per)])
        let spots: [String: CGRect]
        if let stage {
            spots = focus(container, stage: stage, strip: fullscreen ? [] : shown, fullscreen: fullscreen, island: island)
        } else if pictures.count <= aloneTogether, let other = rest.first {
            // Two people is one filling the screen and the other a card, the
            // way a telephone has always drawn it.
            spots = close(container, filling: other, cards: me.map { [$0] } ?? [], island: island)
        } else {
            spots = grid(container, ids: shown, island: island, sides: paged ? pagerReserve : gap)
        }
        return Room(spots: spots, page: showing, pages: pages, shown: shown)
    }
}

// MARK: - Who has the stage

/// Which picture owns the stage (hooks/usePin.ts).
///
/// A share takes the stage when it starts and gives it back when it stops,
/// and the rule the web names as the one easy to miss: only a pin made
/// automatically is taken away automatically. Somebody who put a person on
/// the stage by hand keeps them there when a share starts or stops.
///
/// One rule is the web's intent rather than its code. The web claims the
/// stage for a share whenever nothing is pinned, so pressing the share on the
/// stage, whose label says "Show everybody", put it straight back: the grid
/// could not be reached for as long as anybody shared. Here a share claims
/// the stage when it starts, or when the stage is emptied by something other
/// than the person looking (the picture on it went away); choosing the grid
/// is a choice, and it holds until a new share starts.
public struct StagePin: Equatable, Sendable {
    public private(set) var pinned: String?
    /// The pin this made itself, which is the only one it may take back.
    private var auto: String?
    /// The shares that were already running at the last look, so that only a
    /// new one claims a stage somebody emptied on purpose.
    private var seen: Set<String> = []

    public init() {}

    /// A choice by hand: a picture, or nil for everybody.
    public mutating func pin(_ id: String?) {
        auto = nil
        pinned = id
    }

    /// What a press on a picture does: on to the stage, or back off it.
    public mutating func toggle(_ id: String) {
        auto = nil
        pinned = pinned == id ? nil : id
    }

    /// Brings the pin up to date with the room: `pictures` is every picture
    /// there is, `shares` those that take the stage by themselves (a shared
    /// screen, the video watched together), first first.
    public mutating func update(pictures: [String], shares: [String]) {
        var emptied = false
        if let auto, !shares.contains(auto) {
            self.auto = nil
            if pinned == auto { pinned = nil }
            emptied = true
        }
        // A pinned participant who leaves must not take the stage with them.
        if let pinned, !pictures.contains(pinned) {
            self.pinned = nil
            auto = nil
            emptied = true
        }
        if pinned == nil, let claim = emptied ? shares.first : shares.first(where: { !seen.contains($0) }) {
            pinned = claim
            auto = claim
        }
        seen = Set(shares)
    }
}

// MARK: - Who spoke last

/// When each person was last heard, for bringing speakers forward in a room
/// that runs to more than one page (hooks/useSpeakingOrder.ts).
///
/// By when they were last heard rather than by whether they are speaking now:
/// speech is full of gaps, and sorting on the moment would drop somebody down
/// the grid between two words of a sentence. Written down in the same pass as
/// the speaking flag arrives, because the web's first version wrote it after
/// the render that sorted by it, and moved somebody forward only once they had
/// stopped talking.
public struct SpeakingRecord: Equatable, Sendable {
    private var heard: [String: Double] = [:]

    public init() {}

    /// Notes who is speaking at `now`, and forgets whoever is not `present`:
    /// somebody who left should not keep a place they would take on coming
    /// back under the same identity.
    public mutating func hear(speaking: [String], present: [String], at now: Double) {
        for identity in speaking { heard[identity] = now }
        let here = Set(present)
        heard = heard.filter { here.contains($0.key) }
    }

    /// `ids` with the most recently heard first. Ties keep the order they
    /// arrived in, so a room where nobody has spoken looks as it did.
    public func order(_ ids: [String]) -> [String] {
        ids.enumerated()
            .sorted { a, b in
                let (left, right) = (heard[a.element] ?? 0, heard[b.element] ?? 0)
                return left != right ? left > right : a.offset < b.offset
            }
            .map(\.element)
    }
}
