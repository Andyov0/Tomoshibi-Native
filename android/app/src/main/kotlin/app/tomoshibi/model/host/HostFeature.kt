package app.tomoshibi.model.host

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.Logged
import app.tomoshibi.basics.t
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.CallToast
import app.tomoshibi.model.HearingSetting
import app.tomoshibi.model.Person
import app.tomoshibi.model.Sound
import kotlin.coroutines.coroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.HostStanding
import uniffi.tomoshibi_protocol.Knocker
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.ProtocolException
import uniffi.tomoshibi_protocol.Relay

private const val TAG = "Tomoshibi"

/**
 * Running a room and hearing it: the host's controls, the door, invitations,
 * every person's volume, reactions arriving (web/src/live/host.ts, knock.ts,
 * hearing.ts, hands.ts; components/room/HostPanel.tsx, AtTheDoor.tsx,
 * SoundPanel.tsx; HostFeature.swift).
 *
 * One per call, kept in the call's parts: the room view is composed again
 * whenever the activity is made again, and a feature made with each would
 * listen to the call once more every time, answering every reaction twice.
 * Nothing happens until [run], which the room's overlay calls for as long as
 * the call is on screen.
 *
 * Every method is for the main thread.
 */
class HostFeature private constructor(val call: CallModel) {
    /** What the server last said about this participant and this room (host.ts `useStanding`): nothing until it has said. */
    var standing by mutableStateOf(HostStanding(yours = false, admin = false))
        private set

    private var waitingState by mutableStateOf<List<Knocker>>(emptyList())
    /** Who is knocking, for as long as this participant can let them in. */
    var waiting: List<Knocker>
        get() = waitingState
        private set(value) {
            waitingState = value
            sayWhoIsWaiting()
        }

    /** The knock being answered, so its buttons do nothing twice. */
    var answering by mutableStateOf<String?>(null)
        private set

    /** The link made by the last press of "Invite somebody", until it is revoked. */
    var link by mutableStateOf<String?>(null)
        private set
    var copied by mutableStateOf(false)
        private set

    /**
     * The one action under way, named by what it acts on (HostPanel.tsx
     * `busy`): an identity, or "invite", "revoke", "move", "close".
     */
    var busy by mutableStateOf<String?>(null)
        private set

    /** The relays a meeting can be moved to, once asked for. */
    var relays by mutableStateOf<List<Relay>?>(null)
        private set

    /**
     * Asked once rather than done at once: ending a meeting for eleven people
     * cannot be undone, and its button sits beside three that can, so the
     * press that matters is the second (HostPanel.tsx `ending`).
     */
    var ending by mutableStateOf(false)

    /** Whether the list of relays to move to is open (HostPanel.tsx `Elsewhere`'s `open`). */
    var choosingRelay by mutableStateOf(false)

    private var panelOpenState by mutableStateOf(false)
    /**
     * Whether the host panel is on screen: the door's notice stands down
     * while it is, because the panel lists the same people.
     *
     * Closing it also puts both questions away. On the web they are the
     * panel's own state and go with it; kept here for the whole call, the
     * panel opened again on "Close the room" already armed, one press from
     * ending the meeting for everybody, or with the relays open, where one
     * press moves everybody.
     */
    var panelOpen: Boolean
        get() = panelOpenState
        set(value) {
            panelOpenState = value
            if (!value) {
                ending = false
                choosingRelay = false
            }
            sayWhoIsWaiting()
        }

    /**
     * How many people the door's notice last said were waiting; nought while
     * it is not up. Said again only when that changes, so a notice swiped
     * away stays away until somebody else knocks or somebody goes.
     */
    private var doorSaid = 0

    /**
     * A move older than this, by the call's monotonic clock, is no longer
     * shown, whatever became of it (App.tsx: the overlay's twenty-second
     * limit). Set from the overlay.
     */
    private var movingGaveUp by mutableStateOf<Long?>(null)

    private var started = false

    /** The sounds this feature asked to stop at the server, so it only ever lets flow again what it stopped itself. */
    private val stopped = HashSet<String>()

    val reactions: ReactionBoard get() = ReactionBoard.of(call)

    /** Whether this room is held under a scope, where every member runs it and there is nobody to hand it to (HostPanel.tsx `scoped`). */
    val scoped: Boolean get() = CrateRules.scoped(call.room)

    /** Where a move is taking the meeting, while that is worth showing. */
    val movingTo: String?
        get() {
            val moving = call.moving ?: return null
            val gaveUp = movingGaveUp
            if (gaveUp != null && gaveUp >= moving.atNanos) return null
            return moving.relay
        }

    // Running

    /**
     * Everything this feature does while the call is on screen: listening to
     * the call, asking who runs the room, watching the door every four
     * seconds while this participant may open it. Returns when the coroutine
     * it runs in is cancelled, which is the call view going.
     */
    suspend fun run() {
        begin()
        refreshStanding()
        while (coroutineContext.isActive && call.endedReason == null) {
            if (standing.yours) {
                lookAtTheDoor()
            } else if (waiting.isNotEmpty()) {
                waiting = emptyList()
            }
            // AtTheDoor.tsx `LOOK_EVERY`.
            delay(LOOK_EVERY_MILLIS)
        }
    }

    private fun begin() {
        if (started) return
        started = true
        call.listen { heard(it) }
        applyHearing()
    }

    private fun heard(event: CallEvent) {
        when (event) {
            // Somebody leaving is the ordinary reason a room changes hands,
            // and a connection that came back may have missed whatever
            // happened while it was away (host.ts `useStanding`).
            is CallEvent.ParticipantLeft, is CallEvent.StateChanged -> call.scope.launch { refreshStanding() }
            is CallEvent.Data -> {
                // A handover, said by the server and only by the server: a
                // participant's packet always arrives with its sender, and
                // this one with none (host.ts `told`).
                if (event.topic == "host" && event.identity == null) call.scope.launch { refreshStanding() }
                val from = event.identity
                if (from != null) CrateRules.receivedReaction(event.topic, from, event.payload)?.let { reactions.show(it, from) }
            }
            // A sound that has just arrived was given a player at full
            // volume and is flowing; whatever was decided about it is put
            // back (Audible.tsx `HEARING_EVENTS`).
            is CallEvent.ParticipantJoined, is CallEvent.TrackPublished, is CallEvent.TrackSubscribed -> applyHearing()
            is CallEvent.TrackUnpublished -> stopped.remove(event.sid)
            else -> {}
        }
    }

    private val bearer: String get() = call.call.token()

    suspend fun refreshStanding() {
        val said = attempt { call.deployment.hostStanding(call.room, bearer) } ?: return
        val became = said.yours && !standing.yours
        standing = said
        sayWhoIsWaiting()
        if (!said.yours) {
            waiting = emptyList()
            link = null
            copied = false
        }
        // Looked at once at once rather than at the next round, so a person
        // handed a room with somebody already outside sees them now.
        if (became) lookAtTheDoor()
    }

    // The door

    private suspend fun lookAtTheDoor() {
        if (!standing.yours) return
        // A round that did not answer is not an empty door (AtTheDoor.tsx).
        val said = attempt { call.deployment.knocking(call.room, bearer) } ?: return
        if (said != waiting) waiting = said
    }

    suspend fun answer(knocker: Knocker, admit: Boolean) {
        if (answering != null) return
        answering = knocker.id
        try {
            call.deployment.answerKnock(call.room, knocker.id, bearer, admit)
            // Taken off the list now rather than at the next round, so a
            // press does something the moment it is pressed.
            waiting = waiting.filter { it.id != knocker.id }
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            lookAtTheDoor()
        } finally {
            answering = null
        }
    }

    // The host's actions

    /** One action at a time, and what went wrong said in words chosen for its code (HostPanel.tsx `act`). */
    private suspend fun act(who: String, work: suspend () -> Unit) {
        if (busy != null) return
        busy = who
        try {
            work()
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Log.w(TAG, "host: ${Logged.private(who)} refused: ${fault.message}")
            say(explain(fault), danger = true)
        } finally {
            busy = null
        }
    }

    /** Makes a link anybody can join by, and puts it on the pasteboard with [copy], since whoever pressed this is about to paste it somewhere. */
    suspend fun invite(copy: (String) -> Boolean) {
        act("invite") {
            val made = call.deployment.createInvite(call.room, bearer)
            val address = call.deployment.inviteLink(made.token, call.room)
            link = address
            copied = copy(address)
        }
    }

    suspend fun revoke() {
        act("revoke") {
            val gone = call.deployment.revokeInvites(call.room, bearer).toInt()
            link = null
            copied = false
            // Said, because "nothing happened" and "three links stopped
            // working" look the same otherwise.
            say(if (gone == 1) t("One link stopped working.") else t("{count} links stopped working.", mapOf("count" to gone.toString())))
        }
    }

    /** Quietens somebody's microphone from the server, not by asking them. */
    suspend fun silence(person: Person) {
        val microphone = person.track(MediaSource.MICROPHONE) ?: return
        act(person.id) { call.deployment.silence(call.room, bearer, person.id, microphone.sid) }
    }

    suspend fun turnOut(person: Person) {
        act(person.id) { call.deployment.turnOut(call.room, bearer, person.id) }
    }

    suspend fun handOver(person: Person) {
        act(person.id) { call.deployment.handOver(call.room, bearer, person.id) }
        // The server says so to the room, which asks again; asked here as
        // well, so the panel does not wait on a packet to stop offering
        // controls that now answer "not yours".
        refreshStanding()
    }

    suspend fun move(relay: Relay) {
        act("move") { call.deployment.moveRoom(call.room, bearer, relay.name) }
    }

    suspend fun dissolve() {
        act("close") { call.deployment.dissolve(call.room, bearer) }
    }

    /** The relays on offer, asked once, when the panel that offers them is first opened (HostPanel.tsx `Elsewhere`). */
    suspend fun loadRelays() {
        if (relays != null) return
        relays = attempt { call.deployment.relays() }?.relays ?: emptyList()
    }

    /**
     * Where the meeting can be put: neither a machine under maintenance nor,
     * for anybody but an administrator, one held in reserve. The server
     * refuses those regardless; this is about not showing a door that will
     * not open.
     */
    val usableRelays: List<Relay>
        get() = (relays ?: emptyList()).filter { !it.maintenance && (standing.admin || !it.adminOnly) }

    /** Whether somebody joined without a passphrase, so that the room cannot be handed to them (HostPanel.tsx `HandOver`, by the signature's prefix). */
    fun isGuest(person: Person): Boolean = CrateRules.signature(person.id)?.proven == false

    fun refuseHandOver() {
        say(t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one."), danger = true)
    }

    // Hearing

    fun setting(person: Person, sound: Sound): HearingSetting = Hearing.shared.setting(person.id, sound)

    fun setVolume(person: Person, sound: Sound, volume: Double) {
        Hearing.shared.set(person.id, sound, Hearing.shared.setting(person.id, sound).copy(volume = volume))
        applyHearing()
    }

    fun setBlocked(person: Person, sound: Sound, blocked: Boolean) {
        Hearing.shared.set(person.id, sound, Hearing.shared.setting(person.id, sound).copy(blocked = blocked))
        applyHearing()
    }

    /**
     * Puts every setting back on the call: all of them, every time, rather
     * than working out which one an event was about (Audible.tsx `apply`).
     *
     * A block is asked of the media server, so nothing is downloaded. The
     * core can only ask that of a track it is subscribed to, and it
     * subscribes to every sound the moment it is published and plays it at
     * once; so a blocked sound is also turned to nothing here, which covers
     * the moment between its first packets and the server hearing the block.
     * Only a sound this stopped is ever let flow again, so a track something
     * else held back stays held back.
     */
    fun applyHearing() {
        for (person in call.people) {
            for (sound in Sound.entries) {
                val track = track(person, sound) ?: continue
                val setting = Hearing.shared.setting(person.id, sound)
                call.audio.setVolume(track.sid, if (setting.blocked) 0f else setting.volume.toFloat())
                if (setting.blocked) {
                    // Noted before it is asked, not after it succeeded: the
                    // media library switches the track off here before it
                    // tells the server, so a request that then fails has
                    // still stopped the sound on this side, and has to be
                    // undone when it is let back.
                    stopped.add(track.sid)
                    enable(track.sid, false)
                } else if (stopped.remove(track.sid)) {
                    enable(track.sid, true)
                }
            }
        }
    }

    private fun enable(sid: String, enabled: Boolean) {
        try {
            call.call.setRemoteEnabled(sid, enabled)
        } catch (fault: Exception) {
            Log.d(TAG, "host: $sid could not be ${if (enabled) "let flow" else "stopped"}: ${fault.message}")
        }
    }

    // Notices

    /**
     * What a press did, said in the corner with everything else the call
     * says (notices.ts `actionFailed`, `actionDone`): it fades after the web's
     * ten seconds and has nothing to close it with, because what it reports
     * is over.
     */
    fun say(text: String, danger: Boolean = false) {
        if (danger) call.actionFailed(text) else call.actionDone(text)
    }

    /**
     * Somebody waiting to be let in, said where the host will see it and
     * taken back the moment the door is empty (notices.ts `atTheDoor`).
     *
     * The web's notice only opens the panel, because a decision made from a
     * toast is one made without reading the name or where it came from. This
     * one carries both, the name somebody typed and the address they knocked
     * from, under its words ([DOOR], which the room's overlay attaches), so
     * the decision can be made where it is read. It has no duration and no
     * close button: somebody is outside until they are answered or they
     * leave. It stands down while the panel is open, which lists the same
     * people under the same two answers.
     */
    private fun sayWhoIsWaiting() {
        val count = if (panelOpen || !standing.yours) 0 else waiting.size
        if (count == doorSaid) return
        doorSaid = count
        if (count == 0) {
            call.dismiss(DOOR)
            return
        }
        call.announce(
            CallToast(
                if (count == 1) t("Somebody is waiting to be let in")
                else t("{count} people are waiting to be let in", mapOf("count" to count.toString())),
                kind = CallToast.Kind.Signal, lasts = null, key = DOOR, closable = false, attachment = DOOR,
            ),
        )
    }

    /** The move that began at [atNanos] is no longer worth showing. */
    fun giveUpMoving(atNanos: Long) {
        movingGaveUp = atNanos
    }

    private suspend fun <T> attempt(work: suspend () -> T): T? = try {
        work()
    } catch (fault: CancellationException) {
        throw fault
    } catch (_: Exception) {
        null
    }

    companion object {
        /** The name the door's notice is raised and drawn under. */
        const val DOOR = "door"
        private const val PART = "host"
        /** AtTheDoor.tsx `LOOK_EVERY`. */
        const val LOOK_EVERY_MILLIS = 4_000L

        /** The feature for [call], made the first time anybody asks. */
        fun of(call: CallModel): HostFeature = call.parts.getOrPut(PART) { HostFeature(call) } as HostFeature

        /** The track each sound is carried on (hearing.ts `SOURCE`). */
        fun track(person: Person, sound: Sound): TrackInfo? = when (sound) {
            Sound.Voice -> person.track(MediaSource.MICROPHONE)
            Sound.Screen -> person.track(MediaSource.SCREEN_SHARE_AUDIO)
            Sound.Music -> person.musicTrack
        }

        /**
         * The server sends a code; this owns the sentence (HostPanel.tsx
         * `explain`), each written out because each wants a different next
         * move.
         */
        fun explain(error: Throwable): String {
            val code = (error as? ProtocolException.Refused)?.code ?: return t("That could not be done. Try again.")
            return when (code) {
                "media_unreachable" -> t("That server did not answer. The meeting has not been moved.")
                "no_such_relay" -> t("That server is no longer on this deployment.")
                "relay_not_allowed" -> t("That server is reserved, and this meeting may not be put on it.")
                "no_such_person" -> t("They have already left.")
                "not_proven" -> t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one.")
                "not_yours" -> t("You no longer run this room.")
                "scoped_room" -> t("Every member of this room's scope runs it, so there is nobody to hand it to.")
                "rate_limited" -> t("Too many attempts. Try again in a moment.")
                else -> t("That could not be done. Try again.")
            }
        }
    }
}
