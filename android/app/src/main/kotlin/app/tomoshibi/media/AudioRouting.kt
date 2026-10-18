package app.tomoshibi.media

import android.content.Context
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log

private const val TAG = "Tomoshibi"

/**
 * A microphone or a way out for the call's sound that the person can choose.
 *
 * [id] lasts: it names the kind of device and, for one that comes and goes,
 * the device itself (a Bluetooth headset by its address, a USB one by its
 * name), so a choice kept under the web's `meet-live.device-choice` finds the
 * same headset in the next call. AudioDeviceInfo's own id was used before,
 * and the system issues that afresh each time a device connects, so a
 * remembered headset was forgotten as soon as it was switched off.
 *
 * [name] is the device's own where it has one worth showing (a headset's,
 * a USB microphone's), and empty for the phone's own parts, whose product
 * name is the phone's model ("SM-F956B" says nothing about which is the
 * loudspeaker): a panel names those by [kind] in the reader's language, and
 * numbers two of the same name. [type] is AudioDeviceInfo's, of the device
 * that stands for the route.
 */
data class AudioDevice(val id: String, val name: String, val type: Int = 0) {
    enum class Kind { Earpiece, Speaker, Wired, Usb, Bluetooth, Microphone, Other }

    val kind: Kind get() = kindOf(type)

    /** Something worn: what a call switches to when it arrives. */
    val worn: Boolean get() = kind == Kind.Wired || kind == Kind.Usb || kind == Kind.Bluetooth

    /** One device as the system lists it, before it is made a choice (see [listed]). */
    data class Found(val type: Int, val address: String, val product: String, val source: Boolean, val sink: Boolean)

    companion object {
        /**
         * The choices among what the system lists, in its order.
         *
         * Not everything it lists is something a person picks: the
         * loudspeaker kept for alarms, the telephony and remote-submix
         * endpoints, buses, tuners, and the echo canceller's copy of what is
         * played are the system's own, and are left out. A Bluetooth headset
         * is listed once for calls and once for music (SCO and A2DP, or LE
         * audio's headset and speaker), under one address: it is one choice,
         * standing for the profile a call's voice goes through. The phone's
         * several microphones are one too, since the system picks between
         * them itself (for its noise suppression) whatever is asked. Each
         * choice's [id] lasts across connections ([lastingId]).
         */
        fun listed(found: List<Found>, input: Boolean): List<AudioDevice> {
            val out = LinkedHashMap<String, AudioDevice>()
            for (device in found) {
                if (if (input) !device.source else !device.sink) continue
                if (device.type in (if (input) SYSTEM_INPUTS else SYSTEM_OUTPUTS)) continue
                val kind = kindOf(device.type)
                if (input && (kind == Kind.Speaker || kind == Kind.Earpiece)) continue
                if (!input && kind == Kind.Microphone) continue
                val id = lastingId(device.type, device.address, device.product)
                val name = when (kind) {
                    Kind.Bluetooth, Kind.Usb, Kind.Other -> device.product
                    else -> ""
                }
                val before = out[id]
                if (before == null || callable(device.type) && !callable(before.type)) out[id] = AudioDevice(id, name, device.type)
            }
            return out.values.toList()
        }

        /** A profile a call goes through, rather than the media one beside it. */
        fun callable(type: Int): Boolean =
            type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO || type == TYPE_BLE_HEADSET ||
                type == AudioDeviceInfo.TYPE_WIRED_HEADSET || type == AudioDeviceInfo.TYPE_USB_HEADSET

        /** The echo canceller's copy of what is played, which the platform lists and does not name to apps (AudioDeviceInfo's hidden TYPE_ECHO_REFERENCE). */
        private const val ECHO_REFERENCE = 28

        private val SYSTEM_OUTPUTS = setOf(
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE, AudioDeviceInfo.TYPE_BUS, AudioDeviceInfo.TYPE_IP,
            AudioDeviceInfo.TYPE_FM, AudioDeviceInfo.TYPE_AUX_LINE, AudioDeviceInfo.TYPE_REMOTE_SUBMIX,
            AudioDeviceInfo.TYPE_TELEPHONY, ECHO_REFERENCE,
        )
        private val SYSTEM_INPUTS = setOf(
            AudioDeviceInfo.TYPE_FM_TUNER, AudioDeviceInfo.TYPE_TV_TUNER, AudioDeviceInfo.TYPE_BUS, AudioDeviceInfo.TYPE_IP,
            AudioDeviceInfo.TYPE_REMOTE_SUBMIX, AudioDeviceInfo.TYPE_TELEPHONY, ECHO_REFERENCE,
        )

        fun kindOf(type: Int): Kind = when (type) {
            AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> Kind.Earpiece
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> Kind.Speaker
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> Kind.Microphone
            AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES, AudioDeviceInfo.TYPE_LINE_ANALOG -> Kind.Wired
            AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_ACCESSORY -> Kind.Usb
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_HEARING_AID,
            TYPE_BLE_HEADSET, TYPE_BLE_SPEAKER, TYPE_BLE_BROADCAST -> Kind.Bluetooth
            else -> Kind.Other
        }

        // AudioDeviceInfo's LE audio types, by value: added in API 31 and 33,
        // and this app runs from 29, where they are simply never seen.
        const val TYPE_BLE_HEADSET = 26
        const val TYPE_BLE_SPEAKER = 27
        const val TYPE_BLE_BROADCAST = 30

        /** A device's lasting id (see [AudioDevice]). */
        fun lastingId(type: Int, address: String, product: String): String = when (kindOf(type)) {
            Kind.Earpiece -> "earpiece"
            Kind.Speaker -> "speaker"
            Kind.Microphone -> "microphone"
            Kind.Wired -> "wired"
            Kind.Usb -> "usb:$product"
            Kind.Bluetooth -> "bluetooth:" + address.ifEmpty { product }
            Kind.Other -> "$type:$product"
        }
    }
}

/**
 * Which way out the call's sound takes, by the rule a call app follows: what
 * the person chose while it is there; otherwise the newest headset of any
 * kind; otherwise the loudspeaker, because a video call is held at arm's
 * length and one heard only with the phone at an ear is a call nobody hears;
 * otherwise whatever there is.
 *
 * A headset that arrives is taken at once, over a choice made before: being
 * plugged in or switched on is the latest thing the person did, as every
 * phone's own calls treat it. A choice whose device goes away is dropped,
 * and the rule above decides again. Pure, so that a test can hold it to all
 * of that without a phone (AudioRoutingTest).
 */
class RouteRule {
    /** The way out the person picked, by id; null is the rule's own. */
    var chosen: String? = null
        private set
    /** Worn devices in the order they arrived; the last is the newest. */
    private val arrivals = ArrayList<String>()
    private var known: Set<String>? = null

    fun choose(id: String?) {
        chosen = id
    }

    /**
     * What is there now. Devices not seen before are arrivals, unless this
     * is the first look, which only says what was already there.
     */
    fun present(routes: List<AudioDevice>) {
        val ids = routes.map { it.id }.toSet()
        val before = known
        for (route in routes) {
            if (!route.worn) continue
            val arrived = before != null && route.id !in before
            if (arrived || route.id !in arrivals) {
                arrivals.remove(route.id)
                if (arrived) arrivals.add(route.id) else arrivals.add(0, route.id)
                // Plugged in or switched on now: the person's latest choice.
                if (arrived) chosen = null
            }
        }
        arrivals.retainAll(ids)
        if (chosen != null && chosen !in ids) chosen = null
        known = ids
    }

    /** The way out to take among [routes]. */
    fun resolve(routes: List<AudioDevice>): AudioDevice? {
        chosen?.let { id -> routes.firstOrNull { it.id == id }?.let { return it } }
        arrivals.lastOrNull()?.let { id -> routes.firstOrNull { it.id == id }?.let { return it } }
        routes.lastOrNull { it.worn }?.let { return it }
        return routes.firstOrNull { it.kind == AudioDevice.Kind.Speaker } ?: routes.firstOrNull()
    }
}

/**
 * Where a call's sound goes in and out: the system's audio mode and which
 * device is used for each direction (the Apple apps' AudioDevices and the
 * session's `.videoChat` mode with `.defaultToSpeaker`).
 *
 * Two ways of playing, as CallAudio switches between them. While the
 * microphone records with voice processing, the sound plays as a call
 * (MODE_IN_COMMUNICATION), out of the communication device, which the
 * system's echo canceller hears; Android 12 and later choose that device
 * with `setCommunicationDevice`, and before that the loudspeaker and a
 * Bluetooth headset's SCO link are switched on and off by hand. Otherwise it
 * plays as media in MODE_NORMAL, as any player does, so that other apps keep
 * their sound, a Bluetooth headset stays on its high-quality profile rather
 * than the narrow one a call needs, and the microphone's light is not lit
 * for a microphone that is off (the Apple apps learned all three,
 * CallAudio.swift `listeningOnly`); then the track's preferred device is the
 * way out.
 *
 * The one exception is the earpiece, which only a call can reach: chosen,
 * the sound plays as a call with the microphone off as well ([needsCall]),
 * because the alternative was the call jumping to the loudspeaker, at full
 * level, in the ear of somebody who had just pressed mute.
 */
interface AudioRouting {
    fun devices(input: Boolean): List<AudioDevice>

    /** Whether choosing a device can take now. */
    val choosable: Boolean

    /** The system's device behind an id this lists, for setPreferredDevice. */
    fun info(id: String?, input: Boolean): AudioDeviceInfo?

    /** Whether the device records without any processing at all, for the web's "original sound". */
    val offersUnprocessed: Boolean

    // What follows has a default for a routing that only lists (a page's
    // own check of the devices, a test's), which has nothing to point.

    /** The way out the person picked, by its id, or null for the rule's own; false where it is not there. */
    fun choose(id: String?): Boolean = true

    /** The way out in use, or that would be, where the routing knows: what a devices panel ticks. */
    val current: String? get() = null

    /** Whether the way out is one only a call can reach (the earpiece). */
    val needsCall: Boolean get() = false

    /** The device to play through while the sound plays as media; null leaves it to the system. */
    fun mediaDevice(): AudioDeviceInfo? = null

    /**
     * The call's sound plays as a call, or stops doing so: the system's
     * communication mode, under which its echo canceller hears what is
     * played, and the communication device meanwhile.
     */
    fun communicating(on: Boolean)

    /** Follows devices coming and going, telling [changed] on the main thread, until [stop]. */
    fun start(changed: () -> Unit) {}

    fun stop() {}
}

class SystemAudioRouting(context: Context) : AudioRouting {
    private val manager = context.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private val rule = RouteRule()
    private var inCommunication = false
    private var scoStarted = false
    private var watching: AudioDeviceCallback? = null
    private var changed: (() -> Unit)? = null

    /** The microphones or ways out a person can choose, as [AudioDevice.listed] makes them of what the system lists. */
    override fun devices(input: Boolean): List<AudioDevice> {
        val manager = manager ?: return emptyList()
        val flag = if (input) AudioManager.GET_DEVICES_INPUTS else AudioManager.GET_DEVICES_OUTPUTS
        return AudioDevice.listed(manager.getDevices(flag).map(::found), input)
    }

    private fun found(info: AudioDeviceInfo) =
        AudioDevice.Found(info.type, info.address.orEmpty(), info.productName?.toString().orEmpty(), info.isSource, info.isSink)

    override val choosable: Boolean get() = manager != null

    override val offersUnprocessed: Boolean
        get() = manager?.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true"

    override fun info(id: String?, input: Boolean): AudioDeviceInfo? {
        val manager = manager ?: return null
        id ?: return null
        val flag = if (input) AudioManager.GET_DEVICES_INPUTS else AudioManager.GET_DEVICES_OUTPUTS
        val matching = manager.getDevices(flag).filter { (if (input) it.isSource else it.isSink) && idOf(it) == id }
        // For playing as media, a headset's media profile before its call one.
        return if (input) matching.firstOrNull() else matching.firstOrNull { !callable(it.type) } ?: matching.firstOrNull()
    }

    override fun choose(id: String?): Boolean {
        if (id != null && devices(input = false).none { it.id == id }) return false
        rule.choose(id)
        if (inCommunication) route()
        Log.i(TAG, "sound out: ${id ?: "automatic"} -> $current")
        return true
    }

    override val current: String? get() = resolved()?.id

    override val needsCall: Boolean get() = resolved()?.kind == AudioDevice.Kind.Earpiece

    override fun mediaDevice(): AudioDeviceInfo? {
        val route = resolved() ?: return null
        // Left to the system unless the person chose: it already plays media
        // on the newest headset, and otherwise on the loudspeaker.
        if (rule.chosen == null) return null
        return info(route.id, input = false)
    }

    private fun resolved(): AudioDevice? {
        val routes = devices(input = false)
        rule.present(routes)
        return rule.resolve(routes)
    }

    @Suppress("DEPRECATION")
    override fun communicating(on: Boolean) {
        val manager = manager ?: return
        if (on == inCommunication) {
            if (on) route()
            return
        }
        inCommunication = on
        if (on) {
            manager.mode = AudioManager.MODE_IN_COMMUNICATION
            route()
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                manager.clearCommunicationDevice()
            } else {
                manager.isSpeakerphoneOn = false
                stopSco(manager)
            }
            manager.mode = AudioManager.MODE_NORMAL
        }
    }

    /** Points the call at the way out the rule takes, while it plays as a call. */
    @Suppress("DEPRECATION")
    private fun route() {
        val manager = manager ?: return
        val route = resolved() ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val device = manager.availableCommunicationDevices.filter { idOf(it) == route.id }
                .let { found -> found.firstOrNull { callable(it.type) } ?: found.firstOrNull() }
            if (device == null) {
                Log.w(TAG, "${route.id} is not a way out a call can take")
                return
            }
            if (manager.communicationDevice?.let(::idOf) != route.id && !manager.setCommunicationDevice(device)) {
                Log.w(TAG, "the call would not go out through ${route.id}")
            }
        } else {
            manager.isSpeakerphoneOn = route.kind == AudioDevice.Kind.Speaker
            if (route.kind == AudioDevice.Kind.Bluetooth) {
                if (!scoStarted) {
                    scoStarted = true
                    manager.startBluetoothSco()
                }
                manager.isBluetoothScoOn = true
            } else {
                stopSco(manager)
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun stopSco(manager: AudioManager) {
        if (!scoStarted) return
        scoStarted = false
        manager.isBluetoothScoOn = false
        manager.stopBluetoothSco()
    }

    override fun start(changed: () -> Unit) {
        this.changed = changed
        val manager = manager ?: return
        if (watching != null) return
        // The web listens for `devicechange` (DeviceMenu.tsx) so that a
        // headset plugged in with the panel open is in it; here the arrival
        // also moves the call to it.
        val callback = object : AudioDeviceCallback() {
            override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) = moved("arrived", added)
            override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) = moved("left", removed)
        }
        rule.present(devices(input = false))
        manager.registerAudioDeviceCallback(callback, main)
        watching = callback
    }

    private fun moved(how: String, devices: Array<out AudioDeviceInfo>) {
        if (devices.isEmpty()) return
        Log.i(TAG, "sound devices $how: ${devices.joinToString { idOf(it) }}")
        rule.present(this.devices(input = false))
        if (inCommunication) route()
        changed?.invoke()
    }

    override fun stop() {
        watching?.let { manager?.unregisterAudioDeviceCallback(it) }
        watching = null
        changed = null
        communicating(false)
    }

    private fun idOf(info: AudioDeviceInfo): String =
        AudioDevice.lastingId(info.type, info.address.orEmpty(), info.productName?.toString().orEmpty())

    private fun callable(type: Int): Boolean = AudioDevice.callable(type)
}

/** Routing for a call that plays nothing (a muted test) or runs where there is no audio service. */
object NoAudioRouting : AudioRouting {
    override fun devices(input: Boolean): List<AudioDevice> = emptyList()
    override val choosable: Boolean get() = false
    override val offersUnprocessed: Boolean get() = false
    override fun info(id: String?, input: Boolean): AudioDeviceInfo? = null
    override fun communicating(on: Boolean) {}
}
