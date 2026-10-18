package app.tomoshibi.model

import app.tomoshibi.media.ShareRates
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The device memory, the share's frame rates and the microphone meter, held
 * to what the web writes, reads and offers under the same names
 * (web/src/live/preferences.ts, live/sound.ts, ShareButton.tsx, MicLevel.tsx),
 * as the Apple apps' DevicesTests hold them. What these guard is a call that
 * starts other than as it was left: a camera on that was left off, a garbled
 * record read as a silent microphone, a test run that rewrote the owner's own
 * choices.
 */
class DeviceMemoryTest {
    private val defaults = MemoryDefaults()

    @Test
    fun startsWithTheMicrophoneOnAndTheCameraOffUntilToldOtherwise() {
        val memory = DeviceMemory(defaults)
        assertEquals(DeviceMemory.Joining(camera = false, microphone = true), memory.joining)
        memory.remember(DeviceMemory.Joining(camera = true, microphone = false))
        // Byte for byte what the web's JSON.stringify writes.
        assertEquals("""{"camera":true,"microphone":false}""", defaults.string("meet-live.devices"))
        assertEquals(DeviceMemory.Joining(camera = true, microphone = false), DeviceMemory(defaults).joining)
    }

    @Test
    fun readsWhatItDoesNotRecogniseAsTheDefaultsOneHalfAtATime() {
        val memory = DeviceMemory(defaults)
        for (garbage in listOf("", "nope", "[]", "null", "42", """{"camera":"yes","microphone":0}""")) {
            defaults.set("meet-live.devices", garbage)
            assertEquals(garbage, DeviceMemory.Joining.first, memory.joining)
        }
        defaults.set("meet-live.devices", """{"camera":true}""")
        assertEquals(DeviceMemory.Joining(camera = true, microphone = true), memory.joining)
        // A 1 is a number, not a yes: only a boolean counts, as on the web.
        defaults.set("meet-live.devices", """{"camera":1,"microphone":false}""")
        assertEquals(DeviceMemory.Joining(camera = false, microphone = false), memory.joining)
    }

    @Test
    fun remembersEachKindBesideTheOthersInTheWebsShape() {
        val memory = DeviceMemory(defaults)
        assertNull(memory.choice(DeviceMemory.Kind.Microphone))
        memory.remember(DeviceMemory.Kind.Microphone, "17")
        memory.remember(DeviceMemory.Kind.Camera, "1")
        memory.remember(DeviceMemory.Kind.Microphone, "usb/headset:2")
        assertEquals("usb/headset:2", memory.choice(DeviceMemory.Kind.Microphone))
        assertEquals("1", memory.choice(DeviceMemory.Kind.Camera))
        assertNull(memory.choice(DeviceMemory.Kind.Speakers))
        // Sorted, and a slash written as it is rather than as org.json's \/.
        assertEquals("""{"audioinput":"usb/headset:2","videoinput":"1"}""", defaults.string("meet-live.device-choice"))
    }

    @Test
    fun keepsOnlyWhatCouldBeADevice() {
        val memory = DeviceMemory(defaults)
        defaults.set("meet-live.device-choice", """{"audioinput":"","videoinput":42,"audiooutput":"${"x".repeat(257)}"}""")
        for (kind in DeviceMemory.Kind.entries) assertNull(kind.key, memory.choice(kind))
        memory.remember(DeviceMemory.Kind.Speakers, "y".repeat(257))
        assertNull(memory.choice(DeviceMemory.Kind.Speakers))
        defaults.set("meet-live.device-choice", "[1,2]")
        assertNull(memory.choice(DeviceMemory.Kind.Microphone))
    }

    @Test
    fun asksForARememberedDeviceOnlyWhereItIsStillThere() {
        val memory = DeviceMemory(defaults)
        memory.remember(DeviceMemory.Kind.Camera, "desk")
        assertEquals("desk", memory.restorable(DeviceMemory.Kind.Camera, listOf("laptop", "desk")))
        assertNull(memory.restorable(DeviceMemory.Kind.Camera, listOf("laptop")))
        assertNull(memory.restorable(DeviceMemory.Kind.Camera, emptyList()))
        // Still written down for the day it is plugged back in.
        assertEquals("desk", memory.choice(DeviceMemory.Kind.Camera))
    }

    @Test
    fun originalSoundAndBlurAreOnOnlyWhereTheyWereAskedFor() {
        val memory = DeviceMemory(defaults)
        assertFalse(memory.originalSound)
        memory.remember(originalSound = true)
        assertEquals("on", defaults.string("meet-live.original-sound"))
        assertTrue(memory.originalSound)
        memory.remember(originalSound = false)
        // Off is written as nothing, which older builds read the same way.
        assertFalse(defaults.has("meet-live.original-sound"))
        defaults.set("meet-live.original-sound", "yes")
        assertFalse(memory.originalSound)

        memory.rememberBlur(true)
        assertEquals("on", defaults.string("meet-live.blur"))
        memory.rememberBlur(false)
        assertFalse(defaults.has("meet-live.blur"))
    }

    @Test
    fun aRehearsalKeepsNothingAndRemembersNothing() {
        defaults.set("meet-live.devices", """{"camera":true,"microphone":false}""")
        val memory = DeviceMemory.nothing
        assertEquals(DeviceMemory.Joining.first, memory.joining)
        memory.remember(DeviceMemory.Joining(camera = true, microphone = true))
        memory.remember(DeviceMemory.Kind.Camera, "desk")
        memory.remember(originalSound = true)
        assertNull(memory.choice(DeviceMemory.Kind.Camera))
        assertFalse(memory.originalSound)
        assertEquals("""{"camera":true,"microphone":false}""", defaults.string("meet-live.devices"))
    }

    @Test
    fun offersOnlyTheFrameRatesTheChosenSizeCanCarry() {
        assertEquals(listOf(15, 30, 60, 120, 240), ShareRates.allowed(1080))
        assertEquals(listOf(15, 30, 60, 120), ShareRates.allowed(1440))
        assertEquals(listOf(15, 30, 60), ShareRates.allowed(2160))
        assertEquals(listOf(60), ShareRates.allowed(0))
    }

    @Test
    fun doesNotCarryAFastRateOntoASizeThatCannotTakeIt() {
        assertEquals(60, ShareRates.clamp(240, 2160))
        assertEquals(120, ShareRates.clamp(240, 1440))
        assertEquals(30, ShareRates.clamp(30, 2160))
        assertEquals(240, ShareRates.clamp(240, 1080))
        assertEquals(240, ShareRates.clamp(45, 1080))
        assertEquals(60, ShareRates.clamp(15, 0))
    }

    @Test
    fun theMeterPutsAVoiceHighAndAQuietRoomBelowTheHeardLine() {
        assertEquals(0.0, MicrophoneLevel.shown(0.0), 0.0)
        assertEquals(1.0, MicrophoneLevel.shown(1.0), 0.0)
        // Speech at about -26 dBFS.
        assertTrue(MicrophoneLevel.shown(0.05) > 0.5)
        // A quiet room after voice processing, about -66 dBFS.
        assertTrue(MicrophoneLevel.shown(0.0005) < MicrophoneLevel.HEARD)
        assertTrue(MicrophoneLevel.shown(0.003) > MicrophoneLevel.HEARD)
    }
}
