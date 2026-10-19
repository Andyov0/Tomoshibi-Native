package app.tomoshibi.basics

import org.junit.Assert.assertEquals
import org.junit.Test

/*
 * When a camera fills its tile and when it is shown whole; the Apple apps'
 * FramingTests, case for case.
 */
class FramingTest {
    private fun fills(picture: Pair<Int, Int>, view: Pair<Int, Int>) =
        Framing.fills(picture.first.toFloat(), picture.second.toFloat(), view.first.toFloat(), view.second.toFloat())

    @Test
    fun aCameraOnItsSideIsShownWholeOnAnUprightScreen() {
        assertEquals(false, fills(640 to 480, 300 to 400))
        // A Fold6 opened, and its cover screen.
        assertEquals(false, fills(640 to 480, 1856 to 2160))
        assertEquals(false, fills(640 to 480, 968 to 2376))
        // A telephone held upright, in a window of a computer.
        assertEquals(false, fills(720 to 960, 1440 to 900))
    }

    @Test
    fun aCameraTheSameWayRoundFills() {
        assertEquals(true, fills(720 to 960, 1856 to 2160))
        assertEquals(true, fills(720 to 960, 968 to 2376))
        assertEquals(true, fills(720 to 960, 1080 to 2340))
        assertEquals(true, fills(720 to 1280, 968 to 2376))
        assertEquals(true, fills(1280 to 720, 1440 to 900))
    }

    @Test
    fun aNearShapeFillsWhicheverWayRound() {
        assertEquals(true, fills(480 to 480, 300 to 400))
        assertEquals(true, fills(640 to 480, 1600 to 900))
    }

    @Test
    fun aSquareIsNeitherWayRound() {
        assertEquals(false, fills(1280 to 720, 500 to 500))
    }

    @Test
    fun tooLongEvenTheSameWayRoundIsShownWhole() {
        assertEquals(false, fills(720 to 1280, 400 to 1600))
    }

    @Test
    fun aShapeNotYetKnownFills() {
        assertEquals(true, fills(0 to 0, 300 to 400))
        assertEquals(true, fills(640 to 480, 0 to 0))
    }
}
