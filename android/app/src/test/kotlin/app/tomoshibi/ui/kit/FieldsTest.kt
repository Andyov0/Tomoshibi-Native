package app.tomoshibi.ui.kit

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The kit's fields count a limit as a browser counts `maxLength`, in UTF-16
 * units, and never keep half of a character the limit falls inside.
 */
class FieldsTest {
    @Test
    fun aLimitNeverCutsACharacterInTwo() {
        assertEquals("abc", clampUnits("abcdef", 3))
        assertEquals("short", clampUnits("short", 10))
        // One short of the limit, then an emoji of two UTF-16 units.
        assertEquals("a".repeat(9), clampUnits("a".repeat(9) + "\uD83D\uDE00", 10))
    }
}
