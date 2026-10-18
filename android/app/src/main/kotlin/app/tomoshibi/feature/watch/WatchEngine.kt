package app.tomoshibi.feature.watch

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.quietly
import app.tomoshibi.ui.room.BarOffer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.CallState
import uniffi.tomoshibi_protocol.MediaServer
import uniffi.tomoshibi_protocol.ResolvedLink
import uniffi.tomoshibi_protocol.ResolvedPlayKind
import uniffi.tomoshibi_protocol.ResolvedVideo
import uniffi.tomoshibi_protocol.TicketVideo
import uniffi.tomoshibi_protocol.WatchAnswer
import uniffi.tomoshibi_protocol.WatchAsk
import uniffi.tomoshibi_protocol.WatchKeeper
import uniffi.tomoshibi_protocol.WatchOutput
import uniffi.tomoshibi_protocol.WatchPacket
import uniffi.tomoshibi_protocol.WatchShow
import uniffi.tomoshibi_protocol.WatchTheatre
import uniffi.tomoshibi_protocol.watchExpected

/**
 * What the engine needs from the core and from the device, gathered in one
 * place so that a test can stand in for each: the protocol crate's theatre
 * and keeper, its two rules a player is held by, and the two ways of playing.
 */
interface WatchMachinery {
    fun theatre(identity: String, name: String, sealed: Boolean): WatchTheatre
    fun keeper(): WatchKeeper
    fun expected(show: WatchShow, heardAtMs: Long, nowMs: Long): Double
    fun mainland(region: String?): Boolean

    /** A file or a playlist; null while there is nothing yet to play it with. */
    fun file(url: String, hls: Boolean, capped: Boolean, owner: WatchPlayer): WatchBackend?

    /** YouTube's own player, embedded from the deployment's address. */
    fun youtube(id: String, base: String, owner: WatchPlayer): WatchBackend?
}

/**
 * The machinery of a real call: the core's, and Media3's player or a web view
 * for YouTube, both made with the application's context. The context is lent
 * by the first of the feature's views to be drawn, since nothing a call is
 * made with carries one; until then a player has nothing to play with, and is
 * given its backend on the next beat after one arrives.
 */
class CoreWatchMachinery : WatchMachinery {
    @Volatile var context: Context? = null

    override fun theatre(identity: String, name: String, sealed: Boolean) = WatchTheatre(identity, name, sealed)
    override fun keeper() = WatchKeeper()
    override fun expected(show: WatchShow, heardAtMs: Long, nowMs: Long) = watchExpected(show, heardAtMs.toULong(), nowMs.toULong())
    override fun mainland(region: String?) = uniffi.tomoshibi_protocol.mainland(region)
    override fun file(url: String, hls: Boolean, capped: Boolean, owner: WatchPlayer): WatchBackend? =
        context?.let { ExoBackend(it, url, hls, capped, owner) }
    override fun youtube(id: String, base: String, owner: WatchPlayer): WatchBackend? =
        context?.let { YouTubeBackend(it, id, base, owner) }
}

/**
 * One call's show, as viewer and as holder (WatchFeature.swift
 * `WatchEngine`; web/src/live/watch.ts, components/room/WatchScreen.tsx,
 * WatchPanel.tsx, WatchTab.tsx): the protocol crate's `WatchTheatre` wired to
 * the call as its module header says, the videos' descriptions asked of the
 * deployment, and the player for whatever is on.
 *
 * Made once per call and kept in the call's `parts` ([of]), because the views
 * are made and remade as often as Compose likes, and a show made in a view
 * would say hello to the room every time and leave a timer running.
 *
 * Main thread only, as the call's model is. Everything it starts runs in the
 * call's own scope and ends with the call.
 */
class WatchEngine internal constructor(
    val call: CallModel,
    private val machinery: WatchMachinery,
    /** False for a picture of the panels with nothing behind it: nothing is listened to, asked or timed. */
    live: Boolean = true,
) {
    /** The deployment, for the questions asked of it, and whether the call is sealed, which never changes. */
    val deployment = call.deployment
    val sealed = call.sealed
    private val theatre = machinery.theatre(call.localIdentity, call.localName, call.sealed)

    /**
     * The show as this participant knows it, and when it was heard (or for the
     * holder, when this version of it was first read): what the timeline is
     * reckoned from.
     */
    var show by mutableStateOf<WatchShow?>(null)
        internal set
    var heardAtMs: Long = 0
        private set

    /**
     * Whether there is a show at all, kept apart from [show] so that the room's
     * arrangement, which reads only this, is not redrawn every time the holder
     * says where it is.
     */
    var onStage by mutableStateOf(false)
        internal set

    /** The show runs here. */
    var mine by mutableStateOf(false)
        internal set

    /**
     * Whether this person may read links and start a show: signed in, given
     * watching together, on a deployment with a gateway (watch-api.ts
     * `watchReady`). Asked once, as Room.tsx asks it.
     */
    var ready by mutableStateOf(false)
        internal set

    /** What each ticket plays, asked of the server once and kept for the call. */
    var infos by mutableStateOf<Map<String, WatchInfo>>(emptyMap())
        internal set

    /** The media servers this person may browse, asked when the panel opens. */
    var servers by mutableStateOf<List<MediaServer>>(emptyList())
        internal set

    /** The player for what is on now, one per video as the web keys its player by the video's key. */
    var player by mutableStateOf<WatchPlayer?>(null)
        internal set

    private val asking = HashMap<String, MutableList<CompletableDeferred<WatchInfo>>>()
    private var sending: Job? = null
    private var reconnecting = false
    private var over = false

    internal val scope get() = call.scope

    init {
        // The holder says the show closed before the call goes, as the web
        // does when the room unmounts; said after, it never arrived, and
        // everybody else kept a frozen show until they noticed the holder had
        // left.
        call.beforeLeaving[PART] = { leaving() }
        // Not offered at all in a sealed call: nothing is listened to, said
        // or asked, and the server is not even asked whether this person could
        // start a show (Room.tsx: "an offer there led to a panel that could
        // only say so").
        if (live && !sealed) begin()
    }

    private fun begin() {
        call.listen { hear(it) }
        val now = watchClock()
        // The people already here were announced during the join, before
        // anybody could listen, so the roster is read rather than heard.
        handle(theatre.roster(call.identities, now.toULong()))
        handle(theatre.joined(now.toULong()))

        // Four times a second: the keeper's step (WatchScreen.tsx `KEEP_MS`),
        // and the theatre's tick, which it wants at least once a second.
        scope.launch {
            while (true) {
                delay(WatchPlayer.KEEP_MS)
                beat()
            }
        }
        scope.launch {
            val answer = try {
                deployment.watchReady()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                false
            }
            if (!over) ready = answer
        }
    }

    /** The application's context, from the first of the feature's views on screen; see [CoreWatchMachinery]. */
    fun lend(context: Context) {
        (machinery as? CoreWatchMachinery)?.let { if (it.context == null) it.context = context.applicationContext }
    }

    /**
     * What the control bar offers (WatchFeature.swift `offered`, `playing`;
     * Room.tsx ~295): the button to whoever can start a show, and to
     * everybody once there is one, to see what is on and add to it; never in
     * a sealed call; and a dot on it while the show plays. Read from Compose
     * state, so the bar redraws as these change.
     */
    val offer: BarOffer = object : BarOffer {
        override val offered: Boolean get() = !sealed && (ready || onStage)
        override val playing: Boolean get() = show?.playing == true
    }

    // MARK: The call

    private fun hear(event: CallEvent) {
        if (over) return
        val now = watchClock().toULong()
        when (event) {
            is CallEvent.ParticipantJoined -> handle(theatre.participantConnected(event.participant.identity, now))
            is CallEvent.ParticipantLeft -> handle(theatre.participantDisconnected(event.identity, now))
            is CallEvent.StateChanged -> when (event.state) {
                CallState.RECONNECTING -> reconnecting = true
                CallState.CONNECTED -> if (reconnecting) {
                    // A reconnect replaces the roster without saying who left.
                    reconnecting = false
                    handle(theatre.roster(call.identities, now))
                }
                CallState.DISCONNECTED -> end()
            }
            is CallEvent.Data -> {
                // Only from a participant: the server says nothing on this
                // topic, and anything arriving without a sender is not a
                // show's word.
                val sender = event.identity ?: return
                if (event.topic != TOPIC) return
                handle(theatre.onMessage(sender, call.person(sender)?.name ?: "", event.payload, now))
            }
            is CallEvent.Ended -> end()
            else -> {}
        }
    }

    /**
     * The call is over: the player stops at once rather than when the call's
     * model is let go, which would leave a film playing after the person left
     * the room. For good: [over] is what every late answer looks at first.
     * Only a holder's close says anything (a viewer's does nothing), so on
     * Apple a video's description fetched after this, or a link read after
     * it, went on to read the show again, found the last one still there and
     * built it a player; the film started again on a call already left.
     */
    private fun end() {
        if (over) return
        over = true
        player?.stop()
        player = null
        theatre.close(watchClock().toULong())
        show = null
        onStage = false
    }

    private fun beat() {
        if (over) return
        val now = watchClock()
        handle(theatre.tick(now.toULong()))
        refresh(now)
        // The clock read again: the tick or the refresh may have just built a
        // player, whose source began after `now`; measured against the older
        // reading, that source's age came out below nothing (WatchStall).
        val player = player
        val show = show
        if (player != null && show != null) player.keep(watchClock(), show, heardAtMs)
    }

    /**
     * Reads the show again and follows it: a new video gets a player of its
     * own, and a show that ended takes its player with it.
     */
    internal fun refresh(now: Long) {
        if (over) return
        val snapshot = theatre.state(now.toULong())
        val heard = snapshot.heardAtMs.toLong()
        if (snapshot.show != show || heard != heardAtMs) {
            show = snapshot.show
            heardAtMs = heard
        }
        if (snapshot.mine != mine) mine = snapshot.mine
        if ((show != null) != onStage) onStage = show != null

        // A video that stopped being playable here (askAgain learned the
        // server will not relay it) takes its player with it too, and the
        // stage says so in its place.
        val video = show?.now
        val playable = video?.let { infos[it.ticket]?.video }
        player?.let { current ->
            if (current.video.key != video?.key || playable == null) {
                current.stop()
                player = null
            }
        }
        if (video == null || player != null) return
        val resolved = info(video.ticket)?.video ?: return
        player = WatchPlayer(video, resolved, this, machinery, deployment.baseUrl(), machinery.mainland(call.region))
    }

    // MARK: What the theatre says

    private fun handle(out: WatchOutput, answering: Boolean = true) {
        // Nothing the theatre says once the call is over is said: its words
        // would go into a call already left, and its asks start the fetches
        // that brought the show back.
        if (over) return
        for (packet in out.packets) send(packet)
        for (why in out.refusals) say(WatchWords.refusal(why))
        if (answering) {
            for (ask in out.asks) scope.launch { handle(answer(ask)) }
        }
        if (out.changed) refresh(watchClock())
    }

    /**
     * Published in the order the theatre said them: one at a time, each after
     * the last has gone, since a state overtaken by the word that the show
     * closed would have everybody believe in it again.
     */
    private fun send(packet: WatchPacket) {
        val previous = sending
        sending = scope.launch {
            previous?.join()
            quietly { call.send(packet.topic, packet.payload, packet.destinations, packet.reliable) }
        }
    }

    /** Finds out what the theatre asked and gives it the answer. */
    private suspend fun answer(ask: WatchAsk): WatchOutput = when (ask) {
        is WatchAsk.Resolve -> {
            val answer = try {
                when (val found = deployment.resolveVideo(ask.text, ask.forOther)) {
                    is ResolvedLink.Playable -> {
                        remember(found.video)
                        WatchAnswer.Found(found.video.ticket, found.video.duration, found.video.live, found.video.grantor)
                    }
                    is ResolvedLink.NotPlayable -> WatchAnswer.Refused(WatchWords.code(found.reason))
                }
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                // What the web's resolveVideo makes of anything thrown.
                WatchAnswer.Refused("failed")
            }
            theatre.answer(ask.request, answer, watchClock().toULong())
        }
        is WatchAsk.Read -> {
            val answer = when (val found = readTicket(ask.ticket)) {
                is WatchInfo.Known -> WatchAnswer.Found(found.resolved.ticket, found.resolved.duration, found.resolved.live, found.resolved.grantor)
                WatchInfo.Unavailable -> WatchAnswer.Refused("unavailable")
                WatchInfo.Failed -> WatchAnswer.Refused("failed")
            }
            theatre.answer(ask.request, answer, watchClock().toULong())
        }
    }

    private suspend fun readInfo(ticket: String): WatchInfo = try {
        when (val found = deployment.videoInfo(ticket)) {
            is TicketVideo.Known -> WatchInfo.Known(found.video)
            TicketVideo.Unavailable -> WatchInfo.Unavailable
            TicketVideo.Failed -> WatchInfo.Failed
        }
    } catch (fault: CancellationException) {
        throw fault
    } catch (_: Exception) {
        WatchInfo.Failed
    }

    /**
     * Said in the call's corner, as the web says a refusal (Room.tsx
     * `hearShows`, `actionFailed`), wherever the panel is.
     */
    fun say(text: String) {
        if (!over) call.actionFailed(text)
    }

    // MARK: What each ticket plays (watch-api.ts)

    /** What a ticket plays, if this app knows yet; asks the server once if not, and the answer redraws whoever asked. */
    fun info(ticket: String): WatchInfo? {
        infos[ticket]?.let { return it }
        if (ticket !in asking && !over) {
            asking[ticket] = mutableListOf()
            scope.launch { fetch(ticket) }
        }
        return null
    }

    /** The same question, waited for (watch-api.ts `readTicket`). */
    private suspend fun readTicket(ticket: String): WatchInfo {
        info(ticket)?.let { return it }
        val waiting = CompletableDeferred<WatchInfo>()
        asking.getOrPut(ticket) { mutableListOf() }.add(waiting)
        return waiting.await()
    }

    /**
     * Whoever waits is always answered, even after the call is over, since a
     * wait left unanswered is a task that never ends; the show is only read
     * again while there is one ([refresh] looks at [over]).
     */
    private suspend fun fetch(ticket: String) {
        val found = readInfo(ticket)
        infos = infos + (ticket to found)
        for (one in asking.remove(ticket).orEmpty()) one.complete(found)
        refresh(watchClock())
    }

    /**
     * What a ticket plays, asked again, for a player whose relay stopped
     * partway through a video that plays nowhere else (watch-api.ts
     * `askAgain`). Only "the server will not relay it" replaces what was
     * known: a ticket that still reads as playable is the player's to try
     * again, and one that could not be read this time is no reason to unlearn
     * a video that was playing.
     */
    suspend fun askAgain(ticket: String): WatchInfo {
        val found = readInfo(ticket)
        if (found == WatchInfo.Unavailable) {
            infos = infos + (ticket to WatchInfo.Unavailable)
            refresh(watchClock())
        }
        return found
    }

    /** What this person read themselves, so it is not asked for again. */
    private fun remember(video: ResolvedVideo) {
        infos = infos + (video.ticket to WatchInfo.Known(video))
    }

    // MARK: What the person does

    /**
     * Adds what a link plays to the show; true if it was added, or sent to be
     * (WatchTab.tsx `addVideo`).
     *
     * Read here by anybody who may read links, and the show is handed only
     * the ticket; started here with it when nobody runs one. Somebody who may
     * not hands the link itself to whoever runs the show, who reads it on
     * their behalf. Read here rather than by the holder because of whose
     * choice a video is: a film from a media server is relayed only for the
     * person given that who chose it, and a link the holder reads for
     * somebody else is said to be somebody else's and refused such a film.
     */
    suspend fun add(pasted: String): Boolean {
        val text = pasted.trim()
        if (text.isEmpty() || sealed || over) return false
        val now = watchClock()

        if (!ready) {
            val out = theatre.addLink(text, now.toULong())
            handle(out)
            return out.accepted
        }

        refresh(now)
        if (show == null) {
            val started = theatre.start(now.toULong())
            handle(started)
            if (!started.accepted) return false
            return own(theatre.addLink(text, watchClock().toULong()))
        }
        if (mine) return own(theatre.addLink(text, now.toULong()))

        val found = try {
            deployment.resolveVideo(WatchWords.cut(text, LINK_LENGTH), false)
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            say(WatchWords.refusal("failed"))
            return false
        }
        // Left, or ended, while the link was being read: nothing more.
        if (over) return false
        return when (found) {
            is ResolvedLink.NotPlayable -> {
                say(WatchWords.refusal(WatchWords.code(found.reason)))
                false
            }
            is ResolvedLink.Playable -> {
                remember(found.video)
                // The link beside its ticket, for a holder whose page predates
                // tickets; never for a media server's film, which such a
                // holder would read with its own session and so as its own
                // choice.
                val link = if (found.video.play.kind == ResolvedPlayKind.RELAY) null else text
                // Where the show ended while the link was being read, this
                // starts the next one here with it (the theatre's add_ticket
                // does).
                own(theatre.addTicket(found.video.ticket, link, watchClock().toULong()))
            }
        }
    }

    /**
     * An output whose asks are this person's own adding: answered before
     * saying whether it worked, so the field empties only for a video that
     * went in.
     */
    private suspend fun own(out: WatchOutput): Boolean {
        handle(out, answering = false)
        var added = out.accepted && out.refusals.isEmpty()
        for (ask in out.asks) {
            val answered = answer(ask)
            if (over) return false
            handle(answered)
            if (answered.refusals.isNotEmpty()) added = false
        }
        return added && !over
    }

    /**
     * Play or pause for everybody. Felt here at once: the player is told too,
     * so the press does not wait for the show's answer (WatchScreen.tsx
     * `toggle`).
     */
    fun toggle() {
        val show = show ?: return
        if (show.playing) {
            player?.pauseNow()
            handle(theatre.pause(watchClock().toULong()))
        } else {
            player?.playNow()
            handle(theatre.play(watchClock().toULong()))
        }
    }

    fun setPlaying(playing: Boolean) {
        val show = show ?: return
        if (show.playing != playing) toggle()
    }

    /** Moves everybody to [to], and this player at once (`seekTo`). */
    fun seek(to: Double) {
        player?.seekNow(to)
        handle(theatre.seek(to, watchClock().toULong()))
    }

    /** Past the video named, and only that one: two presses do not skip two. */
    fun skip(key: String) {
        handle(theatre.skip(key, watchClock().toULong()))
    }

    fun remove(key: String) {
        handle(theatre.remove(key, watchClock().toULong()))
    }

    /** The call is about to be left: a holder closes the show and waits for that word to go out. */
    suspend fun leaving() {
        if (!mine || over) return
        handle(theatre.close(watchClock().toULong()))
        sending?.join()
    }

    /** Stop running the show here, saying so last of all. */
    fun stop() {
        handle(theatre.close(watchClock().toULong()))
    }

    fun playerReady(key: String) {
        handle(theatre.playerReady(key, watchClock().toULong()))
    }

    fun playerEnded(key: String) {
        handle(theatre.playerEnded(key, watchClock().toULong()))
    }

    /**
     * The media servers, asked when the panel opens and only of somebody who
     * could start a show: browsing is for choosing, and choosing is theirs
     * (WatchPanel.tsx). The web reads every refusal here as none.
     */
    fun loadServers() {
        if (!ready || sealed || over) return
        scope.launch {
            val found = try {
                deployment.mediaServers()
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                emptyList()
            }
            if (!over && found != servers) servers = found
        }
    }

    companion object {
        const val PART = "watch"
        /** The data topic (watch.ts `TOPIC`). */
        const val TOPIC = "watch"
        /** The longest link a show reads, in UTF-16 units, as the web's field allows. */
        const val LINK_LENGTH = 2000

        /**
         * The call's show, made the first time anybody asks for it, with what
         * it offers the control bar beside it (BarOffer.WATCH).
         */
        fun of(call: CallModel): WatchEngine =
            call.parts[PART] as? WatchEngine ?: WatchEngine(call, CoreWatchMachinery()).also { engine ->
                call.parts[PART] = engine
                call.parts[BarOffer.WATCH] = engine.offer
            }
    }
}
