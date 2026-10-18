package app.tomoshibi.media

import kotlin.math.sqrt

/**
 * How loud a sound is, as a root-mean-square level from 0 to 1 over the last
 * tenth of a second: written by the thread the sound passes through, read by
 * a meter whenever it draws. Holding nothing open of its own, a meter shows
 * exactly what is being sent or played.
 */
class SoundLevel {
    @Volatile
    var rms: Double = 0.0
        private set

    private var energy = 0.0
    private var samples = 0

    /** Adds samples already summed as squares, from a mixer that has them. */
    fun addEnergy(sumOfSquares: Double, count: Int, rate: Int = 48_000) {
        energy += sumOfSquares
        samples += count
        if (samples >= rate / 10) publish()
    }

    /** Adds 16-bit little-endian samples. */
    fun add(pcm: ByteArray, length: Int = pcm.size, rate: Int = 48_000) {
        var i = 0
        while (i + 1 < length) {
            val s = ((pcm[i].toInt() and 0xff) or (pcm[i + 1].toInt() shl 8)).toShort() / 32768.0
            energy += s * s
            samples++
            i += 2
        }
        if (samples >= rate / 10) publish()
    }

    private fun publish() {
        rms = if (samples > 0) sqrt(energy / samples) else 0.0
        energy = 0.0
        samples = 0
    }
}
