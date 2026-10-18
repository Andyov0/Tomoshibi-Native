package app.tomoshibi.media

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The conversion a lossless stream goes through on its way into the mixer.
 * What these guard is the haze the linear interpolation it replaced left in
 * a 44.1 kHz song (the images of the top octave folded back into the band),
 * a stream whose level wobbles with the phase, and two clocks drifting apart.
 * Broken on purpose with a linear interpolation in its place, the image test
 * fails at about -15 dB.
 */
class ResamplerTest {
    /** A source of [frames] interleaved frames made by [sample], and how many have been taken from it. */
    private class Tone(val channels: Int, val frames: Int, val sample: (Int, Int) -> Float) : Resampler.Source {
        var taken = 0
        override fun read(into: FloatArray, offset: Int, frames: Int): Int {
            val count = minOf(frames, this.frames - taken)
            for (f in 0 until count) for (c in 0 until channels) into[offset + f * channels + c] = sample(taken + f, c)
            taken += count
            return count
        }
    }

    /** The amplitude of [frequency] in one channel of interleaved stereo, by a Goertzel filter. */
    private fun amplitude(stereo: FloatArray, channel: Int, from: Int, frames: Int, frequency: Double, rate: Int): Double {
        val w = 2 * PI * frequency / rate
        val k = 2 * cos(w)
        var s1 = 0.0
        var s2 = 0.0
        for (n in 0 until frames) {
            val s = stereo[(from + n) * 2 + channel] + k * s1 - s2
            s2 = s1
            s1 = s
        }
        val power = s1 * s1 + s2 * s2 - k * s1 * s2
        return 2 * sqrt(power) / frames
    }

    private fun render(resampler: Resampler, frames: Int): FloatArray {
        val out = FloatArray(frames * 2)
        // In the mixer's own pieces of ten milliseconds.
        var done = 0
        while (done < frames) {
            val piece = minOf(480, frames - done)
            val block = FloatArray(piece * 2)
            resampler.render(block, piece, 1f)
            System.arraycopy(block, 0, out, done * 2, piece * 2)
            done += piece
        }
        return out
    }

    @Test
    fun aStreamAtTheMixersRateGoesThroughUntouched() {
        val tone = Tone(2, 48_000, { n, c -> ((n * 7 + c * 3) % 101 - 50) / 64f })
        val out = render(Resampler(48_000, 48_000, 2, tone), 4_800)
        for (n in 0 until 4_800) for (c in 0 until 2) assertEquals(tone.sample(n, c), out[n * 2 + c], 0f)
    }

    @Test
    fun aConstantStaysConstantAtEveryPhase() {
        val tone = Tone(1, 200_000, { _, _ -> 0.25f })
        val out = render(Resampler(44_100, 48_000, 1, tone), 48_000)
        // Past the filter's first length, where it is still filling from silence.
        for (n in 200 until 48_000) {
            assertEquals("left at $n", 0.25f, out[n * 2], 2e-6f)
            assertEquals("mono to both sides at $n", out[n * 2], out[n * 2 + 1], 0f)
        }
    }

    @Test
    fun aHighToneKeepsItsLevelAndFoldsNothingBack() {
        // 15 kHz at 44.1 kHz: its first image, at 29.1 kHz, lands at 18.9 kHz
        // once taken to 48 kHz, well inside what anybody hears.
        val rate = 44_100
        val tone = Tone(2, rate * 3, { n, c -> (0.5 * sin(2 * PI * 15_000 * n / rate)).toFloat() * if (c == 0) 1f else -1f })
        val out = render(Resampler(rate, 48_000, 2, tone), 48_000 * 2)
        val from = 4_800
        val frames = 48_000
        val kept = amplitude(out, 0, from, frames, 15_000.0, 48_000)
        val folded = amplitude(out, 0, from, frames, 18_900.0, 48_000)
        assertEquals("15 kHz kept at its level", 0.5, kept, 0.5 * 0.002)
        assertTrue("the image at 18.9 kHz is ${20 * kotlin.math.log10(folded / 0.5)} dB", folded / 0.5 < 1e-4)
        assertEquals("the other channel too, inverted", -out[(from + 10) * 2], out[(from + 10) * 2 + 1], 1e-6f)
    }

    @Test
    fun theTwoClocksNeverDriftApart() {
        // 147 input frames to every 160 out, exactly: ten seconds out take
        // ten seconds in, give or take the filter's length in hand.
        val tone = Tone(2, 44_100 * 20, { _, _ -> 0f })
        val resampler = Resampler(44_100, 48_000, 2, tone)
        render(resampler, 480_000)
        assertTrue("took ${tone.taken} input frames for ten seconds", tone.taken in 441_000..441_000 + 1_200)
    }

    @Test
    fun aDryStreamIsSilenceRatherThanAStall() {
        val tone = Tone(2, 0, { _, _ -> 1f })
        val out = render(Resampler(44_100, 48_000, 2, tone), 960)
        assertTrue(out.all { it == 0f })
    }

    @Test
    fun ratiosAreHeldAsWholeNumbers() {
        assertEquals(160 to 147, Resampler.fraction(44_100, 48_000))
        assertEquals(1 to 2, Resampler.fraction(96_000, 48_000))
        assertEquals(3 to 2, Resampler.fraction(32_000, 48_000))
        assertEquals(640 to 147, Resampler.fraction(11_025, 48_000))
        // A rate nobody records at comes to the nearest fraction that fits,
        // within a millionth.
        val (phases, step) = Resampler.fraction(44_101, 48_000)
        assertTrue(phases <= Resampler.MOST_PHASES)
        assertEquals(44_101.0 / 48_000, step.toDouble() / phases, 1e-6)
    }
}
