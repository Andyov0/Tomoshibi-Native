package app.tomoshibi.media

import android.os.Handler
import android.os.Looper
import android.util.Log
import kotlin.math.exp
import uniffi.tomoshibi_core.LosslessSink

private const val TAG = "Tomoshibi"
/** The rate the call's mixer plays at (CallAudio). */
private const val MIXER_RATE = 48_000

/**
 * The lossless streams somebody's shared sound or music arrives in, played
 * into the call's mixer (the Apple apps' LosslessPlayback).
 *
 * It has to play from the first call: the core asks for a stream whenever a
 * browser offers one, and holds that sound's ordinary Opus track back at the
 * server meanwhile, sink or no sink. A call that never played these heard
 * nothing at all of a song somebody shared.
 *
 * Each stream is taken from its own rate to the mixer's through a
 * windowed-sinc filter (Resampler), at the level the web plays it at
 * (lossless-receiver.ts `levelOf`): the listener's own volume for that
 * person's sound, and for music the song desk's duck on top ([setDuck]).
 * The level is eased rather than stepped, with the web's time constant of
 * thirty milliseconds (`setTargetAtTime(level, now, 0.03)`): under speech
 * the duck moves every forty milliseconds, and a gain stepped that often is
 * heard as clicks. A stream comes in from silence and goes out to it the
 * same way, so neither its first sample nor the cut at its end is a click.
 */
class LosslessPlayback(private val audio: CallAudio) : LosslessSink {
    private val main by lazy { Handler(Looper.getMainLooper()) }
    private val lock = Any()
    private val streams = HashMap<String, Stream>()
    /** The listener's level per stream, kept for streams that start after it was set. */
    private val levels = HashMap<String, Float>()
    /** How far each person's music is turned down now, by identity (duck.ts `musicFactor`). */
    private val ducks = HashMap<String, Float>()

    override fun onLossless(identity: String, music: Boolean, samples: List<Float>, rate: UInt, channels: UInt) {
        val key = key(identity, music)
        val rate = rate.toInt()
        val channels = channels.toInt()
        // The core drops packets at a rate nobody records at (lossless.rs
        // `playable_rate`); this is the second wall, as on Apple, because a
        // block saying nought once brought every listener down there.
        if (rate !in 8_000..384_000 || channels !in 1..2) return
        val (stream, fresh) = synchronized(lock) {
            val existing = streams[key]
            if (existing != null && !existing.ending && existing.rate == rate && existing.ring.channels == channels) {
                existing to false
            } else {
                val made = Stream(key, rate, channels)
                made.volume = levels[key] ?: 1f
                if (music) made.duck = ducks[identity] ?: 1f
                streams[key] = made
                if (existing != null) audio.endLater(existing)
                made to true
            }
        }
        stream.ring.write(samples)
        if (fresh) {
            main.post { audio.attach(stream) }
            Log.i(TAG, "lossless $key at $rate Hz in $channels channels")
        }
    }

    override fun onLosslessEnd(identity: String, music: Boolean) {
        val stream = synchronized(lock) { streams.remove(key(identity, music)) } ?: return
        audio.endLater(stream)
    }

    /** The listener's level for one person's sound, on the stream and on any that starts later. */
    fun setLevel(identity: String, music: Boolean, level: Float) {
        val key = key(identity, music)
        synchronized(lock) {
            levels[key] = level.coerceIn(0f, 1f)
            streams[key]?.volume = level.coerceIn(0f, 1f)
        }
    }

    /**
     * How far this person's music is turned down now, the duck and the
     * fade-in together, as the song desk works them out for the Opus track
     * too (CallAudio.setDuck); kept for a stream that starts later.
     */
    fun setDuck(identity: String, factor: Float) {
        synchronized(lock) {
            ducks[identity] = factor.coerceIn(0f, 1f)
            streams[key(identity, music = true)]?.duck = factor.coerceIn(0f, 1f)
        }
    }

    /** Whether a stream is standing in for this person's sound now. */
    fun playing(identity: String, music: Boolean): Boolean = synchronized(lock) { key(identity, music) in streams }

    /** How loud a stream comes out, after the filter and before its level, for the song desk's log. */
    fun meter(identity: String, music: Boolean): SoundLevel? = synchronized(lock) { streams[key(identity, music)]?.level }

    fun closeAll() {
        val all = synchronized(lock) {
            val out = streams.values.toList()
            streams.clear()
            out
        }
        for (stream in all) audio.endLater(stream)
    }

    /** Fades [stream] out, and takes it out of the mixer once it is silent. */
    private fun CallAudio.endLater(stream: Stream) {
        stream.ending = true
        main.postDelayed({ detach(stream) }, FADE_OUT_MILLIS)
    }

    companion object {
        fun key(identity: String, music: Boolean) = identity + if (music) "/music" else "/screen"

        /** The web's time constant for a change of level (lossless-receiver.ts, `setTargetAtTime`). */
        const val EASE_SECONDS = 0.03
        /** Long enough for the eased level to fall below a thousandth (seven time constants), with room. */
        const val FADE_OUT_MILLIS = 300L
    }

    /** One stream, at its own rate, taken out at the mixer's through its own filter. */
    private class Stream(key: String, val rate: Int, channels: Int) : MixerSource(key) {
        // The Apple version's cushion: twenty milliseconds before it plays,
        // two hundred at most, settling back to forty.
        val ring = PcmRing(channels, startFrames = rate / 50, ceilingFrames = rate / 5, settleFrames = rate / 25)
        private val resampler = Resampler(rate, MIXER_RATE, channels) { into, offset, frames -> ring.read(into, offset, frames) }
        /** Faded out to nothing, and then taken away. */
        @Volatile var ending = false
        /** The level as it is now, easing toward the one asked for; from silence. */
        private var current = 0f
        private val coefficient = (1 - exp(-1.0 / (EASE_SECONDS * MIXER_RATE))).toFloat()
        private var gains = FloatArray(0)

        override fun render(into: FloatArray, frames: Int, gain: Float): Double {
            if (gains.size < frames) gains = FloatArray(frames)
            val target = if (ending) 0f else gain
            var level = current
            for (frame in 0 until frames) {
                level += (target - level) * coefficient
                gains[frame] = level
            }
            current = if (kotlin.math.abs(level - target) < 1e-5f) target else level
            return resampler.render(into, frames, 0f, gains)
        }
    }
}
