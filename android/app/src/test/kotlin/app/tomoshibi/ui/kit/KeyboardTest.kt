package app.tomoshibi.ui.kit

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * Whose a scroll is (ScrollMaker): only one somebody makes puts the keyboard
 * away. Compose calls the scroll that brings a focused field above the
 * keyboard user input as well, and taken at its word the keyboard put
 * itself away a second after it came up: the sign-in under a notice and an
 * update, and the start sheet over a camera picture, took no typing at all.
 */
class KeyboardTest {
    @Test
    fun aFingerOnTheScreenIsSomebodyScrolling() {
        val maker = ScrollMaker()
        maker.saw(wheel = false, pressed = true, at = 1_000)
        assertTrue(maker.somebodyScrolls(1_050))
    }

    @Test
    fun theFieldBroughtAboveTheKeyboardIsNobodys() {
        val maker = ScrollMaker()
        assertFalse(maker.somebodyScrolls(0))
        // The tap on the field, down and up, then the keyboard and the scroll.
        maker.saw(wheel = false, pressed = true, at = 1_000)
        maker.saw(wheel = false, pressed = false, at = 1_080)
        assertFalse(maker.somebodyScrolls(1_400))
    }

    @Test
    fun aWheelIsSomebodysForAMomentAfterItTurns() {
        val maker = ScrollMaker()
        maker.saw(wheel = true, pressed = false, at = 2_000)
        assertTrue(maker.somebodyScrolls(2_000 + ScrollMaker.WHEEL_MILLIS))
        assertFalse(maker.somebodyScrolls(2_001 + ScrollMaker.WHEEL_MILLIS))
    }
}
