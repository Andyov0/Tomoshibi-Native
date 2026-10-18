package app.tomoshibi.ui.room

import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateValueAsState
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Plan
import app.tomoshibi.basics.Theme
import app.tomoshibi.ui.Hinge
import app.tomoshibi.ui.Posture
import app.tomoshibi.ui.inPoints
import app.tomoshibi.ui.settled
import kotlin.math.max

// The call on a foldable half open (ui/Fold.kt). Standing on a table, as
// Samsung's own calls and Google Meet stand, the stage takes the upper half
// and everything that is touched takes the lower one: the controls, the
// panels, the strip of everybody else when somebody is on the stage, the
// messages arriving and the notices. Held as a book, the call is two pages:
// the pictures and the controls on the first, an open panel on the second,
// and with nothing open the pictures shared between the two. Every part
// moves to its place with the arrival curve as the phone is stood up or laid
// flat, and nothing is drawn again for it, so the call goes on through the
// change: the pictures are the same pictures (RoomView keys them by id) and
// only their frames move.

/** Where the hinge divides the room, in the room's own points. */
internal sealed interface RoomFold {
    /** Standing on a table: the hinge across, from [top] to [bottom]. */
    data class Table(val top: Float, val bottom: Float) : RoomFold

    /**
     * Standing on a table with the keyboard up over the lower half: the room
     * keeps to the upper half, above [top], panel and controls with it, so
     * that the message being typed stays in sight above the keyboard.
     */
    data class Above(val top: Float) : RoomFold

    /** Held as a book: the hinge down, from [start] to [end] points from the room's leading edge. */
    data class Book(val start: Float, val end: Float) : RoomFold

    companion object {
        /** The least the upper half holds: a stage with the room's name over it. */
        private const val LEAST_STAGE = 160f

        /** The least the lower half holds: the controls, and a little above them. */
        private const val LEAST_DESK = Plan.ISLAND + 64f

        /** The least a page of a book holds: a column of pictures, or a panel. */
        private const val LEAST_PAGE = 240f

        /**
         * The hinge in the coordinates of a room [width] by [height] whose
         * top left is [left], [top] points into the window: none where there
         * is no hinge, where it does not cross the room, or where one side of
         * it is too small to lay anything out on.
         */
        fun of(hinge: Hinge?, density: Density, direction: LayoutDirection, left: Float, top: Float, width: Float, height: Float): RoomFold? {
            val at = hinge?.inPoints(density) ?: return null
            if (!hinge.vertical) {
                val upper = at.top - top
                val lower = at.bottom - top
                if (upper < LEAST_STAGE || upper >= height) return null
                return if (height - lower >= LEAST_DESK) Table(upper, lower) else Above(upper)
            }
            val fromLeft = at.left - left
            val toRight = at.right - left
            val start = if (direction == LayoutDirection.Ltr) fromLeft else width - toRight
            val end = if (direction == LayoutDirection.Ltr) toRight else width - fromLeft
            if (start < LEAST_PAGE || width - end < LEAST_PAGE) return null
            return Book(start, end)
        }
    }
}

/**
 * Where each part of the room goes, as rectangles in the room's points from
 * its leading top corner (the way the pictures' own spots are given, and
 * drawn with an offset that follows the reading direction).
 */
internal data class RoomShape(
    val posture: Posture,
    /** Where the pictures are laid out (Plan's container). */
    val stage: Rect,
    /** A book's second page, holding half the pictures while nothing is open there. */
    val second: Rect?,
    /** On a table, the band of the lower half the strip goes in while somebody is on the stage. */
    val strip: Rect?,
    /** What the stage keeps clear at its foot for the controls. */
    val island: Float,
    /** The bar, along the foot of this, centred on it. */
    val bar: Rect,
    /** The room's name, the signal and the notices, along the top of this. */
    val furniture: Rect,
    /** An open panel, at the top trailing corner of this. */
    val panel: Rect,
    /** The messages arriving, at the bottom leading corner of this. */
    val corner: Rect,
    /** The call's notices, at the bottom trailing corner of this. */
    val toasts: Rect,
    /** How far above the foot of [toasts] they stand. */
    val toastsClear: Float,
    /** The card for somebody alone in the room, and whether it sits in the middle of it rather than at its foot. */
    val alone: Rect,
    val aloneCentred: Boolean,
    /** Who has our share open, at the top trailing corner of this, and how far down. */
    val watchers: Rect,
    val watchersTop: Float,
) {
    companion object {
        /**
         * The room [size] points as [fold] divides it, with a panel open or
         * not, the stage filling the screen or not, the controls down the
         * side or not, and somebody on the stage or not.
         */
        fun of(size: Size, fold: RoomFold?, panelOpen: Boolean, filled: Boolean, side: Boolean, pinned: Boolean): RoomShape {
            val whole = Rect(Offset.Zero, size)
            val island = if (side) Plan.GAP else Plan.ISLAND
            return when (fold) {
                is RoomFold.Table -> {
                    val stage = Rect(0f, 0f, size.width, fold.top)
                    val desk = Rect(0f, fold.bottom, size.width, size.height)
                    // The strip leaves the panel its corner of the lower
                    // half where both fit, and goes under it where they do
                    // not, as the strip on a phone goes under a panel.
                    val besidePanel = panelOpen && size.width - panelWidth(desk.size) - 24 >= 172 + 22
                    val strip = if (pinned && !filled) {
                        Rect(0f, fold.bottom, if (besidePanel) size.width - panelWidth(desk.size) - 24 else size.width, fold.bottom + Plan.STRIP_BAND)
                    } else {
                        null
                    }
                    RoomShape(
                        Posture.Tabletop, stage, null, strip, Plan.GAP,
                        bar = desk, furniture = stage, panel = desk, corner = desk,
                        toasts = desk, toastsClear = Plan.ISLAND + 8,
                        alone = Rect(0f, fold.bottom, size.width, max(fold.bottom, size.height - Plan.ISLAND - 2)), aloneCentred = false,
                        watchers = stage, watchersTop = 58f,
                    )
                }
                is RoomFold.Book -> {
                    val first = Rect(0f, 0f, fold.start, size.height)
                    val second = Rect(fold.end, 0f, size.width, size.height)
                    val split = !panelOpen && !filled
                    RoomShape(
                        Posture.Book, first, if (split) second else null, null, island,
                        bar = first, furniture = first, panel = second, corner = first,
                        toasts = if (panelOpen) first else second,
                        toastsClear = Plan.ISLAND + 8,
                        alone = if (split) second else Rect(0f, 0f, fold.start, max(0f, size.height - island - 2)),
                        aloneCentred = split,
                        watchers = if (split) second else first,
                        watchersTop = if (split) 12f else 58f,
                    )
                }
                // Flat, or above the keyboard on a table, which is the room
                // laid out flat in the upper half (the size is already that).
                else -> RoomShape(
                    Posture.Flat, whole, null, null, island,
                    bar = whole, furniture = whole, panel = whole, corner = whole,
                    toasts = whole, toastsClear = Plan.ISLAND + 8,
                    alone = Rect(0f, 0f, size.width, max(0f, size.height - island - 2)), aloneCentred = false,
                    watchers = whole, watchersTop = 58f,
                )
            }
        }
    }

    /** Whether the open panel covers pictures (and is darkened under its glass) rather than standing beside them. */
    fun panelCovers(): Boolean = posture != Posture.Book && narrowFor(panel.size)

    /** Whether the panel has to start below the row at the top, which shares its edge. */
    fun panelBelowRow(): Boolean = posture == Posture.Flat && narrowFor(panel.size)

    /** How wide an open panel is: beside the call where there is room, its page less the margins in a book. */
    fun openPanelWidth(): Float = if (posture == Posture.Book) max(0f, panel.width - 24) else panelWidth(panel.size)

    /** The tallest an open panel grows: never past the controls below it, which close it. */
    fun panelTallest(): Float = when (posture) {
        Posture.Book -> max(160f, panel.height - 24)
        Posture.Tabletop -> max(120f, panel.height - Plan.ISLAND - 24)
        Posture.Flat -> max(160f, panel.height - Plan.ISLAND - 24 - (if (panelBelowRow()) 38 else 0))
    }

    private fun narrowFor(size: Size): Boolean = size.width < panelWidth(size) + 24 + 172 + 22

    /**
     * Every picture's place: Plan's room on the stage, and on a foldable
     * the strip moved to the lower half of a phone on a table, or the
     * pictures shared between a book's two pages.
     */
    fun arrange(pictures: List<String>, mine: String, pinned: String?, filled: Boolean, page: Int): Plan.Room {
        val onStage = pinned?.takeIf { it in pictures }
        val strip = strip
        if (strip != null && onStage != null && !filled) {
            val stageSpot = Rect(stage.left + Plan.GAP, stage.top + Plan.GAP, stage.right - Plan.GAP, stage.bottom - Plan.GAP)
            val rest = Plan.strip(strip.size, pictures, mine, onStage, page)
            return Plan.Room(mapOf(onStage to stageSpot) + rest.spots.moved(strip.topLeft), rest.page, rest.pages, rest.shown)
        }
        val second = second
        if (second == null) {
            val room = Plan.room(stage.size, pictures, mine, pinned, filled, page, island)
            return room.copy(spots = room.spots.moved(stage.topLeft))
        }
        if (onStage != null) {
            // The stage on the first page, everybody else on the second.
            val lead = Plan.focus(stage.size, onStage, emptyList(), false, island)
            val rest = Plan.room(second.size, pictures.filter { it != onStage }, mine, null, false, page, Plan.GAP)
            return Plan.Room(lead.moved(stage.topLeft) + rest.spots.moved(second.topLeft), rest.page, rest.pages, rest.shown)
        }
        // The grid's page shared between the pages, the others first and
        // this participant's own picture last, so that in a call of two the
        // person being talked to has the first page and yourself the second.
        val all = Plan.room(Size(stage.width + second.width, stage.height), pictures, mine, null, false, page, island)
        val order = all.shown.filter { it != mine } + all.shown.filter { it == mine }
        val half = (order.size + 1) / 2
        val sides = if (all.pages > 1) Plan.PAGER_RESERVE else Plan.GAP
        // One size of picture on both pages, worked out for the fuller one:
        // each page's own best grid gave five on the first page in two
        // columns and four on the second in one, two sizes side by side.
        val usable = Size(max(0f, stage.width - 2 * sides), max(0f, stage.height - island))
        val aspect = if (Plan.upright(stage.size)) 3f / 4f else 16f / 9f
        val cell = Plan.arrange(usable, half, aspect = aspect) ?: return all
        val spots = pageGrid(stage, order.take(half), cell, island) + pageGrid(second, order.drop(half), cell, island)
        return Plan.Room(spots, all.page, all.pages, all.shown)
    }

    /** [ids] on one page of a book in [cell]'s columns and size, centred as Plan's grid centres, never lower than the controls allow. */
    private fun pageGrid(page: Rect, ids: List<String>, cell: Plan.Arrangement, island: Float): Map<String, Rect> {
        if (ids.isEmpty()) return emptyMap()
        val columns = minOf(cell.columns, ids.size)
        val rows = (ids.size + columns - 1) / columns
        val height = rows * cell.height + (rows - 1) * Plan.GAP
        val top = max(0f, minOf((page.height - height) / 2, page.height - island - height))
        val spots = LinkedHashMap<String, Rect>()
        for ((index, id) in ids.withIndex()) {
            val row = index / columns
            val column = index % columns
            val inRow = if (row == rows - 1) ids.size - row * columns else columns
            val rowWidth = inRow * cell.width + (inRow - 1) * Plan.GAP
            val left = page.left + (page.width - rowWidth) / 2 + column * (cell.width + Plan.GAP)
            spots[id] = Rect(left, page.top + top + row * (cell.height + Plan.GAP), left + cell.width, page.top + top + row * (cell.height + Plan.GAP) + cell.height)
        }
        return spots
    }
}

/**
 * Beside the call where the screen has room for it, and on a screen that has
 * none (a phone held upright) the width of the screen inside the same twelve
 * points on either side (RoomView.swift `panelWidth`).
 */
internal fun panelWidth(size: Size): Float {
    val beside = minOf(360f, size.width - 32)
    return if (size.width < beside + 24 + 172 + 22) max(0f, size.width - 24) else beside
}

private fun Map<String, Rect>.moved(by: Offset): Map<String, Rect> =
    if (by == Offset.Zero) this else mapValues { (_, spot) -> spot.translate(by) }

/** Placed at [rect], in the room's points, from its leading top corner. */
internal fun Modifier.at(rect: Rect): Modifier = offset(rect.left.dp, rect.top.dp).size(max(0f, rect.width).dp, max(0f, rect.height).dp)
