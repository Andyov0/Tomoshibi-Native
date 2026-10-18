package app.tomoshibi.feature.watch

/**
 * When a source of the video watched together that has not begun playing is
 * given up on (WatchScreen.tsx `STALL_MS`), looked at once a second as the
 * web's interval looks (TomoshibiBasics/WatchStall.swift).
 *
 * Every age here is worked out saturating. The clock is read at different
 * moments for one beat: the beat reads it, the show may then move on to the
 * next video and build its player, and the player reads the clock again when
 * its source begins, a millisecond or more later than the beat's reading. On
 * Apple that source came out a negative age as an unsigned subtraction, which
 * trapped: the app quit whenever the show moved on by itself. Kotlin's longs
 * would not trap but would read it as a stall of minus a millisecond, and a
 * wrapped unsigned one as one of half a billion years. A source that began
 * after `now` has been running for no time at all.
 */
class WatchStall(started: Long, val stallMs: Long = 8000) {
    /** When the source being tried began. */
    var started: Long = started
        private set
    private var looked: Long? = null

    /**
     * Another source is being tried, from [at]. When the stall was last
     * looked at is kept: the second is the player's, not the source's.
     */
    fun restart(at: Long) {
        started = at
    }

    /** Whether a second has passed since the last look, which is then this one; the first look is always due. */
    fun due(now: Long): Boolean {
        val last = looked
        if (last != null && since(last, now) < 1000) return false
        looked = now
        return true
    }

    /** Whether the source has gone [stallMs] without beginning. */
    fun late(now: Long): Boolean = since(started, now) > stallMs

    companion object {
        /** How long ago [then] was at [now], and nothing for a [then] read after. */
        fun since(then: Long, now: Long): Long = if (now > then) now - then else 0
    }
}
