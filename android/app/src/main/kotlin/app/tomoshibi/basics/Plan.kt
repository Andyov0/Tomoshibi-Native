package app.tomoshibi.basics

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/**
 * Where every picture sits, as coordinates rather than as a tree
 * (TomoshibiBasics/Plan.swift, which follows web/src/live/plan.ts and
 * layout.ts line for line). Lengths are in points, which on Android are dp.
 *
 * The web learned this the hard way: an arrangement expressed by which
 * container a tile is in destroys and rebuilds the tile whenever the
 * arrangement changes, and a picture that is rebuilt resubscribes and waits
 * for a keyframe, the black screen. A picture placed by coordinates is built
 * once for as long as its owner is in the call, and moving it to the stage is
 * an animation rather than an interruption. The same holds for composables
 * keyed by identity.
 */
object Plan {
    /** Space between pictures, and between a picture and the edge. */
    const val GAP = 8f
    /** Nine keeps a grid readable on a laptop and caps how many streams the call asks for at once. */
    const val TILES_PER_PAGE = 9
    /** The controls' reserve at the foot when nothing has been measured. */
    const val ISLAND = 20f + 58f + GAP
    /** Two people are one person being looked at and one being checked on. */
    const val ALONE_TOGETHER = 2
    private const val STRIP = 112f
    private const val HEADROOM = 56f
    private const val LANDSCAPE = 16f / 9f
    private const val UPRIGHT_ASPECT = 3f / 4f
    private const val FILL = 0.92f

    /**
     * A screen held upright, rather than a window that happens to be tall: a
     * fifth taller than wide, so a window dragged through square does not
     * reshape the call under the pointer.
     */
    fun upright(container: Size): Boolean = container.height > container.width * 1.2f

    private fun aspect(container: Size): Float = if (upright(container)) UPRIGHT_ASPECT else LANDSCAPE

    data class Arrangement(val columns: Int, val rows: Int, val width: Float, val height: Float)

    /** The column count that gives each person the most picture (layout.ts). */
    fun arrange(container: Size, count: Int, gap: Float = GAP, aspect: Float = LANDSCAPE): Arrangement? {
        val width = container.width
        val height = container.height
        if (count <= 0 || width <= 0 || height <= 0) return null
        var best: Arrangement? = null
        var bestScore = 0f
        for (columns in 1..count) {
            val rows = ceil(count.toDouble() / columns).toInt()
            val cellWidth = (width - (columns - 1) * gap) / columns
            val cellHeight = (height - (rows - 1) * gap) / rows
            if (cellWidth <= 0 || cellHeight <= 0) continue
            var tileWidth = min(cellWidth, cellHeight * aspect)
            var tileHeight = tileWidth / aspect
            val ceilingFill = if (container.height > container.width * 1.2f) 1f else FILL
            val ceiling = (height * ceilingFill - (rows - 1) * gap) / rows
            if (tileHeight > ceiling) {
                tileHeight = ceiling
                tileWidth = tileHeight * aspect
            }
            val score = tileWidth * tileHeight * (count.toFloat() / (columns * rows))
            if (score > bestScore) {
                bestScore = score
                best = Arrangement(columns, rows, floor(tileWidth), floor(tileHeight))
            }
        }
        return best
    }

    /**
     * A grid, centred, each row centred on its own.
     *
     * Two departures from plan.ts, both about the edges, as Plan.swift has
     * them. The grid keeps [sides] clear at the left and right, the gap by
     * default, which the stage and the strip always kept and the grid did
     * not: six people in a wide window were drawn edge to edge. And it is
     * centred in the window but never lower than the controls allow: centred
     * on the whole height, a full grid's last row ran a third of the way
     * under the bar.
     */
    fun grid(container: Size, ids: List<String>, island: Float = ISLAND, sides: Float = GAP): Map<String, Rect> {
        val plan = LinkedHashMap<String, Rect>()
        val layout = arrange(
            Size(max(0f, container.width - 2 * sides), max(0f, container.height - island)),
            ids.size, aspect = aspect(container),
        ) ?: return plan
        val gridHeight = layout.rows * layout.height + (layout.rows - 1) * GAP
        val top = max(0f, min((container.height - gridHeight) / 2, container.height - island - gridHeight))
        for ((index, id) in ids.withIndex()) {
            val row = index / layout.columns
            val column = index % layout.columns
            val inRow = if (row == layout.rows - 1) ids.size - row * layout.columns else layout.columns
            val rowWidth = inRow * layout.width + (inRow - 1) * GAP
            plan[id] = rect(
                (container.width - rowWidth) / 2 + column * (layout.width + GAP),
                top + row * (layout.height + GAP),
                layout.width, layout.height,
            )
        }
        return plan
    }

    /** One picture on the stage, the rest in a filmstrip beneath. */
    fun focus(container: Size, stage: String, strip: List<String>, fullscreen: Boolean = false, island: Float = ISLAND): Map<String, Rect> {
        val plan = LinkedHashMap<String, Rect>()
        if (container.width <= 0 || container.height <= 0) return plan
        if (fullscreen) {
            plan[stage] = Rect(Offset.Zero, container)
            return plan
        }
        val bottom = container.height - island
        val showStrip = strip.isNotEmpty()
        val stripTop = bottom - STRIP
        plan[stage] = rect(GAP, GAP, max(0f, container.width - 2 * GAP), max(0f, (if (showStrip) stripTop - GAP else bottom) - GAP))
        if (!showStrip) return plan
        val width = STRIP * aspect(container)
        val total = strip.size * width + (strip.size - 1) * GAP
        val left = (container.width - total) / 2
        for ((index, id) in strip.withIndex()) {
            plan[id] = rect(left + index * (width + GAP), stripTop, width, STRIP)
        }
        return plan
    }

    /** How many the filmstrip holds at this width; never fewer than one. */
    fun stripCapacity(container: Size): Int = stripCapacity(container.width, aspect(container))

    /** The same for a strip [width] long whose pictures have [aspect], which is the window's and not the strip's. */
    private fun stripCapacity(width: Float, aspect: Float): Int {
        val picture = STRIP * aspect
        val usable = width - 2 * GAP + GAP
        return max(1, floor(usable / (picture + GAP)).toInt())
    }

    /** The other person filling the screen and the rest as cards down the trailing edge, the way every telephone draws a call of two. */
    fun close(container: Size, filling: String, cards: List<String>, island: Float = ISLAND): Map<String, Rect> {
        val plan = LinkedHashMap<String, Rect>()
        if (container.width <= 0 || container.height <= 0) return plan
        plan[filling] = Rect(Offset.Zero, container)
        val width = min(container.width * (if (upright(container)) 0.26f else 0.18f), (container.height - island - 2 * GAP) / 2)
        val height = width / aspect(container)
        for ((index, id) in cards.withIndex()) {
            val top = HEADROOM + index * (height + GAP)
            if (top + height > container.height - island) continue
            plan[id] = rect(container.width - width - GAP, top, width, height)
        }
        return plan
    }

    /** How far above the bottom something drawn over the pictures must end to leave these ones uncovered (plan.ts `clearOf`). */
    fun clear(plan: Map<String, Rect>, ids: List<String>, container: Size): Float? {
        val highest = ids.mapNotNull { plan[it]?.top }.minOrNull() ?: return null
        return container.height - highest + GAP
    }

    /**
     * How much further up a card stacked on what is below it in its corner
     * has to go to end [clear] above the bottom, where it now ends [ends]
     * above it: nothing where there is nothing to clear or it is already
     * clear (Room.tsx `aboveStrip`).
     */
    fun lift(clear: Float?, ends: Float): Float = if (clear == null) 0f else max(0f, clear - ends)

    // The whole room

    /**
     * How tall a window has to be before the controls fit down its side
     * (hooks/useRoomFor.ts): the column is a little under four hundred
     * points, and at a phone's landscape height both of its ends, mute and
     * leave, were off the screen.
     */
    const val SIDE_NEEDS = 480f
    /** The lane the controls take down the side when nothing has been measured: the web's `mr-[5.75rem]`. */
    const val SIDE_LANE = 92f
    /** How long a room is left alone before controls that may step aside do, in seconds (hooks/useIdle.ts `AFTER`). */
    const val IDLE_AFTER = 3.0
    /**
     * What a room that pages keeps clear either side for its arrows, so that
     * they sit beside the pictures they turn rather than over the first and
     * the last of them.
     */
    const val PAGER_RESERVE = 52f

    /**
     * Everything the room view needs from one arrangement: where each picture
     * is, which page of how many is showing (already brought back inside the
     * pages there are), and the pictures drawn below or around the stage, in
     * order: the strip, the grid, or the card in a call of two.
     */
    data class Room(val spots: Map<String, Rect>, val page: Int, val pages: Int, val shown: List<String>)

    /**
     * Where every picture goes, as Room.tsx's `Stage` works it out.
     *
     * [pictures] is everybody, in the order they are drawn; [mine] is this
     * participant's own camera, which never pages away: losing sight of
     * yourself because the room got busy is disorienting, and it costs
     * nothing, since it is drawn from the local capture. It takes one of the
     * page's places rather than being added beyond them, so a page is never
     * more than nine. [stage] is whatever is pinned.
     */
    fun room(
        container: Size,
        pictures: List<String>,
        mine: String?,
        stage: String?,
        fullscreen: Boolean = false,
        page: Int = 0,
        island: Float = ISLAND,
    ): Room {
        val onStage = stage?.takeIf { it in pictures }
        val me = mine?.takeIf { it in pictures }
        val kept = if (me != null && me != onStage) listOf(me) else emptyList()
        val rest = pictures.filter { it != me && it != onStage }
        // Counted at the window's own shape, which is the shape the strip's
        // pictures are drawn at (focus). Plan.swift counts them at the shape
        // of the width left between the arrows, and a window a little short
        // of upright (840 by 1000) read as upright there: its strip was
        // counted as narrow cards and drawn as wide ones, off both sides.
        fun places(width: Float): Int =
            max(1, (if (onStage != null) stripCapacity(width, aspect(container)) else TILES_PER_PAGE) - kept.size)
        // A room that will page gives its arrows room, and the strip holds
        // what fits beside them.
        val paged = rest.size > places(container.width)
        val per = places(if (paged) container.width - 2 * PAGER_RESERVE else container.width)
        val pages = max(1, (rest.size + per - 1) / per)
        val showing = min(max(0, page), pages - 1)
        val start = showing * per
        val shown = kept + rest.subList(min(start, rest.size), min(rest.size, start + per))
        val spots = when {
            onStage != null -> focus(container, onStage, if (fullscreen) emptyList() else shown, fullscreen, island)
            // Two people is one filling the screen and the other a card, the
            // way a telephone has always drawn it.
            pictures.size <= ALONE_TOGETHER && rest.isNotEmpty() -> close(container, rest.first(), listOfNotNull(me), island)
            else -> grid(container, shown, island, if (paged) PAGER_RESERVE else GAP)
        }
        return Room(spots, showing, pages, shown)
    }

    /**
     * The filmstrip on its own, along the top of [container], for a stage
     * drawn somewhere else: a foldable standing on a table puts the stage on
     * its upper half and keeps the rest of the call, the strip with it, on
     * the lower half it is touched on. Paged as [room] pages the strip
     * beneath a stage, this participant's own picture on every page.
     */
    fun strip(container: Size, pictures: List<String>, mine: String?, stage: String, page: Int = 0): Room {
        val me = mine?.takeIf { it in pictures && it != stage }
        val kept = listOfNotNull(me)
        val rest = pictures.filter { it != me && it != stage }
        val aspect = aspect(container)
        fun places(width: Float): Int = max(1, stripCapacity(width, aspect) - kept.size)
        val paged = rest.size > places(container.width)
        val per = places(if (paged) container.width - 2 * PAGER_RESERVE else container.width)
        val pages = max(1, (rest.size + per - 1) / per)
        val showing = min(max(0, page), pages - 1)
        val start = showing * per
        val shown = kept + rest.subList(min(start, rest.size), min(rest.size, start + per))
        val spots = LinkedHashMap<String, Rect>()
        val width = STRIP * aspect
        val total = shown.size * width + (shown.size - 1) * GAP
        val left = (container.width - total) / 2
        for ((index, id) in shown.withIndex()) spots[id] = rect(left + index * (width + GAP), GAP, width, STRIP)
        return Room(spots, showing, pages, shown)
    }

    /** How tall [strip] is with the gap above and below it. */
    const val STRIP_BAND = STRIP + 2 * GAP

    private fun rect(x: Float, y: Float, width: Float, height: Float) = Rect(Offset(x, y), Size(width, height))
}

/**
 * Which picture owns the stage (hooks/usePin.ts; Plan.swift `StagePin`).
 *
 * A share takes the stage when it starts and gives it back when it stops,
 * and the rule the web names as the one easy to miss: only a pin made
 * automatically is taken away automatically. Somebody who put a person on
 * the stage by hand keeps them there when a share starts or stops.
 *
 * One rule is the web's intent rather than its code. The web claims the stage
 * for a share whenever nothing is pinned, so pressing the share on the stage,
 * whose label says "Show everybody", put it straight back: the grid could not
 * be reached for as long as anybody shared. Here a share claims the stage when
 * it starts, or when the stage is emptied by something other than the person
 * looking (the picture on it went away); choosing the grid is a choice, and it
 * holds until a new share starts.
 */
data class StagePin(
    val pinned: String? = null,
    /** The pin this made itself, which is the only one it may take back. */
    private val auto: String? = null,
    /** The shares that were already running at the last look, so that only a new one claims a stage somebody emptied on purpose. */
    private val seen: Set<String> = emptySet(),
) {
    /** A choice by hand: a picture, or null for everybody. */
    fun pin(id: String?): StagePin = copy(pinned = id, auto = null)

    /** What a press on a picture does: on to the stage, or back off it. */
    fun toggle(id: String): StagePin = copy(pinned = if (pinned == id) null else id, auto = null)

    /**
     * Brings the pin up to date with the room: [pictures] is every picture
     * there is, [shares] those that take the stage by themselves (a shared
     * screen, the video watched together), first first.
     */
    fun update(pictures: List<String>, shares: List<String>): StagePin {
        var pinned = pinned
        var auto = auto
        var emptied = false
        if (auto != null && auto !in shares) {
            if (pinned == auto) pinned = null
            auto = null
            emptied = true
        }
        // A pinned participant who leaves must not take the stage with them.
        if (pinned != null && pinned !in pictures) {
            pinned = null
            auto = null
            emptied = true
        }
        if (pinned == null) {
            val claim = if (emptied) shares.firstOrNull() else shares.firstOrNull { it !in seen }
            if (claim != null) {
                pinned = claim
                auto = claim
            }
        }
        return StagePin(pinned, auto, shares.toSet())
    }
}

/**
 * When each person was last heard, for bringing speakers forward in a room
 * that runs to more than one page (hooks/useSpeakingOrder.ts; Plan.swift
 * `SpeakingRecord`).
 *
 * By when they were last heard rather than by whether they are speaking now:
 * speech is full of gaps, and sorting on the moment would drop somebody down
 * the grid between two words of a sentence. Written down in the same pass as
 * the speaking flag arrives, because the web's first version wrote it after
 * the render that sorted by it, and moved somebody forward only once they had
 * stopped talking.
 */
data class SpeakingRecord(private val heard: Map<String, Double> = emptyMap()) {
    /**
     * Notes who is speaking at [now], and forgets whoever is not [present]:
     * somebody who left should not keep a place they would take on coming
     * back under the same identity.
     */
    fun hear(speaking: List<String>, present: List<String>, now: Double): SpeakingRecord {
        val next = HashMap(heard)
        for (identity in speaking) next[identity] = now
        val here = present.toSet()
        return SpeakingRecord(next.filterKeys { it in here })
    }

    /** [ids] with the most recently heard first. Ties keep the order they arrived in, so a room where nobody has spoken looks as it did. */
    fun order(ids: List<String>): List<String> =
        ids.withIndex()
            .sortedWith(compareByDescending<IndexedValue<String>> { heard[it.value] ?: 0.0 }.thenBy { it.index })
            .map { it.value }
}
