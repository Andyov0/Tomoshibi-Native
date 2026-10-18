package app.tomoshibi.media

import android.annotation.SuppressLint
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.util.Log
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.math.sqrt
import uniffi.tomoshibi_core.AudioInput
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.PcmSink

private const val TAG = "Tomoshibi"
private const val RATE = 48_000
/** Ten milliseconds, the piece the mixer works in and the core thinks in. */
private const val FRAMES = RATE / 100

/**
 * Everything a call hears and says (the Apple apps' CallAudio).
 *
 * Every remote sound gets a source of its own in one mixer, so a person's
 * voice, the sound of their screen and the music are turned up, down or off
 * separately, as the web client's sound panel does (web/src/live/hearing.ts),
 * and all of them leave through one AudioTrack: the system's echo canceller
 * takes back out of the microphone what that track plays, which is why it is
 * one track rather than one per sound.
 *
 * The core hands each sound over on its own threads, which must never wait:
 * it is written into a ring (PcmRing) and nothing else, and the mixer's own
 * thread takes from every ring ten milliseconds at a time, paced by the
 * track's blocking write.
 *
 * While the microphone is off the call only plays, as an ordinary player
 * does, with no voice processing and the media volume; while it records it
 * plays as a call, so the echo canceller has its reference. On the Apple apps
 * the call's session recorded from the first sound to the last, microphone
 * on or off, and music in a call sounded like a telephone call however still
 * the microphone was; the owner chose a short gap at each press of the
 * microphone over that (2026-10-17).
 *
 * [muted] plays nothing and only measures (a test's `mute`): the mixer keeps
 * its pace on the clock instead and logs each sound's level once a second,
 * which is the proof that a sound arriving is also taken.
 *
 * Every method is for the main thread.
 */
class CallAudio(
    private val routing: AudioRouting,
    private val muted: Boolean,
) {
    /** What the call is told about the sound that it did not ask for. */
    sealed class News {
        /** Sound stopped and could not be brought back: nothing is heard until the microphone is turned on again. */
        data class GaveUp(val error: Throwable) : News()
        /** It runs again after having been given up. */
        data object Back : News()
        /** The microphone stopped under the call (another app took it, or it went away). */
        data class MicrophoneLost(val error: Throwable) : News()
    }

    /** Told on the main thread; the call says these to the person. */
    var onNews: ((News) -> Unit)? = null

    /** The web's "original sound": no echo cancellation, noise suppression or gain control. */
    var originalSound = false
        private set

    /**
     * Whether the system's voice processing is really on, which is not the
     * same as asked for: a device without an echo canceller records without
     * one.
     */
    var voiceProcessing = false
        private set

    /** How loud the microphone is as it is sent, while it is on: the devices panel's meter. */
    var microphoneLevel: SoundLevel? = null
        private set

    /** The microphone and speakers picked, by AudioRouting's ids; null is the system's own choice. */
    var chosenInput: String? = null
        private set
    var chosenOutput: String? = null
        private set

    val started: Boolean get() = output != null

    private val main by lazy { Handler(Looper.getMainLooper()) }
    private val players = HashMap<String, RingSource>()
    private val sources = CopyOnWriteArrayList<MixerSource>()
    private var output: Output? = null
    private var capture: Capture? = null
    /** The track the microphone feeds while it is on, kept so a restart of the capture finds it again. */
    private var microphone: AudioInput? = null

    // MARK: Playing

    /** Starts the mixer, if it is not already running. */
    fun start() {
        if (output != null) return
        output = Output().also { it.start() }
    }

    /** Plays a remote track of [channels] channels through a source of its own. */
    fun play(sid: String, channels: Int, call: Call) {
        start()
        stopPlaying(sid)
        val source = RingSource(sid, PcmRing(channels))
        players[sid] = source
        sources.add(source)
        call.attachPcm(sid, source.sink)
        Log.i(TAG, "playing $sid in $channels channels")
    }

    fun stopPlaying(sid: String, call: Call? = null) {
        val source = players.remove(sid) ?: return
        call?.detach(sid)
        sources.remove(source)
        source.ring.clear()
    }

    /** The listener's own setting for this sound, 0 to 1. */
    fun setVolume(sid: String, volume: Float) {
        players[sid]?.volume = volume.coerceIn(0f, 1f)
    }

    /** The music's dip while somebody talks (web/src/live/duck.ts), applied on top of the listener's own setting. */
    fun setDuck(sid: String, factor: Float) {
        players[sid]?.duck = factor.coerceIn(0f, 1f)
    }

    fun volume(sid: String): Float? = players[sid]?.volume

    /** How loud a sound playing here is, read as it is mixed, before the listener's volume and the duck. */
    fun meter(sid: String): SoundLevel? = players[sid]?.level

    /**
     * Plays sound this app makes itself through a source of its own, filed
     * under [key] beside the remote tracks, so that the volume, the duck and
     * stopping work on it as on any other sound: the song desk's holder hears
     * the music this way, through the same track the echo canceller hears.
     */
    fun playLocally(key: String, channels: Int): PcmRing {
        start()
        stopPlaying(key)
        val source = RingSource(key, PcmRing(channels))
        players[key] = source
        sources.add(source)
        return source.ring
    }

    /** A source somebody else plays into the call's mixer (the lossless streams). */
    internal fun attach(source: MixerSource) {
        start()
        sources.add(source)
    }

    internal fun detach(source: MixerSource) {
        sources.remove(source)
    }

    // MARK: The microphone

    /**
     * Sends the microphone to [input] until [stopMicrophone]. Throws where it
     * will not open; a microphone the person refused is caught before this,
     * by DeviceAccess, because a refused AudioRecord does not always fail but
     * may record silence.
     */
    fun startMicrophone(input: AudioInput) {
        microphone = null
        capture?.stop()
        capture = null
        start()
        val record = openRecord()
        val capture = Capture(input, record, effects(record), SoundLevel())
        routing.communicating(voiceProcessing)
        try {
            capture.begin()
        } catch (fault: Throwable) {
            capture.release()
            routing.communicating(false)
            output?.voice = false
            throw fault
        }
        this.capture = capture
        microphone = input
        microphoneLevel = capture.level
        // Played as a call from now on, so the echo canceller hears it.
        output?.voice = voiceProcessing
        Log.i(TAG, "microphone on, voice processing ${if (voiceProcessing) "on" else "off"}")
    }

    fun stopMicrophone() {
        microphone = null
        microphoneLevel = null
        capture?.stop()
        capture = null
        output?.voice = false
        routing.communicating(false)
    }

    /**
     * Turns the system's voice processing off or on. A microphone that is on
     * is opened again with the other source at once, so the call and what it
     * sends agree.
     */
    fun setOriginalSound(original: Boolean) {
        if (original == originalSound) return
        originalSound = original
        restartMicrophone()
    }

    /** The devices and processing a call starts with, taken by the next microphone or track opened. */
    fun prefer(input: String?, output: String?, original: Boolean) {
        val changed = input != chosenInput || original != originalSound
        chosenInput = input
        chosenOutput = output
        originalSound = original
        this.output?.preferred = routing.info(output, input = false)
        if (changed) restartMicrophone()
    }

    /**
     * Another microphone or speakers. A microphone that will not take puts
     * the one before back and throws, so that the call and the panel agree.
     */
    fun use(device: AudioDevice, input: Boolean) {
        if (input) {
            val before = chosenInput
            chosenInput = device.id
            try {
                if (routing.info(device.id, input = true) == null) throw IllegalStateException("no such microphone")
                restartMicrophone()
            } catch (fault: Throwable) {
                chosenInput = before
                restartMicrophone()
                throw fault
            }
        } else {
            val info = routing.info(device.id, input = false) ?: throw IllegalStateException("no such speakers")
            chosenOutput = device.id
            output?.preferred = info
        }
    }

    /** The microphone or speakers in use, or that would be used. */
    fun current(input: Boolean): String? = if (input) chosenInput else chosenOutput

    /** The microphones or speakers somebody can pick. */
    fun devices(input: Boolean): List<AudioDevice> = routing.devices(input)

    /** Whether choosing a device can take now. */
    val choosable: Boolean get() = routing.choosable

    private fun restartMicrophone() {
        val input = microphone ?: return
        try {
            startMicrophone(input)
        } catch (fault: Throwable) {
            Log.w(TAG, "the microphone would not open again: ${fault.message}")
            microphone = null
            onNews?.invoke(News.MicrophoneLost(fault))
        }
    }

    @SuppressLint("MissingPermission")
    private fun openRecord(): AudioRecord {
        // The web's "original sound" asks the browser for no processing at
        // all; the closest Android has is the unprocessed source where the
        // device offers it, and the plain microphone where it does not.
        val source = when {
            !originalSound -> MediaRecorder.AudioSource.VOICE_COMMUNICATION
            unprocessedOffered() -> MediaRecorder.AudioSource.UNPROCESSED
            else -> MediaRecorder.AudioSource.MIC
        }
        val least = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val record = AudioRecord.Builder()
            .setAudioSource(source)
            .setAudioFormat(AudioFormat.Builder().setSampleRate(RATE).setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setChannelMask(AudioFormat.CHANNEL_IN_MONO).build())
            .setBufferSizeInBytes(maxOf(least, FRAMES * 2 * 4))
            .build()
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            record.release()
            throw IllegalStateException("the microphone did not open")
        }
        routing.info(chosenInput, input = true)?.let { record.setPreferredDevice(it) }
        voiceProcessing = source == MediaRecorder.AudioSource.VOICE_COMMUNICATION
        return record
    }

    /**
     * The system's echo canceller, noise suppressor and gain control on a
     * voice recorder. Most devices attach them to the voice source by
     * themselves; some attach nothing, and a call without an echo canceller is
     * everybody hearing themselves a beat late. Released with the recorder.
     */
    private fun effects(record: AudioRecord): List<android.media.audiofx.AudioEffect> {
        if (!voiceProcessing) return emptyList()
        val session = record.audioSessionId
        return listOfNotNull(
            if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(session) else null,
            if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(session) else null,
            if (AutomaticGainControl.isAvailable()) AutomaticGainControl.create(session) else null,
        ).onEach { it.enabled = true }
    }

    private fun unprocessedOffered(): Boolean = routing.offersUnprocessed

    // MARK: Ending

    fun stop() {
        microphone = null
        microphoneLevel = null
        capture?.stop()
        capture = null
        routing.communicating(false)
        for (source in players.values) source.ring.clear()
        players.clear()
        sources.clear()
        output?.finish()
        output = null
    }

    private fun tell(news: News) {
        main.post { onNews?.invoke(news) }
    }

    // MARK: The mixer

    /**
     * The one thread that plays: ten milliseconds from every source, mixed
     * into stereo floats and written to the track, whose blocking write is
     * the clock. A track that dies under it (a device that went away) is made
     * again; one that will not be made for five seconds is given up and said,
     * and said again when it comes back.
     */
    private inner class Output : Thread("tomoshibi-mixer") {
        @Volatile private var running = true
        /** Whether to play as a call (the echo canceller's reference) or as media. */
        @Volatile var voice = false
        @Volatile var preferred: android.media.AudioDeviceInfo? = null

        override fun run() {
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
            val mix = FloatArray(FRAMES * 2)
            var track: AudioTrack? = null
            var trackVoice = false
            var trackPreferred: android.media.AudioDeviceInfo? = null
            var next = System.nanoTime()
            var reported = next
            var failingSince = 0L
            var gaveUp = false
            while (running) {
                java.util.Arrays.fill(mix, 0f)
                for (source in sources) source.mixInto(mix, FRAMES)
                val now = System.nanoTime()
                if (muted) {
                    next += 10_000_000L
                    val wait = next - System.nanoTime()
                    if (wait > 0) nap(wait / 1_000_000, (wait % 1_000_000).toInt()) else if (wait < -200_000_000L) next = System.nanoTime()
                } else {
                    if (track != null && (trackVoice != voice || trackPreferred !== preferred)) {
                        track.release()
                        track = null
                    }
                    if (track == null) {
                        track = try {
                            makeTrack(voice).also {
                                trackVoice = voice
                                trackPreferred = preferred
                                preferred?.let { device -> it.setPreferredDevice(device) }
                            }
                        } catch (fault: Throwable) {
                            if (failingSince == 0L) failingSince = now
                            if (!gaveUp && now - failingSince > 5_000_000_000L) {
                                gaveUp = true
                                Log.e(TAG, "the call's sound will not start: ${fault.message}")
                                tell(News.GaveUp(fault))
                            }
                            null
                        }
                        if (track != null) {
                            failingSince = 0L
                            if (gaveUp) {
                                gaveUp = false
                                tell(News.Back)
                            }
                        }
                    }
                    val playing = track
                    if (playing == null) {
                        nap(10)
                    } else {
                        for (i in mix.indices) mix[i] = mix[i].coerceIn(-1f, 1f)
                        val wrote = playing.write(mix, 0, mix.size, AudioTrack.WRITE_BLOCKING)
                        if (wrote < 0) {
                            Log.w(TAG, "the call's sound track failed ($wrote); making it again")
                            playing.release()
                            track = null
                        }
                    }
                }
                if (now - reported >= 1_000_000_000L) {
                    reported = now
                    if (muted) for (source in sources) source.report()
                }
            }
            track?.release()
        }

        private fun makeTrack(voice: Boolean): AudioTrack {
            val attributes = if (voice) {
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
            } else {
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build()
            }
            val format = AudioFormat.Builder().setSampleRate(RATE).setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build()
            val least = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_FLOAT)
            val track = AudioTrack.Builder()
                .setAudioAttributes(attributes)
                .setAudioFormat(format)
                .setBufferSizeInBytes(maxOf(least, FRAMES * 2 * 4 * 4))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
            if (track.state != AudioTrack.STATE_INITIALIZED) {
                track.release()
                throw IllegalStateException("the audio track did not open")
            }
            track.play()
            return track
        }

        /** A pause that ends early, and quietly, when the mixer is finished. */
        private fun nap(millis: Long, nanos: Int = 0) {
            try {
                sleep(millis, nanos)
            } catch (_: InterruptedException) {
            }
        }

        fun finish() {
            running = false
            interrupt()
        }
    }

    // MARK: The microphone's thread

    /** Reads the microphone ten milliseconds at a time and hands each piece to the core. */
    private inner class Capture(
        private val input: AudioInput,
        private val record: AudioRecord,
        private val effects: List<android.media.audiofx.AudioEffect>,
        val level: SoundLevel,
    ) {
        @Volatile private var running = true
        private var thread: Thread? = null

        fun begin() {
            record.startRecording()
            if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
                throw IllegalStateException("the microphone would not start recording")
            }
            thread = Thread({
                Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
                val chunk = ByteArray(FRAMES * 2)
                var peak = 0
                val began = System.nanoTime()
                var reported = false
                while (running) {
                    val read = record.read(chunk, 0, chunk.size)
                    if (read < 0) {
                        if (running) {
                            Log.w(TAG, "the microphone stopped ($read)")
                            tell(News.MicrophoneLost(IllegalStateException("the microphone stopped ($read)")))
                        }
                        break
                    }
                    if (read == 0) continue
                    level.add(chunk, read)
                    // How loud the microphone was in its first two seconds:
                    // a microphone the system silences gives zeros rather
                    // than an error, with nothing else to say which.
                    if (!reported) {
                        var i = 0
                        while (i + 1 < read) {
                            peak = maxOf(peak, kotlin.math.abs(((chunk[i].toInt() and 0xff) or (chunk[i + 1].toInt() shl 8)).toShort().toInt()))
                            i += 2
                        }
                        if (System.nanoTime() - began > 2_000_000_000L) {
                            reported = true
                            Log.i(TAG, "microphone peak in its first 2 s: $peak of 32767")
                        }
                    }
                    input.pushPcm(if (read == chunk.size) chunk else chunk.copyOf(read))
                }
                try {
                    record.stop()
                } catch (_: IllegalStateException) {
                }
                close()
            }, "tomoshibi-microphone").also { it.start() }
        }

        /**
         * Waits the few milliseconds the last read takes: the thread releases
         * the recorder itself, and a microphone opened again at once (another
         * device, original sound) beside one not yet released may not open.
         */
        fun stop() {
            running = false
            thread?.join(100)
        }

        /** A capture that never began: let go at once, on the caller's thread. */
        fun release() {
            running = false
            close()
        }

        private fun close() {
            for (effect in effects) effect.release()
            record.release()
        }
    }
}

/**
 * One sound in the mixer, with its listener's volume and its duck, read by
 * the mixer thread on every pass.
 */
abstract class MixerSource(val key: String) {
    @Volatile var volume = 1f
    @Volatile var duck = 1f
    val level = SoundLevel()
    private var energy = 0.0
    private var frames = 0

    /** Adds [frames] frames into [into] (interleaved stereo) at the source's gain. */
    fun mixInto(into: FloatArray, frames: Int) {
        val sum = render(into, frames, volume * duck)
        energy += sum
        this.frames += frames
        level.addEnergy(sum, frames)
    }

    /** Adds the sound in at [gain] and returns the sum of squares of the sound before the gain. */
    protected abstract fun render(into: FloatArray, frames: Int, gain: Float): Double

    /** Logs how loud the sound arrived over the last second (a muted test's proof). */
    fun report() {
        if (frames == 0) return
        Log.i(TAG, "sound $key rms ${"%.4f".format(sqrt(energy / frames))} over $frames frames")
        energy = 0.0
        frames = 0
    }
}

/** A remote track, or a sound of the app's own, from a ring at 48 kHz. */
class RingSource(key: String, val ring: PcmRing) : MixerSource(key) {
    /** What the core writes this track into, on its own threads. */
    val sink = object : PcmSink {
        override fun onPcm(pcm: ByteArray, sampleRate: UInt, channels: UInt) {
            ring.write(pcm, channels.toInt().coerceAtLeast(1))
        }
    }

    override fun render(into: FloatArray, frames: Int, gain: Float): Double = ring.mix(into, frames, gain)
}
