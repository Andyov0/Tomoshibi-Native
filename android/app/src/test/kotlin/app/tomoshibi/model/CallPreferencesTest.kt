package app.tomoshibi.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The call's choices as the settings keep them: under the web's names and
 * with its values, a device picked over what a call picked meanwhile rather
 * than over the page's copy, and nothing written in a rehearsal. What these
 * guard is a choice made in the settings that a call reads differently, or
 * that wipes out one made in the call.
 */
class CallPreferencesTest {
    private val defaults = MemoryDefaults()

    @Test
    fun choicesAreKeptUnderTheWebsNamesWithItsValues() {
        val preferences = CallPreferences(keeps = true, defaults = defaults, deviceDefaults = defaults)
        assertEquals("auto", preferences.watchQuality)
        assertEquals("lossless", preferences.musicQuality)
        assertTrue(preferences.musicDucks)
        assertFalse(preferences.originalSound)

        preferences.watchQuality = "720p"
        preferences.musicQuality = "best"
        preferences.musicDucks = false
        preferences.originalSound = true
        assertEquals("720p", defaults.string("meet-live.watch-quality"))
        assertEquals("best", defaults.string("meet-live.music-quality"))
        assertEquals("off", defaults.string("meet-live.music-duck"))
        assertEquals("on", defaults.string("meet-live.original-sound"))

        // The defaults are kept as nothing, as the web keeps them.
        preferences.watchQuality = "auto"
        preferences.musicQuality = "lossless"
        preferences.musicDucks = true
        preferences.originalSound = false
        for (key in listOf("meet-live.watch-quality", "meet-live.music-quality", "meet-live.music-duck", "meet-live.original-sound")) {
            assertFalse("$key is still kept", defaults.has(key))
        }
    }

    @Test
    fun aValueTheWebWouldNotWriteReadsAsNothingChosen() {
        defaults.set("meet-live.watch-quality", "4k")
        defaults.set("meet-live.music-quality", "loud")
        defaults.set("meet-live.device-choice", """{"audioinput":"","videoinput":7,"audiooutput":"22","other":"x"}""")
        val preferences = CallPreferences(keeps = true, defaults = defaults, deviceDefaults = defaults)
        assertEquals("auto", preferences.watchQuality)
        assertEquals("lossless", preferences.musicQuality)
        assertEquals(mapOf("audiooutput" to "22"), preferences.devices)
    }

    @Test
    fun aDevicePickedHereKeepsTheOnePickedInTheCallMeanwhile() {
        val preferences = CallPreferences(keeps = true, defaults = defaults, deviceDefaults = defaults)
        preferences.choose("videoinput", "1")
        // The call remembers a headset, as DeviceMemory writes it.
        DeviceMemory(defaults).remember(DeviceMemory.Kind.Microphone, "31")
        preferences.choose("audiooutput", "32")
        assertEquals("""{"audioinput":"31","audiooutput":"32","videoinput":"1"}""", defaults.string("meet-live.device-choice"))
        assertEquals(mapOf("audioinput" to "31", "audiooutput" to "32", "videoinput" to "1"), preferences.devices)

        preferences.choose("videoinput", null)
        assertEquals("""{"audioinput":"31","audiooutput":"32"}""", defaults.string("meet-live.device-choice"))
        preferences.forgetDevices()
        assertNull(defaults.string("meet-live.device-choice"))
        assertTrue(preferences.devices.isEmpty())
    }

    @Test
    fun refreshingReadsWhatACallWroteAndWritesNothingBack() {
        val preferences = CallPreferences(keeps = true, defaults = defaults, deviceDefaults = defaults)
        defaults.set("meet-live.original-sound", "on")
        defaults.set("meet-live.device-choice", """{"audioinput":"31"}""")
        assertFalse(preferences.originalSound)
        preferences.refresh()
        assertTrue(preferences.originalSound)
        assertEquals(mapOf("audioinput" to "31"), preferences.devices)
        defaults.remove("meet-live.original-sound")
        preferences.refresh()
        assertFalse(preferences.originalSound)
        assertFalse(defaults.has("meet-live.original-sound"))
    }

    @Test
    fun aRehearsalShowsItsChoicesAndKeepsNone() {
        val preferences = CallPreferences(keeps = false, defaults = defaults, deviceDefaults = null)
        preferences.watchQuality = "1080p"
        preferences.musicDucks = false
        preferences.originalSound = true
        preferences.choose("audioinput", "31")
        assertEquals("1080p", preferences.watchQuality)
        assertEquals(mapOf("audioinput" to "31"), preferences.devices)
        assertFalse(defaults.has("meet-live.watch-quality"))
        assertFalse(defaults.has("meet-live.music-duck"))
        assertFalse(defaults.has("meet-live.original-sound"))
        assertFalse(defaults.has("meet-live.device-choice"))
    }
}
