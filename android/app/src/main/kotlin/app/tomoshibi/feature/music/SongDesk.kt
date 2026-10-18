package app.tomoshibi.feature.music

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import app.tomoshibi.basics.t
import app.tomoshibi.media.SoundLevel
import app.tomoshibi.model.CallModel
import app.tomoshibi.model.HearingSetting
import app.tomoshibi.model.Person
import app.tomoshibi.model.Rehearsal
import app.tomoshibi.model.Sound
import app.tomoshibi.model.host.Hearing
import app.tomoshibi.model.quietly
import app.tomoshibi.ui.room.BarOffer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.CallState
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.DeskAction
import uniffi.tomoshibi_protocol.DeskEntry
import uniffi.tomoshibi_protocol.DeskLibrary
import uniffi.tomoshibi_protocol.DeskOutput
import uniffi.tomoshibi_protocol.DeskPacket
import uniffi.tomoshibi_protocol.DeskPrepared
import uniffi.tomoshibi_protocol.DeskSong
import uniffi.tomoshibi_protocol.DeskState
import uniffi.tomoshibi_protocol.JukeboxDesk
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.MusicLibrary
import uniffi.tomoshibi_protocol.MusicLink
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.MusicTrack
import uniffi.tomoshibi_protocol.SoundKind
import uniffi.tomoshibi_protocol.deskLibrary
import uniffi.tomoshibi_protocol.soundLosslessOffered

private const val TAG = "Tomoshibi"

/** Which part of the panel is showing (MusicPanel.tsx `Tab`). */
enum class MusicTab { Desk, Search, Playlist }

/** The music's track as published, and how to take it down. */
class PublishedMusic(val input: MusicInput, val unpublish: suspend () -> Unit)

/**
 * What the desk needs from the core and from the device, gathered so that a
 * test can stand in for each: the protocol crate's desk and its two rules,
 * fetching a song, and publishing the music's track.
 */
interface DeskMachinery {
    fun desk(identity: String, name: String, sealed: Boolean): JukeboxDesk
    fun deskLibrary(libraries: List<DeskLibrary>): String?
    fun losslessOffered(wanted: Boolean, encrypted: Boolean): Boolean
    suspend fun load(entry: DeskEntry, quality: MusicQuality, keepOriginal: Boolean, call: CallModel): MusicLoader.Outcome
    suspend fun publish(call: CallModel, losslessOffered: Boolean): PublishedMusic
}

/** The machinery of a real call. */
object CoreDeskMachinery : DeskMachinery {
    override fun desk(identity: String, name: String, sealed: Boolean) = JukeboxDesk(identity, name, sealed)
    override fun deskLibrary(libraries: List<DeskLibrary>) = uniffi.tomoshibi_protocol.deskLibrary(libraries)

    /** Any participant here can send it, which in the web only Chromium can. */
    override fun losslessOffered(wanted: Boolean, encrypted: Boolean) = soundLosslessOffered(wanted, encrypted, true)

    override suspend fun load(entry: DeskEntry, quality: MusicQuality, keepOriginal: Boolean, call: CallModel) =
        MusicLoader.load(entry.source, entry.id, quality, keepOriginal, call.deployment)

    override suspend fun publish(call: CallModel, losslessOffered: Boolean): PublishedMusic {
        val input = call.call.publishAudio(SoundKind.LibraryMusic(losslessOffered))
        return PublishedMusic(CoreMusicInput(input)) { input.unpublish() }
    }
}

/**
 * One call's song desk (MusicFeature.swift `SongDesk`; web/src/live/jukebox.ts,
 * music.ts, sound.ts, duck.ts, components/room/MusicPanel.tsx): the protocol
 * crate's `JukeboxDesk` fed from the call and its answers carried out, the
 * music it plays, and the music it hears turned down under speech.
 *
 * The desk itself is the protocol crate's (core/protocol/src/jukebox): this
 * tells it what the call says and does what it asks -- fetch and decode a
 * song, publish it, pause it, stop it, search the library, read a link --
 * answering with what came of each. Everything it says to the room goes out
 * on the `jukebox` topic exactly as the web writes it.
 *
 * Every answer to the desk is given from a task of its own, never from inside
 * the handling of another of its outputs: what one call asks must all be
 * done, in order, before anything the next one says.
 *
 * Made once per call and kept in the call's `parts` ([of]). Main thread only;
 * everything it starts runs in the call's own scope and ends with the call.
 */
class SongDesk internal constructor(
    val call: CallModel,
    private val machinery: DeskMachinery,
    /** False for a picture of the panel with nothing behind it: nothing is listened to, asked or timed. */
    live: Boolean = true,
) {
    // MARK: What the panel draws

    var state by mutableStateOf<DeskState?>(null)
        internal set
    var mine by mutableStateOf(false)
        internal set
    /** The libraries this participant can reach; null where there is none or they are not signed in, which is most people (Room.tsx `library`). */
    var libraries by mutableStateOf<List<MusicLibrary>?>(null)
        internal set
    /** An ask is being looked up in the library here. */
    var asking by mutableStateOf(false)
        internal set
    /** The music published from here, playing or paused or between songs. */
    var playingHere by mutableStateOf(false)
        internal set
    /**
     * The panel is on screen. While it is, the bar keeps its button even
     * after the music has stopped, so the button that opened the panel is
     * there to close it. Set by the panel.
     */
    var panelOpen by mutableStateOf(false)

    // These three are kept between calls under the web's names, and in a
    // rehearsal not at all (`Rehearsal.choices`): on the Mac a test's
    // `monitor50` and `noduck` steps were written into the owner's own
    // defaults, and the owner's next real call played the music at half
    // volume in their own speakers.

    private var qualityState by mutableStateOf(
        if (Rehearsal.choices?.string(QUALITY_KEY) == "best") MusicQuality.BEST else MusicQuality.LOSSLESS,
    )
    /** The library's quality, this participant's own (preferences.ts `meet-live.music-quality`): lossless unless the highest was chosen. */
    var quality: MusicQuality
        get() = qualityState
        set(value) {
            qualityState = value
            Rehearsal.choices?.set(QUALITY_KEY, if (value == MusicQuality.BEST) "best" else null)
        }

    private var ducksState by mutableStateOf(Rehearsal.choices?.string(DUCK_KEY) != "off")
    private val duck = MusicDuck(ducksState)
    /** Whether the music is turned down while anybody talks (duck.ts `meet-live.music-duck`, on unless turned off). */
    var ducks: Boolean
        get() = ducksState
        set(value) {
            ducksState = value
            Rehearsal.choices?.set(DUCK_KEY, if (value) null else "off")
            duck.enabled = value
            applyDuck(clock())
        }

    private var monitorState by mutableFloatStateOf(
        // A volume nothing was kept for is full, and so is one that is not a
        // volume: sound.ts once read "nothing kept" as nought and played the
        // music to everybody but its own speakers.
        Rehearsal.choices?.string(MONITOR_KEY)?.toFloatOrNull()?.takeIf { it in 0f..1f } ?: 1f,
    )
    /**
     * How loud the music playing from here is in this phone's own speakers
     * (sound.ts `monitorVolume`, `meet-live.music-monitor`). Only there: what
     * is sent is the decoded file whatever this says.
     */
    var monitorVolume: Float
        get() = monitorState
        set(value) {
            monitorState = value
            Rehearsal.choices?.set(MONITOR_KEY, if (value == 1f) null else value.toString())
            call.audio.setVolume(MusicDuck.LOCAL, value)
        }

    // What the panel was showing, kept for the call: a playlist somebody had
    // opened was gone the moment they looked at the desk otherwise
    // (MusicPanel.tsx `useRemembered`).
    var tab by mutableStateOf(MusicTab.Desk)
    var searchSource by mutableStateOf("")
    var searchQuery by mutableStateOf("")
    var searching by mutableStateOf(false)
        internal set
    var searchResults by mutableStateOf<List<MusicTrack>?>(null)
        internal set
    var searchProblem by mutableStateOf<String?>(null)
        internal set
    var searchAdded by mutableStateOf<Set<String>>(emptySet())
    var pasted by mutableStateOf("")
    var reading by mutableStateOf(false)
        internal set
    var linked by mutableStateOf<MusicLink?>(null)
        internal set
    var linkProblem by mutableStateOf<String?>(null)
        internal set
    var linkAdded by mutableStateOf<Set<String>>(emptySet())

    // MARK: Machinery

    private val desk = machinery.desk(call.localIdentity, call.localName, call.sealed)
    private val outbox = Channel<DeskPacket>(Channel.UNLIMITED)
    private val loads = HashMap<ULong, Job>()
    private val prepared = HashMap<ULong, DecodedSong>()
    private var output: MusicOutput? = null
    private var published: PublishedMusic? = null
    private var opening: Deferred<MusicOutput>? = null
    /** Bumped by every stop, so that a track still being published when the music was stopped finds out and takes itself down (sound.ts `generations`). */
    private var generation = 0
    private val meters = HashMap<String, Pair<String, SoundLevel>>()
    private var ducking: Job? = null
    private var lastDuck = 0.0
    private var wasFading = false
    private var asked: ULong? = null
    private var over = false
    /** What was last handed to the call for each person's music, so the book is only put back on the call where it changed. */
    private val heard = HashMap<String, HearingSetting>()

    private val scope get() = call.scope

    /**
     * Whether the music goes out with a lossless stream beside it, as the
     * web's `playDecoded(room, decoded, rememberedLossless())` decides: where
     * it is wanted, and never in a sealed call, whose data is not sealed and
     * would carry the file's samples across the relay in the clear.
     */
    val losslessOffered: Boolean
        get() = machinery.losslessOffered(wanted = Rehearsal.choices?.string(LOSSLESS_KEY) != "off", encrypted = call.sealed)

    init {
        if (live) begin()
    }

    private fun begin() {
        // One sender, so the room hears the desk's words in the order it said them.
        scope.launch {
            for (packet in outbox) quietly { call.send(packet.topic, packet.payload, packet.destinations, packet.reliable) }
        }
        call.listen { on(it) }

        // Whoever is here already was announced while the call was being
        // joined, before this existed: the roster, their microphones and any
        // music they are playing are taken up now.
        val now = now()
        apply(desk.roster(call.identities, now))
        apply(desk.joined(now))
        for (person in call.people) {
            for (track in person.tracks.values) subscribed(person.id, track)
            if (person.musicTrack != null) arrived(person.id)
        }
        refresh()
        followTheBook()

        // The holder's word every fifteen seconds, and a desk heard of that
        // has gone quiet: the desk wants the time at least once a second.
        scope.launch {
            while (true) {
                delay(1000)
                if (over) return@launch
                apply(desk.tick(now()))
            }
        }
        scope.launch { findLibraries() }
    }

    /**
     * The libraries, asked once, as Room.tsx asks: a deployment without one,
     * or somebody not signed in, is no library and no tabs to search it.
     * Asked in a sealed call too, where the desk plays for its holder alone.
     */
    private suspend fun findLibraries() {
        val found = try {
            call.deployment.musicSources()
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            emptyList()
        }
        libraries = found.ifEmpty { null }
        if (searchSource.isEmpty()) searchSource = (found.firstOrNull { it.signedIn } ?: found.firstOrNull())?.id ?: ""
    }

    // MARK: The call

    private fun on(event: CallEvent) {
        if (over) return
        val now = now()
        when (event) {
            is CallEvent.ParticipantJoined -> apply(desk.participantConnected(event.participant.identity, now))
            is CallEvent.ParticipantLeft -> {
                apply(desk.participantDisconnected(event.identity, now))
                meters.entries.removeAll { it.value.first == event.identity }
                duck.left(event.identity)
            }
            // A reconnect replaces the roster without saying who left.
            is CallEvent.StateChanged -> if (event.state == CallState.CONNECTED) apply(desk.roster(call.identities, now))
            is CallEvent.Data -> {
                // Only from somebody in the room: the server's own messages
                // are not the desk's (jukebox.ts `onData`, `!participant`).
                val sender = event.identity ?: return
                if (event.topic != TOPIC) return
                apply(desk.onMessage(sender, call.person(sender)?.name ?: "", event.payload, now))
            }
            is CallEvent.TrackPublished -> if (isMusic(event.track)) arrived(event.identity)
            is CallEvent.TrackSubscribed -> subscribed(event.identity, event.track)
            is CallEvent.TrackUnpublished -> gone(event.identity, event.sid)
            is CallEvent.TrackUnsubscribed -> gone(event.identity, event.sid)
            is CallEvent.Ended -> end()
            else -> {}
        }
    }

    private fun gone(identity: String, sid: String) {
        meters.remove(sid)
        if (call.person(identity)?.musicTrack == null) {
            duck.left(identity)
            heard.remove(identity)
        }
    }

    /**
     * A remote sound began to play here: a microphone is listened to for the
     * duck, and music is faded in, turned down under speech and set to the
     * listener's volume. The call model has already started it playing in a
     * source of its own, which is what the meter listens to.
     */
    private fun subscribed(identity: String, track: TrackInfo) {
        if (track.kind != MediaKind.AUDIO || call.audio.volume(track.sid) == null) return
        if (track.source == MediaSource.MICROPHONE) call.audio.meter(track.sid)?.let { meters[track.sid] = identity to it }
        if (isMusic(track)) {
            arrived(identity)
            // The player was made at full volume: the listener's setting goes
            // back on it, and on the core, which decides from it whether to
            // ask for the lossless stream at all.
            heard.remove(identity)
            hear(identity)
            val factor = duck.factor(identity, clock()).toFloat()
            call.audio.setDuck(track.sid, factor)
            call.lossless.setDuck(identity, factor)
        }
    }

    /** Music arriving from somebody whose desk is not heard here, which still has a volume (MusicPanel.tsx `player`). */
    val player: Person? get() = if (state == null) call.people.firstOrNull { it.musicTrack != null } else null

    /**
     * What the control bar offers (MusicFeature.swift `available`,
     * `playing`; Room.tsx ~280): the panel to somebody who can reach the
     * library, before there is any music; to everybody once there is a desk
     * or music to turn down or ask for; and while the panel is open, so the
     * button that opened it is there to close it after the music has
     * stopped. A sealed call is no exception: the desk there plays for its
     * holder alone, and everybody else gets a volume for the music.
     */
    val offer: BarOffer = object : BarOffer {
        override val offered: Boolean
            get() = libraries != null || state != null || playingHere || panelOpen || call.people.any { it.musicTrack != null }
        override val playing: Boolean get() = state?.now != null || playingHere || player != null
    }

    private fun end() {
        over = true
        for (task in loads.values) task.cancel()
        loads.clear()
        for (song in prepared.values) song.close()
        prepared.clear()
        ducking?.cancel()
        output?.stop()
        output = null
        outbox.close()
    }

    // MARK: The desk's words and wishes

    private fun apply(out: DeskOutput) {
        for (packet in out.packets) outbox.trySend(packet)
        for (action in out.actions) perform(action)
        for (entry in out.skipped) say(t("Skipped {title}: the library's account cannot play it.", mapOf("title" to entry.title)))
        if (out.changed) refresh()
    }

    internal fun refresh() {
        val snapshot = desk.state(now())
        if (state != snapshot.state) state = snapshot.state
        if (mine != snapshot.mine) mine = snapshot.mine
    }

    private fun perform(action: DeskAction) {
        when (action) {
            is DeskAction.Prepare -> prepare(action.request, action.entry)
            is DeskAction.Abandon -> {
                loads.remove(action.request)?.cancel()
                prepared.remove(action.request)?.close()
            }
            is DeskAction.Play -> play(action.request, action.entry)
            DeskAction.Stop -> stopMusic()
            is DeskAction.Pause -> output?.pause(action.paused)
            is DeskAction.Search -> search(action.request, action.query, action.mine)
            is DeskAction.ReadLink -> readLink(action.request, action.text, action.mine)
        }
    }

    /** Describe, fetch and decode, off the main thread; the desk is told what came of it unless it abandoned the request meanwhile. */
    private fun prepare(request: ULong, entry: DeskEntry) {
        val quality = quality
        // The file's own samples are kept only where a lossless stream will
        // carry them: the track already out says, or else whether the next
        // one would be offered.
        val keepOriginal = output?.lossless ?: losslessOffered
        loads[request] = scope.launch {
            var song: DecodedSong? = null
            val outcome = try {
                when (val loaded = machinery.load(entry, quality, keepOriginal, call)) {
                    is MusicLoader.Outcome.Ready -> {
                        song = loaded.song
                        DeskPrepared.Ready(loaded.song.quality)
                    }
                    MusicLoader.Outcome.Unavailable -> DeskPrepared.Unavailable
                }
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                Log.w(TAG, "music: ${entry.title} could not be prepared: ${fault.message}")
                DeskPrepared.Failed
            }
            if (loads.remove(request) == null || over) {
                song?.close()
                return@launch
            }
            song?.let { prepared[request] = it }
            apply(desk.prepared(request, outcome, now()))
        }
    }

    private fun play(request: ULong, entry: DeskEntry) {
        val song = prepared.remove(request)
        val started = generation
        scope.launch {
            var ok = false
            if (song != null) {
                // Any failure, a stop that overtook the publishing among
                // them, is a song that did not play, which the desk is told.
                val output = try {
                    openOutput()
                } catch (_: Exception) {
                    null
                }
                if (output != null && started == generation && !over) {
                    output.play(song, entry.key)
                    ok = true
                }
            }
            if (!ok) song?.close()
            if (over) return@launch
            apply(desk.played(request, ok, now()))
        }
    }

    /** The music's own track, published the first time a song plays and kept across the songs that follow until the music stops. */
    private suspend fun openOutput(): MusicOutput {
        output?.let { return it }
        opening?.let { return it.await() }
        val started = generation
        val task = scope.async {
            val offered = losslessOffered
            val track = machinery.publish(call, offered)
            // Stopped while it was being published: taken down again rather
            // than left playing silence into the room (sound.ts, "a stop that
            // arrives in between used to be outrun").
            if (started != generation || over) {
                quietly { track.unpublish() }
                throw CancellationException("the music stopped while it was being published")
            }
            val ring = try {
                call.audio.playLocally(MusicDuck.LOCAL, 2)
            } catch (fault: Exception) {
                // The music still goes out; only this phone's own speakers go
                // without it.
                Log.w(TAG, "music: no monitor: ${fault.message}")
                null
            }
            val made = MusicOutput(track.input, offered, ring) { key -> songEnded(key) }
            arrived(MusicDuck.LOCAL)
            call.audio.setVolume(MusicDuck.LOCAL, monitorVolume)
            call.audio.setDuck(MusicDuck.LOCAL, duck.factor(MusicDuck.LOCAL, clock()).toFloat())
            made.start()
            output = made
            published = track
            playingHere = true
            Log.i(TAG, "music: published, lossless offered $offered")
            made
        }
        opening = task
        try {
            return task.await()
        } finally {
            if (opening === task) opening = null
        }
    }

    private fun stopMusic() {
        generation += 1
        opening = null
        output?.stop()
        published?.let { track -> scope.launch { quietly { track.unpublish() } } }
        output = null
        published = null
        playingHere = false
        call.audio.stopPlaying(MusicDuck.LOCAL)
        meters.remove(MusicDuck.LOCAL)
        duck.left(MusicDuck.LOCAL)
    }

    private fun songEnded(key: String) {
        if (!over) apply(desk.trackEnded(key, now()))
    }

    /** The top hit for a keyword, from the library the desk searches (`desk_library`: the first signed in, or else the first). */
    private fun search(request: ULong, query: String, mine: Boolean) {
        val library = machinery.deskLibrary(libraries.orEmpty().map { DeskLibrary(it.id, it.signedIn) })
        scope.launch {
            var songs = emptyList<DeskSong>()
            var failed = false
            if (library != null) {
                try {
                    songs = searched(library, query).map { song(it, library) }
                } catch (fault: CancellationException) {
                    throw fault
                } catch (_: Exception) {
                    failed = true
                }
            }
            if (over) return@launch
            val out = desk.found(request, songs, now())
            apply(out)
            if (!mine) return@launch
            answered(request, if (failed) t("The library did not answer. Try again.") else if (out.accepted) null else t("Nothing found."))
        }
    }

    private fun readLink(request: ULong, text: String, mine: Boolean) {
        scope.launch {
            var songs = emptyList<DeskSong>()
            var failed = false
            try {
                call.deployment.musicLink(text)?.let { link -> songs = link.tracks.map { song(it, link.source) } }
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                failed = true
            }
            if (over) return@launch
            val out = desk.found(request, songs, now())
            apply(out)
            if (!mine) return@launch
            answered(request, if (failed) t("The library did not answer. Try again.") else if (out.accepted) null else t("That link names no playlist or song."))
        }
    }

    private fun answered(request: ULong, problem: String?) {
        if (asked == request) {
            asked = null
            asking = false
        }
        if (problem != null) say(problem)
    }

    // MARK: What the person does

    /** Where nobody runs a desk and this participant can reach a library, it starts here first (MusicPanel.tsx `add`, `ask`). */
    private fun startIfNobodyRunsOne() {
        if (state == null && libraries != null) apply(desk.start(now()))
    }

    /** Adds songs chosen from the library: to the desk here, or sent to whoever runs the one heard. */
    fun add(songs: List<DeskSong>) {
        startIfNobodyRunsOne()
        val out = desk.addSongs(songs, now())
        apply(out)
        if (!out.accepted) say(t("Nobody is running the song desk."))
    }

    /** Asks for a song by what was typed: a song's name, or a playlist or song link (MusicPanel.tsx `ask`). */
    fun ask(typed: String) {
        val text = typed.trim()
        if (text.isEmpty() || asking) return
        startIfNobodyRunsOne()
        val out = desk.ask(text, now())
        // The search or link this set off, where the desk is here: the box
        // stays busy until it is answered, as the web's awaits it.
        for (action in out.actions) {
            val request = when (action) {
                is DeskAction.Search -> action.request.takeIf { action.mine }
                is DeskAction.ReadLink -> action.request.takeIf { action.mine }
                else -> null
            } ?: continue
            asked = request
            asking = true
        }
        apply(out)
        if (!out.accepted) say(if (mine) t("Nothing found.") else t("Nobody is running the song desk."))
    }

    fun vote() = apply(desk.vote(now()))
    fun skip() = apply(desk.skip(now()))
    fun pause(paused: Boolean) = apply(desk.pause(paused, now()))
    fun remove(key: String) = apply(desk.remove(key, now()))
    fun toTop(key: String) = apply(desk.toTop(key, now()))
    fun close() = apply(desk.close(now()))

    private suspend fun searched(library: String, query: String): List<MusicTrack> = call.deployment.musicSearch(library, query, 1u)

    // MARK: How loud somebody's music is here

    /**
     * The listener's own setting for somebody's music: the one the sound
     * panel keeps for that person's music, so a change in either is the
     * other's too (MusicPanel.tsx uses the sound panel's own row for it).
     */
    fun hearing(identity: String): HearingSetting = Hearing.shared.setting(identity, Sound.Music)

    fun setVolume(identity: String, volume: Double) {
        Hearing.shared.set(identity, Sound.Music, hearing(identity).copy(volume = volume))
        hear(identity)
    }

    /**
     * Muting is a block at the media server, not a volume of nought: a muted
     * song is not downloaded at all, and its lossless stream not even asked
     * for (MusicPanel.tsx, hearing.ts `setBlocked`).
     */
    fun setBlocked(identity: String, blocked: Boolean) {
        Hearing.shared.set(identity, Sound.Music, hearing(identity).copy(blocked = blocked))
        hear(identity)
    }

    /**
     * Puts the book's setting for this person's music on the call, where it
     * changed: the Opus track's source, the lossless stream standing in for
     * it, and the core, which blocks both at the server and decides whether
     * the lossless stream is asked for. Through `CallModel.hear`, as the web's
     * sound panel row goes through hearing.ts and Audible.tsx; on Apple,
     * before this, the panel's volume moved only the Opus node, so a lossless
     * stream played on at full level and a muted song went on downloading.
     */
    private fun hear(identity: String) {
        if (call.person(identity)?.musicTrack == null) return
        val setting = hearing(identity)
        if (heard[identity] == setting) return
        heard[identity] = setting
        // Silenced, not only blocked, is what keeps the lossless stream from
        // being asked for: hearing.ts `silenced` counts a volume of nought,
        // which the core's own record does not, and a stream asked for and
        // played at nothing is upload the sender spends on nobody.
        scope.launch { quietly { call.hear(identity, MediaSource.UNKNOWN, setting.volume.toFloat(), setting.silenced) } }
    }

    /**
     * Follows the book wherever it is written: the sound panel's music row
     * writes the same setting, and the lossless stream and the core must hear
     * of it as they do from here. What is watched is the settings of whoever
     * plays music, which the book's own change counter is read through.
     */
    private fun followTheBook() {
        scope.launch {
            snapshotFlow { call.people.filter { it.musicTrack != null }.map { it.id to hearing(it.id) } }
                .drop(1)
                .collect { playing -> if (!over) for ((identity, _) in playing) hear(identity) }
        }
    }

    /** Searches the library chosen in the panel's search tab. */
    suspend fun searchLibrary() {
        val asked = searchQuery.trim()
        if (asked.isEmpty() || searching) return
        searching = true
        searchProblem = null
        try {
            searchResults = searched(searchSource, asked)
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            searchProblem = t("The library did not answer. Try again.")
        } finally {
            searching = false
        }
    }

    fun chooseLibrary(id: String) {
        searchSource = id
        searchResults = null
    }

    /** Reads the link pasted in the panel's playlist tab. */
    suspend fun openLink() {
        if (pasted.isBlank() || reading) return
        reading = true
        linkProblem = null
        try {
            val found = call.deployment.musicLink(pasted)
            if (found == null) linkProblem = t("That link names no playlist or song.")
            linked = found
            linkAdded = emptySet()
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            linkProblem = t("The library did not answer. Try again.")
        } finally {
            reading = false
        }
    }

    /**
     * Said in the call's corner, as the web says it (MusicPanel.tsx
     * `actionFailed`): drawn only inside the panel it went unseen whenever the
     * panel was shut, which is when a song is skipped.
     */
    private fun say(text: String) {
        call.actionFailed(text)
    }

    // MARK: Turning the music down under speech

    /** A music stream has started from somebody, or from here: faded in, and the microphones listened to while there is music to turn down. */
    private fun arrived(key: String) {
        duck.arrived(key, clock())
        if (ducking != null || over) return
        lastDuck = clock()
        ducking = scope.launch {
            while (true) {
                delay(MusicDuck.TICK_MS.toLong())
                if (over) return@launch
                if (!duckTick()) {
                    ducking = null
                    return@launch
                }
            }
        }
    }

    /**
     * One reading of every microphone that is on, this participant's
     * included, and the music moved (duck.ts `tick`). False once there is no
     * music and nothing still moving, when the clock stops.
     *
     * The levels are the mixer's: each a tenth of a second's RMS, where the
     * web reads 1024 samples every 40 ms. Two readings in a row can be the
     * same tenth, so a sound has to last a little longer here to count as
     * speech, and a click spread over a tenth of a second is mostly silence.
     */
    private fun duckTick(): Boolean {
        val now = clock()
        val music = output != null || opening != null || call.people.any { it.musicTrack != null }
        val levels = ArrayList<Double>()
        for ((sid, meter) in meters) {
            if (call.person(meter.first)?.tracks?.get(sid)?.muted != true) levels.add(meter.second.rms)
        }
        if (music && call.microphoneOn) call.audio.microphoneLevel?.let { levels.add(it.rms) }

        // Once more on the first tick after a fade-in ends: the duck says it
        // moved only while a stream is inside its fade, and on Apple a tick
        // that came late left the music at whatever the last tick inside the
        // fade had made it: 0.56 for good, measured.
        val fadedIn = wasFading && !duck.fading(now)
        if (duck.tick(levels, now, now - lastDuck) || fadedIn) applyDuck(now)
        wasFading = duck.fading(now)
        lastDuck = now
        return music || !duck.settled(now)
    }

    private fun applyDuck(now: Double) {
        for (person in call.people) {
            val sid = person.musicTrack?.sid ?: continue
            val factor = duck.factor(person.id, now).toFloat()
            call.audio.setDuck(sid, factor)
            // And the lossless stream standing in for it, which is played at
            // the same level (lossless-receiver.ts `levelOf`).
            call.lossless.setDuck(person.id, factor)
        }
        if (output != null) call.audio.setDuck(MusicDuck.LOCAL, duck.factor(MusicDuck.LOCAL, now).toFloat())
    }

    /** How far the music is turned down now. */
    val duckLevel: Double get() = duck.level

    /** Where the song playing from here is and how long it is, for the holder's own panel; nobody else knows, since the desk does not say. */
    val progress: Pair<Double, Double>? get() = output?.progress

    companion object {
        const val PART = "music"
        /** The data topic (jukebox.ts `TOPIC`). */
        const val TOPIC = "jukebox"
        const val QUALITY_KEY = "meet-live.music-quality"
        const val DUCK_KEY = "meet-live.music-duck"
        const val MONITOR_KEY = "meet-live.music-monitor"
        /**
         * Shared with the share menu's switch: the web keeps one choice for
         * both, sound shared on its own and the desk's music (sound.ts
         * `rememberedLossless`, on unless turned off).
         */
        const val LOSSLESS_KEY = "meet-live.lossless"

        /** The call's desk, made the first time anybody asks for it, with what it offers the control bar beside it (BarOffer.MUSIC). */
        fun of(call: CallModel): SongDesk =
            call.parts[PART] as? SongDesk ?: SongDesk(call, CoreDeskMachinery).also { desk ->
                call.parts[PART] = desk
                call.parts[BarOffer.MUSIC] = desk.offer
            }

        /**
         * The song desk's music, in either form (sound.ts `isMusic`,
         * lossless.ts `MUSIC`): its own source, so that it and a screen's
         * sound never take each other's place.
         */
        fun isMusic(track: TrackInfo): Boolean =
            track.kind == MediaKind.AUDIO && track.source == MediaSource.UNKNOWN && (track.name == "music" || track.name == "music-lossless")

        fun song(track: MusicTrack, source: String) =
            DeskSong(source, track.id, track.title, track.artists, track.album, track.cover, track.duration)

        /** The wall clock the desk's keys and staleness are written in, as `Date.now()` is in jukebox.ts. */
        fun now(): ULong = System.currentTimeMillis().toULong()

        /** A clock that never jumps, for the duck, in milliseconds. */
        fun clock(): Double = System.nanoTime() / 1e6
    }
}
