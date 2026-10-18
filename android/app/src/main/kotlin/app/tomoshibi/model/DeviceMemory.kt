package app.tomoshibi.model

import org.json.JSONObject
import kotlin.math.log10

internal const val TAG = "Tomoshibi"

/**
 * The choices about one's own devices that a call starts from, kept under
 * the web's own names (live/preferences.ts, live/sound.ts) and read the way
 * the web reads them.
 *
 * `meet-live.devices` is whether the microphone and the camera start on;
 * `meet-live.device-choice` is which microphone, camera and speakers were
 * last picked, by kind; `meet-live.original-sound` is whether the voice goes
 * as it is. Until these were kept, the switches on the join screen started
 * from the same defaults every time and a chosen headset was picked again at
 * the start of every call.
 *
 * Each is a string, as the browser's storage holds it, so that what the app
 * keeps reads the same as what the web keeps. Every reader treats a value it
 * does not recognise as never having been told anything (preferences.ts): a
 * value half written, or written by a later build, may not become a call that
 * starts with a silent microphone.
 */
class DeviceMemory(
    /**
     * Where it is all kept; null keeps nothing and remembers nothing, which
     * is what a test driving the app gets (AppModel.rehearsal). The other
     * choices a call remembers (the signal's figures, the share's size and
     * rate) are kept here too, so that a rehearsal keeps none of them either.
     */
    val defaults: Defaults?,
) {
    /** The kinds of device somebody picks from a list (preferences.ts `CHOSEN_KINDS`). */
    enum class Kind(val key: String) {
        Microphone("audioinput"),
        Camera("videoinput"),
        Speakers("audiooutput"),
    }

    /** Whether the camera and the microphone start on. */
    data class Joining(val camera: Boolean, val microphone: Boolean) {
        companion object {
            /**
             * The microphone on and the camera off: how most people walk
             * into a call, and the cheaper of the two mistakes (preferences.ts).
             */
            val first = Joining(camera = false, microphone = true)
        }
    }

    /** Whether anything is kept at all. */
    val keeps: Boolean get() = defaults != null

    val joining: Joining
        get() {
            val parsed = defaults?.string(DEVICES_KEY)?.let(::objectOf) ?: return Joining.first
            // Each on its own: a record that lost one half keeps the other.
            // Only a real boolean counts, as only `typeof === "boolean"` does.
            fun flag(key: String, fallback: Boolean) = parsed.opt(key) as? Boolean ?: fallback
            return Joining(camera = flag("camera", Joining.first.camera), microphone = flag("microphone", Joining.first.microphone))
        }

    fun remember(joining: Joining) {
        defaults?.set(DEVICES_KEY, "{\"camera\":${joining.camera},\"microphone\":${joining.microphone}}")
    }

    /** The device last picked of this kind, or null. */
    fun choice(kind: Kind): String? = choices()[kind]

    private fun choices(): Map<Kind, String> {
        val parsed = defaults?.string(CHOICE_KEY)?.let(::objectOf) ?: return emptyMap()
        val out = LinkedHashMap<Kind, String>()
        for (kind in Kind.entries) {
            val id = parsed.opt(kind.key) as? String ?: continue
            if (id.isNotEmpty() && id.length <= MOST_ID) out[kind] = id
        }
        return out
    }

    /**
     * Kept once the device took, and not before (DeviceMenu.tsx `select`): a
     * choice remembered before the device came up would bring back the one
     * that failed.
     */
    fun remember(kind: Kind, id: String) {
        val defaults = defaults ?: return
        if (id.isEmpty() || id.length > MOST_ID) return
        val all = choices().toMutableMap()
        all[kind] = id
        val fields = all.entries.sortedBy { it.key.key }.joinToString(",") { "${Json.quote(it.key.key)}:${Json.quote(it.value)}" }
        defaults.set(CHOICE_KEY, "{$fields}")
    }

    /**
     * Which of the remembered devices can be asked for, given what is listed
     * now (preferences.ts `restorable`): one that is listed. One that is not
     * is unplugged or gone, so the system's default stands and the choice
     * stays written down for the day it comes back.
     */
    fun restorable(kind: Kind, present: List<String>): String? {
        val id = choice(kind) ?: return null
        return if (id in present) id else null
    }

    /** Whether the voice was last sent as it is (sound.ts `rememberedOriginal`). */
    val originalSound: Boolean get() = defaults?.string(ORIGINAL_KEY) == "on"

    fun remember(originalSound: Boolean) {
        defaults?.set(ORIGINAL_KEY, if (originalSound) "on" else null)
    }

    /** The background blurred behind somebody's camera (preferences.ts `blurWanted`). */
    val blur: Boolean get() = defaults?.string(BLUR_KEY) == "on"

    fun rememberBlur(on: Boolean) {
        defaults?.set(BLUR_KEY, if (on) "on" else null)
    }

    companion object {
        const val DEVICES_KEY = "meet-live.devices"
        const val CHOICE_KEY = "meet-live.device-choice"
        const val ORIGINAL_KEY = "meet-live.original-sound"
        const val BLUR_KEY = "meet-live.blur"

        /** Longer than any device's identifier, and short enough to refuse what is not one. */
        const val MOST_ID = 256

        val nothing = DeviceMemory(null)

        private fun objectOf(text: String): JSONObject? = try {
            JSONObject(text)
        } catch (_: Exception) {
            null
        }
    }
}

/** How the microphone meter reads a level (MicLevel.tsx). */
object MicrophoneLevel {
    /** Anything above this has been heard (MicLevel.tsx's 0.06). */
    const val HEARD = 0.06

    /**
     * How full the meter is, 0 to 1, for a root-mean-square level of the
     * samples, 0 to 1: drawn in decibels from -60 to -10, which puts a voice in
     * the upper half of the bar and the hiss of a quiet room under the
     * "heard" line. Drawn linearly, speech at -26 dBFS barely moved the meter.
     */
    fun shown(rms: Double): Double {
        if (rms <= 0) return 0.0
        val decibels = 20 * log10(rms)
        return ((decibels + 60) / 50).coerceIn(0.0, 1.0)
    }
}

/** JSON written as the web's `JSON.stringify` writes it. */
internal object Json {
    /**
     * A string as JSON, escaping what JSON must and nothing else: org.json
     * writes `/` as `\/`, which the web never does, and a record written so
     * would differ byte for byte from the browser's.
     */
    fun quote(text: String): String {
        val out = StringBuilder(text.length + 2).append('"')
        for (c in text) {
            when (c) {
                '"' -> out.append("\\\"")
                '\\' -> out.append("\\\\")
                '\n' -> out.append("\\n")
                '\r' -> out.append("\\r")
                '\t' -> out.append("\\t")
                '\b' -> out.append("\\b")
                '\u000C' -> out.append("\\f")
                else -> if (c < ' ') out.append(String.format("\\u%04x", c.code)) else out.append(c)
            }
        }
        return out.append('"').toString()
    }

    /**
     * A number as JavaScript writes one: 1 rather than 1.0, and 0.0005
     * rather than Java's 5.0E-4. Java's shortest digits are JavaScript's for
     * every volume a slider can make, so only the notation is changed.
     */
    fun number(value: Double): String = when {
        value == Math.rint(value) && kotlin.math.abs(value) < 9.0e15 -> value.toLong().toString()
        else -> java.math.BigDecimal(value.toString()).stripTrailingZeros().toPlainString()
    }
}
