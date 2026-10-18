package app.tomoshibi.model

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The sound panel's record, held to what the web writes and reads under the
 * same name (web/src/live/hearing.ts and hearing.test.ts), as the Apple apps'
 * HearingTests hold it. What these guard is a setting quietly not being what
 * somebody chose: a guest's block surviving onto whoever is issued that
 * identity next, a hand-edited volume of 3 played three times too loud, a book
 * the web wrote that the app cannot read.
 */
class HearingTest {
    private val proven = HearingFiling("ABCDEFGHIJ", keeps = true)
    private val guest = HearingFiling("g0123456789abcdef0123456789abcdef", keeps = false)

    @Test
    fun everybodyIsAsSentUntilSomebodySaysOtherwise() {
        val book = HearingBook()
        for (sound in Sound.entries) assertEquals(HearingSetting.asSent, book.setting(proven, sound))
        assertNull(book.stored)
    }

    @Test
    fun aProvenSettingIsKeptInTheWebsShape() {
        val book = HearingBook()
        assertTrue(book.set(proven, Sound.Music, HearingSetting(0.25, false)))
        assertTrue(book.set(proven, Sound.Voice, HearingSetting(1.0, true)))
        val stored = book.stored!!
        // Byte for byte what the web's JSON.stringify writes for the same book.
        assertEquals("""{"ABCDEFGHIJ/music":{"volume":0.25,"blocked":false},"ABCDEFGHIJ/voice":{"volume":1,"blocked":true}}""", stored)
        val parsed = JSONObject(stored)
        assertEquals(0.25, parsed.getJSONObject("ABCDEFGHIJ/music").getDouble("volume"), 0.0)
        // And read back as it was written, by the next launch.
        val again = HearingBook(stored)
        assertEquals(HearingSetting(0.25, false), again.setting(proven, Sound.Music))
        assertEquals(HearingSetting(1.0, true), again.setting(proven, Sound.Voice))
        assertEquals(HearingSetting.asSent, again.setting(proven, Sound.Screen))
    }

    @Test
    fun aVolumeIsWrittenInItsShortestForm() {
        val book = HearingBook()
        book.set(proven, Sound.Voice, HearingSetting(0.35, false))
        assertEquals("""{"ABCDEFGHIJ/voice":{"volume":0.35,"blocked":false}}""", book.stored)
        book.set(proven, Sound.Voice, HearingSetting(0.0, false))
        assertEquals("""{"ABCDEFGHIJ/voice":{"volume":0,"blocked":false}}""", book.stored)
        // As JavaScript writes it, not as Java's 5.0E-4.
        book.set(proven, Sound.Voice, HearingSetting(0.0005, false))
        assertEquals("""{"ABCDEFGHIJ/voice":{"volume":0.0005,"blocked":false}}""", book.stored)
    }

    @Test
    fun aGuestsSettingLastsTheCallAndIsNeverWritten() {
        val book = HearingBook()
        assertFalse(book.set(guest, Sound.Voice, HearingSetting(0.0, true)))
        assertEquals(HearingSetting(0.0, true), book.setting(guest, Sound.Voice))
        assertNull(book.stored)
    }

    @Test
    fun turningSomebodyBackUpRemovesThemFromStorage() {
        val book = HearingBook()
        book.set(proven, Sound.Voice, HearingSetting(0.5, false))
        assertNotNull(book.stored)
        assertTrue(book.set(proven, Sound.Voice, HearingSetting.asSent))
        assertNull(book.stored)
    }

    @Test
    fun whatTheWebWroteIsReadAndWhatWasEditedByHandIsClamped() {
        val book = HearingBook(
            """
            {"AAAAAAAAAA/voice":{"volume":3,"blocked":false},
             "BBBBBBBBBB/voice":{"volume":-1},
             "CCCCCCCCCC/voice":{"volume":"loud","blocked":1},
             "DDDDDDDDDD/voice":{"volume":false,"blocked":true},
             "EEEEEEEEEE/voice":{"volume":0.4,"blocked":true}}
            """.trimIndent(),
        )
        fun setting(key: String) = book.setting(HearingFiling(key, keeps = true), Sound.Voice)
        assertEquals(HearingSetting(1.0, false), setting("AAAAAAAAAA"))
        assertEquals(HearingSetting(0.0, false), setting("BBBBBBBBBB"))
        // A string is not a volume and a 1 is not a block, as `=== true` has it.
        assertEquals(HearingSetting(1.0, false), setting("CCCCCCCCCC"))
        // JSON's false is not a volume of nought either.
        assertEquals(HearingSetting(1.0, true), setting("DDDDDDDDDD"))
        assertEquals(HearingSetting(0.4, true), setting("EEEEEEEEEE"))
    }

    @Test
    fun unreadableStorageIsNothingDecided() {
        assertEquals(HearingBook(), HearingBook("not json"))
        assertEquals(HearingBook(), HearingBook("[1,2]"))
        assertEquals(HearingBook(), HearingBook(null))
    }

    @Test
    fun aVolumeOutsideTheRangeIsClampedOnTheWayIn() {
        val book = HearingBook()
        book.set(proven, Sound.Screen, HearingSetting(7.0, false))
        // Clamped to 1 and not blocked: as sent, so nothing is kept at all.
        assertEquals(HearingSetting.asSent, book.setting(proven, Sound.Screen))
        assertNull(book.stored)
    }

    @Test
    fun silencedIsABlockOrNothingToHear() {
        assertTrue(HearingSetting(0.0, false).silenced)
        assertTrue(HearingSetting(1.0, true).silenced)
        assertFalse(HearingSetting(0.05, false).silenced)
    }
}
