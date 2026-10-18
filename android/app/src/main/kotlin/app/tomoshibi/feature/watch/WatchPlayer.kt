package app.tomoshibi.feature.watch

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.model.Rehearsal
import kotlin.math.abs
import kotlinx.coroutines.launch
import uniffi.tomoshibi_protocol.ResolvedPlayKind
import uniffi.tomoshibi_protocol.ResolvedProxyKind
import uniffi.tomoshibi_protocol.ResolvedVideo
import uniffi.tomoshibi_protocol.WatchPlayerReading
import uniffi.tomoshibi_protocol.WatchShow
import uniffi.tomoshibi_protocol.WatchVideo

private const val TAG = "Tomoshibi"

/**
 * One way of playing: Media3's player for a file or a playlist, or YouTube's
 * own player in a web view (WatchScreen.tsx `Handle`; WatchPlayer.swift).
 */
interface WatchBackend {
    /** It has the video and can say where it is. */
    val ready: Boolean
    val time: Double
    /**
     * Not paused, as a video element's `!paused && !ended` reads: a player
     * waiting for data it was told to play is playing.
     */
    val playing: Boolean
    /** The rate it runs at when it plays. */
    val rate: Double
    /** It can run a little fast or slow; YouTube's cannot by small amounts. */
    val nudges: Boolean
    val youtube: Boolean
    /** It has enough to go on playing: a video element's `readyState >= 3`. */
    val flowing: Boolean
    fun play()
    fun pause()
    fun seek(to: Double)
    fun setRate(rate: Double)
    fun setVolume(level: Double, muted: Boolean)
    fun stop()

    /** Loads the same address again from where it stood: what hls.js's `startLoad` does for a relay that stopped. */
    fun reload() {}

    /** YouTube's frame takes presses only while this is on. */
    fun setTappable(on: Boolean) {}
}

/**
 * The video everybody is watching, as this app plays it (WatchScreen.tsx
 * `Player`; WatchPlayer.swift): one per video, as the web keys its player by
 * the video.
 *
 * Each viewer plays the video themselves and keeps to the show's timeline.
 * Where it plays from is decided by trying: the place the video is, and if
 * that fails -- an error, or a player that has still not begun [STALL_MS]
 * after it should have -- the same video relayed by the deployment. YouTube
 * is the one source whose addresses nobody else can use, so it plays through
 * YouTube's own player where YouTube can be reached and through the relay
 * otherwise. On a deployment that relays nothing there is no second place,
 * and a viewer whose connection cannot reach the video is told so.
 *
 * The Apple player's picture in picture is its own window per video. On
 * Android the small window is the whole activity's, which the media layer
 * owns, and the stage it shows is this one; so there is no button for it
 * here.
 */
class WatchPlayer internal constructor(
    val video: WatchVideo,
    val info: ResolvedVideo,
    private val engine: WatchEngine,
    private val machinery: WatchMachinery,
    /** The deployment's address, for YouTube's player, read once while the call is certainly there. */
    private val base: String,
    mainland: Boolean,
) {
    /**
     * Why a video does not play for this viewer: their connection does not
     * reach it, the site refused it ([Refused], [Failed]), or it is a kind of
     * video this app's player cannot open at all ([Format]), which no
     * connection and no relay cures.
     */
    enum class Lost { Network, Refused, Failed, Format }

    /** Where a relay-only video that had been playing stands, once its relay stopped. */
    enum class Halted { Resuming, Stopped }

    /** Playing the deployment's relayed copy rather than the original. */
    var relayed by mutableStateOf(false)
        private set
    /** YouTube's player was asked to play and did not: the platform wants a press inside the player itself. */
    var blocked by mutableStateOf(false)
        private set
    /** Not playing for this viewer, with no relay to fall back on. */
    var unreachable by mutableStateOf<Lost?>(null)
        internal set
    var halted by mutableStateOf<Halted?>(null)
        internal set
    /** Nothing to hold to yet: a spinner. */
    var waiting by mutableStateOf(true)
        internal set
    /** Where the clock under the video reads. */
    var position by mutableDoubleStateOf(0.0)
        internal set
    var backend by mutableStateOf<WatchBackend?>(null)
        private set

    private var volumeState by mutableDoubleStateOf(1.0)
    var volume: Double
        get() = volumeState
        set(value) {
            volumeState = value
            applyVolume()
            remember()
        }
    private var mutedState by mutableStateOf(false)
    var muted: Boolean
        get() = mutedState
        set(value) {
            mutedState = value
            applyVolume()
        }

    /**
     * Played at all: a later stall is buffering, not a source that does not
     * work, and is not given the relay. Measured on the web: before this, any
     * seek more than twelve seconds in moved a working direct source onto the
     * relay for good.
     */
    private var played = false
    /** For a video that starts on the relay: whether the copy from where it is has been tried. */
    private var triedDirect = false
    /** Whether the relay has been tried, for a video that starts where it is. */
    private var triedRelay = false
    /** Some place it was tried from answered with a file the player cannot open, as opposed to not answering. */
    private var unopenable = false
    private var resumed = 0
    /** A resume is being asked about; a second failure meanwhile is the same. */
    private var askingAgain = false
    private val stall = WatchStall(0, STALL_MS)
    /** Said once, where there is no relay: a stall goes on being one. */
    private var gaveUp = false
    private var keeper = machinery.keeper()
    private var playingShow = false
    private var stopped = false

    init {
        volumeState = Rehearsal.choices?.string(VOLUME_KEY)?.toDoubleOrNull()?.let { if (it.isFinite() && it in 0.0..1.0) it else 1.0 } ?: 1.0
        // Straight to the relay, for a YouTube video and a viewer who came in
        // through a relay in mainland China, where the server relays it at all
        // (WatchScreen.tsx ~240): YouTube's script does not arrive from there,
        // and every such viewer sat at a black tile waiting for it to fail
        // before being given the relay they were always going to get. Which
        // relay a call came in through is the one honest thing an app knows
        // about where its viewer is. A ticket the server will not relay has no
        // proxy, and its viewer goes on trying YouTube.
        val straightToRelay = mainland && info.play.kind == ResolvedPlayKind.YOUTUBE && info.proxy != null
        relayed = info.relay || straightToRelay
        build()
    }

    // MARK: Where it plays from

    private val relayOnly: Boolean get() = info.play.kind == ResolvedPlayKind.RELAY || info.play.kind == ResolvedPlayKind.BANGUMI

    val playsAsYoutube: Boolean get() = info.play.kind == ResolvedPlayKind.YOUTUBE && !relayed

    private data class Source(val url: String, val hls: Boolean)

    /** The relay, unless this plays from where the video is; a relay-only source has nowhere else to play from. */
    private val source: Source?
        get() {
            if (relayed || info.play.kind == ResolvedPlayKind.YOUTUBE || relayOnly) {
                val proxy = info.proxy ?: return null
                return Source(proxy.url, proxy.kind == ResolvedProxyKind.HLS)
            }
            val url = info.play.url ?: return null
            return Source(url, info.play.kind == ResolvedPlayKind.HLS)
        }

    private fun build() {
        backend?.stop()
        backend = null
        gaveUp = false
        if (relayed) triedRelay = true
        stall.restart(watchClock())
        keeper = machinery.keeper()
        attach()
    }

    /**
     * Gives this player the backend its source wants. Again on a beat while
     * there is none, for a player made before the device lent the means to
     * play ([CoreWatchMachinery]).
     */
    private fun attach() {
        val id = info.play.id
        val source = source
        backend = if (playsAsYoutube && id != null) {
            machinery.youtube(id, base, this)
        } else if (source != null) {
            // Held to the size it is shown at where the relay is not the one
            // chosen for its quality: a film at four times the pixels the tile
            // has is the deployment's bandwidth spent on nothing anybody sees.
            // Not where the deployment chose the relay for its better
            // qualities (hls.js `capLevelToPlayerSize: !info.relay`).
            machinery.file(source.url, source.hls, capped = !info.relay, owner = this)
        } else {
            null
        }
        if (backend != null) {
            // The wait for it to begin counts from when it could begin, not
            // from a player that had nothing to play with for a while.
            stall.restart(watchClock())
            applyVolume()
            Log.i(TAG, "watch: ${if (playsAsYoutube) "youtube" else if (relayed) "relay" else "direct"} source for ${video.key}")
        }
    }

    /**
     * Where the video is has failed this viewer: the relay, or where there is
     * none, saying so. The site's answer stands: a stall after it is that
     * answer, not the connection.
     */
    private fun giveUp(why: Lost = Lost.Network) {
        if (info.proxy != null) {
            if (!relayed) {
                relayed = true
                build()
            }
        } else {
            val was = unreachable
            unreachable = if (was != null && was != Lost.Network) was else why
        }
    }

    /**
     * A file or playlist that failed: given the other place once, and only
     * while it has never played; said once neither plays.
     *
     * The web stops at the second failure and goes on showing its spinner,
     * which in a browser is the rare case. On Apple, whose player opens no
     * WebM, a video like that spun for ever with nothing said; Media3 opens
     * more than AVPlayer does, but not every codec on every phone. So the
     * second failure is the end: "this video does not reach you", with the
     * sentence for why and the link to open it elsewhere.
     */
    private fun failed(why: String, format: Boolean = false) {
        if (played) return
        if (format) unopenable = true
        val lost = if (unopenable) Lost.Format else Lost.Network
        if (!relayed) {
            if (gaveUp) return
            Log.i(TAG, "watch: direct failed $why")
            if (info.proxy != null && !triedRelay) {
                relayed = true
                build()
            } else {
                gaveUp = true
                unreachable = lost
            }
        } else if (info.relay && !relayOnly && !triedDirect) {
            // The relay was the first choice and failed: the lower quality
            // from where the video is, rather than nothing.
            Log.i(TAG, "watch: relay failed $why, trying direct")
            triedDirect = true
            relayed = false
            build()
        } else {
            Log.i(TAG, "watch: relay failed $why, nowhere left to play it from")
            gaveUp = true
            unreachable = lost
        }
    }

    // MARK: What the backends report

    /**
     * It can play: the holder's timeline starts now, for everybody. Said
     * here, with the backend it belongs to: a holder whose YouTube fell back to
     * the relay has a file player only from then on, and an ending it never
     * heard left the whole room at the last frame on the web.
     */
    fun backendPlayable(from: WatchBackend) {
        if (from === backend && !stopped) engine.playerReady(video.key)
    }

    fun backendBegan(from: WatchBackend) {
        if (from !== backend) return
        played = true
        resumed = 0
        if (blocked) {
            blocked = false
            letYoutubeTakePresses()
        }
        if (unreachable != null) unreachable = null
        if (halted != null) halted = null
    }

    fun backendEnded(from: WatchBackend) {
        if (from === backend && !stopped) engine.playerEnded(video.key)
    }

    /**
     * The player could not play it. Before it has played, a source that does
     * not work; after, for a video that plays only through the relay, the
     * relay stopped, and it is asked about and started again.
     */
    fun backendFailed(from: WatchBackend, why: String, format: Boolean) {
        if (from !== backend) return
        if (played && relayOnly && !from.youtube) {
            engine.scope.launch { resume(from) }
        } else {
            failed(why, format)
        }
    }

    /** YouTube's script never arrived, or its player never became ready. */
    fun youtubeUnreachable(from: WatchBackend) {
        if (from !== backend) return
        Log.i(TAG, "watch: youtube unreachable")
        giveUp()
    }

    /** YouTube's player refused the video: the relay, or where there is none, which of the two it was. */
    fun youtubeRefused(from: WatchBackend, code: Int) {
        if (from !== backend) return
        Log.i(TAG, "watch: youtube refused $code")
        giveUp(if (code in EMBEDDING_REFUSED) Lost.Refused else Lost.Failed)
    }

    /**
     * A relay-only video that stopped after it had played: the server is
     * asked about the ticket again first, since the switch turned off or a
     * grant withdrawn is no fault a retry cures; told so, the stage says the
     * video cannot be played here and this player goes. Otherwise it is
     * started again from where it stood, [RESUMES] times, and then the viewer
     * is told it stopped and given a press.
     */
    private suspend fun resume(from: WatchBackend) {
        if (askingAgain) return
        askingAgain = true
        halted = Halted.Resuming
        val now = engine.askAgain(video.ticket)
        askingAgain = false
        if (from !== backend || now == WatchInfo.Unavailable) return
        if (resumed >= RESUMES) {
            halted = Halted.Stopped
            return
        }
        resumed += 1
        from.reload()
    }

    /** The press once it has stopped. */
    fun tryAgain() {
        resumed = 0
        halted = Halted.Resuming
        backend?.reload()
    }

    // MARK: Held to the show

    /** One keep step (WatchScreen.tsx `keep`, the rules in the protocol crate's `WatchKeeper`). */
    fun keep(now: Long, show: WatchShow, heardAtMs: Long) {
        if (stopped) return
        playingShow = show.playing
        if (backend == null) attach()
        checkStall(now)
        val want = machinery.expected(show, heardAtMs, now)
        val backend = backend
        if (backend == null) {
            if (!waiting) waiting = true
            if (!video.live) setPosition(want)
            return
        }
        // A refused player is not asked again.
        val refusedForGood = unreachable == Lost.Refused || unreachable == Lost.Failed || unreachable == Lost.Format
        val reading = WatchPlayerReading(
            ready = backend.ready && !refusedForGood, time = backend.time, playing = backend.playing,
            rate = backend.rate, nudges = backend.nudges, youtube = backend.youtube,
        )
        val step = keeper.keep(reading, show.playing, want, video.live, now.toULong())
        if (waiting != step.waiting) waiting = step.waiting
        if (step.play) backend.play()
        if (step.pause) backend.pause()
        step.seek?.let { backend.seek(it) }
        step.rate?.let { backend.setRate(it) }
        if (step.refused && !blocked) {
            blocked = true
            letYoutubeTakePresses()
        }
        step.position?.let { setPosition(it) }
    }

    private fun setPosition(at: Double) {
        if (abs(at - position) >= 0.01) position = at
    }

    /** Asked once a second, as the web's interval asks it. */
    private fun checkStall(now: Long) {
        val backend = backend ?: return
        if (!stall.due(now)) return
        val late = stall.late(now)
        if (backend.youtube) {
            // The API arrived but the player never became ready: the relay. A
            // player that is ready and refused a press is the platform's
            // doing, and is waited on.
            if (!played && playingShow && late && !backend.ready) youtubeUnreachable(backend)
        } else {
            if (played || (relayed && !info.relay) || (relayed && triedDirect)) return
            if (playingShow && !backend.flowing && late) failed("stalled")
        }
    }

    // MARK: Pressed here

    /** At once here, so the press is felt; the show's answer brings everybody else. */
    fun playNow() {
        backend?.play()
    }

    fun pauseNow() {
        backend?.pause()
    }

    /** The viewer moved the player: counted as a jump, so the keeper does not jump it back before the show has heard the seek. */
    fun seekNow(to: Double) {
        keeper.seeked(watchClock().toULong())
        backend?.seek(to)
        setPosition(to)
    }

    /** A refused YouTube player is started by a press inside YouTube's own frame, so the frame takes presses while it is refused. */
    val tapYoutube: Boolean get() = blocked && playsAsYoutube

    private fun applyVolume() {
        backend?.setVolume(volumeState, mutedState)
        letYoutubeTakePresses()
    }

    /**
     * YouTube's frame takes no presses -- the controls drive the show, and
     * YouTube's own would drive only this player -- except while it is
     * refused, when a press inside it is the only thing that starts it.
     */
    private fun letYoutubeTakePresses() {
        backend?.setTappable(tapYoutube)
    }

    private fun remember() {
        // Kept only when it is not the default, as the web keeps it, and never
        // in a rehearsal, whose volume is no business of the owner's next film
        // (`Rehearsal.choices`).
        Rehearsal.choices?.set(VOLUME_KEY, if (volumeState == 1.0) null else volumeState.toString())
    }

    fun stop() {
        stopped = true
        backend?.stop()
        backend = null
    }

    companion object {
        /** How often the player is held to the show (`KEEP_MS`). */
        const val KEEP_MS = 250L
        /**
         * A player that has not begun this long after it should have is given
         * the relay: the same as the show's own wait for its holder. A longer
         * one was measured on the web as twelve seconds of a spinner for a
         * viewer whose connection to the video's own servers was slow
         * (`STALL_MS`).
         */
        const val STALL_MS = 8000L
        /** How many times a relay-only video that stopped partway is started again by itself before the viewer is asked to (`RESUMES`). */
        const val RESUMES = 2
        /** YouTube's errors for a video whose owner allows it on no other site. */
        val EMBEDDING_REFUSED = setOf(101, 150)
        /** Where this viewer's volume is kept: the web's own key, so the habit carries over (WatchScreen.tsx `VOLUME_KEY`). */
        const val VOLUME_KEY = "meet-live.watch-volume"
    }
}
