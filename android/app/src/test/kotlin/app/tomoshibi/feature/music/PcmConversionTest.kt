package app.tomoshibi.feature.music

import android.media.AudioFormat
import android.media.MediaFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * From what the platform's decoder hands over to the song's samples
 * (PcmReader). What went wrong here was heard by everybody in a call and
 * by no test: a 16-bit FLAC read as floats came out at half its length, an
 * octave up and clipped to a square wave. The platform's FLAC extractor
 * decodes the file itself and hands over audio/raw, which the raw decoder
 * passes on byte for byte while reporting whatever encoding it was asked
 * for; [PassThrough] stands in for it here, so that asking it for the wrong
 * thing is caught the way it was heard.
 */
class PcmConversionTest {
    /**
     * The platform's raw decoder (Codec2's C2SoftRawDec, the old
     * OMX.google.raw.decoder before it): the bytes go out as they came in,
     * and the encoding it reports is the one it was configured with, or the
     * track's own when it was configured with none.
     */
    private class PassThrough(trackEncoding: Int, asked: Int?) {
        val reports = asked ?: trackEncoding
    }

    /** One second of a [hertz] tone at [rate] in two channels of 16-bit integers, as the FLAC extractor gives a CD's track. */
    private fun sixteenBit(rate: Int, hertz: Double, level: Double = 0.5): ByteBuffer {
        val buffer = ByteBuffer.allocate(rate * 2 * 2).order(ByteOrder.nativeOrder())
        for (frame in 0 until rate) {
            val value = (level * sin(2 * PI * hertz * frame / rate) * 32_767).roundToInt().toShort()
            buffer.putShort(value).putShort(value)
        }
        buffer.flip()
        return buffer
    }

    /** One second of a [hertz] tone at [rate] in two channels of floats, as a decoder asked for floats gives it. */
    private fun floats(rate: Int, hertz: Double, level: Double = 0.5): ByteBuffer {
        val buffer = ByteBuffer.allocate(rate * 2 * 4).order(ByteOrder.nativeOrder())
        for (frame in 0 until rate) {
            val value = (level * sin(2 * PI * hertz * frame / rate)).toFloat()
            buffer.putFloat(value).putFloat(value)
        }
        buffer.flip()
        return buffer
    }

    /** The left channel's pitch, from how often it crosses nought upwards over the whole of [samples]. */
    private fun pitch(samples: FloatArray, rate: Int): Double {
        var rises = 0
        for (frame in 1 until samples.size / 2) {
            if (samples[(frame - 1) * 2] < 0f && samples[frame * 2] >= 0f) rises++
        }
        return rises * rate.toDouble() / (samples.size / 2)
    }

    private fun peak(samples: FloatArray) = samples.maxOf { abs(it) }

    @Test
    fun aDecodedFlacIsLeftInItsOwnEncoding() {
        assertNull(PcmReader.requested(MediaFormat.MIMETYPE_AUDIO_RAW))
        // A real decoder is still asked for floats, which the lossless stream
        // recovers every integer of a 24-bit file from.
        assertEquals(AudioFormat.ENCODING_PCM_FLOAT, PcmReader.requested(MediaFormat.MIMETYPE_AUDIO_FLAC))
        assertEquals(AudioFormat.ENCODING_PCM_FLOAT, PcmReader.requested(MediaFormat.MIMETYPE_AUDIO_MPEG))
    }

    @Test
    fun aCdTrackTheExtractorDecodedKeepsItsLengthPitchAndLevel() {
        // "Do" in the test library: 523 Hz, 44.1 kHz, 16 bits, at half scale.
        val asked = PcmReader.requested(MediaFormat.MIMETYPE_AUDIO_RAW)
        val decoder = PassThrough(AudioFormat.ENCODING_PCM_16BIT, asked)
        val samples = PcmReader.stereo(sixteenBit(44_100, 523.0), decoder.reports, 2)
        assertEquals(44_100, samples.size / 2)
        assertEquals(523.0, pitch(samples, 44_100), 2.0)
        assertEquals(0.5f, peak(samples), 0.001f)
    }

    @Test
    fun aDeeperTrackTheExtractorGaveAsFloatsIsReadAsFloats() {
        // A 24-bit FLAC, where the extractor gives floats and says so.
        val asked = PcmReader.requested(MediaFormat.MIMETYPE_AUDIO_RAW)
        val decoder = PassThrough(AudioFormat.ENCODING_PCM_FLOAT, asked)
        val samples = PcmReader.stereo(floats(96_000, 659.0), decoder.reports, 2)
        assertEquals(96_000, samples.size / 2)
        assertEquals(659.0, pitch(samples, 96_000), 2.0)
        assertEquals(0.5f, peak(samples), 0.001f)
    }

    @Test
    fun sixteenBitsReadAsFloatsWereHalfASongOfNoise() {
        // What the fault was, so that the test above is known to tell: the
        // same second read as floats is half a second long, and its values
        // are two samples' bits taken for one number.
        val samples = PcmReader.stereo(sixteenBit(44_100, 523.0), AudioFormat.ENCODING_PCM_FLOAT, 2)
        assertEquals(22_050, samples.size / 2)
        assertTrue(abs(pitch(samples, 44_100) - 523.0) > 100 || peak(samples) > 1f || peak(samples) < 0.1f)
    }

    @Test
    fun aMonoTrackIsHeardOnBothSides() {
        val buffer = ByteBuffer.allocate(4).order(ByteOrder.nativeOrder()).putShort(16_384).putShort(-16_384)
        buffer.flip()
        val samples = PcmReader.stereo(buffer, AudioFormat.ENCODING_PCM_16BIT, 1)
        assertEquals(listOf(0.5f, 0.5f, -0.5f, -0.5f), samples.toList())
    }
}
