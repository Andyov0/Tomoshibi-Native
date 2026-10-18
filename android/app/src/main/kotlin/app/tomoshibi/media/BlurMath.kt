package app.tomoshibi.media

import kotlin.math.ceil
import kotlin.math.exp
import kotlin.math.roundToInt

/**
 * The arithmetic of blurring what is behind somebody, on a camera frame in
 * three packed planes (I420), apart from finding the person
 * (BackgroundBlur): pure, so a test can hold it to its numbers.
 *
 * The web's pipeline, step for step (livekit track-processors, webgl/index.ts,
 * which the web's blur.ts runs): the frame is taken down to a quarter of its
 * size, blurred there with a Gaussian of a quarter of the radius, and laid
 * under the frame through the person's mask, softened at its edge so that the
 * outline of hair does not step. The radius is the Apple apps' rule
 * (BackgroundBlur.swift): the web's 12 at a picture 720 rows high, in
 * proportion below that, so a phone's 360 rows look as a computer's 720 do.
 *
 * A quarter is also what keeps it cheap: the blur itself runs over a
 * sixteenth of the pixels, and the full-size work is one blend per pixel.
 */
class BlurMath(val width: Int, val height: Int) {
    /** The quarter-size grid the blur and the mask live on. */
    val gridWidth = ceil(width / 4.0).toInt().coerceAtLeast(1)
    val gridHeight = ceil(height / 4.0).toInt().coerceAtLeast(1)

    private val lowY = FloatArray(gridWidth * gridHeight)
    private val lowU = FloatArray(gridWidth * gridHeight)
    private val lowV = FloatArray(gridWidth * gridHeight)
    private val scratch = FloatArray(gridWidth * gridHeight)
    private val kernel: FloatArray = gaussian(SIGMA_AT_720 * height / 720.0 / 4.0)

    // Where each full-size column and row falls on the grid, worked out once:
    // the grid cell before it and how far towards the next.
    private val lumaX = Taps(width, 4, gridWidth)
    private val lumaY = Taps(height, 4, gridHeight)
    private val chromaX = Taps((width + 1) / 2, 2, gridWidth)
    private val chromaY = Taps((height + 1) / 2, 2, gridHeight)

    /**
     * Blurs the background of the packed planes in place: [y] of [width] by
     * [height], [u] and [v] at half that each way. [person] is the grid's
     * share of person in each cell, 0 to 1, or null where nobody has been
     * found yet, in which case the whole frame is blurred: somebody who asked
     * for their room to be hidden is shown nothing of it while the segmenter
     * is still starting.
     */
    fun blur(y: ByteArray, u: ByteArray, v: ByteArray, person: FloatArray?) {
        val cw = (width + 1) / 2
        val ch = (height + 1) / 2
        shrink(y, width, height, 4, lowY)
        shrink(u, cw, ch, 2, lowU)
        shrink(v, cw, ch, 2, lowV)
        smooth(lowY)
        smooth(lowU)
        smooth(lowV)
        blend(y, width, height, lumaX, lumaY, lowY, person)
        blend(u, cw, ch, chromaX, chromaY, lowU, person)
        blend(v, cw, ch, chromaX, chromaY, lowV, person)
    }

    /** A plane averaged down onto the grid, [factor] of its pixels to a cell each way. */
    private fun shrink(plane: ByteArray, planeWidth: Int, planeHeight: Int, factor: Int, out: FloatArray) {
        java.util.Arrays.fill(out, 0f)
        val counts = scratch
        java.util.Arrays.fill(counts, 0f)
        for (row in 0 until planeHeight) {
            val cell = minOf(row / factor, gridHeight - 1) * gridWidth
            val at = row * planeWidth
            for (column in 0 until planeWidth) {
                val index = cell + minOf(column / factor, gridWidth - 1)
                out[index] += (plane[at + column].toInt() and 0xff).toFloat()
                counts[index] += 1f
            }
        }
        for (i in out.indices) if (counts[i] > 0f) out[i] /= counts[i]
    }

    /** The Gaussian on the grid, across and then down, with the edges held. */
    private fun smooth(plane: FloatArray) {
        val radius = kernel.size / 2
        if (radius == 0) return
        val gw = gridWidth
        val gh = gridHeight
        for (row in 0 until gh) {
            val at = row * gw
            for (column in 0 until gw) {
                var sum = 0f
                for (k in -radius..radius) sum += kernel[k + radius] * plane[at + (column + k).coerceIn(0, gw - 1)]
                scratch[at + column] = sum
            }
        }
        for (row in 0 until gh) {
            for (column in 0 until gw) {
                var sum = 0f
                for (k in -radius..radius) sum += kernel[k + radius] * scratch[(row + k).coerceIn(0, gh - 1) * gw + column]
                plane[row * gw + column] = sum
            }
        }
    }

    /**
     * Each pixel of a plane between itself and the blurred grid under it,
     * by how much of a person the mask says is there: kept where it is all
     * person, the blur where it is none.
     */
    private fun blend(plane: ByteArray, planeWidth: Int, planeHeight: Int, xs: Taps, ys: Taps, low: FloatArray, person: FloatArray?) {
        val gw = gridWidth
        for (row in 0 until planeHeight) {
            val r0 = ys.cell[row] * gw
            val r1 = ys.next[row] * gw
            val fy = ys.toward[row]
            val at = row * planeWidth
            for (column in 0 until planeWidth) {
                val c0 = xs.cell[column]
                val c1 = xs.next[column]
                val fx = xs.toward[column]
                val alpha = if (person == null) 0f else bilinear(person, r0, r1, c0, c1, fx, fy)
                if (alpha >= 0.996f) continue
                val behind = bilinear(low, r0, r1, c0, c1, fx, fy)
                val pixel = (plane[at + column].toInt() and 0xff).toFloat()
                plane[at + column] = (behind + alpha * (pixel - behind)).roundToInt().coerceIn(0, 255).toByte()
            }
        }
    }

    private fun bilinear(grid: FloatArray, r0: Int, r1: Int, c0: Int, c1: Int, fx: Float, fy: Float): Float {
        val top = grid[r0 + c0] + (grid[r0 + c1] - grid[r0 + c0]) * fx
        val bottom = grid[r1 + c0] + (grid[r1 + c1] - grid[r1 + c0]) * fx
        return top + (bottom - top) * fy
    }

    /** For each pixel along one side of a plane [factor] times the grid's [cells]: its cell, the next one, and how far between. */
    private class Taps(count: Int, factor: Int, cells: Int) {
        val cell = IntArray(count)
        val next = IntArray(count)
        val toward = FloatArray(count)
        init {
            for (i in 0 until count) {
                val position = ((i + 0.5f) / factor - 0.5f).coerceIn(0f, (cells - 1).toFloat())
                val at = position.toInt()
                cell[i] = at
                next[i] = minOf(at + 1, cells - 1)
                toward[i] = position - at
            }
        }
    }

    companion object {
        /** The web's radius, at a picture 720 rows high (blur.ts `RADIUS`). */
        const val SIGMA_AT_720 = 12.0

        /** A normalised Gaussian of [sigma] cells, out to three sigma. */
        fun gaussian(sigma: Double): FloatArray {
            if (sigma < 0.3) return floatArrayOf(1f)
            val radius = ceil(sigma * 3).toInt()
            val out = FloatArray(radius * 2 + 1)
            var sum = 0.0
            for (i in -radius..radius) {
                val w = exp(-(i * i) / (2 * sigma * sigma))
                out[i + radius] = w.toFloat()
                sum += w
            }
            for (i in out.indices) out[i] = (out[i] / sum).toFloat()
            return out
        }

        /**
         * Where a pixel of the frame as captured ([x], [y] of [width] by
         * [height]) is in the picture turned upright by [rotation] clockwise
         * degrees, which is how the segmenter sees it: a person on their side
         * is a person it does not know.
         */
        fun upright(x: Int, y: Int, width: Int, height: Int, rotation: Int): Pair<Int, Int> = when (rotation) {
            90 -> (height - 1 - y) to x
            180 -> (width - 1 - x) to (height - 1 - y)
            270 -> y to (width - 1 - x)
            else -> x to y
        }

        /** And back: where an upright picture's pixel came from in the frame. */
        fun captured(ux: Int, uy: Int, width: Int, height: Int, rotation: Int): Pair<Int, Int> = when (rotation) {
            90 -> uy to (height - 1 - ux)
            180 -> (width - 1 - ux) to (height - 1 - uy)
            270 -> (width - 1 - uy) to ux
            else -> ux to uy
        }

        /**
         * The person's mask, upright at [maskWidth] by [maskHeight], laid on
         * the grid of a frame of [width] by [height] captured at [rotation],
         * and its edge softened by a few cells (the web blurs its mask by
         * eight pixels at full size, `maskBlurRadius`, which is two here).
         */
        fun personOnGrid(mask: FloatArray, maskWidth: Int, maskHeight: Int, width: Int, height: Int, rotation: Int, gridWidth: Int, gridHeight: Int): FloatArray {
            val turned = rotation == 90 || rotation == 270
            val uprightWidth = if (turned) height else width
            val uprightHeight = if (turned) width else height
            val out = FloatArray(gridWidth * gridHeight)
            for (gy in 0 until gridHeight) {
                val y = minOf(gy * 4 + 2, height - 1)
                for (gx in 0 until gridWidth) {
                    val x = minOf(gx * 4 + 2, width - 1)
                    val (ux, uy) = upright(x, y, width, height, rotation)
                    val mx = (ux * maskWidth / uprightWidth).coerceIn(0, maskWidth - 1)
                    val my = (uy * maskHeight / uprightHeight).coerceIn(0, maskHeight - 1)
                    out[gy * gridWidth + gx] = mask[my * maskWidth + mx].coerceIn(0f, 1f)
                }
            }
            // A box of five cells, twice, across and down: a soft edge of
            // about two cells either side.
            val scratch = FloatArray(out.size)
            repeat(2) {
                box(out, scratch, gridWidth, gridHeight, across = true)
                box(scratch, out, gridWidth, gridHeight, across = false)
            }
            return out
        }

        private fun box(from: FloatArray, to: FloatArray, w: Int, h: Int, across: Boolean) {
            for (row in 0 until h) for (column in 0 until w) {
                var sum = 0f
                for (k in -2..2) {
                    val r = if (across) row else (row + k).coerceIn(0, h - 1)
                    val c = if (across) (column + k).coerceIn(0, w - 1) else column
                    sum += from[r * w + c]
                }
                to[row * w + column] = sum / 5f
            }
        }
    }
}
