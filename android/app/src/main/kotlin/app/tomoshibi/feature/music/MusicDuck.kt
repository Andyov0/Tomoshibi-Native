package app.tomoshibi.feature.music

import kotlin.math.max
import kotlin.math.min

/**
 * Music that makes way for speech (web/src/live/duck.ts;
 * TomoshibiBasics/MusicDuck.swift).
 *
 * Whenever anybody in the call is talking, this person included, the song
 * desk's music is turned down where it is heard, and brought back up a moment
 * after they stop: what a radio presenter's desk does under a voice. Each
 * participant decides for itself, from the microphones it can hear, and turns
 * down only its own playback, so what goes out is still the file and nobody's
 * choice reaches anybody else.
 *
 * Music also arrives gently: a stream that starts is faded in over a second
 * and a half rather than starting at full level in the middle of a sentence.
 *
 * This is the arithmetic alone, driven by whoever reads the microphones, with
 * the times given in milliseconds by the caller's clock, so that the tests can
 * hold it to the web's own cases without waiting for real time to pass.
 */
class MusicDuck(
    /** Whether the music is turned down under speech at all; the switch in the panel, kept as `meet-live.music-duck`. */
    var enabled: Boolean = true,
) {
    var level = 1.0
        private set
    private var lastSpeech = Double.NEGATIVE_INFINITY
    private var loud = 0
    private val arrivals = HashMap<String, Double>()

    /** Whether somebody is talking, as far as the music is concerned. */
    val ducking: Boolean get() = enabled && level < 1

    /**
     * One reading of every microphone that is on, and the level moved by the
     * time since the last. Returns whether any music's factor moved.
     */
    fun tick(levels: List<Double>, now: Double, elapsed: Double): Boolean {
        val speaking = levels.any { it > SPEECH }
        loud = if (speaking) loud + 1 else 0
        return step(loud >= ONSET, now, elapsed)
    }

    /**
     * Moves the level one tick, given whether anybody is talking (duck.ts
     * `step`): toward [DUCKED] at the attack's pace, held there until
     * [HOLD_MS] after the last word, then back at the release's pace.
     */
    fun step(speaking: Boolean, now: Double, elapsed: Double): Boolean {
        val before = level
        if (speaking) lastSpeech = now
        val target = if (now - lastSpeech < HOLD_MS) DUCKED else 1.0
        if (level > target) {
            level = max(target, level - (1 - DUCKED) * elapsed / ATTACK_MS)
        } else if (level < target) {
            level = min(target, level + (1 - DUCKED) * elapsed / RELEASE_MS)
        }
        return level != before || fading(now)
    }

    /** What to multiply a music stream's volume by now: the duck, and the fade-in of a stream that has just started. */
    fun factor(key: String, now: Double): Double {
        val fade = arrivals[key]?.let { min(1.0, max(0.0, (now - it) / FADE_IN_MS)) } ?: 1.0
        return (if (enabled) level else 1.0) * fade
    }

    /** A music stream has started from [key]. Faded in once; again only after it has gone, so the songs that follow on the same stream are not. */
    fun arrived(key: String, now: Double) {
        if (key !in arrivals) arrivals[key] = now
    }

    fun left(key: String) {
        arrivals.remove(key)
    }

    /** Whether some stream is still fading in, and so still moving. */
    fun fading(now: Double): Boolean = arrivals.values.any { now - it < FADE_IN_MS + TICK_MS }

    /** Whether anything still moves without a microphone saying so. */
    fun settled(now: Double): Boolean = level == 1.0 && !fading(now)

    companion object {
        /** How far the music comes down while somebody talks. */
        const val DUCKED = 0.3
        /** Down quickly, so the first word is heard; up slowly, so a pause is not a lurch. */
        const val ATTACK_MS = 150.0
        const val HOLD_MS = 700.0
        const val RELEASE_MS = 1200.0
        const val FADE_IN_MS = 1500.0
        /** How often the microphones are read and the level moved. */
        const val TICK_MS = 40.0
        /**
         * The RMS of a frame that counts as talking, about -38 dB below full
         * scale: a voice after the system's processing sits well above it, and
         * the residue of music after echo cancelling well below.
         */
        const val SPEECH = 0.012
        /** Readings in a row above [SPEECH] before it is taken for speech: a click is one. */
        const val ONSET = 2
        /** The key a participant's own music output is filed under, beside the identities of whoever else plays music (duck.ts `LOCAL`). */
        const val LOCAL = "\u0000local"
    }
}
