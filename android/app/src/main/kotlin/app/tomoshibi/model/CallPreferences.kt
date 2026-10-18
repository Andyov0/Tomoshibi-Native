package app.tomoshibi.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONObject

/**
 * The choices a call reads that have no model of their own outside one,
 * under the web's storage names and with its values (Settings.swift
 * `CallPreferences`; live/preferences.ts, live/duck.ts, live/sound.ts).
 * Nothing is written in a rehearsal: a test drives the very app somebody
 * uses, and its settings are that person's.
 *
 * One for the app (AppModel `preferences`) rather than one per settings
 * page, and read again from storage whenever it may have changed
 * ([refresh]): a call writes some of these too, and a copy read once when the
 * page opened went on saying, and writing back, what had been true then.
 * Setting a value writes it; [refresh] assigns what is stored without
 * writing anything back.
 */
class CallPreferences(
    /** Whether the watch and music choices are written: false in a rehearsal. */
    val keeps: Boolean,
    /** The app's store, where the watch and music choices are kept. */
    private val defaults: Defaults?,
    /**
     * Where the choices about one's own devices are kept: the same store as
     * the app's DeviceMemory, which the call reads and the devices panel in a
     * call writes; null in a rehearsal that keeps nothing.
     */
    private val deviceDefaults: Defaults?,
) {
    private var originalState by mutableStateOf(false)
    private var watchState by mutableStateOf("auto")
    private var musicState by mutableStateOf("lossless")
    private var ducksState by mutableStateOf(true)
    private var devicesState by mutableStateOf<Map<String, String>>(emptyMap())

    init {
        refresh()
    }

    /** sound.ts `meet-live.original-sound`: "on", or nothing. */
    var originalSound: Boolean
        get() = originalState
        set(value) {
            originalState = value
            put(DeviceMemory.ORIGINAL_KEY, if (value) "on" else null, deviceDefaults)
        }

    /** preferences.ts `meet-live.watch-quality`: a ceiling, "auto" kept as nothing. */
    var watchQuality: String
        get() = watchState
        set(value) {
            watchState = value
            keep(WATCH_KEY, if (value == "auto") null else value)
        }

    /**
     * preferences.ts `meet-live.music-quality`: "best", or nothing for
     * lossless. The music desk reads the same key when a call opens it.
     */
    var musicQuality: String
        get() = musicState
        set(value) {
            musicState = value
            keep(MUSIC_KEY, if (value == "best") "best" else null)
        }

    /** duck.ts `meet-live.music-duck`: on unless "off". */
    var musicDucks: Boolean
        get() = ducksState
        set(value) {
            ducksState = value
            keep(DUCK_KEY, if (value) null else "off")
        }

    /**
     * preferences.ts `meet-live.device-choice`: the device last chosen of
     * each kind, under the web's kind names. An identifier here is the
     * system's own (an AudioDeviceInfo id, a camera id), which means nothing
     * on another device, as the web says of its own.
     *
     * Changed one kind at a time ([choose]), over what is stored at that
     * moment: the Apple apps once wrote the whole object from the copy read
     * when the window opened, so a camera picked there erased the headset
     * picked in the call a minute before.
     */
    val devices: Map<String, String> get() = devicesState

    /** Reads everything again, assigning only what changed, and writing nothing back. */
    fun refresh() {
        val original = deviceDefaults?.string(DeviceMemory.ORIGINAL_KEY) == "on"
        if (original != originalState) originalState = original
        val stored = defaults?.string(WATCH_KEY) ?: "auto"
        val watch = if (stored in WATCH_QUALITIES) stored else "auto"
        if (watch != watchState) watchState = watch
        val music = if (defaults?.string(MUSIC_KEY) == "best") "best" else "lossless"
        if (music != musicState) musicState = music
        val ducks = defaults?.string(DUCK_KEY) != "off"
        if (ducks != ducksState) ducksState = ducks
        // A rehearsal that keeps nothing has nothing stored to read, and
        // shows what it chose in this run.
        val deviceDefaults = deviceDefaults ?: return
        val fresh = readDevices(deviceDefaults)
        if (fresh != devicesState) devicesState = fresh
    }

    /** One kind's device, or the system's (null), written over what is stored now rather than over what this showed. */
    fun choose(kind: String, id: String?) {
        val deviceDefaults = deviceDefaults
        if (deviceDefaults == null) {
            devicesState = devicesState.toMutableMap().apply { if (id == null) remove(kind) else put(kind, id) }
            return
        }
        val next = readDevices(deviceDefaults).toMutableMap()
        if (id == null) next.remove(kind) else next[kind] = id
        devicesState = next
        write(next, deviceDefaults)
    }

    /** Forgets every device choice. */
    fun forgetDevices() {
        devicesState = emptyMap()
        deviceDefaults?.remove(DeviceMemory.CHOICE_KEY)
    }

    /**
     * Written as the web's `JSON.stringify` writes it, keys in order, the way
     * DeviceMemory writes the same record, so that whichever of the two wrote
     * last the record reads the same.
     */
    private fun write(choice: Map<String, String>, defaults: Defaults) {
        if (choice.isEmpty()) {
            defaults.remove(DeviceMemory.CHOICE_KEY)
            return
        }
        val fields = choice.entries.sortedBy { it.key }.joinToString(",") { "${Json.quote(it.key)}:${Json.quote(it.value)}" }
        defaults.set(DeviceMemory.CHOICE_KEY, "{$fields}")
    }

    private fun keep(key: String, value: String?) {
        if (!keeps) return
        put(key, value, defaults)
    }

    private fun put(key: String, value: String?, defaults: Defaults?) {
        val store = defaults ?: return
        if (value == null) store.remove(key) else store.set(key, value)
    }

    companion object {
        const val WATCH_KEY = "meet-live.watch-quality"
        const val MUSIC_KEY = "meet-live.music-quality"
        const val DUCK_KEY = "meet-live.music-duck"

        /** The ceilings a watched video may be held to (preferences.ts `WATCH_QUALITIES`). */
        val WATCH_QUALITIES = listOf("auto", "1080p", "720p")

        /**
         * The web's reading: an object of strings no longer than any device's
         * identifier, and nothing for anything else (DeviceMemory reads it the
         * same way).
         */
        internal fun readDevices(defaults: Defaults): Map<String, String> {
            val text = defaults.string(DeviceMemory.CHOICE_KEY) ?: return emptyMap()
            val parsed = try {
                JSONObject(text)
            } catch (_: Exception) {
                return emptyMap()
            }
            val out = LinkedHashMap<String, String>()
            for (kind in DeviceMemory.Kind.entries) {
                val id = parsed.opt(kind.key) as? String ?: continue
                if (id.isNotEmpty() && id.length <= DeviceMemory.MOST_ID) out[kind.key] = id
            }
            return out
        }
    }
}
