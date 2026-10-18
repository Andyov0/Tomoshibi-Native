package app.tomoshibi.media

import kotlin.math.sqrt

/**
 * How loud a sound is, as a root-mean-square level from 0 to 1 over the last
 * tenth of a second: written by the thread the sound passes through, read by
 * a meter whenever it draws. Holding nothing open of its own, a meter shows
 * exactly what is being sent or played.
 *
 * Also whether anything has arrived at all ([delivered], [lastChunk],
 * [lastDelivered]): the devices panel says "No sound from it" for a
 * microphone that has handed over nothing two seconds after it was opened
 * (the Apple apps' MicrophoneMeterRow, by SoundLevel's `lastChunk`), which a
 * level of nought cannot tell from a quiet room. A microphone the system
 * silences still delivers its zeros, and is "Say something", not "No sound".
 */
class SoundLevel {
    @Volatile
    var rms: Double = 0.0
        private set

    /** How many samples the latest delivery held; nought until one has come. */
    @Volatile
    var lastChunk: Int = 0
        private set

    /** When the latest delivery came, on System.nanoTime's clock; nought until one has. */
    @Volatile
    var lastDelivered: Long = 0L
        private set

    /** Whether any sound has been handed over at all. */
    val delivered: Boolean get() = lastChunk > 0

    private var energy = 0.0
    private var samples = 0

    /** Adds samples already summed as squares, from a mixer that has them. */
    fun addEnergy(sumOfSquares: Double, count: Int, rate: Int = 48_000) {
        arrived(count)
        energy += sumOfSquares
        samples += count
        if (samples >= rate / 10) publish()
    }

    /** Adds 16-bit little-endian samples. */
    fun add(pcm: ByteArray, length: Int = pcm.size, rate: Int = 48_000) {
        arrived(length / 2)
        var i = 0
        while (i + 1 < length) {
            val s = ((pcm[i].toInt() and 0xff) or (pcm[i + 1].toInt() shl 8)).toShort() / 32768.0
            energy += s * s
            samples++
            i += 2
        }
        if (samples >= rate / 10) publish()
    }

    private fun arrived(count: Int) {
        if (count <= 0) return
        lastChunk = count
        lastDelivered = System.nanoTime()
    }

    private fun publish() {
        rms = if (samples > 0) sqrt(energy / samples) else 0.0
        energy = 0.0
        samples = 0
    }
}
