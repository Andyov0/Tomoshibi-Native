package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * Whether a microphone delivers anything, which the devices panel's "No sound
 * from it" reads. What this guards is a silenced microphone (zeros, a quiet
 * room) taken for one that hands over nothing at all, the rule the Apple
 * apps keep by SoundLevel's last chunk.
 */
class SoundLevelTest {
    @Test
    fun nothingHandedOverIsNotDelivered() {
        val level = SoundLevel()
        assertFalse(level.delivered)
        assertEquals(0L, level.lastDelivered)
    }

    @Test
    fun silenceHandedOverIsDelivered() {
        val level = SoundLevel()
        level.add(ByteArray(960))
        assertTrue("zeros are still sound arriving", level.delivered)
        assertEquals(480, level.lastChunk)
        assertTrue(level.lastDelivered > 0)
        assertEquals(0.0, level.rms, 0.0)
    }

    @Test
    fun aMixersEnergyCountsAsDelivered() {
        val level = SoundLevel()
        level.addEnergy(0.0, 480)
        assertTrue(level.delivered)
    }
}
