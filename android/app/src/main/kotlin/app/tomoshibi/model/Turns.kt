package app.tomoshibi.model

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Work that must not overlap, done one piece at a time in the order it was
 * asked for (Sharing's changes of a running share).
 *
 * A share's change of size publishes a new picture track, moves the capture
 * onto it and takes the old one down, with a network round trip between each
 * step. Two of them at once, the menu's and the uplink card's, each took down
 * the track it had found at the start, so the one published in between was
 * left on the stage with nothing feeding it. Each now waits for the one before
 * it. The coroutine mutex is fair, first come first served, which is the
 * order Swift's Turns keeps by hand.
 *
 * Nothing that must always happen goes through here: stopping a share or
 * leaving the call never waits behind a change that may itself be waiting on
 * the network.
 */
class Turns {
    private val mutex = Mutex()

    /** Whether a piece of work is running. */
    val busy: Boolean get() = mutex.isLocked

    /** Runs [work] once everything asked for before it has finished. */
    suspend fun <T> take(work: suspend () -> T): T = mutex.withLock { work() }
}
