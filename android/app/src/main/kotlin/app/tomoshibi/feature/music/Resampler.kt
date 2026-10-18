package app.tomoshibi.feature.music

import kotlin.math.PI
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Two channels of sound brought from one rate to another, a piece at a time:
 * a polyphase filter with a Kaiser-windowed sinc, at the exact ratio of the
 * two rates (160 to 147 for 44.1 kHz into 48).
 *
 * Why the app has one of its own: the core's Opus source takes 48 kHz and
 * nothing else, and the web leaves the step to the browser, which does it
 * well. On Apple the step is AVAudioConverter at its mastering quality
 * (MusicLoader.swift `decode`); Android offers nothing like it for a buffer
 * an app holds. A linear step, the obvious stand-in, folds the top octave of
 * a 44.1 kHz recording back down as audible whistles.
 *
 * Sixty-four taps a phase, and more where the rate comes down, with the
 * passband ending at 91 per cent of the lower Nyquist frequency: about 90 dB
 * below the music for anything folded, at 20 kHz for a CD, which nobody
 * hears and Opus throws away anyway.
 */
class Resampler(val inRate: Int, val outRate: Int = 48_000) {
    private val up: Int
    private val down: Int
    private val taps: Int
    private val table: FloatArray
    private val delay: Long

    /** Input frames kept, interleaved, and the index in the whole stream of the first. */
    private var held = FloatArray(0)
    private var heldFrames = 0
    private var heldStart = 0L
    /** Input frames given so far, and output frames made. */
    private var given = 0L
    private var made = 0L
    private var finished = false

    init {
        require(inRate > 0 && outRate > 0)
        val common = gcd(inRate, outRate)
        up = outRate / common
        down = inRate / common
        taps = 64 * max(1, ceil(inRate.toDouble() / outRate).toInt())
        val length = up * taps
        // The cut-off on the upsampled grid, in cycles a sample.
        val cutoff = 0.91 * 0.5 * min(inRate, outRate) / (inRate.toDouble() * up)
        val centre = (length - 1) / 2.0
        val beta = 8.0
        val norm = besselI0(beta)
        table = FloatArray(length)
        for (k in 0 until length) {
            val x = k - centre
            val sinc = if (x == 0.0) 2 * cutoff else sin(2 * PI * cutoff * x) / (PI * x)
            val r = x / centre
            val window = besselI0(beta * sqrt(max(0.0, 1 - r * r))) / norm
            // Times `up`: stuffing zeros between the samples divides their
            // energy by as much.
            table[k] = (sinc * window * up).toFloat()
        }
        delay = ((length - 1) / 2).toLong()
    }

    /** Whether the rates are the same, and the sound goes through untouched. */
    val passes: Boolean get() = up == down

    /**
     * Takes [frames] frames of [input], two channels interleaved, and hands
     * [out] every frame of output they complete, interleaved, with how many.
     */
    fun push(input: FloatArray, frames: Int, out: (FloatArray, Int) -> Unit) {
        if (passes) {
            out(input, frames)
            return
        }
        hold(input, frames)
        given += frames
        run(out)
    }

    /** The end of the input: whatever the filter still holds comes out, against silence. */
    fun finish(out: (FloatArray, Int) -> Unit) {
        if (passes || finished) return
        finished = true
        run(out)
    }

    /** The number of output frames a whole input of [frames] frames makes. */
    fun outputFrames(frames: Long): Long = if (passes) frames else (frames * up + down - 1) / down

    private fun hold(input: FloatArray, frames: Int) {
        // What no output to come can reach is dropped: everything before the
        // oldest frame the next output's filter touches.
        val next = (made * down + delay) / up
        val keepFrom = max(heldStart, next - taps)
        val drop = (keepFrom - heldStart).toInt().coerceIn(0, heldFrames)
        val need = (heldFrames - drop + frames) * 2
        if (need > held.size) {
            val grown = FloatArray(max(need, held.size * 2))
            System.arraycopy(held, drop * 2, grown, 0, (heldFrames - drop) * 2)
            held = grown
        } else if (drop > 0) {
            System.arraycopy(held, drop * 2, held, 0, (heldFrames - drop) * 2)
        }
        heldFrames -= drop
        heldStart += drop
        System.arraycopy(input, 0, held, heldFrames * 2, frames * 2)
        heldFrames += frames
    }

    private fun run(out: (FloatArray, Int) -> Unit) {
        val total = outputFrames(given)
        val piece = FloatArray(PIECE * 2)
        var count = 0
        while (made < total) {
            val t = made * down + delay
            val base = t / up
            // Not yet: the newest frame this output needs has not been given.
            if (!finished && base >= given) break
            val phase = (t % up).toInt()
            var left = 0f
            var right = 0f
            var k = phase
            var index = (base - heldStart).toInt()
            val size = table.size
            val limit = heldFrames
            val frames = held
            while (k < size) {
                if (index in 0..<limit) {
                    val coefficient = table[k]
                    left += coefficient * frames[index * 2]
                    right += coefficient * frames[index * 2 + 1]
                }
                k += up
                index -= 1
            }
            piece[count * 2] = left
            piece[count * 2 + 1] = right
            count += 1
            made += 1
            if (count == PIECE) {
                out(piece, count)
                count = 0
            }
        }
        if (count > 0) out(piece, count)
    }

    private companion object {
        const val PIECE = 4096

        fun gcd(a: Int, b: Int): Int = if (b == 0) a else gcd(b, a % b)

        /** The modified Bessel function of the first kind, order nought, by its series. */
        fun besselI0(x: Double): Double {
            var sum = 1.0
            var term = 1.0
            val half = x / 2
            var k = 1
            while (term > 1e-12 * sum) {
                term *= (half / k) * (half / k)
                sum += term
                k++
            }
            return sum
        }
    }
}
