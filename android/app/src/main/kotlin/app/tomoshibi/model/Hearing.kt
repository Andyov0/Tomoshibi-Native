package app.tomoshibi.model

import org.json.JSONObject

/*
 * How loud everybody else is, and who is not to be heard at all: the record
 * the sound panel writes and the call applies (web/src/live/hearing.ts).
 *
 * The web keeps this record itself rather than on the media library's objects,
 * because those forget: a volume lives on a participant that a reconnect
 * rebuilds, and a block lives on a publication that does not outlive its
 * track, so somebody who stops a share and starts it again comes back audible.
 * The apps forget the same way, more so: every remote sound gets a player of
 * its own when it is subscribed, at full volume. So this is the record, and
 * what the call holds is filled again from it whenever the room changes.
 */

/** One thing there is to hear from somebody (hearing.ts `Sound`). */
enum class Sound(val key: String) { Voice("voice"), Screen("screen"), Music("music") }

/** What somebody has decided about one sound. */
data class HearingSetting(
    /** Between silent and as loud as it was sent. */
    val volume: Double,
    /**
     * Stopped at the media server rather than turned down here: a volume of
     * nought still downloads every packet of a shared film and throws it away.
     */
    val blocked: Boolean,
) {
    /** Whether this is something somebody has decided not to hear. */
    val silenced: Boolean get() = blocked || volume == 0.0

    companion object {
        /** Untouched, which is what everybody is until somebody says otherwise. */
        val asSent = HearingSetting(1.0, false)
    }
}

/**
 * Where a setting is filed, and whether it outlives the call. A proven
 * signature is the same on every visit and worth writing down; anybody else's
 * identity is fresh each time, and a setting kept against one would be applied
 * to whoever is issued that string next (hearing.ts `filing`).
 */
data class HearingFiling(val key: String, val keeps: Boolean)

/** Every setting, filed under `<who>/<sound>`, and what of it is kept. */
class HearingBook private constructor(private val entries: MutableMap<String, Held>) {
    private data class Held(val setting: HearingSetting, val keeps: Boolean)

    constructor() : this(HashMap())

    /**
     * The book as it was stored: the web's JSON, `{"<who>/<sound>": {"volume":
     * 0.5, "blocked": false}}`. Anything unreadable reads as nothing decided.
     */
    constructor(stored: String?) : this(read(stored))

    /** What has been decided about one sound from one person, clamped on the way out as well. */
    fun setting(filing: HearingFiling, sound: Sound): HearingSetting {
        val held = entries[place(filing, sound)] ?: return HearingSetting.asSent
        return HearingSetting(clamp(held.setting.volume), held.setting.blocked)
    }

    /**
     * Writes a decision down. Returns whether what is kept changed, which is
     * the only time storage needs writing.
     */
    fun set(filing: HearingFiling, sound: Sound, setting: HearingSetting): Boolean {
        val place = place(filing, sound)
        val clean = HearingSetting(clamp(setting.volume), setting.blocked)
        val before = entries[place]
        // Nothing decided is not a decision to write down, so turning
        // somebody back up removes them rather than leaving a row saying they
        // are ordinary.
        if (clean == HearingSetting.asSent) entries.remove(place) else entries[place] = Held(clean, filing.keeps)
        return filing.keeps && before != entries[place]
    }

    /**
     * What is kept, as the web writes it, or null where nothing is: the web
     * removes the key rather than storing an empty object. Written by hand,
     * each entry's fields in the order the web gives them.
     */
    val stored: String?
        get() {
            val kept = entries.filter { it.value.keeps }.toSortedMap()
            if (kept.isEmpty()) return null
            return kept.entries.joinToString(",", "{", "}") { (place, held) ->
                "${Json.quote(place)}:{\"volume\":${Json.number(held.setting.volume)},\"blocked\":${held.setting.blocked}}"
            }
        }

    override fun equals(other: Any?) = other is HearingBook && other.entries == entries
    override fun hashCode() = entries.hashCode()

    companion object {
        /** The web's own storage name, old product name and all. */
        const val STORAGE_KEY = "meet-live.hearing"

        private fun place(filing: HearingFiling, sound: Sound) = "${filing.key}/${sound.key}"

        /**
         * A volume between nought and one. A slider cannot produce anything
         * else; a hand-edited entry can, and a player handed 3 would be three
         * times as loud as the person sent it (hearing.ts `level`).
         */
        fun clamp(volume: Double): Double = if (volume.isFinite()) volume.coerceIn(0.0, 1.0) else HearingSetting.asSent.volume

        private fun read(stored: String?): MutableMap<String, Held> {
            val out = HashMap<String, Held>()
            val parsed = try {
                JSONObject(stored ?: return out)
            } catch (_: Exception) {
                return out
            }
            for (place in parsed.keys()) {
                val value = parsed.opt(place) as? JSONObject ?: continue
                out[place] = Held(HearingSetting(level(value.opt("volume")), value.opt("blocked") == true), keeps = true)
            }
            return out
        }

        /**
         * A stored volume, read as the web reads one: only a number is a
         * volume. org.json hands JSON's `true` and `false` over as Booleans,
         * which are not Numbers, so they read as untouched, as the web's
         * `typeof` has it (Swift had to tell them apart inside NSNumber).
         */
        private fun level(value: Any?): Double = if (value is Number) clamp(value.toDouble()) else HearingSetting.asSent.volume
    }
}
