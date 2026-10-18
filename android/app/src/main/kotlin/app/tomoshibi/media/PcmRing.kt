package app.tomoshibi.media

/**
 * Sound handed over from the core's threads to the mixer's (the Apple apps'
 * PCMRing, with the same numbers).
 *
 * The core delivers each remote track in 10 ms pieces on a clock of its own;
 * the speaker asks for its buffer on another. The two drift, so the ring keeps
 * a little in hand, starts playing only once it has some, plays silence rather
 * than stale sound when it runs dry, and throws away the oldest sound when it
 * has gathered more than it should hold: a voice that falls further and
 * further behind is worse than one that skips a few milliseconds once. WebRTC
 * has already absorbed the network's jitter before any of this sees the
 * sound, so the cushion here is only for the two clocks.
 *
 * The writer is a core thread, which must never wait on anything slow; the
 * only wait is this ring's own short lock, held for one copy at a time.
 * Frames are per channel; at 48 kHz, 480 frames are 10 ms.
 */
class PcmRing(
    val channels: Int,
    private val startFrames: Int = 1440,
    private val ceilingFrames: Int = 9600,
    private val settleFrames: Int = 2880,
) {
    private val capacity = ceilingFrames * 2
    private val samples = FloatArray(capacity * channels)
    private var read = 0
    private var count = 0
    private var playing = false
    private val lock = Any()

    /** Frames written and played since the last [take], for a log that proves sound arrives. */
    private var written = 0
    private var rendered = 0

    /** Interleaved 16-bit little-endian samples, as the core's PcmSink hands them over. */
    fun write(pcm: ByteArray, sourceChannels: Int = channels) {
        val frames = pcm.size / 2 / sourceChannels
        synchronized(lock) {
            written += frames
            for (frame in 0 until frames) {
                val at = slot()
                for (c in 0 until channels) {
                    // A track that arrives in another channel count than it
                    // was announced with is folded rather than misread: one
                    // channel copied to both, or the first of two taken.
                    val from = (frame * sourceChannels + minOf(c, sourceChannels - 1)) * 2
                    val sample = (pcm[from].toInt() and 0xff) or (pcm[from + 1].toInt() shl 8)
                    samples[at + c] = sample.toShort() / 32768f
                }
                count++
            }
            settle()
        }
    }

    /** Interleaved 32-bit float samples, as a lossless stream arrives. */
    fun write(floats: List<Float>) {
        val frames = floats.size / channels
        synchronized(lock) {
            written += frames
            for (frame in 0 until frames) {
                val at = slot()
                for (c in 0 until channels) samples[at + c] = floats[frame * channels + c]
                count++
            }
            settle()
        }
    }

    /** Where the next frame goes, dropping the oldest when the ring is full. */
    private fun slot(): Int {
        if (count == capacity) {
            read = (read + 1) % capacity
            count--
        }
        return ((read + count) % capacity) * channels
    }

    private fun settle() {
        if (count > ceilingFrames) {
            val drop = count - settleFrames
            read = (read + drop) % capacity
            count -= drop
        }
    }

    /**
     * Adds [frames] frames of this ring, times [gain], into [into], which is
     * interleaved stereo; a mono ring goes to both sides. Silence where the
     * ring has run dry. Returns the sum of squares of what was added, for a
     * level.
     */
    fun mix(into: FloatArray, frames: Int, gain: Float): Double {
        var energy = 0.0
        synchronized(lock) {
            val available = available(frames)
            for (frame in 0 until available) {
                val at = ((read + frame) % capacity) * channels
                val left = samples[at]
                val right = if (channels > 1) samples[at + 1] else left
                into[frame * 2] += left * gain
                into[frame * 2 + 1] += right * gain
                energy += (left * left + right * right) / 2.0
            }
            read = (read + available) % capacity
            count -= available
            rendered += frames
        }
        return energy
    }

    /**
     * Takes up to [frames] frames out, interleaved in this ring's own channel
     * count, for a reader that resamples; returns how many it took, nought
     * while the ring is gathering its cushion or has run dry.
     */
    fun read(into: FloatArray, frames: Int): Int = read(into, 0, frames)

    /** The same, written into [into] from [offset] (in samples, not frames). */
    fun read(into: FloatArray, offset: Int, frames: Int): Int {
        synchronized(lock) {
            val available = available(frames)
            for (frame in 0 until available) {
                val at = ((read + frame) % capacity) * channels
                for (c in 0 until channels) into[offset + frame * channels + c] = samples[at + c]
            }
            read = (read + available) % capacity
            count -= available
            rendered += available
            return available
        }
    }

    private fun available(frames: Int): Int {
        if (!playing && count >= startFrames) playing = true
        if (playing && count == 0) playing = false
        return if (playing) minOf(frames, count) else 0
    }

    /** Frames written in and played out since the last call. */
    fun take(): Pair<Int, Int> = synchronized(lock) {
        val out = written to rendered
        written = 0
        rendered = 0
        out
    }

    /** How many frames are waiting, for a test. */
    val buffered: Int get() = synchronized(lock) { count }

    fun clear() {
        synchronized(lock) {
            read = 0
            count = 0
            playing = false
        }
    }
}
