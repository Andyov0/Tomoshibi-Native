package app.tomoshibi.feature.music

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The web's own cases for music that makes way for speech
 * (web/src/live/duck.test.ts), against the Kotlin port, as MusicTimingTests.swift
 * holds the Swift one to them.
 *
 * What would go wrong unnoticed: the music turned down by itself, so that it
 * never comes back up; a click counted as somebody talking; a fade-in that
 * never finishes, which is music that is never heard; a switch that says off
 * while the music still ducks.
 */
class MusicDuckTest {
    private val duck = MusicDuck()
    private var clock = 0.0

    /** Both clocks on together, tick by tick, with the microphones reading [levels] throughout. */
    private fun pass(ms: Double, levels: List<Double>) {
        var t = 0.0
        while (t < ms) {
            clock += MusicDuck.TICK_MS
            duck.tick(levels, clock, MusicDuck.TICK_MS)
            t += MusicDuck.TICK_MS
        }
    }

    @Test
    fun comesDownWithinTheAttackOnceSomebodyTalksAndOnlyAsFarAsDucked() {
        pass(2 * MusicDuck.TICK_MS + MusicDuck.ATTACK_MS + MusicDuck.TICK_MS, listOf(0.1))
        assertEquals(MusicDuck.DUCKED, duck.factor("gfriend", clock), 1e-9)
        assertTrue(duck.ducking)
    }

    @Test
    fun waitsAfterTheTalkingStopsThenComesBackOverTheReleaseNotAtOnce() {
        pass(400.0, listOf(0.1))
        pass(MusicDuck.HOLD_MS - 2 * MusicDuck.TICK_MS, listOf(0.0))
        assertEquals(MusicDuck.DUCKED, duck.factor("gfriend", clock), 1e-9)

        pass(MusicDuck.RELEASE_MS / 2, listOf(0.0))
        val midway = duck.factor("gfriend", clock)
        assertTrue(midway > MusicDuck.DUCKED)
        assertTrue(midway < 1)

        pass(MusicDuck.RELEASE_MS, listOf(0.0))
        assertEquals(1.0, duck.factor("gfriend", clock), 0.0)
    }

    @Test
    fun takesAClickForNothing() {
        pass(MusicDuck.TICK_MS, listOf(0.5))
        pass(MusicDuck.ATTACK_MS * 2, listOf(0.0))
        assertEquals(1.0, duck.factor("gfriend", clock), 0.0)
    }

    @Test
    fun ignoresWhatIsBelowSpeech() {
        pass(2000.0, listOf(MusicDuck.SPEECH * 0.8))
        assertEquals(1.0, duck.factor("gfriend", clock), 0.0)
    }

    @Test
    fun anyOneMicrophoneIsEnough() {
        pass(500.0, listOf(0.0, 0.1, 0.0))
        assertEquals(MusicDuck.DUCKED, duck.factor("gfriend", clock), 1e-9)
    }

    @Test
    fun doesNothingOnceSwitchedOff() {
        pass(500.0, listOf(0.1))
        duck.enabled = false
        assertEquals(1.0, duck.factor("gfriend", clock), 0.0)
        assertFalse(duck.ducking)
    }

    @Test
    fun fadesAStreamInFromNothingAndFinishesWithNoMicrophones() {
        duck.arrived(MusicDuck.LOCAL, clock)
        assertEquals(0.0, duck.factor(MusicDuck.LOCAL, clock), 0.0)
        pass(MusicDuck.FADE_IN_MS / 2, emptyList())
        assertEquals(0.5, duck.factor(MusicDuck.LOCAL, clock), 0.05)
        pass(MusicDuck.FADE_IN_MS, emptyList())
        assertEquals(1.0, duck.factor(MusicDuck.LOCAL, clock), 0.0)
        assertTrue(duck.settled(clock))
    }

    @Test
    fun fadesAStreamInOnceNotAgainBetweenSongs() {
        duck.arrived("gholder", clock)
        pass(MusicDuck.FADE_IN_MS + 100, emptyList())
        duck.arrived("gholder", clock)
        assertEquals(1.0, duck.factor("gholder", clock), 0.0)
    }

    @Test
    fun fadesInAgainAfterTheStreamWent() {
        duck.arrived("gholder", clock)
        pass(MusicDuck.FADE_IN_MS + 100, emptyList())
        duck.left("gholder")
        duck.arrived("gholder", clock)
        assertEquals(0.0, duck.factor("gholder", clock), 0.0)
    }
}

/** The clock the music is sent by: a song handed to the core faster than it plays is heard at the wrong speed or not at all. */
class MusicPacerTest {
    private val ms = 1_000_000L

    @Test
    fun handsOverNoMoreThanRealTimeHowEverOftenItIsAsked() {
        val pacer = MusicPacer(0)
        var blocks = 0
        // Asked every millisecond for a second: a hundred 10 ms blocks, and
        // never one before its time.
        for (step in 0..1000) {
            val now = step * ms
            blocks += pacer.due(now)
            assertTrue(blocks <= now / (10 * ms))
        }
        assertEquals(100, blocks)
    }

    @Test
    fun askedLateItMakesUpWhatIsOwed() {
        val pacer = MusicPacer(0)
        assertEquals(3, pacer.due(35 * ms))
        assertEquals(0, pacer.due(35 * ms))
        assertEquals(1, pacer.due(40 * ms))
        assertEquals(50 * ms, pacer.nextDeadline)
    }

    @Test
    fun afterAStallItLetsTheBacklogGoAndKeepsRealTimeAfterwards() {
        val pacer = MusicPacer(0, mostBehind = 20)
        assertEquals(10, pacer.due(100 * ms))
        // Five seconds held up: at most twenty blocks at once, not five
        // hundred, and then one block per ten milliseconds again.
        assertEquals(20, pacer.due(5100 * ms))
        assertEquals(0, pacer.due(5100 * ms))
        assertEquals(5, pacer.due(5150 * ms))
    }
}
