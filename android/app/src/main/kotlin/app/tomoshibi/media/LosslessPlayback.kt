package app.tomoshibi.media

import android.os.Handler
import android.os.Looper
import android.util.Log
import uniffi.tomoshibi_core.LosslessSink

private const val TAG = "Tomoshibi"

/**
 * The lossless streams somebody's shared sound or music arrives in, played
 * into the call's mixer (the Apple apps' LosslessPlayback).
 *
 * It has to play from the first call: the core asks for a stream whenever a
 * browser offers one, and holds that sound's ordinary Opus track back at the
 * server meanwhile, sink or no sink. A call that never played these heard
 * nothing at all of a song somebody shared.
 *
 * TODO(media): this plays each stream through a linear resampler to the
 * mixer's 48 kHz, which is heard but is not lossless for a 44.1 kHz source,
 * and it has no fade at the edges of a stream. The Apple version fades the
 * level in and out over the stream's own rate, and plays at the stream's own
 * rate through the engine. Also to do: the duck for music while somebody
 * talks (`setDuck`).
 */
class LosslessPlayback(private val audio: CallAudio) : LosslessSink {
    private val main by lazy { Handler(Looper.getMainLooper()) }
    private val lock = Any()
    private val streams = HashMap<String, Stream>()
    /** The listener's level per stream, kept for streams that start after it was set. */
    private val levels = HashMap<String, Float>()

    override fun onLossless(identity: String, music: Boolean, samples: List<Float>, rate: UInt, channels: UInt) {
        val key = key(identity, music)
        val rate = rate.toInt()
        val channels = channels.toInt().coerceAtLeast(1)
        val (stream, fresh) = synchronized(lock) {
            val existing = streams[key]
            if (existing != null && existing.rate == rate && existing.ring.channels == channels) {
                existing to false
            } else {
                val made = Stream(key, rate, channels)
                made.volume = levels[key] ?: 1f
                streams[key] = made
                if (existing != null) audio.detachLater(existing)
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
        audio.detachLater(stream)
    }

    /** The listener's level for one person's sound, on the stream and on any that starts later. */
    fun setLevel(identity: String, music: Boolean, level: Float) {
        val key = key(identity, music)
        synchronized(lock) {
            levels[key] = level.coerceIn(0f, 1f)
            streams[key]?.volume = level.coerceIn(0f, 1f)
        }
    }

    /** Whether a stream is standing in for this person's sound now. */
    fun playing(identity: String, music: Boolean): Boolean = synchronized(lock) { key(identity, music) in streams }

    fun closeAll() {
        val all = synchronized(lock) {
            val out = streams.values.toList()
            streams.clear()
            out
        }
        for (stream in all) audio.detachLater(stream)
    }

    private fun CallAudio.detachLater(stream: Stream) {
        main.post { detach(stream) }
    }

    companion object {
        fun key(identity: String, music: Boolean) = identity + if (music) "/music" else "/screen"
    }

    /**
     * One stream, at its own rate, taken out at the mixer's 48 kHz by linear
     * interpolation between the two source frames either side.
     */
    private class Stream(key: String, val rate: Int, channels: Int) : MixerSource(key) {
        // The Apple version's cushion: twenty milliseconds before it plays,
        // two hundred at most, settling back to forty.
        val ring = PcmRing(channels, startFrames = rate / 50, ceilingFrames = rate / 5, settleFrames = rate / 25)
        private val step = rate / 48_000.0
        private var position = 1.0
        private val previous = FloatArray(channels)
        private val next = FloatArray(channels)
        private val block = FloatArray(512 * channels)
        private var blockFrames = 0
        private var blockAt = 0

        override fun render(into: FloatArray, frames: Int, gain: Float): Double {
            val channels = ring.channels
            var energy = 0.0
            for (frame in 0 until frames) {
                while (position >= 1.0) {
                    System.arraycopy(next, 0, previous, 0, channels)
                    if (!pull()) java.util.Arrays.fill(next, 0f)
                    position -= 1.0
                }
                val f = position.toFloat()
                val left = previous[0] + (next[0] - previous[0]) * f
                val right = if (channels > 1) previous[1] + (next[1] - previous[1]) * f else left
                into[frame * 2] += left * gain
                into[frame * 2 + 1] += right * gain
                energy += (left * left + right * right) / 2.0
                position += step
            }
            return energy
        }

        /** The next source frame into [next], from the block or the ring; false where there is none. */
        private fun pull(): Boolean {
            val channels = ring.channels
            if (blockAt >= blockFrames) {
                blockFrames = ring.read(block, block.size / channels)
                blockAt = 0
                if (blockFrames == 0) return false
            }
            System.arraycopy(block, blockAt * channels, next, 0, channels)
            blockAt++
            return true
        }
    }
}
