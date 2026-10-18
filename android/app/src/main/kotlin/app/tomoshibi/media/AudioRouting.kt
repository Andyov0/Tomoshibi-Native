package app.tomoshibi.media

import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build

/**
 * A microphone or a pair of speakers the person can choose: [name] is the
 * system's (the product name the device reports, which may be empty), and
 * [type] is AudioDeviceInfo's, for a panel to name the kind in the reader's
 * language where the device gives no name of its own.
 */
data class AudioDevice(val id: String, val name: String, val type: Int = 0)

/**
 * Where a call's sound goes in and out: the system's audio mode and which
 * device is used for each direction.
 *
 * TODO(media): choosing devices properly. What is here is the least a working
 * call needs: the list of devices for a panel, and a phone's sound on its
 * loudspeaker rather than its earpiece while it records for a call, unless a
 * headset is plugged in or paired. Still to do: following devices that come
 * and go (AudioDeviceCallback, the web's `devicechange`), Bluetooth SCO and
 * LE audio, a wired headset's own microphone, and remembering a device by
 * something more lasting than AudioDeviceInfo's id, which the system issues
 * afresh each time a device connects, so a choice remembered by it
 * (`meet-live.device-choice`) only comes back within one connection.
 */
interface AudioRouting {
    fun devices(input: Boolean): List<AudioDevice>

    /** Whether choosing a device can take now. */
    val choosable: Boolean

    /** The system's device behind an id this lists, for setPreferredDevice. */
    fun info(id: String?, input: Boolean): AudioDeviceInfo?

    /** Whether the device records without any processing at all, for the web's "original sound". */
    val offersUnprocessed: Boolean

    /**
     * The call starts or stops recording a voice for a call: the system's
     * communication mode, under which its echo canceller hears what is
     * played, and where the sound goes meanwhile.
     */
    fun communicating(on: Boolean)
}

class SystemAudioRouting(context: Context) : AudioRouting {
    private val manager = context.getSystemService(AudioManager::class.java)
    private var inCommunication = false

    override fun devices(input: Boolean): List<AudioDevice> {
        val manager = manager ?: return emptyList()
        val flag = if (input) AudioManager.GET_DEVICES_INPUTS else AudioManager.GET_DEVICES_OUTPUTS
        return manager.getDevices(flag)
            .filter { if (input) it.isSource else it.isSink }
            // The telephony and remote-submix endpoints are the system's, not
            // something a person picks.
            .filter { it.type != AudioDeviceInfo.TYPE_TELEPHONY && it.type != AudioDeviceInfo.TYPE_REMOTE_SUBMIX }
            .map { AudioDevice(it.id.toString(), it.productName?.toString().orEmpty(), it.type) }
    }

    override val choosable: Boolean get() = true

    override val offersUnprocessed: Boolean
        get() = manager?.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true"

    override fun info(id: String?, input: Boolean): AudioDeviceInfo? {
        val manager = manager ?: return null
        val wanted = id?.toIntOrNull() ?: return null
        val flag = if (input) AudioManager.GET_DEVICES_INPUTS else AudioManager.GET_DEVICES_OUTPUTS
        return manager.getDevices(flag).firstOrNull { it.id == wanted }
    }

    @Suppress("DEPRECATION")
    override fun communicating(on: Boolean) {
        val manager = manager ?: return
        if (on == inCommunication) return
        inCommunication = on
        if (on) {
            manager.mode = AudioManager.MODE_IN_COMMUNICATION
            // A video call is held at arm's length: in communication mode a
            // phone sends its sound to the earpiece unless told otherwise,
            // and a call that can only be heard with the phone at an ear is a
            // call nobody hears.
            if (!headset()) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    manager.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                        ?.let { manager.setCommunicationDevice(it) }
                } else {
                    manager.isSpeakerphoneOn = true
                }
            }
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) manager.clearCommunicationDevice() else manager.isSpeakerphoneOn = false
            manager.mode = AudioManager.MODE_NORMAL
        }
    }

    private fun headset(): Boolean {
        val manager = manager ?: return false
        val worn = setOf(
            AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES, AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_BLE_HEADSET,
        )
        return manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any { it.type in worn }
    }
}

/** Routing for a call that plays nothing (a muted test) or runs where there is no audio service. */
object NoAudioRouting : AudioRouting {
    override fun devices(input: Boolean): List<AudioDevice> = emptyList()
    override val choosable: Boolean get() = false
    override val offersUnprocessed: Boolean get() = false
    override fun info(id: String?, input: Boolean): AudioDeviceInfo? = null
    override fun communicating(on: Boolean) {}
}
