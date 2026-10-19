import CoreGraphics

// The room laid out around a fold, for an iPhone Duo held half open.
//
// Laid out flat over a fold, the call put a face down the middle of a book:
// two people's pictures, the grid and the stage ran across the curve, and the
// controls' middle buttons sat in it. Apple's guidance for a manually laid out
// screen (Human Interface Guidelines, Designing for iPhone Duo, Reserved
// regions) is the Android port's for a Fold (ui/room/RoomShape.kt): move only
// what has to move, keep what is interactive out of the curve, prefer an even
// number of columns, and as a book keep content on its page; standing on a
// table, what is watched from a distance goes up top and what is touched goes
// on the half lying on the table.

extension Plan {
    /// The two sides of a fold, in the room's own coordinates.
    public enum Halves: Equatable, Sendable {
        /// Held as a book: the leading page and the trailing one.
        case book(first: CGRect, second: CGRect)
        /// Standing on a table: the half held up and the half lying flat.
        case table(upper: CGRect, lower: CGRect)
    }

    /// Where everything goes in a folded room.
    public struct Folded: Equatable {
        /// The pictures, in the room's coordinates, and the paging of those
        /// that page (everybody but this participant).
        public var room: Room
        /// The space the controls sit at the foot of, centred across it.
        public var controls: CGRect
        /// Where an open panel stands: the second page of a book; above the
        /// controls on the half lying on a table.
        public var panel: CGRect
        /// Where a notice or a card centred on the room goes instead, clear
        /// of the fold: the page the pictures are on, or the upper half.
        public var centre: CGRect
    }

    /// The room laid out on either side of a fold. `island` is what the
    /// controls take at the foot of their half, as in `room`.
    public static func folded(_ halves: Halves, pictures: [String], mine: String?, stage: String?,
                              fullscreen: Bool = false, page: Int = 0, panelOpen: Bool = false,
                              island: CGFloat = island) -> Folded {
        switch halves {
        case let .table(upper, lower):
            // Up top what is looked at, laid out as a room of its own with
            // nothing to keep clear at its foot; the controls and an open
            // panel on the half on the table, the panel above the controls.
            var room = Plan.room(upper.size, pictures: pictures, mine: mine, stage: stage,
                                 fullscreen: fullscreen, page: page, island: gap)
            room.spots = room.spots.mapValues { $0.offsetBy(dx: upper.minX, dy: upper.minY) }
            let panel = CGRect(x: lower.minX, y: lower.minY, width: lower.width,
                               height: max(0, lower.height - island))
            return Folded(room: room, controls: lower, panel: panel, centre: upper)
        case let .book(first, second):
            let foot = island
            func placed(_ room: Room, in pane: CGRect) -> Room {
                var room = room
                room.spots = room.spots.mapValues { $0.offsetBy(dx: pane.minX, dy: pane.minY) }
                return room
            }
            // A stage filling the screen fills its page, and a panel open
            // takes the other page whole: the pictures keep to the first.
            if fullscreen || panelOpen {
                let room = placed(Plan.room(first.size, pictures: pictures, mine: mine, stage: stage,
                                            fullscreen: fullscreen, page: page, island: foot), in: first)
                return Folded(room: room, controls: first, panel: second, centre: first)
            }
            let me = mine.flatMap { pictures.contains($0) ? $0 : nil }
            let others = pictures.filter { $0 != me }
            if let stage = stage.flatMap({ pictures.contains($0) ? $0 : nil }) {
                // The stage has the first page, everybody else the second as
                // a grid, this participant first and never paged away.
                let rest = others.filter { $0 != stage }
                let kept = me.flatMap { $0 == stage ? nil : [$0] } ?? []
                let grid = pagedGrid(second.size, kept: kept, rest: rest, page: page, island: gap)
                var spots = Plan.focus(first.size, stage: stage, strip: [], island: foot)
                    .mapValues { $0.offsetBy(dx: first.minX, dy: first.minY) }
                for (id, spot) in grid.spots { spots[id] = spot.offsetBy(dx: second.minX, dy: second.minY) }
                return Folded(room: Room(spots: spots, page: grid.page, pages: grid.pages, shown: grid.shown),
                              controls: first, panel: second, centre: first)
            }
            guard let me, !others.isEmpty else {
                // Alone, or only somebody else: one page, as a flat room is.
                let room = placed(Plan.room(first.size, pictures: pictures, mine: mine, stage: nil,
                                            page: page, island: foot), in: first)
                return Folded(room: room, controls: first, panel: second, centre: first)
            }
            // The person talked to on the first page and yourself on the
            // second; with more, the others shared between the pages and the
            // pictures on both kept to one size, as one grid would draw them.
            let perPage = tilesPerPage - 1
            let pages = max(1, (others.count + perPage - 1) / perPage)
            let showing = min(max(0, page), pages - 1)
            let shown = Array(others[(showing * perPage)..<min(others.count, (showing + 1) * perPage)])
            let leadCount = max(1, (shown.count + 1) / 2)
            let lead = Array(shown.prefix(leadCount))
            let trail = [me] + shown.dropFirst(leadCount)
            var spots: [String: CGRect] = [:]
            if shown.count == 1 {
                // A call of two is two people each filling a page, the two
                // pictures the same size either side of the fold, and both
                // ending above the controls: filling the first page to its
                // foot as a flat call of two does, the other person's name
                // lay under the bar.
                let height = max(0, min(first.height, second.height) - foot - gap)
                spots[lead[0]] = CGRect(x: gap, y: gap, width: max(0, first.width - 2 * gap), height: height)
                    .offsetBy(dx: first.minX, dy: first.minY)
                spots[me] = CGRect(x: gap, y: gap, width: max(0, second.width - 2 * gap), height: height)
                    .offsetBy(dx: second.minX, dy: second.minY)
            } else {
                let size = (grid(first.size, ids: lead, island: foot).values.map(\.size)
                    + grid(second.size, ids: trail, island: foot).values.map(\.size))
                    .min { $0.width * $0.height < $1.width * $1.height } ?? .zero
                let left = grid(first.size, ids: lead, island: foot).mapValues { resized($0, to: size) }
                let right = grid(second.size, ids: trail, island: foot).mapValues { resized($0, to: size) }
                // Both pages on one line, the taller of the two clear of the
                // controls: each grid centred on its own page, the one picture
                // on the first page of a call of three stood higher than the
                // two beside it, pushed up by the controls under it.
                let boxes = [left, right].compactMap(bounds)
                let tallest = boxes.map(\.height).max() ?? 0
                let height = min(first.height, second.height)
                let middle = min(height / 2, height - foot - tallest / 2)
                func lined(_ group: [String: CGRect]) -> [String: CGRect] {
                    guard let box = bounds(group) else { return group }
                    return group.mapValues { $0.offsetBy(dx: 0, dy: middle - box.midY) }
                }
                for (id, spot) in lined(left) { spots[id] = spot.offsetBy(dx: first.minX, dy: first.minY) }
                for (id, spot) in lined(right) { spots[id] = spot.offsetBy(dx: second.minX, dy: second.minY) }
            }
            return Folded(room: Room(spots: spots, page: showing, pages: pages, shown: [me] + shown),
                          controls: first, panel: second, centre: first)
        }
    }

    /// A grid of `kept` and a page of `rest`, as `room` pages a grid.
    static func pagedGrid(_ container: CGSize, kept: [String], rest: [String], page: Int, island: CGFloat) -> Room {
        let per = max(1, tilesPerPage - kept.count)
        let pages = max(1, (rest.count + per - 1) / per)
        let showing = min(max(0, page), pages - 1)
        let shown = kept + Array(rest[min(showing * per, rest.count)..<min(rest.count, (showing + 1) * per)])
        return Room(spots: grid(container, ids: shown, island: island), page: showing, pages: pages, shown: shown)
    }

    /// `spot` made `size`, about its own centre.
    static func resized(_ spot: CGRect, to size: CGSize) -> CGRect {
        CGRect(x: spot.midX - size.width / 2, y: spot.midY - size.height / 2, width: size.width, height: size.height)
    }

    /// The rectangle round a set of pictures, nil for none.
    static func bounds(_ spots: [String: CGRect]) -> CGRect? {
        spots.values.reduce(nil) { box, spot in box?.union(spot) ?? spot }
    }
}
