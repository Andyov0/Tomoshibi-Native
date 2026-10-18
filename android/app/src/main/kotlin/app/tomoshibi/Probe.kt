package app.tomoshibi

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.util.Size
import android.view.Surface
import android.view.TextureView
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import app.tomoshibi.core.CoreNative
import java.util.concurrent.Executors
import kotlin.math.sqrt
import uniffi.tomoshibi_core.AudioInput
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.PcmSink
import uniffi.tomoshibi_core.PictureShape
import uniffi.tomoshibi_core.VideoInput

private const val TAG = "Tomoshibi"
private const val RATE = 48_000

/**
 * A remote picture, drawn by the core into this view's surface
 * (core/src/call/window.rs). The view only turns and fits it.
 */
class RemoteVideoView(context: Context) : TextureView(context), TextureView.SurfaceTextureListener {
    private var call: Call? = null
    private var sid: String? = null
    private var window = 0L
    private var shape = Triple(0, 0, 0)
    private val main = Handler(Looper.getMainLooper())

    init {
        surfaceTextureListener = this
    }

    fun show(call: Call, sid: String) {
        if (this.call === call && this.sid == sid) return
        detach()
        this.call = call
        this.sid = sid
        attach()
    }

    private fun attach() {
        val call = call ?: return
        val sid = sid ?: return
        val texture = surfaceTexture ?: return
        if (window != 0L) return
        val surface = Surface(texture)
        window = CoreNative.windowFromSurface(surface)
        surface.release()
        if (window == 0L) return
        call.attachVideoWindow(sid, window.toULong(), object : PictureShape {
            override fun changed(width: UInt, height: UInt, rotation: UInt) {
                main.post {
                    shape = Triple(width.toInt(), height.toInt(), rotation.toInt())
                    Log.i(TAG, "picture $sid is ${width}x$height turned $rotation")
                    fit()
                }
            }
        })
    }

    private fun detach() {
        val call = call
        val sid = sid
        if (window != 0L && call != null && sid != null) call.detachVideoWindow(sid, window.toULong())
        window = 0L
    }

    /** The picture upright and whole, in the middle of the view. */
    private fun fit() {
        val (w, h, turn) = shape
        if (w == 0 || h == 0 || width == 0 || height == 0) return
        val turned = turn == 90 || turn == 270
        val (pw, ph) = if (turned) h.toFloat() to w.toFloat() else w.toFloat() to h.toFloat()
        val scale = minOf(width / pw, height / ph)
        val m = Matrix()
        val cx = width / 2f
        val cy = height / 2f
        // The surface's buffer is stretched to the view first; undo that, turn
        // it, and scale it to fit.
        m.postScale(w / width.toFloat(), h / height.toFloat(), cx, cy)
        m.postRotate(turn.toFloat(), cx, cy)
        m.postScale(scale, scale, cx, cy)
        setTransform(m)
    }

    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) = attach()
    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = fit()
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        detach()
        return true
    }
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) {}
}

/**
 * A remote sound into an AudioTrack, written without blocking from the
 * core's thread. With `muted` nothing is played and only its level is logged:
 * a test room's browser beeps once a second, and the phone is on somebody's
 * desk.
 */
class PcmPlayer(private val sid: String, private val muted: Boolean) : PcmSink {
    private var track: AudioTrack? = null
    private var channels = 0
    private var energy = 0.0
    private var samples = 0
    private var reported = System.nanoTime()

    override fun onPcm(pcm: ByteArray, sampleRate: UInt, channels: UInt) {
        if (muted) {
            measure(pcm)
            return
        }
        if (track == null || this.channels != channels.toInt()) {
            track?.release()
            this.channels = channels.toInt()
            val mask = if (this.channels == 2) AudioFormat.CHANNEL_OUT_STEREO else AudioFormat.CHANNEL_OUT_MONO
            track = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
                .setAudioFormat(AudioFormat.Builder().setSampleRate(sampleRate.toInt())
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(mask).build())
                .setBufferSizeInBytes(sampleRate.toInt() / 10 * 2 * this.channels)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build().also { it.play() }
        }
        track?.write(pcm, 0, pcm.size, AudioTrack.WRITE_NON_BLOCKING)
    }

    private fun measure(pcm: ByteArray) {
        var i = 0
        while (i + 1 < pcm.size) {
            val s = ((pcm[i + 1].toInt() shl 8) or (pcm[i].toInt() and 0xff)).toShort().toDouble()
            energy += s * s
            samples++
            i += 2
        }
        val now = System.nanoTime()
        if (now - reported > 1_000_000_000L && samples > 0) {
            Log.i(TAG, "sound $sid rms ${"%.1f".format(sqrt(energy / samples))} over $samples samples")
            energy = 0.0
            samples = 0
            reported = now
        }
    }

    fun close() {
        track?.release()
        track = null
    }
}

/** The microphone, ten milliseconds at a time, with the system's echo canceller. */
class Microphone(private val input: AudioInput) {
    @Volatile private var running = true
    private var thread: Thread? = null

    @SuppressLint("MissingPermission")
    fun start() {
        val size = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val record = AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, RATE,
            AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(size, RATE / 50 * 2))
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            Log.w(TAG, "microphone did not open")
            return
        }
        thread = Thread({
            val chunk = ByteArray(RATE / 100 * 2)
            record.startRecording()
            var pushed = 0
            while (running) {
                val read = record.read(chunk, 0, chunk.size)
                if (read == chunk.size) {
                    input.pushPcm(chunk.copyOf())
                    if (++pushed % 500 == 0) Log.i(TAG, "microphone pushed $pushed chunks")
                }
            }
            record.stop()
            record.release()
        }, "tomoshibi-microphone").also { it.start() }
    }

    fun stop() {
        running = false
        thread?.join(500)
    }
}

/** The front camera at 720p, each frame's planes handed to the core. */
class Camera(private val context: Context, private val owner: LifecycleOwner, private val input: VideoInput) {
    private val worker = Executors.newSingleThreadExecutor()
    private var provider: ProcessCameraProvider? = null

    fun start() {
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            val provider = future.get()
            this.provider = provider
            val analysis = ImageAnalysis.Builder()
                .setResolutionSelector(ResolutionSelector.Builder()
                    .setResolutionStrategy(ResolutionStrategy(Size(1280, 720), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER))
                    .build())
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_YUV_420_888)
                .build()
            var frames = 0
            analysis.setAnalyzer(worker) { image ->
                val (y, u, v) = image.planes
                fun bytes(plane: androidx.camera.core.ImageProxy.PlaneProxy): ByteArray {
                    val buffer = plane.buffer
                    buffer.rewind()
                    return ByteArray(buffer.remaining()).also { buffer.get(it) }
                }
                input.pushPlanes(bytes(y), bytes(u), bytes(v), y.rowStride.toUInt(), u.rowStride.toUInt(),
                    u.pixelStride.toUInt(), image.width.toUInt(), image.height.toUInt(),
                    image.imageInfo.timestamp / 1000, image.imageInfo.rotationDegrees.toUInt())
                if (++frames % 150 == 0) Log.i(TAG, "camera sent $frames frames ${image.width}x${image.height} turned ${image.imageInfo.rotationDegrees}")
                image.close()
            }
            provider.unbindAll()
            provider.bindToLifecycle(owner, CameraSelector.DEFAULT_FRONT_CAMERA, analysis)
        }, ContextCompat.getMainExecutor(context))
    }

    fun stop() {
        provider?.unbindAll()
        worker.shutdown()
    }
}
