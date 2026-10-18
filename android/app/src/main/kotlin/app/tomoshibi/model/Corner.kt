package app.tomoshibi.model

// The rules for the corner of a call, where notices and what people said
// appear, kept apart from the views that draw them so that they can be held
// to the web's numbers by a test (TomoshibiBasics/Corner.swift).

/** How long the call's notices stay (web/src/live/notices.ts). */
object NoticeTimes {
    /** An ordinary notice: long enough to read four words (`BRIEFLY`). */
    const val BRIEFLY = 2.2
    /** A share, because the picture is about to change under you (`A_MOMENT`). */
    const val A_MOMENT = 3.0
    /** The longest any notice stays (`AT_MOST`). Ten seconds has been seen; for ever has been ignored. */
    const val AT_MOST = 10.0
    /** What fits above the controls; the fourth pushes the oldest out (Notices.tsx `visibleToasts`). */
    const val SHOWN = 3
    /**
     * The least a held notice is given once let go. Sonner resumes with
     * whatever was left, so a notice read to its last tenth of a second
     * vanished the instant the pointer moved off it.
     */
    const val AFTER_HOLDING = 1.0

    /**
     * Which notices to drop so that no more than [limit] remain, oldest
     * first: only ones that go by themselves (`lasting` false). One that stays
     * until it is dealt with, somebody at the door, is never pushed out by
     * somebody arriving.
     */
    fun overflow(lasting: List<Boolean>, limit: Int = SHOWN): List<Int> {
        var excess = lasting.size - limit
        val drop = ArrayList<Int>()
        for ((index, stays) in lasting.withIndex()) {
            if (excess <= 0) break
            if (!stays) {
                drop.add(index)
                excess--
            }
        }
        return drop
    }
}

/**
 * A notice's countdown, which waits while the corner is being read (sonner
 * pauses every timer while its stack is expanded under the pointer). Times are
 * in seconds on whatever monotonic clock the caller keeps.
 */
data class NoticeClock(
    /** Seconds still to go, as of [since] when running. */
    val left: Double,
    /** When it last started running. */
    val since: Double?,
) {
    constructor(lasts: Double, now: Double, running: Boolean) : this(lasts, if (running) now else null)

    val running: Boolean get() = since != null

    /** Stops it, keeping what was left. */
    fun hold(now: Double): NoticeClock {
        val since = since ?: return this
        return NoticeClock(maxOf(0.0, left - (now - since)), null)
    }

    /** Starts it again, with at least [NoticeTimes.AFTER_HOLDING] to go. */
    fun run(now: Double): NoticeClock {
        if (since != null) return this
        return NoticeClock(maxOf(left, NoticeTimes.AFTER_HOLDING), now)
    }

    /** Seconds from [now] until it is due; null while it is held. */
    fun due(now: Double): Double? {
        val since = since ?: return null
        return maxOf(0.0, left - (now - since))
    }
}

/**
 * Where each notice in the corner sits: piled, the newest in front and the
 * others peeking above it, until the corner is read, when they spread into a
 * column (sonner's stack). Lengths are in density-independent pixels.
 */
object NoticePile {
    const val GAP = 8f
    const val PEEK = 9f
    const val SHRINK = 0.05f

    /** How far up from the bottom each notice's bottom edge sits, newest first. */
    fun lifts(heights: List<Float>, spread: Boolean): List<Float> {
        val lifts = ArrayList<Float>(heights.size)
        var below = 0f
        for ((place, height) in heights.withIndex()) {
            if (spread) {
                lifts.add(below)
                below += height + GAP
            } else {
                lifts.add(place * PEEK)
            }
        }
        return lifts
    }

    /** The height the whole corner takes. */
    fun height(heights: List<Float>, spread: Boolean): Float {
        val front = heights.firstOrNull() ?: return 0f
        if (spread) return heights.sum() + GAP * (heights.size - 1)
        return front + PEEK * (heights.size - 1)
    }

    /** The scale a notice is drawn at in its place. */
    fun scale(place: Int, spread: Boolean): Float = if (spread) 1f else maxOf(0.5f, 1 - SHRINK * place)
}

/**
 * Keeping the notices and what people said out of each other's way: where
 * both columns cover the width (a phone), the notices rise above the
 * messages, because a message stays six seconds and a notice two, so the one
 * that comes and goes is the one that moves.
 */
object CornerLanes {
    const val GAP = 8f

    /**
     * How far the notices must rise, given the messages' right edge and
     * height and the leading edge of the notices' own column; nothing while
     * the two do not share any width.
     */
    fun lift(notices: Float, saidRight: Float?, saidHeight: Float?): Float {
        if (saidRight == null || saidHeight == null || saidHeight <= 0f) return 0f
        if (saidRight + GAP <= notices) return 0f
        return saidHeight + GAP
    }
}

/** The parts of the web's chat that are rules rather than drawing. */
object ChatRules {
    /** The longest message the box takes (ChatPanel.tsx `maxLength`). */
    const val LONGEST = 2000

    /**
     * [text] cut to [limit], counted as a browser counts `maxLength`, in
     * UTF-16 code units, but never through the middle of a character: a
     * pasted emoji at the limit is left out whole rather than sent as half a
     * surrogate pair.
     */
    fun clamp(text: String, limit: Int = LONGEST): String {
        if (text.length <= limit) return text
        // The last boundary between characters at or before the limit, a
        // character being what a reader sees as one (a flag, an emoji joined
        // to another), as Swift's Character is.
        val boundaries = java.text.BreakIterator.getCharacterInstance()
        boundaries.setText(text)
        val end = boundaries.preceding(limit + 1)
        return text.substring(0, if (end == java.text.BreakIterator.DONE) 0 else end)
    }

    /** Whether the line at [index] carries on a run by the same person (ChatPanel.tsx, Said.tsx). */
    fun continues(speakers: List<String>, index: Int): Boolean =
        index > 0 && index < speakers.size && speakers[index - 1] == speakers[index]
}
