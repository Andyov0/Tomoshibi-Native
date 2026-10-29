package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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

    /*
     * The window's size follows its shape and nothing else an app can say, so
     * a call with no picture asks for the square one, the smallest: it was
     * sixteen by nine around a single initial ("the picture in picture is too big", 2026-10-19).
     * What has a picture keeps the picture's shape, the video watched together
     * and a shared screen their wide one.
     */
    @Test
    fun aCallWithNoPictureAsksForTheSmallestWindow() {
        assertEquals(1 to 1, CallWindow.windowShape(null))
        assertEquals(1778 to 1000, CallWindow.windowShape(1778 to 1000))
        assertEquals(1920 to 1080, CallWindow.windowShape(1920 to 1080))
        assertEquals(720 to 960, CallWindow.windowShape(720 to 960))
    }

    /*
     * A control on the activity's intent, which any app can start: only the
     * notification's Unmute, carrying this process's proof, is taken. An app
     * that started the activity with Unmute or Leave on its intent turned on
     * the microphone of a muted call, or hung it up.
     */
    @Test
    fun onlyTheAppsOwnUnmuteIsTakenFromTheActivitysIntent() {
        val proof = CallWindow.controlProof
        assertEquals(48, proof.length)
        assertEquals(CallKeeper.Control.Unmute, CallWindow.admitted("Unmute", proof, proof))
        assertNull(CallWindow.admitted("Unmute", null, proof))
        assertNull(CallWindow.admitted("Unmute", "", proof))
        assertNull(CallWindow.admitted("Unmute", proof.reversed(), proof))
        // The rest go to the receiver nobody else can reach, never here,
        // proof or none.
        assertNull(CallWindow.admitted("Leave", proof, proof))
        assertNull(CallWindow.admitted("Mute", proof, proof))
        assertNull(CallWindow.admitted("StopSharing", proof, proof))
    }
}
