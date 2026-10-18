package app.tomoshibi.ui.room

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.Plan
import app.tomoshibi.basics.SpeakingRecord
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.host.CrateRules
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_protocol.Joining
import kotlin.coroutines.coroutineContext

// The pieces the stage is built from that are not a picture
// (Kit+Stage.swift): what the room remembers between redraws, when the
// controls may step aside, and the last press on a picture. The window's full
// screen and its reader are the Mac's; on a phone the stage fills the glass
// (RoomView).

/**
 * The stage's record of one call: who spoke last, whether this participant
 * is speaking, and whether a plain link lets anybody in (`StageRecord`).
 *
 * Kept in the call's parts rather than in the view, for the reason
 * WatchFeature gives on Apple: the room is composed again whenever the
 * activity is made again, and a listener made there would be made again
 * each time, with no way to take the first one off the call.
 */
class StageRecord private constructor(private val call: CallModel) {
    /** When each person was last heard (useSpeakingOrder.ts). */
    var speaking by mutableStateOf(SpeakingRecord())
        private set

    /**
     * Whether this participant is speaking, from the same event that says it
     * of everybody else: the web marks your own picture as it marks theirs.
     */
    var meSpeaking by mutableStateOf(false)
        private set

    /**
     * Who may enter a room that already exists, asked once for the call
     * (useJoining.ts); null until the deployment has answered, so nothing is
     * drawn on a guess and then taken away.
     */
    var joining by mutableStateOf<Joining?>(null)
        private set

    init {
        val me = call.localIdentity
        call.listen { event ->
            if (event is CallEvent.Speaking) {
                meSpeaking = me in event.identities
                speaking = speaking.hear(event.identities, call.identities, System.currentTimeMillis() / 1000.0)
            }
        }
        call.scope.launch {
            // A deployment that will not say is read as the one every
            // deployment starts as, as the web's `deployment()` falls back
            // to PLAIN: a page that cannot ask must not invent a restriction.
            joining = try {
                call.deployment.deploymentInfo().joinedBy
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                Joining.ANYONE
            }
        }
    }

    /**
     * Whether a plain link to this room is a way into it (useJoining.ts
     * `useLinkWorks`): only where anybody may use a room that exists, and
     * never for a room under a scope, whose members can already name it and
     * whose other people need an invitation.
     */
    val linkWorks: Boolean get() = joining == Joining.ANYONE && !CrateRules.scoped(call.room)

    /**
     * The room's plain address, `{base}/#/{room}`, which is what the web
     * copies from its address bar: the base as the deployment gives it, so a
     * deployment behind a path or another port keeps them.
     */
    val link: String get() = call.deployment.baseUrl().trimEnd('/') + "/#/" + call.room

    companion object {
        private const val PART = "stage"

        fun of(call: CallModel): StageRecord = call.parts.getOrPut(PART) { StageRecord(call) } as StageRecord
    }
}

/** Puts words on the system's clipboard (`Pasteboard`). */
object Pasteboard {
    fun copy(context: Context, text: String): Boolean {
        val board = context.getSystemService(ClipboardManager::class.java) ?: return false
        board.setPrimaryClip(ClipData.newPlainText("Tomoshibi", text))
        return true
    }
}

/**
 * Whether nobody has touched anything for a while (hooks/useIdle.ts;
 * `IdleWatch`).
 *
 * Only a touch, a key or the pointer count; not the frames of a picture,
 * because a call where somebody is talking and nobody is touching anything is
 * exactly the call whose controls should get out of the way. A touch is
 * written to an unobserved field, so a finger crossing the screen redraws
 * nothing until it wakes something that was asleep.
 */
class IdleWatch {
    var idle by mutableStateOf(false)
        private set

    /**
     * How long it has been quiet, counted in the watch's own steps rather
     * than read off the system's clock, so that it is the same quarter
     * seconds the watch waits that it counts (and a test's clock moves it).
     */
    private var quiet = 0L

    fun poke() {
        quiet = 0
        if (idle) idle = false
    }

    /**
     * Watches until the coroutine it runs in is cancelled, which is the room
     * going or the controls no longer being allowed to step aside.
     */
    suspend fun run() {
        poke()
        try {
            while (coroutineContext.isActive) {
                delay(STEP_MILLIS)
                quiet += STEP_MILLIS
                if (!idle && quiet >= (Plan.IDLE_AFTER * 1000).toLong()) idle = true
            }
        } finally {
            idle = false
        }
    }

    private companion object {
        const val STEP_MILLIS = 250L
    }
}

/**
 * The last press on a picture, for telling a double press from two single
 * ones (`Presses`). Not observed: remembering a press redraws nothing.
 */
class Presses {
    var last: Pair<String, Long>? = null

    companion object {
        /** The interval a touch screen's own double tap uses (UIKit's, and Android's ViewConfiguration's 300 ms). */
        const val INTERVAL_MILLIS = 300L
    }
}
