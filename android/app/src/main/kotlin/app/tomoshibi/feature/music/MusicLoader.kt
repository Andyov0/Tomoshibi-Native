package app.tomoshibi.feature.music

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.util.Log
import java.io.Closeable
import java.io.File
import java.io.FileInputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.Executors
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.describeMusicAudio

private const val TAG = "Tomoshibi"

/**
 * A song at 48 kHz in two channels of 16 bits, as the core's Opus source
 * takes it and the holder's own speakers play it: kept in pieces of one
 * second rather than as one array, so that a five-minute song is never asked
 * for as one block of fifty-odd megabytes, nor copied whole to trim it. A
 * second is a whole number of the music's 10 ms blocks, so no block is ever
 * split between two pieces.
 */
class Pcm48(private val pieces: List<ByteArray>, val frames: Int) {
    /** Copies [count] frames from [position] into the start of [into]. */
    fun copy(position: Int, into: ByteArray, count: Int) {
        var done = 0
        while (done < count) {
            val at = position + done
            val piece = pieces[at / PIECE_FRAMES]
            val within = at % PIECE_FRAMES
            val run = minOf(count - done, PIECE_FRAMES - within)
            System.arraycopy(piece, within * 4, into, done * 4, run * 4)
            done += run
        }
    }

    companion object {
        const val PIECE_FRAMES = 48_000
    }
}

/**
 * A song ready for the call (MusicLoader.swift `DecodedSong`).
 *
 * Two forms of the same sound. [pcm] is what the core's Opus source takes and
 * the holder hears. [original] is the file as it decodes, at its own rate,
 * for the lossless stream beside the Opus track: the web plays the decoded
 * file through a context at the file's own rate and its lossless sender reads
 * it there (sound.ts `fetchLibraryTrack`, `playDecoded`), so a listener hears
 * the file's samples and not a resampling of them. Only where a lossless
 * stream is offered.
 */
class DecodedSong(
    val pcm: Pcm48,
    val original: OriginalSamples?,
    /** The file's own samples a second, which the lossless stream runs at. */
    val rate: Int,
    /** What the file is, as the panel says it: "FLAC · 24-bit · 44.1 kHz". */
    val quality: String?,
) : Closeable {
    val frames: Int get() = pcm.frames

    /** Lets the file the lossless stream reads from go. */
    override fun close() {
        original?.close()
    }
}

/** The file's own samples for the lossless stream, taken 10 ms at a time by the music's clock ([OriginalStream]). */
interface OriginalSamples : Closeable {
    /** The rate the samples are at, which the lossless stream runs at. */
    val rate: Int

    /** Up to [count] frames into [into], two channels interleaved, and how many there were. */
    fun take(count: Int, into: FloatArray): Int
}

/**
 * A sound file read out as two channels of floats at its own rate, a piece
 * at a time: the platform's extractor and decoder, which take FLAC, ALAC,
 * AAC, MP3, Ogg, Opus and WAV by what the bytes are rather than by a file's
 * name, as the web's `decodeAudioData` does.
 *
 * Asked for floats, which the platform's FLAC decoder gives scaled by a power
 * of two on both signs, so the core's one-scale `to_int24` recovers every
 * integer of a 16- or 24-bit file for the lossless stream; a decoder that
 * gives 16-bit integers anyway is read by the same power of two.
 */
class PcmReader(input: FileInputStream) : Closeable {
    private val extractor = MediaExtractor()
    private val codec: MediaCodec
    private val info = MediaCodec.BufferInfo()
    private var inputDone = false
    private var outputDone = false
    private var encoding = AudioFormat.ENCODING_PCM_16BIT

    /** The rate and channels the decoder said it gives; the file's own until it says. */
    var rate: Int
        private set
    var channels: Int
        private set

    /** How long the file says it is, in seconds, or nought. */
    val seconds: Double

    init {
        extractor.setDataSource(input.fd)
        val track = (0 until extractor.trackCount).firstOrNull {
            extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true
        }
        if (track == null) {
            extractor.release()
            throw IOException("no sound in the file")
        }
        extractor.selectTrack(track)
        val format = extractor.getTrackFormat(track)
        rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        seconds = if (format.containsKey(MediaFormat.KEY_DURATION)) format.getLong(MediaFormat.KEY_DURATION) / 1e6 else 0.0
        format.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_FLOAT)
        codec = try {
            MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!).also {
                it.configure(format, null, null, 0)
                it.start()
            }
        } catch (fault: Exception) {
            extractor.release()
            throw IOException("no decoder for ${format.getString(MediaFormat.KEY_MIME)}", fault)
        }
    }

    /** The next piece, two channels of floats interleaved, or null at the end. */
    fun read(): FloatArray? {
        while (!outputDone) {
            if (!inputDone) {
                val index = codec.dequeueInputBuffer(WAIT_US)
                if (index >= 0) {
                    val buffer = codec.getInputBuffer(index)!!
                    val size = extractor.readSampleData(buffer, 0)
                    if (size < 0) {
                        codec.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        inputDone = true
                    } else {
                        codec.queueInputBuffer(index, 0, size, extractor.sampleTime, 0)
                        extractor.advance()
                    }
                }
            }
            val index = codec.dequeueOutputBuffer(info, WAIT_US)
            if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                val format = codec.outputFormat
                rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                encoding = if (format.containsKey(MediaFormat.KEY_PCM_ENCODING)) format.getInteger(MediaFormat.KEY_PCM_ENCODING) else AudioFormat.ENCODING_PCM_16BIT
            } else if (index >= 0) {
                val buffer = codec.getOutputBuffer(index)!!
                buffer.position(info.offset)
                buffer.limit(info.offset + info.size)
                val piece = stereo(buffer.slice().order(ByteOrder.nativeOrder()))
                codec.releaseOutputBuffer(index, false)
                if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outputDone = true
                if (piece.isNotEmpty()) return piece
            }
        }
        return null
    }

    /** The decoder's samples as two channels of floats: one copied to both, more folded down. */
    private fun stereo(bytes: ByteBuffer): FloatArray {
        val width = when (encoding) {
            AudioFormat.ENCODING_PCM_FLOAT, AudioFormat.ENCODING_PCM_32BIT -> 4
            AudioFormat.ENCODING_PCM_24BIT_PACKED -> 3
            AudioFormat.ENCODING_PCM_8BIT -> 1
            else -> 2
        }
        val count = channels.coerceAtLeast(1)
        val frames = bytes.remaining() / width / count
        val out = FloatArray(frames * 2)
        val sample = FloatArray(count)
        for (frame in 0 until frames) {
            for (c in 0 until count) {
                sample[c] = when (encoding) {
                    AudioFormat.ENCODING_PCM_FLOAT -> bytes.getFloat()
                    AudioFormat.ENCODING_PCM_32BIT -> bytes.getInt() / 2_147_483_648f
                    AudioFormat.ENCODING_PCM_24BIT_PACKED -> {
                        val low = bytes.get().toInt() and 0xff
                        val mid = bytes.get().toInt() and 0xff
                        val high = bytes.get().toInt()
                        ((high shl 16) or (mid shl 8) or low) / 8_388_608f
                    }
                    AudioFormat.ENCODING_PCM_8BIT -> ((bytes.get().toInt() and 0xff) - 128) / 128f
                    else -> bytes.getShort() / 32_768f
                }
            }
            val (left, right) = fold(sample, count)
            out[frame * 2] = left
            out[frame * 2 + 1] = right
        }
        return out
    }

    override fun close() {
        try {
            codec.stop()
        } catch (_: Exception) {
        }
        codec.release()
        extractor.release()
    }

    companion object {
        private const val WAIT_US = 10_000L

        /**
         * Two channels from however many there are. Left alone, a mono file
         * was a left channel and silence on Apple; it is both sides. More than
         * two are folded as the usual downmix folds them (front, centre and
         * surround, in the order Android names them), scaled so that the fold
         * cannot be louder than full scale.
         */
        fun fold(sample: FloatArray, count: Int): Pair<Float, Float> = when {
            count == 1 -> sample[0] to sample[0]
            count == 2 -> sample[0] to sample[1]
            else -> {
                val centre = if (count > 2) sample[2] * 0.7071f else 0f
                val backLeft = if (count > 4) sample[4] * 0.7071f else 0f
                val backRight = if (count > 5) sample[5] * 0.7071f else 0f
                val scale = 1f / (1f + 0.7071f + if (count > 4) 0.7071f else 0f)
                (sample[0] + centre + backLeft) * scale to (sample[1] + centre + backRight) * scale
            }
        }
    }
}

/**
 * The file's own samples for the lossless stream, two channels of floats
 * interleaved, read from the file a few seconds ahead of where the stream is
 * rather than held whole (MusicLoader.swift `OriginalStream`).
 *
 * Held whole on Apple at first, the length of the song in floats at the
 * file's own rate was a hundred megabytes for five minutes of CD audio and
 * over two hundred at 96 kHz, for the song playing and again for the next one
 * prepared beside it: on a phone, the memory the system ends an app for. So
 * the fetched file is read from for as long as the song is wanted, the
 * reading happens on a thread of its own, and the music's clock takes each
 * 10 ms of it from what was read, never waiting on the disk or the decoder.
 *
 * The file is open from the moment this is made, so its name can be removed
 * at once and what is read is read through the open file: kept by name until
 * the song was let go, every song of a call that ended without that stayed
 * behind in the cache.
 */
class OriginalStream(file: File) : OriginalSamples {
    private val input = FileInputStream(file)
    private val reader: PcmReader = try {
        PcmReader(input)
    } catch (fault: Exception) {
        input.close()
        throw fault
    }
    private val lock = Any()
    /** What has been read, as a ring: frames `taken until read` of the file. */
    private val capacity: Int
    private val ring: FloatArray
    private var taken = 0L
    private var read = 0L
    private var reading = false
    private var finished = false
    private var closed = false
    /** A piece the decoder gave that did not yet fit in the ring. */
    private var pending: FloatArray? = null
    private var pendingAt = 0

    override val rate: Int

    init {
        // The first piece read at once: the decoder says its rate only once
        // it has decoded something, and the stream begins with the song.
        capacity = maxOf(PIECE * 2, (maxOf(reader.rate, 8000) * AHEAD).toInt())
        ring = FloatArray(capacity * 2)
        fill()
        rate = reader.rate
    }

    /**
     * Up to `count` frames from where the stream is, into [into], and how many
     * there were: fewer only at the end of the file, or if the reading fell
     * behind, which a decoder many times faster than playing does not. Asks
     * for more to be read once less than half the ring is left.
     */
    override fun take(count: Int, into: FloatArray): Int = synchronized(lock) {
        val have = minOf(count.toLong(), read - taken, (into.size / 2).toLong()).toInt().coerceAtLeast(0)
        for (frame in 0 until have) {
            val at = (((taken + frame) % capacity) * 2).toInt()
            into[frame * 2] = ring[at]
            into[frame * 2 + 1] = ring[at + 1]
        }
        taken += have
        if (!reading && !finished && !closed && read - taken < capacity / 2) {
            reading = true
            READER.execute { fill() }
        }
        have
    }

    /** Reads until the ring is full or the file ends. On the reading thread, or from the initialiser before anything else can touch the file. */
    private fun fill() {
        try {
            while (true) {
                val piece = pending ?: run {
                    if (synchronized(lock) { closed }) return
                    val next = try {
                        reader.read()
                    } catch (fault: Exception) {
                        Log.w(TAG, "music: the lossless reading stopped: ${fault.message}")
                        null
                    }
                    if (next == null) {
                        synchronized(lock) { finished = true }
                        return
                    }
                    pendingAt = 0
                    next
                }
                val frames = piece.size / 2
                val fitted = synchronized(lock) {
                    if (closed) return
                    val room = capacity - (read - taken).toInt()
                    val run = minOf(room, frames - pendingAt)
                    for (frame in 0 until run) {
                        val at = (((read + frame) % capacity) * 2).toInt()
                        ring[at] = piece[(pendingAt + frame) * 2]
                        ring[at + 1] = piece[(pendingAt + frame) * 2 + 1]
                    }
                    read += run
                    run
                }
                pendingAt += fitted
                if (pendingAt < frames) {
                    pending = piece
                    return
                }
                pending = null
            }
        } finally {
            synchronized(lock) { reading = false }
        }
    }

    /** Lets the file go, on the reading thread, after whatever reading is under way. */
    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
        }
        READER.execute {
            reader.close()
            input.close()
        }
    }

    private companion object {
        /** How much is read ahead, in seconds, and the least the ring holds, in frames. */
        const val AHEAD = 4.0
        const val PIECE = 16_384

        val READER = Executors.newSingleThreadExecutor { work ->
            Thread(work, "Tomoshibi music original").apply { isDaemon = true }
        }
    }
}

/**
 * Fetching and decoding a library track (sound.ts `fetchLibraryTrack`,
 * jukebox.ts `Desk.load`; MusicLoader.swift).
 */
object MusicLoader {
    sealed class Outcome {
        class Ready(val song: DecodedSong) : Outcome()
        /** The library's account may not play it. */
        data object Unavailable : Outcome()
    }

    /**
     * Describes, fetches and decodes one song. Throws where it could not be
     * fetched or decoded, and when the task is cancelled, which is how a song
     * taken off the queue is abandoned mid-download.
     */
    suspend fun load(source: String, id: String, quality: MusicQuality, keepOriginal: Boolean, deployment: Deployment): Outcome =
        withContext(Dispatchers.IO) {
            val audio = deployment.musicTrack(source, id, quality) ?: return@withContext Outcome.Unavailable
            ensureActive()
            val file = download(deployment.musicAudioUrl(source, id, quality), deployment.cookieHeader())
            try {
                val decoded = decodeFile(file, keepOriginal)
                Outcome.Ready(DecodedSong(decoded.pcm, decoded.original, decoded.rate, describeMusicAudio(audio)))
            } finally {
                file.delete()
            }
        }

    /**
     * The audio, account-gated like everything under /api/music: the session
     * is in the core's jar, not in any of the platform's, so it is handed
     * over as the header the core would send.
     */
    private suspend fun download(address: String, cookie: String?): File {
        val connection = URL(address).openConnection() as HttpURLConnection
        connection.instanceFollowRedirects = true
        connection.connectTimeout = 15_000
        connection.readTimeout = 30_000
        if (cookie != null) connection.setRequestProperty("Cookie", cookie)
        try {
            val code = connection.responseCode
            if (code !in 200..299) throw IOException("the library answered $code")
            val file = File.createTempFile("tomoshibi-music-", ".audio")
            try {
                connection.inputStream.use { body ->
                    file.outputStream().use { out ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            if (!currentCoroutineContext().isActive) throw CancellationException("abandoned")
                            val count = body.read(buffer)
                            if (count < 0) break
                            out.write(buffer, 0, count)
                        }
                    }
                }
            } catch (fault: Throwable) {
                file.delete()
                throw fault
            }
            return file
        } finally {
            connection.disconnect()
        }
    }

    class Decoded(val pcm: Pcm48, val original: OriginalStream?, val rate: Int, val kind: String?)

    /**
     * Decodes a fetched file whole, to 48 kHz in two channels, and opens the
     * lossless stream's own reading of it beside, before its name goes.
     *
     * Read at the file's own rate and converted once, here ([Resampler]). The
     * web decodes at the file's rate and lets the browser bring it to 48 kHz on
     * the way into Opus, which runs at 48 kHz whatever it is given; the core
     * takes 48 kHz and nothing else, so the same step happens before the
     * samples are handed over instead.
     */
    suspend fun decodeFile(file: File, keepOriginal: Boolean): Decoded {
        val kind = FileInputStream(file).use { stream ->
            val head = ByteArray(64)
            sniff(head.copyOf(maxOf(0, stream.read(head))))
        }
        val original = if (keepOriginal) OriginalStream(file) else null
        try {
            FileInputStream(file).use { input ->
                PcmReader(input).use { reader ->
                    val pieces = ArrayList<ByteArray>()
                    var piece = ByteArray(Pcm48.PIECE_FRAMES * 4)
                    var at = 0
                    var frames = 0L
                    fun emit(samples: FloatArray, count: Int) {
                        for (frame in 0 until count) {
                            if (at == Pcm48.PIECE_FRAMES) {
                                pieces.add(piece)
                                piece = ByteArray(Pcm48.PIECE_FRAMES * 4)
                                at = 0
                            }
                            for (c in 0..1) {
                                val value = (samples[frame * 2 + c].coerceIn(-1f, 1f) * 32_767f).let { Math.round(it) }
                                piece[at * 4 + c * 2] = value.toByte()
                                piece[at * 4 + c * 2 + 1] = (value shr 8).toByte()
                            }
                            at += 1
                        }
                        frames += count
                    }
                    var resampler: Resampler? = null
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        val read = reader.read() ?: break
                        val step = resampler ?: Resampler(reader.rate).also { resampler = it }
                        step.push(read, read.size / 2, ::emit)
                    }
                    resampler?.finish(::emit)
                    if (frames == 0L) throw IOException("nothing decoded from a ${kind ?: "file"}")
                    if (at > 0) pieces.add(piece)
                    Log.i(TAG, "music: decoded a ${kind ?: "file"} at ${reader.rate} Hz, ${frames / 48} ms")
                    return Decoded(Pcm48(pieces, frames.toInt()), original, reader.rate, kind)
                }
            }
        } catch (fault: Throwable) {
            original?.close()
            throw fault
        }
    }

    /**
     * What a file is by its first bytes (MusicLoader.swift `sniff`), for the
     * log: the platform's extractor reads the bytes for itself, so this only
     * says what a file was that would not decode.
     */
    fun sniff(head: ByteArray): String? {
        fun at(offset: Int, text: String): Boolean {
            val want = text.toByteArray(Charsets.US_ASCII)
            if (head.size < offset + want.size) return false
            return want.indices.all { head[offset + it] == want[it] }
        }
        if (at(0, "fLaC")) return "flac"
        if (at(0, "RIFF") && at(8, "WAVE") || at(0, "RF64")) return "wav"
        if (at(0, "FORM") && (at(8, "AIFF") || at(8, "AIFC"))) return "aiff"
        if (at(4, "ftyp")) return "m4a"
        if (at(0, "caff")) return "caf"
        if (at(0, "OggS")) return if (at(28, "OpusHead")) "opus" else "ogg"
        if (at(0, "ID3")) return "mp3"
        // A frame sync: an ADTS AAC frame has layer bits 00, an MPEG audio
        // frame anything else.
        if (head.size >= 2 && head[0] == 0xFF.toByte() && head[1].toInt() and 0xE0 == 0xE0) {
            return if (head[1].toInt() and 0x06 == 0) "aac" else "mp3"
        }
        return null
    }
}
