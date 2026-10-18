package app.tomoshibi.feature.music

import android.os.Handler
import android.os.Looper
import android.os.Process
import android.util.Log
import app.tomoshibi.media.PcmRing
import java.lang.ref.WeakReference
import java.util.concurrent.locks.LockSupport

private const val TAG = "Tomoshibi"

/**
 * Where the music goes: the published track and, beside it, the lossless
 * stream. An interface over the core's `AudioInput` so that the clock can be
 * driven in a test without the core.
 */
interface MusicInput {
    /** 10 ms at 48 kHz, two channels of 16 bits, little-endian. */
    fun pushPcm(pcm: ByteArray)

    /** The file's own samples beside it, two channels of floats, at its own rate. */
    fun pushLossless(samples: FloatArray, rate: Int, timestampUs: Long)
}

/** The core's published track as a [MusicInput]. */
class CoreMusicInput(val input: uniffi.tomoshibi_core.AudioInput) : MusicInput {
    override fun pushPcm(pcm: ByteArray) = input.pushPcm(pcm)
    override fun pushLossless(samples: FloatArray, rate: Int, timestampUs: Long) =
        input.pushLossless(samples.asList(), 2u, rate.toUInt(), timestampUs)
}

/**
 * The song desk's music going out: one published track, kept across songs,
 * fed in real time (sound.ts `Output`; MusicOutput.swift), and the lossless
 * stream beside it where one is offered.
 *
 * The web keeps one audio context and one published track for as long as the
 * songs that follow share a rate and bit depth, so that everybody listening
 * hears the next song without the stream starting again. Every song here is
 * brought to 48 kHz before it goes into Opus, so one track serves every song
 * and is only closed when the music stops. The lossless stream is the file at
 * its own rate, as the web's is; a song at another rate starts a new stream on
 * the same track (the core's sender begins a new stream on a change of rate,
 * and a listener starts a player afresh for it), which is what the web's
 * closing and reopening amounts to for a listener, without the Opus track
 * going away between the two.
 *
 * Something always goes out, if only zeros. Paused, between one song and the
 * next, or past the end of a song while the desk fetches the next, both keep
 * flowing with silence: the web keeps a constant source connected for the
 * same reason, having measured that a track with nothing playing into it
 * stops being delivered at all, and that a lossless stream with nothing in it
 * for four seconds is given up on by every listener for the rest of the track.
 *
 * The holder hears the same samples through the call's own mixer (the web's
 * `monitor`), behind a volume of their own that changes nothing sent.
 */
class MusicOutput(
    val input: MusicInput,
    /** Whether the track was published with a lossless stream beside it. */
    val lossless: Boolean,
    private val monitor: PcmRing?,
    /** Told the key of a song that played to its end, on the main thread. */
    private val ended: (String) -> Unit,
) {
    private val lock = Any()
    private var song: DecodedSong? = null
    private var key: String? = null
    /**
     * Where the song is, in 48 kHz frames for Opus; the lossless stream's
     * place in the file's own frames is its reader's. Kept apart rather than
     * one worked out from the other, so neither stream ever repeats or skips a
     * sample to round.
     */
    private var position = 0
    private var paused = false
    private var told = false
    private var running = true
    /** The rate the lossless stream runs at: the song's, kept through the silence after it until a song at another rate plays. */
    private var losslessRate = 0
    /**
     * Frames of the lossless stream sent since it began at this rate, and the
     * 10 ms blocks of Opus sent meanwhile, which say how many are due: a rate
     * like 44.1 kHz does not divide into 10 ms, and counting the two against
     * each other keeps the stream at its rate without drifting.
     */
    private var losslessSent = 0L
    private var blocksAtRate = 0L
    private var worstLate = 0L

    /**
     * Starts the clock. The thread holds this output only while it fills a
     * block, so an output nobody holds any more (the call gone without a
     * word) ends its thread rather than sending into a closed track for ever.
     */
    fun start() {
        val held = WeakReference(this)
        val thread = Thread({
            // The clock the song goes out by: late wake-ups are bursts every
            // listener has to absorb.
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
            val pacer = MusicPacer(System.nanoTime())
            val block = ByteArray(FRAME * 4)
            while (true) {
                val output = held.get() ?: return@Thread
                if (!output.send(pacer, block)) return@Thread
                val deadline = pacer.nextDeadline
                val wait = deadline - System.nanoTime()
                if (wait > 0) LockSupport.parkNanos(wait)
                val late = System.nanoTime() - deadline
                if (late in 0..<10_000_000_000) output.noteLate(late)
            }
        }, "Tomoshibi music")
        thread.isDaemon = true
        thread.start()
    }

    /**
     * Plays [song] from its start, in place of whatever was playing, and
     * unpaused: the next song plays unpaused (jukebox.ts `Desk.pause`). The
     * song it replaces lets its file go.
     */
    fun play(song: DecodedSong, key: String) {
        val replaced = synchronized(lock) {
            val was = this.song
            this.song = song
            this.key = key
            position = 0
            paused = false
            told = false
            if (lossless && song.original != null && song.rate != losslessRate) {
                losslessRate = song.rate
                losslessSent = 0
                blocksAtRate = 0
            }
            was
        }
        if (replaced !== song) replaced?.close()
    }

    /** Pauses where it is, or goes on from there. Both streams keep flowing with silence meanwhile; see the type's comment. */
    fun pause(paused: Boolean) {
        synchronized(lock) { this.paused = paused }
    }

    /** Ends the thread and lets the song's file go; the caller unpublishes the track. */
    fun stop() {
        val last = synchronized(lock) {
            running = false
            song.also { song = null }
        }
        last?.close()
    }

    private fun noteLate(late: Long) {
        synchronized(lock) { worstLate = maxOf(worstLate, late) }
    }

    /** How late the clock woke at worst since this was last asked, in milliseconds, for the log. */
    fun takeWorstLate(): Double = synchronized(lock) {
        val worst = worstLate
        worstLate = 0
        worst / 1e6
    }

    /** Where the song playing is, in seconds, and how long it is. */
    val progress: Pair<Double, Double>?
        get() = synchronized(lock) {
            val song = song ?: return@synchronized null
            position / 48_000.0 to song.frames / 48_000.0
        }

    /** Hands over every block that is due. False once stopped. */
    internal fun send(pacer: MusicPacer, block: ByteArray): Boolean {
        repeat(pacer.due(System.nanoTime())) {
            val filled = fill(block) ?: return false
            try {
                input.pushPcm(block)
                monitor?.write(block)
                filled.original?.let { input.pushLossless(it.samples, it.rate, it.at) }
            } catch (fault: Exception) {
                // A track the call already took down; the desk stops the
                // output once it hears, and the next block is the same.
                Log.d(TAG, "music: a block did not go: ${fault.message}")
            }
            filled.ended?.let { key -> MAIN.post { ended(key) } }
        }
        return synchronized(lock) { running }
    }

    /** The lossless stream's share of one 10 ms block: the samples, their rate, and where they start on the stream's clock, which the sender checks for holes. */
    internal class Original(val samples: FloatArray, val rate: Int, val at: Long)

    internal class Filled(val original: Original?, val ended: String?)

    /** The next 10 ms of both streams: the song where it is playing, silence anywhere else; null once stopped. */
    internal fun fill(block: ByteArray): Filled? = synchronized(lock) {
        if (!running) return@synchronized null
        val song = song
        val playing = song != null && !paused
        val original = nextOriginal(playing)
        block.fill(0)
        if (song == null || !playing || position >= song.frames) return@synchronized Filled(original, null)
        val count = minOf(FRAME, song.frames - position)
        song.pcm.copy(position, block, count)
        position += count
        val key = key
        if (position >= song.frames && !told && key != null) {
            told = true
            return@synchronized Filled(original, key)
        }
        Filled(original, null)
    }

    /** As many frames of the file as are due with this block. Called with the lock held. */
    private fun nextOriginal(playing: Boolean): Original? {
        if (!lossless || losslessRate <= 0) return null
        blocksAtRate += 1
        val due = blocksAtRate * losslessRate / 100
        val count = (due - losslessSent).toInt()
        if (count <= 0) return null
        val samples = FloatArray(count * 2)
        val song = song
        val file = song?.original
        if (playing && file != null && song.rate == losslessRate) file.take(count, samples)
        val at = losslessSent * 1_000_000 / losslessRate
        losslessSent = due
        return Original(samples, losslessRate, at)
    }

    companion object {
        /** 10 ms at 48 kHz, the frame the core's source cuts sound into. */
        const val FRAME = 480
        private val MAIN by lazy { Handler(Looper.getMainLooper()) }
    }
}
