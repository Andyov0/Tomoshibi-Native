package app.tomoshibi.feature.watch

import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.media.PictureInPicture
import app.tomoshibi.media.Sharing
import app.tomoshibi.model.CallModel
import kotlin.coroutines.CoroutineContext
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.LosslessSink
import uniffi.tomoshibi_core.NoHandle
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.MediaItem
import uniffi.tomoshibi_protocol.MediaServer
import uniffi.tomoshibi_protocol.MusicLibrary
import uniffi.tomoshibi_protocol.MusicLink
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.MusicTrack
import uniffi.tomoshibi_protocol.ResolvedLink
import uniffi.tomoshibi_protocol.ResolvedPlay
import uniffi.tomoshibi_protocol.ResolvedPlayKind
import uniffi.tomoshibi_protocol.ResolvedProxy
import uniffi.tomoshibi_protocol.ResolvedProxyKind
import uniffi.tomoshibi_protocol.ResolvedVideo
import uniffi.tomoshibi_protocol.TicketVideo
import uniffi.tomoshibi_protocol.WatchAnswer
import uniffi.tomoshibi_protocol.WatchAsk
import uniffi.tomoshibi_protocol.WatchKeepStep
import uniffi.tomoshibi_protocol.WatchKeeper
import uniffi.tomoshibi_protocol.WatchOutput
import uniffi.tomoshibi_protocol.WatchPacket
import uniffi.tomoshibi_protocol.WatchPlayerReading
import uniffi.tomoshibi_protocol.WatchShow
import uniffi.tomoshibi_protocol.WatchSnapshot
import uniffi.tomoshibi_protocol.WatchTheatre
import uniffi.tomoshibi_protocol.WatchVideo

/*
 * Stand-ins for the core, the device and the deployment, for the tests of
 * watching together and the song desk and for the pictures of their panels:
 * UniFFI's fake objects with what each test needs answered.
 */

/** The call, as far as the features layered over it reach into it. */
internal class TestCall : Call(NoHandle) {
    val sent = mutableListOf<Pair<String, String>>()
    val hearing = mutableListOf<String>()

    override fun roomName() = "film-night"
    override fun localIdentity() = "a-me"
    override fun localName() = "Me"
    override fun attachPcm(sid: String, sink: PcmSink) {}
    override fun detach(sid: String) {}
    override fun setLosslessSink(sink: LosslessSink) {}
    override suspend fun leave() {}
    override suspend fun sendData(topic: String, payload: ByteArray, destinations: List<String>, reliable: Boolean) {
        sent += topic to String(payload)
    }
    override suspend fun setHearing(identity: String, music: Boolean, level: Float, blocked: Boolean) {
        hearing += "$identity music=$music level=$level blocked=$blocked"
    }
}

/** The deployment, answering as each test says. */
internal open class TestDeployment : Deployment(uniffi.tomoshibi_protocol.NoHandle) {
    var ready = true
    var resolve: suspend (String, Boolean) -> ResolvedLink = { _, _ -> throw IllegalStateException("not asked for") }
    var tickets: suspend (String) -> TicketVideo = { TicketVideo.Failed }
    var servers = emptyList<MediaServer>()
    var media = emptyList<MediaItem>()
    var libraries = emptyList<MusicLibrary>()
    var search: suspend (String, String) -> List<MusicTrack> = { _, _ -> emptyList() }
    var link: suspend (String) -> MusicLink? = { null }
    val resolved = mutableListOf<String>()

    override fun baseUrl() = "https://meet.example"
    override fun cookieHeader(): String? = null
    override suspend fun watchReady() = ready
    override suspend fun resolveVideo(pasted: String, forOther: Boolean): ResolvedLink {
        resolved += "$pasted forOther=$forOther"
        return resolve(pasted, forOther)
    }
    override suspend fun videoInfo(ticket: String): TicketVideo = tickets(ticket)
    override suspend fun mediaServers() = servers
    override suspend fun browseMedia(server: String, parent: String?) = media
    override suspend fun searchMedia(server: String, words: String) = media
    override suspend fun musicSources() = libraries
    override suspend fun musicSearch(source: String, query: String, page: UInt) = search(source, query)
    override suspend fun musicLink(pasted: String) = link(pasted)
    override fun musicAudioUrl(source: String, id: String, quality: MusicQuality) = "https://meet.example/api/music/audio"
}

internal fun callModel(
    call: TestCall = TestCall(),
    deployment: Deployment = TestDeployment(),
    context: CoroutineContext,
    sealed: Boolean = false,
    audio: CallAudio = CallAudio(NoAudioRouting, muted = true),
): CallModel = CallModel(
    call, deployment, region = null, sealed = sealed, audio = audio, makeCamera = { error("no camera in a test") },
    sharing = Sharing(), pictureInPicture = PictureInPicture(), context = context,
)

internal fun output(
    packets: List<WatchPacket> = emptyList(),
    asks: List<WatchAsk> = emptyList(),
    refusals: List<String> = emptyList(),
    changed: Boolean = false,
    accepted: Boolean = true,
) = WatchOutput(packets, asks, refusals, changed, accepted)

internal fun packet(text: String) = WatchPacket("watch", text.toByteArray(), emptyList(), true)

/** The theatre, answering as each test says and writing down what it was asked. */
internal open class TestTheatre : WatchTheatre(uniffi.tomoshibi_protocol.NoHandle) {
    var snapshot = WatchSnapshot(null, 0u, false, false, 0.0)
    val asked = mutableListOf<String>()
    var onAddLink: (String) -> WatchOutput = { output() }
    var onStart: () -> WatchOutput = { output() }
    var onAddTicket: (String, String?) -> WatchOutput = { _, _ -> output() }
    var onAnswer: (ULong, WatchAnswer) -> WatchOutput = { _, _ -> output() }
    var onPause: () -> WatchOutput = { output() }
    var onClose: () -> WatchOutput = { output() }

    override fun state(nowMs: ULong) = snapshot
    override fun roster(identities: List<String>, nowMs: ULong) = output()
    override fun joined(nowMs: ULong) = output()
    override fun tick(nowMs: ULong) = output()
    override fun participantConnected(identity: String, nowMs: ULong) = output()
    override fun participantDisconnected(identity: String, nowMs: ULong) = output()
    override fun onMessage(sender: String, senderName: String, payload: ByteArray, nowMs: ULong) = output()
    override fun addLink(text: String, nowMs: ULong): WatchOutput {
        asked += "addLink $text"
        return onAddLink(text)
    }
    override fun start(nowMs: ULong): WatchOutput {
        asked += "start"
        return onStart()
    }
    override fun addTicket(ticket: String, link: String?, nowMs: ULong): WatchOutput {
        asked += "addTicket $ticket $link"
        return onAddTicket(ticket, link)
    }
    override fun answer(request: ULong, answer: WatchAnswer, nowMs: ULong): WatchOutput {
        asked += "answer $request $answer"
        return onAnswer(request, answer)
    }
    override fun pause(nowMs: ULong): WatchOutput {
        asked += "pause"
        return onPause()
    }
    override fun play(nowMs: ULong): WatchOutput {
        asked += "play"
        return output()
    }
    override fun seek(to: Double, nowMs: ULong): WatchOutput {
        asked += "seek $to"
        return output()
    }
    override fun skip(key: String?, nowMs: ULong): WatchOutput {
        asked += "skip $key"
        return output()
    }
    override fun remove(key: String, nowMs: ULong): WatchOutput {
        asked += "remove $key"
        return output()
    }
    override fun close(nowMs: ULong): WatchOutput {
        asked += "close"
        return onClose()
    }
    override fun playerReady(key: String, nowMs: ULong): WatchOutput {
        asked += "ready $key"
        return output()
    }
    override fun playerEnded(key: String, nowMs: ULong): WatchOutput {
        asked += "ended $key"
        return output()
    }
}

internal class TestKeeper : WatchKeeper(uniffi.tomoshibi_protocol.NoHandle) {
    var step = WatchKeepStep(waiting = false, position = null, play = false, pause = false, seek = null, rate = null, refused = false)
    override fun keep(player: WatchPlayerReading, showPlaying: Boolean, expected: Double, live: Boolean, nowMs: ULong) =
        step.copy(position = step.position ?: expected)
    override fun seeked(nowMs: ULong) {}
}

/** A way of playing that only writes down what it was told. */
internal class TestBackend(val url: String, override val youtube: Boolean = false) : WatchBackend {
    override var ready = false
    override var time = 0.0
    override var playing = false
    override val rate = 1.0
    override val nudges: Boolean get() = !youtube
    override var flowing = false
    var reloads = 0
    var stopped = false
    var takesPresses = false
    override fun play() {
        playing = true
    }
    override fun pause() {
        playing = false
    }
    override fun seek(to: Double) {
        time = to
    }
    override fun setRate(rate: Double) {}
    override fun setVolume(level: Double, muted: Boolean) {}
    override fun stop() {
        stopped = true
    }
    override fun reload() {
        reloads += 1
    }
    override fun setTappable(on: Boolean) {
        takesPresses = on
    }
}

internal class TestMachinery(val theatre: TestTheatre = TestTheatre(), var mainland: Boolean = false) : WatchMachinery {
    val made = mutableListOf<TestBackend>()
    override fun theatre(identity: String, name: String, sealed: Boolean) = theatre
    override fun keeper() = TestKeeper()
    override fun expected(show: WatchShow, heardAtMs: Long, nowMs: Long) = show.position
    override fun mainland(region: String?) = mainland
    override fun file(url: String, hls: Boolean, capped: Boolean, owner: WatchPlayer) = TestBackend(url).also { made += it }
    override fun youtube(id: String, base: String, owner: WatchPlayer) = TestBackend("youtube:$id", youtube = true).also { made += it }
}

internal fun resolved(
    ticket: String,
    title: String = "The General",
    kind: ResolvedPlayKind = ResolvedPlayKind.FILE,
    url: String? = "https://cdn.example/$ticket.mp4",
    id: String? = null,
    proxy: ResolvedProxy? = ResolvedProxy(ResolvedProxyKind.HLS, "https://meet.example/api/watch/media?t=$ticket"),
    relay: Boolean = false,
    duration: Double = 4_620.0,
    link: String = "https://archive.example/$ticket",
) = ResolvedVideo(ticket, title, duration, "", link, false, relay, ResolvedPlay(kind, url, id), proxy, null)

internal fun video(key: String, ticket: String = "t-$key", by: String = "Ada", duration: Double = 4_620.0) =
    WatchVideo(key, by, ticket, duration, false)

internal fun show(now: WatchVideo?, queue: List<WatchVideo> = emptyList(), playing: Boolean = true, position: Double = 0.0) =
    WatchShow("g-ada", "Ada", now, queue, playing, position, 1u)
