package app.tomoshibi.feature.music

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.log10
import kotlin.math.sin
import kotlin.math.sqrt
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The step from a file's rate to the core's 48 kHz. What would go wrong
 * unheard in a unit test and heard by everybody in a call: a song a little
 * fast or slow (a length that does not follow the ratio), the top octave of a
 * CD folded back down as whistles (images left in), a click every time a
 * piece of the decoder's output began (a filter that forgot its history), a
 * quieter song (a gain that is not one).
 */
class ResamplerTest {
    private fun tone(rate: Int, hertz: Double, seconds: Double, level: Double = 0.5): FloatArray {
        val frames = (rate * seconds).toInt()
        return FloatArray(frames * 2) { i -> (level * sin(2 * PI * hertz * (i / 2) / rate)).toFloat() }
    }

    private fun run(resampler: Resampler, input: FloatArray, piece: Int = Int.MAX_VALUE): FloatArray {
        val out = ArrayList<Float>()
        val take: (FloatArray, Int) -> Unit = { samples, frames -> for (i in 0 until frames * 2) out.add(samples[i]) }
        var at = 0
        val frames = input.size / 2
        while (at < frames) {
            val count = minOf(piece, frames - at)
            resampler.push(input.copyOfRange(at * 2, (at + count) * 2), count, take)
            at += count
        }
        resampler.finish(take)
        return out.toFloatArray()
    }

    /** How strong [hertz] is in the left channel of [samples], through a Hann window over its middle. */
    private fun strength(samples: FloatArray, rate: Int, hertz: Double): Double {
        val frames = samples.size / 2
        val length = 16_384
        val start = (frames - length) / 2
        var re = 0.0
        var im = 0.0
        for (n in 0 until length) {
            val window = 0.5 - 0.5 * cos(2 * PI * n / (length - 1))
            val x = samples[(start + n) * 2] * window
            val angle = 2 * PI * hertz * n / rate
            re += x * cos(angle)
            im -= x * sin(angle)
        }
        return sqrt(re * re + im * im) / (length / 4.0)
    }

    private fun decibels(ratio: Double) = 20 * log10(ratio)

    @Test
    fun aSongAt48kHzGoesThroughUntouched() {
        val resampler = Resampler(48_000)
        assertTrue(resampler.passes)
        val input = tone(48_000, 1000.0, 0.1)
        assertArrayEquals(input, run(resampler, input), 0f)
    }

    @Test
    fun aCdIsTheRightLengthAndTheRightLoudness() {
        val input = tone(44_100, 1000.0, 1.0)
        val output = run(Resampler(44_100), input)
        // Every 147 frames in are 160 out, rounded up at the end.
        assertEquals((44_100L * 160 + 146) / 147, (output.size / 2).toLong())
        assertEquals(0.5, strength(output, 48_000, 1000.0), 0.005)
        // The tone is where it was: nothing of it a few hertz either side.
        assertTrue(strength(output, 48_000, 1000.0) > 20 * strength(output, 48_000, 1100.0))
    }

    @Test
    fun theImagesOfAHighNoteAreNotHeard() {
        // A 10 kHz note in a 44.1 kHz file leaves an image at 34.1 kHz on
        // the way up, which a 48 kHz stream folds back to 13.9 kHz.
        val output = run(Resampler(44_100), tone(44_100, 10_000.0, 1.0))
        val note = strength(output, 48_000, 10_000.0)
        assertEquals(0.5, note, 0.01)
        assertTrue("image at ${decibels(strength(output, 48_000, 13_900.0) / note)} dB", decibels(strength(output, 48_000, 13_900.0) / note) < -80)
    }

    @Test
    fun comingDownFromHiResWhatCannotBeKeptIsTakenOutNotFolded() {
        // 30 kHz in a 96 kHz file would fold to 18 kHz at 48 kHz.
        val folded = run(Resampler(96_000), tone(96_000, 30_000.0, 0.5))
        assertTrue(decibels(strength(folded, 48_000, 18_000.0) / 0.5) < -70)
        val kept = run(Resampler(96_000), tone(96_000, 10_000.0, 0.5))
        assertEquals(0.5, strength(kept, 48_000, 10_000.0), 0.01)
    }

    @Test
    fun theSameSoundComesOutHoweverTheDecoderCutsIt() {
        val input = tone(44_100, 440.0, 0.5)
        val whole = run(Resampler(44_100), input)
        val pieces = run(Resampler(44_100), input, piece = 1153)
        assertArrayEquals(whole, pieces, 0f)
    }
}

/**
 * The music going out, 10 ms at a time. What would go wrong: a song that
 * ends and is never said to have ended (the desk waits for ever), a song that
 * is said to end twice (two songs skipped), a lossless stream that drifts
 * from its rate or has holes in its clock (every listener gives up on it),
 * and a stream that stops while paused (the web measured that a silent track
 * stops being delivered).
 */
class MusicOutputTest {
    private class Taken : MusicInput {
        override fun pushPcm(pcm: ByteArray) {}
        override fun pushLossless(samples: FloatArray, rate: Int, timestampUs: Long) {}
    }

    private class Original(override val rate: Int) : OriginalSamples {
        var taken = 0L
        var closed = false
        override fun take(count: Int, into: FloatArray): Int {
            for (frame in 0 until count) {
                into[frame * 2] = 0.25f
                into[frame * 2 + 1] = -0.25f
            }
            taken += count
            return count
        }
        override fun close() {
            closed = true
        }
    }

    /** A song of [frames] frames whose every sample says which frame it is. */
    private fun song(frames: Int, original: OriginalSamples? = null): DecodedSong {
        val pieces = ArrayList<ByteArray>()
        var at = 0
        while (at < frames) {
            val count = minOf(Pcm48.PIECE_FRAMES, frames - at)
            val piece = ByteArray(Pcm48.PIECE_FRAMES * 4)
            for (frame in 0 until count) {
                val value = (at + frame) % 30_000
                for (c in 0..1) {
                    piece[frame * 4 + c * 2] = value.toByte()
                    piece[frame * 4 + c * 2 + 1] = (value shr 8).toByte()
                }
            }
            pieces.add(piece)
            at += count
        }
        return DecodedSong(Pcm48(pieces, frames), original, original?.rate ?: 48_000, null)
    }

    private fun frameAt(block: ByteArray, frame: Int): Int = (block[frame * 4].toInt() and 0xff) or (block[frame * 4 + 1].toInt() shl 8)

    @Test
    fun aSongPlaysThenSaysOnceThatItEndedThenSilenceFlows() {
        val output = MusicOutput(Taken(), lossless = false, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(1000), "song-1")
        assertNull(output.fill(block)!!.ended)
        assertEquals(0, frameAt(block, 0))
        assertEquals(479, frameAt(block, 479))
        assertNull(output.fill(block)!!.ended)
        assertEquals(480, frameAt(block, 0))
        assertEquals("song-1", output.fill(block)!!.ended)
        assertEquals(960, frameAt(block, 0))
        assertEquals(0, frameAt(block, 40))
        // After the end, silence goes on, and the end is not said again.
        repeat(5) {
            val filled = output.fill(block)!!
            assertNull(filled.ended)
            assertTrue(block.all { it == 0.toByte() })
        }
    }

    /** Across a piece's edge, which a block never straddles because a second is a whole number of them. */
    @Test
    fun aLongSongIsReadAcrossItsPiecesWithoutAGap() {
        val output = MusicOutput(Taken(), lossless = false, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(Pcm48.PIECE_FRAMES + 960), "long")
        repeat(Pcm48.PIECE_FRAMES / MusicOutput.FRAME) { output.fill(block) }
        output.fill(block)
        assertEquals(Pcm48.PIECE_FRAMES % 30_000, frameAt(block, 0))
        assertEquals((Pcm48.PIECE_FRAMES + 479) % 30_000, frameAt(block, 479))
    }

    @Test
    fun pausedItSendsSilenceAndGoesOnFromWhereItWas() {
        val output = MusicOutput(Taken(), lossless = false, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(5000), "song")
        output.fill(block)
        output.pause(true)
        repeat(3) {
            assertNotNull(output.fill(block))
            assertTrue(block.all { it == 0.toByte() })
        }
        output.pause(false)
        output.fill(block)
        assertEquals(480, frameAt(block, 0))
    }

    @Test
    fun theLosslessStreamKeepsItsRateWithNoHolesInItsClock() {
        val original = Original(22_050)
        val output = MusicOutput(Taken(), lossless = true, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(48_000 * 3, original), "song")
        var frames = 0L
        var expectedAt = 0L
        repeat(200) {
            val sent = output.fill(block)!!.original
            if (sent != null) {
                assertEquals(22_050, sent.rate)
                // Where each piece begins is where the last one ended.
                assertEquals(expectedAt, sent.at)
                frames += sent.samples.size / 2
                expectedAt = frames * 1_000_000 / 22_050
            }
        }
        // Two seconds of blocks, two seconds of the file: 220 and 221 frames
        // in turn, never drifting from 22,050 a second.
        assertEquals(44_100L, frames)
        assertEquals(44_100L, original.taken)
    }

    @Test
    fun theLosslessStreamGoesOnInSilenceWhilePaused() {
        val original = Original(44_100)
        val output = MusicOutput(Taken(), lossless = true, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(48_000, original), "song")
        output.pause(true)
        val sent = output.fill(block)!!.original!!
        assertEquals(441, sent.samples.size / 2)
        assertTrue(sent.samples.all { it == 0f })
        assertEquals(0L, original.taken)
    }

    @Test
    fun aSongAtAnotherRateStartsTheStreamAfreshAndTheOneItReplacedLetsItsFileGo() {
        val first = Original(44_100)
        val output = MusicOutput(Taken(), lossless = true, monitor = null) {}
        val block = ByteArray(MusicOutput.FRAME * 4)
        output.play(song(48_000, first), "first")
        repeat(10) { output.fill(block) }
        val second = Original(96_000)
        output.play(song(48_000, second), "second")
        assertTrue(first.closed)
        val sent = output.fill(block)!!.original!!
        assertEquals(96_000, sent.rate)
        assertEquals(0L, sent.at)
        assertEquals(960, sent.samples.size / 2)
        output.stop()
        assertTrue(second.closed)
        assertNull(output.fill(block))
    }

    @Test
    fun withoutALosslessStreamOnlyTheOpusBlocksGo() {
        val output = MusicOutput(Taken(), lossless = false, monitor = null) {}
        output.play(song(48_000, Original(44_100)), "song")
        assertNull(output.fill(ByteArray(MusicOutput.FRAME * 4))!!.original)
    }
}

/** What a fetched file is, by its first bytes, for the log of one that would not decode. */
class MusicSniffTest {
    private fun head(vararg parts: Pair<Int, String>): ByteArray {
        val bytes = ByteArray(64)
        for ((at, text) in parts) text.toByteArray(Charsets.US_ASCII).copyInto(bytes, at)
        return bytes
    }

    @Test
    fun knowsTheLibrariesFormatsByTheirMarks() {
        assertEquals("flac", MusicLoader.sniff(head(0 to "fLaC")))
        assertEquals("wav", MusicLoader.sniff(head(0 to "RIFF", 8 to "WAVE")))
        assertEquals("aiff", MusicLoader.sniff(head(0 to "FORM", 8 to "AIFC")))
        assertEquals("m4a", MusicLoader.sniff(head(4 to "ftyp")))
        assertEquals("opus", MusicLoader.sniff(head(0 to "OggS", 28 to "OpusHead")))
        assertEquals("ogg", MusicLoader.sniff(head(0 to "OggS")))
        assertEquals("mp3", MusicLoader.sniff(head(0 to "ID3")))
        assertEquals("aac", MusicLoader.sniff(byteArrayOf(0xFF.toByte(), 0xF1.toByte(), 0, 0)))
        assertEquals("mp3", MusicLoader.sniff(byteArrayOf(0xFF.toByte(), 0xFB.toByte(), 0, 0)))
        assertNull(MusicLoader.sniff(ByteArray(64)))
        assertNull(MusicLoader.sniff(ByteArray(0)))
    }

    @Test
    fun aMonoFileIsBothSidesAndMoreChannelsAreFoldedWithinFullScale() {
        assertEquals(0.5f to 0.5f, PcmReader.fold(floatArrayOf(0.5f), 1))
        val (left, right) = PcmReader.fold(floatArrayOf(1f, 1f, 1f, 1f, 1f, 1f), 6)
        assertTrue(abs(left) <= 1f && abs(right) <= 1f)
        assertFalse(left == 0f)
    }
}
