package app.tomoshibi.media

import android.annotation.SuppressLint
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioPlaybackCaptureConfiguration
import android.media.AudioRecord
import android.media.projection.MediaProjection
import android.os.Process
import android.util.Log
import uniffi.tomoshibi_core.AudioInput

private const val TAG = "Tomoshibi"
private const val RATE = 48_000
private const val CHANNELS = 2
/** Ten milliseconds, the piece the core's audio source takes. */
private const val FRAMES = RATE / 100

/**
 * The sound of a shared screen: whatever other apps play, captured through
 * the screen's projection (AudioPlaybackCapture, Android 10 and later), and
 * sent to the share's sound track or to sound shared on its own (Sharing).
 *
 * One capture serves both. On a Mac the screen and the sound each have a
 * capture of their own, of what was picked for each; on Android there is
 * only one thing to capture, every app's media at once, so the screen and
 * the sound shared on its own hear the same, and [sink] says which of the
 * two it goes to now: never both, since the screen's own sound stands down
 * while sound is shared on its own (room.ts `alongside`).
 *
 * What it captures is what the system lets be captured: the media, game and
 * unknown usages, and of those only from apps that allow it, so a call in
 * another app, a ringtone or a notification is never shared. This app's own
 * sound is left out, as the Mac leaves it out (`excludesCurrentProcessAudio`):
 * a share would otherwise send everybody back their own voices.
 *
 * The samples go out as 16-bit for the Opus track and, where the sound was
 * offered losslessly, as the floats they were captured in (`pushLossless`),
 * stamped on a clock that advances by the frames read, so the stream has
 * the steady timing the lossless sender expects.
 */
internal class PlaybackCapture {
    /** Where the sound goes now; nowhere while there is nobody to send it to. */
    @Volatile var sink: AudioInput? = null
    /** Whether [sink] was offered without loss, and so wants the floats too. */
    @Volatile var lossless = false

    private var record: AudioRecord? = null
    private var thread: Thread? = null
    @Volatile private var running = false

    val started: Boolean get() = record != null

    /** Starts capturing through [projection]; throws where the system will not. */
    @SuppressLint("MissingPermission")
    fun start(projection: MediaProjection) {
        stop()
        val configuration = AudioPlaybackCaptureConfiguration.Builder(projection)
            .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
            .addMatchingUsage(AudioAttributes.USAGE_GAME)
            .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
            .excludeUid(Process.myUid())
            .build()
        val format = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_FLOAT).setSampleRate(RATE)
            .setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build()
        val least = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_STEREO, AudioFormat.ENCODING_PCM_FLOAT)
        val record = AudioRecord.Builder()
            .setAudioFormat(format)
            .setAudioPlaybackCaptureConfig(configuration)
            .setBufferSizeInBytes(maxOf(least, FRAMES * CHANNELS * 4 * 8))
            .build()
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            record.release()
            throw IllegalStateException("the shared sound would not open")
        }
        record.startRecording()
        if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
            record.release()
            throw IllegalStateException("the shared sound would not start")
        }
        this.record = record
        running = true
        thread = Thread({ read(record) }, "tomoshibi-shared-sound").also { it.start() }
        Log.i(TAG, "capturing the shared sound")
    }

    /** The projection was replaced: the same capture on the new one. */
    fun moved(projection: MediaProjection) {
        if (record == null) return
        try {
            start(projection)
        } catch (fault: Exception) {
            Log.w(TAG, "the shared sound could not follow the new projection: ${fault.message}")
        }
    }

    private fun read(record: AudioRecord) {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        val floats = FloatArray(FRAMES * CHANNELS)
        val pcm = ByteArray(FRAMES * CHANNELS * 2)
        val began = System.nanoTime() / 1000
        var frames = 0L
        var peak = 0f
        var reported = false
        while (running) {
            val read = record.read(floats, 0, floats.size, AudioRecord.READ_BLOCKING)
            if (read < 0) {
                if (running) Log.w(TAG, "the shared sound stopped ($read)")
                break
            }
            if (read == 0) continue
            val sink = sink
            if (sink != null) {
                for (i in 0 until read) {
                    val sample = (floats[i].coerceIn(-1f, 1f) * 32767f).toInt()
                    pcm[i * 2] = sample.toByte()
                    pcm[i * 2 + 1] = (sample shr 8).toByte()
                }
                sink.pushPcm(if (read == floats.size) pcm else pcm.copyOf(read * 2))
                if (lossless) {
                    val stamp = began + frames * 1_000_000L / RATE
                    sink.pushLossless((if (read == floats.size) floats else floats.copyOf(read)).asList(), CHANNELS.toUInt(), RATE.toUInt(), stamp)
                }
            }
            frames += read / CHANNELS
            // How loud the first two seconds were, for the log: a capture the
            // system silences (an app that does not allow it) gives zeros.
            if (!reported) {
                for (i in 0 until read) peak = maxOf(peak, kotlin.math.abs(floats[i]))
                if (frames >= RATE * 2) {
                    reported = true
                    Log.i(TAG, "shared sound peak in its first 2 s: ${"%.4f".format(peak)}")
                }
            }
        }
        try {
            record.stop()
        } catch (_: IllegalStateException) {
        }
        record.release()
    }

    fun stop() {
        val record = record ?: return
        this.record = null
        running = false
        // Stopped first, which ends a read waiting for sound that may never
        // come; the thread releases the recorder once that read returns.
        try {
            record.stop()
        } catch (_: IllegalStateException) {
        }
        thread?.join(200)
        thread = null
    }
}
