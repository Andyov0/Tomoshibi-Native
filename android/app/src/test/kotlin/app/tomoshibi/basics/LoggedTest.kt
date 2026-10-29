package app.tomoshibi.basics

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * Who and where, in the log (Logged). A release build wrote the room's name
 * and every participant's signed identity to logcat; it writes a mark of
 * each instead, the same for the same thing, so a log still follows one
 * person in and out.
 */
class LoggedTest {
    private val identity = "tAndy!d3adbeefcafe"

    @Test
    fun aDebugBuildSaysWhoItIs() {
        assertEquals(identity, Logged.private(identity, debug = true))
    }

    @Test
    fun aReleaseBuildSaysAMarkInstead() {
        val mark = Logged.private(identity, debug = false)
        assertTrue(mark, Regex("#[0-9a-f]{8}").matches(mark))
        assertFalse(mark.contains("Andy"))
        assertEquals(mark, Logged.private(identity, debug = false))
        assertNotEquals(mark, Logged.private("team-room", debug = false))
    }
}
