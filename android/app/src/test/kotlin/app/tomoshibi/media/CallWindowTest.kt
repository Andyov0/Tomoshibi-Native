package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Test

/*
 * The small window's shape. Android refuses an aspect ratio beyond 2.39 to 1
 * either way with an exception, and a phone's screen shared upright is
 * 1080 by 2340: asked for as it is, the window's settings would be refused
 * and it would not open at all.
 */
class CallWindowTest {
    @Test
    fun aPictureIsShownInItsOwnShape() {
        assertEquals(1280 to 720, CallWindow.aspect(1280, 720))
        assertEquals(1080 to 2340, CallWindow.aspect(1080, 2340))
    }

    @Test
    fun aShapeTooLongForASmallWindowIsHeldToTheLongestAllowed() {
        assertEquals(238 to 100, CallWindow.aspect(3000, 1000))
        assertEquals(100 to 238, CallWindow.aspect(1000, 3000))
    }

    @Test
    fun noPictureYetIsSixteenByNine() {
        assertEquals(16 to 9, CallWindow.aspect(0, 0))
    }
}
