package app.tomoshibi.media

import android.media.AudioDeviceInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/*
 * Which way out a call's sound takes. What these guard is a video call heard
 * only at the earpiece (nobody holds a video call to an ear), a headset
 * plugged in and ignored, a choice that outlives its device, and a headset
 * forgotten because the system gave it a new id when it reconnected.
 */
class AudioRoutingTest {
    private val earpiece = AudioDevice("earpiece", "", AudioDeviceInfo.TYPE_BUILTIN_EARPIECE)
    private val speaker = AudioDevice("speaker", "", AudioDeviceInfo.TYPE_BUILTIN_SPEAKER)
    private val wired = AudioDevice("wired", "", AudioDeviceInfo.TYPE_WIRED_HEADSET)
    private val buds = AudioDevice("bluetooth:AA:BB", "Buds", AudioDeviceInfo.TYPE_BLUETOOTH_SCO)

    @Test
    fun aCallWithNothingWornIsOnTheLoudspeakerNotTheEarpiece() {
        val rule = RouteRule()
        val routes = listOf(earpiece, speaker)
        rule.present(routes)
        assertEquals(speaker, rule.resolve(routes))
    }

    @Test
    fun aHeadsetAlreadyThereIsTaken() {
        val rule = RouteRule()
        val routes = listOf(earpiece, speaker, buds)
        rule.present(routes)
        assertEquals(buds, rule.resolve(routes))
    }

    @Test
    fun aHeadsetPluggedInIsTakenOverAChoiceMadeBefore() {
        val rule = RouteRule()
        rule.present(listOf(earpiece, speaker, buds))
        rule.choose("earpiece")
        assertEquals(earpiece, rule.resolve(listOf(earpiece, speaker, buds)))
        val now = listOf(earpiece, speaker, buds, wired)
        rule.present(now)
        assertNull("plugging in is the person's latest choice", rule.chosen)
        assertEquals(wired, rule.resolve(now))
    }

    @Test
    fun theNewestHeadsetWinsAndTheOneBeforeComesBackWhenItGoes() {
        val rule = RouteRule()
        rule.present(listOf(speaker, wired))
        val both = listOf(speaker, wired, buds)
        rule.present(both)
        assertEquals(buds, rule.resolve(both))
        val budsGone = listOf(speaker, wired)
        rule.present(budsGone)
        assertEquals(wired, rule.resolve(budsGone))
    }

    @Test
    fun aChoiceWhoseDeviceWentIsDropped() {
        val rule = RouteRule()
        val routes = listOf(speaker, buds)
        rule.present(routes)
        rule.choose("speaker")
        assertEquals(speaker, rule.resolve(routes))
        // The speaker cannot go, but a chosen headset can.
        rule.choose("bluetooth:AA:BB")
        rule.present(listOf(speaker))
        assertNull(rule.chosen)
        assertEquals(speaker, rule.resolve(listOf(speaker)))
    }

    @Test
    fun theListIsWhatAPersonPicksOncePerDevice() {
        val found = listOf(
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_EARPIECE, "", "SM-F956B", source = false, sink = true),
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, "", "SM-F956B", source = false, sink = true),
            // The loudspeaker kept for alarms, and the echo canceller's copy.
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE, "", "SM-F956B", source = false, sink = true),
            AudioDevice.Found(28, "", "", source = false, sink = true),
            AudioDevice.Found(AudioDeviceInfo.TYPE_TELEPHONY, "", "", source = false, sink = true),
            // One headset, listed for music and for calls.
            AudioDevice.Found(AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, "AA:BB", "Buds", source = false, sink = true),
            AudioDevice.Found(AudioDeviceInfo.TYPE_BLUETOOTH_SCO, "AA:BB", "Buds", source = false, sink = true),
            // A second headset of the same name is another device.
            AudioDevice.Found(AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, "CC:DD", "Buds", source = false, sink = true),
            AudioDevice.Found(AudioDeviceInfo.TYPE_HDMI, "", "Living room TV", source = false, sink = true),
        )
        val out = AudioDevice.listed(found, input = false)
        assertEquals(listOf("earpiece", "speaker", "bluetooth:AA:BB", "bluetooth:CC:DD", "${AudioDeviceInfo.TYPE_HDMI}:Living room TV"), out.map { it.id })
        assertEquals("the call's own profile stands for the headset", AudioDeviceInfo.TYPE_BLUETOOTH_SCO, out[2].type)
        assertEquals("the phone's own parts are named by their kind, not its model", "", out[1].name)
        assertEquals("Buds", out[2].name)
    }

    @Test
    fun thePhonesMicrophonesAreOneChoice() {
        val found = listOf(
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_MIC, "bottom", "SM-F956B", source = true, sink = false),
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_MIC, "back", "SM-F956B", source = true, sink = false),
            AudioDevice.Found(AudioDeviceInfo.TYPE_FM_TUNER, "", "", source = true, sink = false),
            AudioDevice.Found(AudioDeviceInfo.TYPE_BLUETOOTH_SCO, "AA:BB", "Buds", source = true, sink = false),
            AudioDevice.Found(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, "", "SM-F956B", source = false, sink = true),
        )
        assertEquals(listOf("microphone", "bluetooth:AA:BB"), AudioDevice.listed(found, input = true).map { it.id })
    }

    @Test
    fun aDeviceIsKnownByWhatItIsNotByTheSystemsPassingId() {
        assertEquals("bluetooth:AA:BB:CC", AudioDevice.lastingId(AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, "AA:BB:CC", "Buds"))
        assertEquals("bluetooth:AA:BB:CC", AudioDevice.lastingId(AudioDeviceInfo.TYPE_BLUETOOTH_SCO, "AA:BB:CC", "Buds"))
        assertEquals("bluetooth:Buds", AudioDevice.lastingId(AudioDeviceInfo.TYPE_BLUETOOTH_SCO, "", "Buds"))
        assertEquals("speaker", AudioDevice.lastingId(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, "", "SM-F956B"))
        assertEquals("usb:Yeti", AudioDevice.lastingId(AudioDeviceInfo.TYPE_USB_HEADSET, "card=1", "Yeti"))
        assertEquals(AudioDevice.Kind.Bluetooth, AudioDevice.kindOf(AudioDevice.TYPE_BLE_HEADSET))
        assertEquals(AudioDevice.Kind.Other, AudioDevice.kindOf(AudioDeviceInfo.TYPE_TELEPHONY))
    }
}
