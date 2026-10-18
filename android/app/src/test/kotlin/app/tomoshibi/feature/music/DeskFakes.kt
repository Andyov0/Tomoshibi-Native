package app.tomoshibi.feature.music

import app.tomoshibi.model.CallModel
import kotlinx.coroutines.CompletableDeferred
import uniffi.tomoshibi_protocol.DeskAction
import uniffi.tomoshibi_protocol.DeskEntry
import uniffi.tomoshibi_protocol.DeskLibrary
import uniffi.tomoshibi_protocol.DeskOutput
import uniffi.tomoshibi_protocol.DeskPacket
import uniffi.tomoshibi_protocol.DeskPrepared
import uniffi.tomoshibi_protocol.DeskSnapshot
import uniffi.tomoshibi_protocol.DeskSong
import uniffi.tomoshibi_protocol.DeskState
import uniffi.tomoshibi_protocol.JukeboxDesk
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.NoHandle

internal fun deskOutput(
    packets: List<DeskPacket> = emptyList(),
    actions: List<DeskAction> = emptyList(),
    skipped: List<DeskEntry> = emptyList(),
    changed: Boolean = false,
    accepted: Boolean = true,
) = DeskOutput(packets, actions, skipped, changed, accepted)

internal fun entry(key: String, title: String = "Song $key", by: String = "Ada", artists: List<String> = listOf("The Band")) =
    DeskEntry("netease", "id-$key", title, artists, "Album", "", 214.0, key, by)

internal fun deskState(
    now: DeskEntry? = null,
    queue: List<DeskEntry> = emptyList(),
    holder: String = "g-ada",
    votes: List<String> = emptyList(),
    needed: UInt = 2u,
    loading: Boolean = false,
    next: DeskEntry? = null,
    paused: Boolean = false,
    quality: String? = null,
) = DeskState(holder, "Ada", now, queue, votes, needed, loading, next, paused, quality)

/** The protocol's desk, answering as each test says and writing down what it was told. */
internal class TestDesk : JukeboxDesk(NoHandle) {
    var snapshot = DeskSnapshot(null, mine = false, broadcast = true)
    val told = mutableListOf<String>()
    /** What the next of the person's presses (vote, skip, ...) says. */
    var next: DeskOutput = deskOutput()
    var onAsk: (String) -> DeskOutput = { deskOutput() }
    var onAdd: (List<DeskSong>) -> DeskOutput = { deskOutput() }
    var onFound: (ULong, List<DeskSong>) -> DeskOutput = { _, _ -> deskOutput() }

    private fun press(what: String): DeskOutput {
        told += what
        return next.also { next = deskOutput() }
    }

    override fun state(nowMs: ULong) = snapshot
    override fun roster(identities: List<String>, nowMs: ULong) = deskOutput()
    override fun joined(nowMs: ULong) = deskOutput()
    override fun tick(nowMs: ULong) = deskOutput()
    override fun participantConnected(identity: String, nowMs: ULong) = deskOutput()
    override fun participantDisconnected(identity: String, nowMs: ULong) = deskOutput()
    override fun onMessage(sender: String, senderName: String, payload: ByteArray, nowMs: ULong) = deskOutput()
    override fun start(nowMs: ULong): DeskOutput {
        told += "start"
        return deskOutput()
    }
    override fun ask(text: String, nowMs: ULong): DeskOutput {
        told += "ask $text"
        return onAsk(text)
    }
    override fun addSongs(songs: List<DeskSong>, nowMs: ULong): DeskOutput {
        told += "add ${songs.map { it.title }}"
        return onAdd(songs)
    }
    override fun found(request: ULong, songs: List<DeskSong>, nowMs: ULong): DeskOutput {
        told += "found $request ${songs.map { it.title }}"
        return onFound(request, songs)
    }
    override fun prepared(request: ULong, outcome: DeskPrepared, nowMs: ULong): DeskOutput {
        val said = when (outcome) {
            is DeskPrepared.Ready -> outcome.toString()
            DeskPrepared.Unavailable -> "Unavailable"
            DeskPrepared.Failed -> "Failed"
        }
        told += "prepared $request $said"
        return deskOutput()
    }
    override fun played(request: ULong, ok: Boolean, nowMs: ULong): DeskOutput {
        told += "played $request $ok"
        return deskOutput()
    }
    override fun trackEnded(key: String, nowMs: ULong): DeskOutput {
        told += "ended $key"
        return deskOutput()
    }
    override fun vote(nowMs: ULong) = press("vote")
    override fun skip(nowMs: ULong) = press("skip")
    override fun pause(paused: Boolean, nowMs: ULong) = press("pause $paused")
    override fun remove(key: String, nowMs: ULong) = press("remove $key")
    override fun toTop(key: String, nowMs: ULong) = press("top $key")
    override fun close(nowMs: ULong) = press("close")
}

/** A published track that writes down whether it was taken down. */
internal class TestInput : MusicInput {
    var blocks = 0
    override fun pushPcm(pcm: ByteArray) {
        blocks += 1
    }
    override fun pushLossless(samples: FloatArray, rate: Int, timestampUs: Long) {}
}

internal class TestDeskMachinery(val desk: TestDesk = TestDesk()) : DeskMachinery {
    /** What each load waits for, by the entry's key; a load not given one fails. */
    val loads = HashMap<String, CompletableDeferred<MusicLoader.Outcome>>()
    val loaded = mutableListOf<String>()
    var publishing: CompletableDeferred<Unit>? = null
    var published = 0
    var unpublished = 0

    override fun desk(identity: String, name: String, sealed: Boolean) = desk
    override fun deskLibrary(libraries: List<DeskLibrary>) = (libraries.firstOrNull { it.signedIn } ?: libraries.firstOrNull())?.id
    override fun losslessOffered(wanted: Boolean, encrypted: Boolean) = wanted && !encrypted
    override suspend fun load(entry: DeskEntry, quality: MusicQuality, keepOriginal: Boolean, call: CallModel): MusicLoader.Outcome {
        loaded += entry.key
        val waiting = loads[entry.key] ?: throw IllegalStateException("no such song")
        return waiting.await()
    }
    override suspend fun publish(call: CallModel, losslessOffered: Boolean): PublishedMusic {
        publishing?.await()
        published += 1
        return PublishedMusic(TestInput()) { unpublished += 1 }
    }
}

/** A decoded song of a second of silence, saying what it is. */
internal fun silentSong(quality: String? = "FLAC · 16-bit · 44.1 kHz") =
    DecodedSong(Pcm48(listOf(ByteArray(Pcm48.PIECE_FRAMES * 4)), Pcm48.PIECE_FRAMES), null, 44_100, quality)
