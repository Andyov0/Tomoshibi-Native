package app.tomoshibi.media

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The arithmetic of the background blur. What these guard is a person blurred
 * along with their room, a room left sharp, a mask laid on the frame turned
 * the wrong way (a phone held upright captures on its side), and the radius
 * not following the picture's size.
 */
class BlurMathTest {
    private val width = 64
    private val height = 36

    /** A frame of fine vertical stripes in luma, and flat chroma. */
    private fun stripes(): Triple<ByteArray, ByteArray, ByteArray> {
        val y = ByteArray(width * height) { i -> if ((i % width) % 2 == 0) 40 else 200.toByte() }
        val u = ByteArray((width / 2) * (height / 2)) { 100 }
        val v = ByteArray((width / 2) * (height / 2)) { 150.toByte() }
        return Triple(y, u, v)
    }

    private fun contrast(y: ByteArray, from: Int, until: Int): Int {
        var most = 0
        for (row in 0 until height) for (column in from until until - 1) {
            val a = y[row * width + column].toInt() and 0xff
            val b = y[row * width + column + 1].toInt() and 0xff
            most = maxOf(most, kotlin.math.abs(a - b))
        }
        return most
    }

    @Test
    fun wherethereIsAllPersonTheFrameIsKeptAsItIs() {
        val (y, u, v) = stripes()
        val before = y.copyOf()
        val math = BlurMath(width, height)
        math.blur(y, u, v, FloatArray(math.gridWidth * math.gridHeight) { 1f })
        assertArrayEquals(before, y)
    }

    @Test
    fun withNobodyFoundYetTheWholeFrameIsBlurred() {
        val (y, u, v) = stripes()
        BlurMath(width, height).blur(y, u, v, null)
        assertTrue("stripes 160 apart are smoothed to ${contrast(y, 0, width)}", contrast(y, 0, width) < 20)
        // Flat chroma stays as it was.
        assertTrue(u.all { (it.toInt() and 0xff) in 99..101 })
    }

    @Test
    fun theBackgroundIsBlurredAndThePersonKept() {
        val (y, u, v) = stripes()
        val math = BlurMath(width, height)
        // The person on the left half of the grid.
        val person = FloatArray(math.gridWidth * math.gridHeight) { i -> if (i % math.gridWidth < math.gridWidth / 2) 1f else 0f }
        math.blur(y, u, v, person)
        assertEquals("the person's side keeps its stripes", 160, contrast(y, 0, width / 2 - 8))
        assertTrue("the room's side is smoothed", contrast(y, width / 2 + 8, width) < 20)
    }

    @Test
    fun theRadiusFollowsTheSize() {
        // 12 at 720 rows, a quarter of it on the quarter grid: 3 cells at
        // 720, 1.5 at 360, out to three sigma.
        assertEquals(19, BlurMath.gaussian(12.0 * 720 / 720 / 4).size)
        assertEquals(11, BlurMath.gaussian(12.0 * 360 / 720 / 4).size)
        assertEquals(1f, BlurMath.gaussian(1.5).sum(), 1e-5f)
    }

    @Test
    fun turningUprightAndBackComesBackToTheSamePixel() {
        for (rotation in listOf(0, 90, 180, 270)) {
            for ((x, y) in listOf(0 to 0, 63 to 0, 0 to 35, 17 to 9)) {
                val (ux, uy) = BlurMath.upright(x, y, width, height, rotation)
                assertEquals("rotation $rotation", x to y, BlurMath.captured(ux, uy, width, height, rotation))
            }
        }
        // A phone held upright captures on its side, turned 90 degrees: the
        // frame's top left is the upright picture's top right.
        assertEquals((height - 1) to 0, BlurMath.upright(0, 0, width, height, 90))
    }

    @Test
    fun anUprightMaskIsLaidOnTheFrameItWasFoundIn() {
        // Upright, the person fills the top half of the picture.
        val turned = 90
        val uw = height
        val uh = width
        val mask = FloatArray(uw * uh) { i -> if (i / uw < uh / 2) 1f else 0f }
        val math = BlurMath(width, height)
        val grid = BlurMath.personOnGrid(mask, uw, uh, width, height, turned, math.gridWidth, math.gridHeight)
        // The upright top is the captured frame's left, for a turn of 90.
        val left = grid[(math.gridHeight / 2) * math.gridWidth + 1]
        val right = grid[(math.gridHeight / 2) * math.gridWidth + math.gridWidth - 2]
        assertTrue("left is person ($left)", left > 0.9f)
        assertTrue("right is room ($right)", right < 0.1f)
    }
}
