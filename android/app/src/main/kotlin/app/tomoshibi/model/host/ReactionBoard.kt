package app.tomoshibi.model.host

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.CallModel
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The reactions on screen in one call (hands.ts `useReactions`;
 * Reactions.swift `ReactionBoard`).
 *
 * A reaction is an event rather than a state: it happened, it is seen for
 * four seconds, and it is gone, so nobody arriving later sees a thumbs-up
 * from before they came. Held in the call's parts, because the three places
 * that need it are given nothing else in common: the tile a reaction floats
 * over, the control that sends one, and the feature that hears one arrive.
 * One per call rather than one per room view, so that a room drawn again
 * (the activity turned round) does not show a second board's copy.
 */
class ReactionBoard private constructor(private val call: CallModel) {
    data class Shown(
        /** Whose it is, as the identity their token was signed with. */
        val from: String,
        val what: String,
        val id: Long = next.incrementAndGet(),
    )

    var shown by mutableStateOf<List<Shown>>(emptyList())
        private set

    /**
     * Shows one, and takes it down again after the web's four seconds by the
     * timer that put it up rather than by a sweep (hands.ts `REACTION_FOR`).
     */
    fun show(what: String, from: String) {
        val one = Shown(from, what)
        shown = shown + one
        call.scope.launch {
            delay(CrateRules.reactionShownForMs())
            shown = shown.filter { it.id != one.id }
        }
    }

    fun on(identity: String): List<Shown> = shown.filter { it.from == identity }

    /**
     * Sends a reaction to everybody and shows it on this participant's own
     * picture at once: data does not come back to whoever sent it, so the
     * sender would otherwise never see their own (hands.ts `react`).
     *
     * The topic is the web's `REACTION`, sent reliably as the web sends it;
     * the payload is the reaction's own text, which the protocol crate
     * refuses to make for anything but the six a browser would show.
     */
    fun send(what: String) {
        val payload = CrateRules.reactionPayload(what) ?: return
        show(what, call.localIdentity)
        call.scope.launch {
            try {
                call.send(TOPIC, payload, reliable = true)
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                // A reaction that did not leave is not worth a notice: it was
                // already shown here, and its moment has passed.
            }
        }
    }

    companion object {
        /** hands.ts `REACTION`. */
        const val TOPIC = "reaction"
        private const val PART = "reactions"
        private val next = AtomicLong()

        fun of(call: CallModel): ReactionBoard = call.parts.getOrPut(PART) { ReactionBoard(call) } as ReactionBoard
    }
}
