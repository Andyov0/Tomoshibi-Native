package app.tomoshibi.media

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * A stream at its own rate taken out at the mixer's, through a windowed-sinc
 * filter (a Kaiser window over 48 zero crossings a side, in a polyphase bank).
 *
 * A lossless stream arrives at whatever rate it was captured at: a song at
 * 44.1 kHz, a Mac's shared sound at 48, a file at 96. On Apple the engine
 * plays each at its own rate and its mixer makes the one conversion the
 * hardware needs (LosslessPlayback.swift); here one AudioTrack carries the
 * whole call at 48 kHz, so that the echo canceller hears everything the call
 * plays, and the conversion is this. It replaced a linear interpolation,
 * which for a 44.1 kHz song is a filter whose stopband barely exists: the
 * images of everything above a few kilohertz folded back into the band as a
 * haze, in the one sound somebody chose to send without loss.
 *
 * The filter passes everything below 94.5 percent of the lower of the two
 * Nyquist frequencies (20.8 kHz for 44.1 kHz in) and is down about ninety
 * decibels by the Nyquist itself, so nothing folds back into what anybody
 * hears. Each phase is normalised to a gain of exactly one, so a constant
 * stays constant and no phase is louder than the next. A stream already at
 * the mixer's rate is copied through untouched.
 *
 * The ratio is held as a fraction of whole numbers, so the two clocks never
 * drift apart by rounding: 44.1 to 48 is 147 input frames to 160 output,
 * exactly, for ever. A rate whose fraction would need more than
 * [MOST_PHASES] phases (none that anybody records at) is taken at the
 * nearest fraction that does not, a pitch error of under a millionth, which
 * the ring under it absorbs as it absorbs the network's clock.
 *
 * Not thread-safe: one per stream, used by the mixer's thread alone.
 */
class Resampler(val from: Int, val to: Int, val channels: Int, private val source: Source) {
    /** Where input frames come from: interleaved into [into] at [offset], up to [frames]; how many came. */
    fun interface Source {
        fun read(into: FloatArray, offset: Int, frames: Int): Int
    }

    /** Output frames per [step] input frames: the ratio as phases and stride. */
    private val phases: Int
    private val step: Int
    /** Taps a side; each output reads [taps] = 2 * [half] input frames. */
    private val half: Int
    private val taps: Int
    private val bank: FloatArray
    private val passthrough = from == to

    /** Input frames in hand, interleaved: [start] is the oldest the next output reads, [end] one past the newest. */
    private val buffer: FloatArray
    private var start = 0
    private var end = 0
    /** Where the next output falls between input frames, in phases. */
    private var phase = 0

    init {
        require(from > 0 && to > 0 && channels > 0)
        if (passthrough) {
            phases = 1
            step = 1
            half = 0
            taps = 1
            bank = FloatArray(0)
        } else {
            val (p, s) = fraction(from, to)
            phases = p
            step = s
            // The filter's scale: its cutoff in input frames, below the
            // lower of the two Nyquists.
            val scale = ROLLOFF * minOf(1.0, to.toDouble() / from)
            val window = ZERO_CROSSINGS / scale
            half = ceil(window).toInt() + 1
            taps = 2 * half
            bank = FloatArray(phases * taps)
            for (p0 in 0 until phases) {
                var sum = 0.0
                val row = p0 * taps
                for (k in 0 until taps) {
                    // How far this tap's input frame is from the output's
                    // instant, in input frames.
                    val distance = (half - 1 - k) + p0.toDouble() / phases
                    val value = if (abs(distance) >= window) 0.0 else sinc(distance * scale) * kaiser(distance / window)
                    bank[row + k] = value.toFloat()
                    sum += value
                }
                for (k in 0 until taps) bank[row + k] = (bank[row + k] / sum).toFloat()
            }
        }
        buffer = FloatArray((taps + BLOCK) * channels)
        // The first output stands at the newest of `half` frames of silence,
        // so the stream starts at once rather than after a filter's length.
        end = maxOf(half - 1, 0)
    }

    /**
     * Adds [frames] frames at [to] into [into] (interleaved stereo; a mono
     * stream goes to both sides), each times its own gain from [gains] when
     * given or [gain] otherwise, taking input frames from the source as
     * needed. A source that has run dry gives none, and silence is filtered
     * in its place. Returns the sum of squares of what was added, before the
     * gain.
     */
    fun render(into: FloatArray, frames: Int, gain: Float, gains: FloatArray? = null): Double {
        var energy = 0.0
        val channels = channels
        val buffer = buffer
        for (frame in 0 until frames) {
            if (end - start < taps) fill()
            val at = start * channels
            var left: Float
            var right: Float
            if (passthrough) {
                left = buffer[at]
                right = if (channels > 1) buffer[at + 1] else left
                start++
            } else {
                val row = phase * taps
                var l = 0f
                var r = 0f
                var i = at
                if (channels > 1) {
                    for (k in row until row + taps) {
                        val c = bank[k]
                        l += c * buffer[i]
                        r += c * buffer[i + 1]
                        i += channels
                    }
                } else {
                    for (k in row until row + taps) {
                        l += bank[k] * buffer[i]
                        i++
                    }
                    r = l
                }
                left = l
                right = r
                phase += step
                while (phase >= phases) {
                    phase -= phases
                    start++
                }
            }
            val g = gains?.get(frame) ?: gain
            into[frame * 2] += left * g
            into[frame * 2 + 1] += right * g
            energy += (left * left + right * right) / 2.0
        }
        return energy
    }

    /** Input frames from the source until a whole filter's worth is in hand, silence where it has none. */
    private fun fill() {
        val capacity = buffer.size / channels
        if (start > 0 && end + BLOCK / 2 > capacity) {
            System.arraycopy(buffer, start * channels, buffer, 0, (end - start) * channels)
            end -= start
            start = 0
        }
        while (end - start < taps) {
            val got = source.read(buffer, end * channels, capacity - end)
            if (got <= 0) {
                val upTo = start + taps
                java.util.Arrays.fill(buffer, end * channels, upTo * channels, 0f)
                end = upTo
                return
            }
            end += got
        }
    }

    companion object {
        /** Zero crossings of the sinc on each side of the centre. */
        const val ZERO_CROSSINGS = 48.0
        /** Where the passband ends, as a share of the lower Nyquist. */
        const val ROLLOFF = 0.945
        /** Kaiser's beta, for about ninety decibels down in the stopband. */
        const val BETA = 9.0
        /** The most phases a bank is built with: 640 holds 11.025 kHz exactly. */
        const val MOST_PHASES = 1024
        /** Input frames read from the ring at a time. */
        private const val BLOCK = 512

        private fun sinc(x: Double): Double = if (x == 0.0) 1.0 else sin(PI * x) / (PI * x)

        /** The Kaiser window at [x], from -1 to 1. */
        private fun kaiser(x: Double): Double {
            val inside = 1.0 - x * x
            if (inside <= 0.0) return 0.0
            return bessel(BETA * sqrt(inside)) / bessel(BETA)
        }

        /** The modified Bessel function of the first kind, order zero, by its series. */
        private fun bessel(x: Double): Double {
            var sum = 1.0
            var term = 1.0
            val half = x / 2
            for (k in 1 until 64) {
                term *= (half / k) * (half / k)
                sum += term
                if (term < sum * 1e-12) break
            }
            return sum
        }

        /**
         * Output frames over input frames as whole numbers (phases, step):
         * [to] over [from] in lowest terms, or the nearest fraction with no
         * more than [MOST_PHASES] phases, by its continued fraction.
         */
        internal fun fraction(from: Int, to: Int): Pair<Int, Int> {
            val common = gcd(from, to)
            if (to / common <= MOST_PHASES) return (to / common) to (from / common)
            // The convergents of from/to, the last whose denominator fits.
            var x = from.toDouble() / to
            var h0 = 0L
            var h1 = 1L
            var k0 = 1L
            var k1 = 0L
            var best = 1 to maxOf(1, Math.round(x).toInt())
            for (i in 0 until 32) {
                val a = Math.floor(x).toLong()
                val h2 = a * h1 + h0
                val k2 = a * k1 + k0
                if (k2 > MOST_PHASES) break
                best = k2.toInt() to h2.toInt()
                h0 = h1
                h1 = h2
                k0 = k1
                k1 = k2
                val rest = x - a
                if (rest < 1e-12) break
                x = 1 / rest
            }
            return best
        }

        private tailrec fun gcd(a: Int, b: Int): Int = if (b == 0) a else gcd(b, a % b)
    }
}
