package app.tomoshibi.basics

/**
 * Whether a camera's picture fills its tile, cut to the tile's shape, or is
 * shown whole inside it (TomoshibiBasics/Framing.swift, case for case).
 *
 * A face is fine cut at the edges, so a picture of nearly the tile's shape
 * fills it, and so does one that is only longer the same way round: a
 * telephone held upright, shown on a screen held upright, loses a little of
 * each side and nobody misses it. A picture the other way round is another
 * matter: a camera held on its side, filling an upright screen, kept little
 * more than its middle, and on a Fold's cover screen hardly that (the
 * owner's Fold6, 2026-10-19). Those are shown whole, the rest of the tile
 * given to a blurred copy of the picture (RemoteVideo).
 */
object Framing {
    /** As close as this, a fill loses so little that it fills whichever way round the two are. */
    const val CLOSE = 0.75

    /** The same way round, a fill keeps at least this much of the picture before it is shown whole. */
    const val SAME_WAY = 0.5

    /** [pictureWidth] by [pictureHeight] as it is shown, that is turned; [viewWidth] by [viewHeight] the tile. */
    fun fills(pictureWidth: Float, pictureHeight: Float, viewWidth: Float, viewHeight: Float): Boolean {
        if (pictureWidth <= 0f || pictureHeight <= 0f || viewWidth <= 0f || viewHeight <= 0f) return true
        // In doubles, as the Apple apps work it out, so that a shape on the
        // line (a square picture in a three-by-four tile keeps exactly 0.75)
        // comes out the same on both.
        val picture = pictureWidth.toDouble() / pictureHeight
        val view = viewWidth.toDouble() / viewHeight
        // The share of the picture a fill keeps along the side it cuts.
        val kept = minOf(picture, view) / maxOf(picture, view)
        if (kept >= CLOSE) return true
        // A square is neither way round.
        val sameWay = (picture > 1.0 && view > 1.0) || (picture < 1.0 && view < 1.0)
        return sameWay && kept >= SAME_WAY
    }
}
