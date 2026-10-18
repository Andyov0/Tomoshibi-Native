package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * The ring every remote sound passes through between the core's threads and
 * the mixer's. What these guard is a voice that falls further and further
 * behind (the ring never shedding), one that stutters from the first word
 * (playing before it has a cushion), and stale sound replayed when it runs
 * dry.
 */
class PcmRingTest {
    /** [frames] frames of a constant [value], interleaved for [channels], as the core's little-endian bytes. */
    private fun pcm(frames: Int, channels: Int, value: Short): ByteArray {
        val out = ByteArray(frames * channels * 2)
        for (i in 0 until frames * channels) {
            out[i * 2] = (value.toInt() and 0xff).toByte()
            out[i * 2 + 1] = (value.toInt() shr 8).toByte()
        }
        return out
    }

    @Test
    fun playsNothingUntilItHasItsCushionThenPlaysWhatCame() {
        val ring = PcmRing(1, startFrames = 1440, ceilingFrames = 9600, settleFrames = 2880)
        ring.write(pcm(960, 1, 16384))
        val mix = FloatArray(480 * 2)
        ring.mix(mix, 480, 1f)
        assertTrue("silent while gathering", mix.all { it == 0f })
        assertEquals(960, ring.buffered)
        ring.write(pcm(480, 1, 16384))
        ring.mix(mix, 480, 1f)
        // Mono goes to both sides, at the level it came.
        assertEquals(0.5f, mix[0], 1e-6f)
        assertEquals(0.5f, mix[1], 1e-6f)
        assertEquals(960, ring.buffered)
    }

    @Test
    fun runningDryIsSilenceAndAFreshCushionBeforePlayingAgain() {
        val ring = PcmRing(1, startFrames = 480, ceilingFrames = 9600, settleFrames = 2880)
        ring.write(pcm(480, 1, 8192))
        val mix = FloatArray(960 * 2)
        ring.mix(mix, 960, 1f)
        // The half it had, then nothing rather than anything stale.
        assertEquals(0.25f, mix[0], 1e-6f)
        assertEquals(0f, mix[480 * 2], 0f)
        // The next pass finds it empty, and from then it gathers again.
        val dry = FloatArray(480 * 2)
        ring.mix(dry, 480, 1f)
        assertTrue(dry.all { it == 0f })
        ring.write(pcm(100, 1, 8192))
        val again = FloatArray(480 * 2)
        ring.mix(again, 480, 1f)
        assertTrue("waits for a cushion again", again.all { it == 0f })
    }

    @Test
    fun gatheringPastTheCeilingSettlesBackRatherThanFallingBehind() {
        val ring = PcmRing(1, startFrames = 1440, ceilingFrames = 9600, settleFrames = 2880)
        ring.write(pcm(9601, 1, 1000))
        assertEquals(2880, ring.buffered)
    }

    @Test
    fun aGainAndAStereoTrackAreMixedAsTheyAre() {
        val ring = PcmRing(2, startFrames = 1, ceilingFrames = 9600, settleFrames = 2880)
        val frames = ByteArray(4)
        // Left 0.5, right -0.25.
        frames[0] = 0; frames[1] = 0x40
        frames[2] = 0; frames[3] = 0xE0.toByte()
        ring.write(frames)
        val mix = FloatArray(2)
        ring.mix(mix, 1, 0.5f)
        assertEquals(0.25f, mix[0], 1e-6f)
        assertEquals(-0.125f, mix[1], 1e-6f)
    }

    @Test
    fun aTrackInOtherChannelsThanAnnouncedIsFoldedNotMisread() {
        val stereo = PcmRing(2, startFrames = 1, ceilingFrames = 9600, settleFrames = 2880)
        stereo.write(pcm(4, 1, 16384), sourceChannels = 1)
        assertEquals(4, stereo.buffered)
        val mix = FloatArray(8)
        stereo.mix(mix, 4, 1f)
        assertTrue(mix.all { kotlin.math.abs(it - 0.5f) < 1e-6f })
    }
}
