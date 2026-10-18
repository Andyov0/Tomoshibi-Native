package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Test

/*
 * The size a remote picture is asked of the server at. What this guards is
 * the owner's phone growing hot on a shared screen, sent the 1080 rung for a
 * stage a third that size because it asked at three pixels a point.
 */
class PictureDensityTest {
    @Test
    fun aPhoneAsksAtTwicePointsAtMost() {
        // A stage of 440 by 285 points on a phone of density three.
        assertEquals(880 to 570, PictureDensity.asked(1320, 855, 3f, phone = true, hot = false))
        // A phone already at two or less asks for what it draws.
        assertEquals(800 to 450, PictureDensity.asked(800, 450, 2f, phone = true, hot = false))
    }

    @Test
    fun aTabletKeepsTheScreensDensity() {
        assertEquals(1320 to 855, PictureDensity.asked(1320, 855, 3f, phone = false, hot = false))
    }

    @Test
    fun anythingHotAsksAtItsPoints() {
        assertEquals(440 to 285, PictureDensity.asked(1320, 855, 3f, phone = true, hot = true))
        assertEquals(440 to 285, PictureDensity.asked(1320, 855, 3f, phone = false, hot = true))
    }
}
