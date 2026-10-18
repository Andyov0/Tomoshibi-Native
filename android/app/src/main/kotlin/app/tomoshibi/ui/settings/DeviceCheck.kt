package app.tomoshibi.ui.settings

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.util.Log
import androidx.camera.core.Preview
import androidx.camera.view.PreviewView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import app.tomoshibi.media.AudioRouting
import app.tomoshibi.media.CameraCapture
import app.tomoshibi.media.DeviceAccess
import app.tomoshibi.media.DeviceKind
import app.tomoshibi.media.DeviceRefused
import app.tomoshibi.media.SoundLevel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.sqrt

/**
 * The camera and the microphone, opened only while the page that shows them
 * is open (Settings.swift `DeviceCheck`): a light that stays on after the
 * page has gone would be a light that lies.
 */
internal interface DeviceCheck {
    /** How loudly the microphone hears, 0 to 1, on MicLevel.tsx's curve. */
    val level: Float
    val cameraRunning: Boolean
    val cameraRefused: Boolean
    val microphoneRefused: Boolean
    /** Whether the devices are meant to be open: between [start] and [stop]. */
    val started: Boolean

    fun start(camera: String?, microphone: String?)
    fun stop()
    /** Stops for good: the page has gone. */
    fun close()

    /** The camera's picture of the person, drawn wherever it is placed. */
    @Composable
    fun Picture(modifier: Modifier)
}

/** The check, with the device's own camera and microphone. */
internal class SystemDeviceCheck(private val capture: CameraCapture, private val routing: AudioRouting) : DeviceCheck {
    override var level by mutableFloatStateOf(0f)
        private set
    override var cameraRunning by mutableStateOf(false)
        private set
    override var cameraRefused by mutableStateOf(false)
        private set
    override var microphoneRefused by mutableStateOf(false)
        private set
    override var started = false
        private set

    private val scope = MainScope()
    /** Moved on by every start and stop, so that work begun for an earlier one does nothing when it comes back. */
    private var generation = 0
    private var surface: Preview.SurfaceProvider? = null
    private val reading = SoundLevel()
    private var listening: Listening? = null
    private var ticking: Job? = null

    override fun start(camera: String?, microphone: String?) {
        started = true
        generation++
        val mine = generation
        startCamera(camera, mine)
        startMicrophone(microphone, mine)
    }

    override fun stop() {
        started = false
        generation++
        capture.stop()
        cameraRunning = false
        stopMicrophone()
    }

    override fun close() {
        stop()
        scope.cancel()
    }

    @Composable
    override fun Picture(modifier: Modifier) {
        AndroidView(
            factory = { context ->
                // A TextureView under it rather than a SurfaceView, so the
                // picture takes the page's rounded corners and its fades.
                PreviewView(context).apply {
                    implementationMode = PreviewView.ImplementationMode.COMPATIBLE
                    scaleType = PreviewView.ScaleType.FILL_CENTER
                    surface = surfaceProvider
                    // Made after the camera came up: the camera is bound to
                    // it again, which a preview taken by the next start
                    // would otherwise wait for.
                    if (cameraRunning) scope.launch { runCatching { capture.showPreview(surfaceProvider) } }
                }
            },
            modifier = modifier,
        )
    }

    // MARK: The camera

    private fun startCamera(chosen: String?, mine: Int) {
        scope.launch {
            try {
                DeviceAccess.ensure(DeviceKind.Camera)
            } catch (_: DeviceRefused) {
                cameraRefused = true
                return@launch
            }
            if (mine != generation) return@launch
            try {
                capture.showPreview(surface)
                capture.start(chosen)
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                Log.w(TAG, "the camera would not open for the check: ${fault.message}")
                return@launch
            }
            // The page may have gone while it opened.
            if (mine != generation) {
                capture.stop()
                return@launch
            }
            cameraRunning = true
        }
    }

    // MARK: The microphone

    private fun startMicrophone(chosen: String?, mine: Int) {
        stopMicrophone()
        scope.launch {
            try {
                DeviceAccess.ensure(DeviceKind.Microphone)
            } catch (_: DeviceRefused) {
                microphoneRefused = true
                return@launch
            }
            if (mine != generation) return@launch
            val opened = Listening.open(routing, chosen, reading) ?: return@launch
            listening = opened
            ticking = scope.launch {
                while (isActive) {
                    delay(50)
                    // MicLevel.tsx's curve: the square root, lifted, so a
                    // voice at an ordinary distance fills most of the meter.
                    val shown = minOf(1f, sqrt(reading.rms).toFloat() * 2.4f)
                    level = if (shown > level) shown else level * 0.8f + shown * 0.2f
                }
            }
        }
    }

    private fun stopMicrophone() {
        ticking?.cancel()
        ticking = null
        listening?.close()
        listening = null
        level = 0f
    }

    /** A microphone recording on a thread of its own into the level, and nowhere else. */
    private class Listening(private val record: AudioRecord, private val reading: SoundLevel) : Thread("tomoshibi-check") {
        @Volatile private var running = true

        override fun run() {
            val buffer = ByteArray(FRAMES * 2)
            while (running) {
                val read = record.read(buffer, 0, buffer.size)
                if (read > 0) reading.add(buffer, read, RATE) else if (read < 0) break
            }
        }

        fun close() {
            running = false
            runCatching { record.stop() }
            join(500)
            record.release()
        }

        companion object {
            private const val RATE = 48_000
            private const val FRAMES = RATE / 50

            /** Opened on the chosen microphone, or the system's; null where it will not open. */
            @SuppressLint("MissingPermission")
            fun open(routing: AudioRouting, chosen: String?, reading: SoundLevel): Listening? {
                var record: AudioRecord? = null
                return try {
                    val least = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
                    val made = AudioRecord.Builder()
                        .setAudioSource(MediaRecorder.AudioSource.MIC)
                        .setAudioFormat(
                            AudioFormat.Builder().setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                                .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
                        )
                        .setBufferSizeInBytes(maxOf(least, FRAMES * 4))
                        .build()
                    record = made
                    check(made.state == AudioRecord.STATE_INITIALIZED) { "the recorder did not initialise" }
                    routing.info(chosen, input = true)?.let { made.preferredDevice = it }
                    made.startRecording()
                    Listening(made, reading).also { it.start() }
                } catch (fault: Exception) {
                    Log.w(TAG, "the microphone would not open for the check: ${fault.message}")
                    record?.release()
                    null
                }
            }
        }
    }

    private companion object {
        const val TAG = "Tomoshibi"
    }
}
