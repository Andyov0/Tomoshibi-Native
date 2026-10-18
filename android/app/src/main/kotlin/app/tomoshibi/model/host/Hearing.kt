package app.tomoshibi.model.host

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.Defaults
import app.tomoshibi.model.HearingBook
import app.tomoshibi.model.HearingFiling
import app.tomoshibi.model.HearingSetting
import app.tomoshibi.model.Rehearsal
import app.tomoshibi.model.Sound

/**
 * The sound panel's record for this app, kept where the web keeps it and
 * under the web's name (hearing.ts; HostFeature.swift `Hearing`). One for the
 * whole app rather than one per call, as the web's is one per page: a call
 * that is moved, or comes back after a drop, is joined again as the same
 * identity, and its settings should be waiting for it.
 *
 * Nowhere in a rehearsal ([Rehearsal.choices]): a test's `mute` or `volume30`
 * is written against a proven identity like anybody's, and on the Mac was
 * saved into the owner's own book, where it would have silenced that person
 * in the owner's next real call.
 *
 * The song desk's music volume is filed here too (Sound.Music), so the music
 * feature reads and writes [shared] as the sound panel does.
 */
class Hearing(private val defaults: Defaults?) {
    private val book = HearingBook(defaults?.string(HearingBook.STORAGE_KEY))

    /** Moved on by every change, so that whatever drew a setting draws it again. */
    private var version by mutableIntStateOf(0)

    fun setting(identity: String, sound: Sound): HearingSetting {
        version
        return book.setting(filing(identity), sound)
    }

    fun set(identity: String, sound: Sound, setting: HearingSetting) {
        if (book.set(filing(identity), sound, setting)) {
            val stored = book.stored
            if (stored != null) defaults?.set(HearingBook.STORAGE_KEY, stored) else defaults?.remove(HearingBook.STORAGE_KEY)
        }
        version++
    }

    companion object {
        /** Made the first time it is asked for, which is after AppModel has said whether this is a rehearsal. */
        val shared: Hearing by lazy { Hearing(Rehearsal.choices) }

        /**
         * Filed under the signature for a proven identity, which is the same
         * on every visit, and under the identity itself otherwise, for the
         * call only (hearing.ts `filing`).
         */
        fun filing(identity: String): HearingFiling {
            val signature = CrateRules.signature(identity)
            if (signature != null && signature.proven) return HearingFiling(signature.trip, keeps = true)
            return HearingFiling(identity, keeps = false)
        }
    }
}
