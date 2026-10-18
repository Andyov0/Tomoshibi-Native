package app.tomoshibi.feature.music

/**
 * When the next block of music is due, by a real clock
 * (TomoshibiBasics/MusicPacer.swift).
 *
 * The core's music source sends whatever it is handed at once: it has no clock
 * of its own (its queue is set to nothing, so that a voice is not delayed). A
 * song pushed as fast as it decodes would go out in a few seconds, at the
 * wrong speed, and the listeners' jitter buffers would throw most of it away.
 * So the song is handed over one 10 ms block at a time, as many as the time
 * since it started says are due, which is what a browser's audio context does
 * by playing the buffer in real time.
 *
 * A thread that was held up (a busy phone, the screen going off) owes a burst.
 * Up to [mostBehind] blocks are made up at once, which the listeners' jitter
 * buffers absorb; anything beyond is let go and the clock starts again from
 * there, so the song resumes where it stopped rather than racing through
 * seconds of it to catch up.
 */
class MusicPacer(start: Long, val blockNanos: Long = 10_000_000, val mostBehind: Int = 20) {
    private var base = start
    private var sent = 0L

    /** How many blocks are due by [now], which are then counted as sent. */
    fun due(now: Long): Int {
        if (now <= base) return 0
        val owed = (now - base) / blockNanos
        if (owed <= sent) return 0
        var due = owed - sent
        if (due > mostBehind) {
            // Let the rest go: the clock restarts as if only `mostBehind`
            // blocks had been owed.
            base += (due - mostBehind) * blockNanos
            due = mostBehind.toLong()
        }
        sent += due
        return due.toInt()
    }

    /** When the block after those sent falls due. */
    val nextDeadline: Long get() = base + (sent + 1) * blockNanos
}
