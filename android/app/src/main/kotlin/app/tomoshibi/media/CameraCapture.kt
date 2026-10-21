package app.tomoshibi.media

import android.content.Context
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraManager
import android.util.Log
import android.util.Size
import android.view.OrientationEventListener
import android.view.Surface
import androidx.camera.core.CameraSelector
import androidx.camera.core.CameraState
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import java.util.concurrent.Executors
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.suspendCancellableCoroutine
import uniffi.tomoshibi_core.VideoInput
import uniffi.tomoshibi_protocol.CameraDevice
import uniffi.tomoshibi_protocol.cameraCapture

private const val TAG = "Tomoshibi"

/** A camera somebody can pick: the system's id, and which way it faces. */
data class CameraChoice(val id: String, val facing: Facing) {
    enum class Facing { Front, Back, External }

    /**
     * Whether one's own picture from this camera is shown mirrored
     * (live/facing.ts): a camera that films its owner is, one pointed at the
     * world is not, because flipping the world puts every sign and every page
     * the wrong way round.
     */
    val facesUser: Boolean get() = facing != Facing.Back
}

/**
 * The camera, sent to a video input (the Apple apps' CameraCapture).
 *
 * The size is the web's (share.rs `camera_capture`, room.ts ~562-566): a
 * phone sends 640x360 at twenty frames to spare its uplink, a tablet 1280x720
 * at thirty. Captured in YUV_420_888, whose planes the core copies into the
 * encoder's picture as they are; a camera that offers only twice the size
 * wanted is halved here on the way, as the Apple version scales.
 *
 * CameraX runs the camera against a lifecycle; this keeps its own, resumed
 * exactly while something is captured, so that a call's camera does not stop
 * and start with whichever screen happens to be showing.
 *
 * Every method is for the main thread.
 */
class CameraCapture(context: Context) {
    private val context = context.applicationContext
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "tomoshibi-camera") }
    private val lifecycle = Owner()
    private var provider: ProcessCameraProvider? = null
    @Volatile private var input: VideoInput? = null
    private var analysis: ImageAnalysis? = null
    private var preview: Preview? = null
    private var previewSurface: Preview.SurfaceProvider? = null
    /** The use cases this capture has bound, which are all it may unbind. */
    private var own: List<androidx.camera.core.UseCase> = emptyList()
    private var orientation: OrientationEventListener? = null
    /** The bound camera's state, watched for a camera taken away under the call. */
    private var watched: androidx.lifecycle.LiveData<CameraState>? = null

    /** What this device sends, by the web's rule for its kind. */
    val size = cameraCapture(
        if (context.resources.configuration.smallestScreenWidthDp >= 600) CameraDevice.TABLET else CameraDevice.PHONE,
    )
    val width: UInt get() = size.width
    val height: UInt get() = size.height

    /**
     * The camera picked for the next time the camera goes on: one chosen from
     * the devices panel while the camera was off, or remembered from an
     * earlier call (`meet-live.device-choice`).
     */
    var preferred: String? = null

    /** The camera that is sending, if one is. */
    var device: CameraChoice? = null
        private set

    /** Whether frames are being taken, for a call or a preview. */
    val running: Boolean get() = device != null

    /**
     * Whether the background behind the person is blurred in what is sent
     * (BackgroundBlur), remembered by the call as the web remembers it
     * (`meet-live.blur`). The segmenter exists only while this is on.
     *
     * The picture of oneself drawn from the camera's own preview is not
     * blurred: CameraX hands the preview to its surface before any frame
     * reaches the analyser. A self view that shows the blur is a view drawn
     * from what is sent, which is the room view's to choose.
     */
    @Volatile var blurring: Boolean = false
        set(value) {
            if (value == field) return
            field = value
            if (value) {
                blur = BackgroundBlur(context).also { made ->
                    made.onFailure = { fault -> android.os.Handler(android.os.Looper.getMainLooper()).post { onBlurFailed?.invoke(fault) } }
                }
                Log.i(TAG, "blur on")
            } else {
                blur?.close()
                blur = null
                Log.i(TAG, "blur off")
            }
        }
    @Volatile private var blur: BackgroundBlur? = null

    /** Told on the main thread when blur cannot be done here (the segmenter would not start). */
    var onBlurFailed: ((Throwable) -> Unit)? = null

    /** Told on the main thread when a camera that was running stops under the call (another app took it). */
    var onFailure: ((Throwable) -> Unit)? = null

    /** Frames sent since the capture was made, for a debug build's log. */
    @Volatile internal var sent = 0
        private set

    /** The cameras somebody can pick, front first ([cameras]). */
    fun cameras(): List<CameraChoice> = cameras(context)

    /** The camera that is sending, or that would be sent if the camera went on now. */
    val current: String? get() = current(cameras())

    /**
     * Which of [present] is sending, or would be sent if the camera went on
     * now: what the devices panel ticks. Asks nothing of the camera service,
     * so the panel can tick from a list it read away from the main thread.
     */
    fun current(present: List<CameraChoice>): String? {
        device?.let { return it.id }
        preferred?.let { id -> if (present.any { it.id == id }) return id }
        return fallback(present)?.id
    }

    companion object {
        /**
         * The cameras somebody can pick, front first. One question to the
         * camera service for each camera, its whole description each time
         * (a few milliseconds apiece on a phone with several): safe from any
         * thread, and for the devices panel read off the main one.
         */
        fun cameras(context: Context): List<CameraChoice> {
            val manager = context.getSystemService(CameraManager::class.java) ?: return emptyList()
            return try {
                manager.cameraIdList.map { id ->
                    val facing = when (manager.getCameraCharacteristics(id).get(CameraCharacteristics.LENS_FACING)) {
                        CameraCharacteristics.LENS_FACING_FRONT -> CameraChoice.Facing.Front
                        CameraCharacteristics.LENS_FACING_BACK -> CameraChoice.Facing.Back
                        else -> CameraChoice.Facing.External
                    }
                    CameraChoice(id, facing)
                }.sortedBy { it.facing.ordinal }
            } catch (fault: Exception) {
                Log.w(TAG, "the cameras could not be listed: ${fault.message}")
                emptyList()
            }
        }

        /** The camera used when nothing was picked: the front one, as on a phone everywhere. */
        fun fallback(present: List<CameraChoice>): CameraChoice? =
            present.firstOrNull { it.facing == CameraChoice.Facing.Front } ?: present.firstOrNull()
    }

    /**
     * Starts sending [chosen] (or the preferred camera, or the front one) to
     * [into]; with no input, only the preview runs (the join screen's picture
     * of yourself). Throws where there is no camera or it would not open.
     */
    suspend fun start(chosen: String? = null, into: VideoInput? = null) {
        val present = cameras()
        val camera = chosen?.let { id -> present.firstOrNull { it.id == id } }
            ?: preferred?.let { id -> present.firstOrNull { it.id == id } }
            ?: fallback(present)
            ?: throw IllegalStateException("there is no camera")
        val provider = provider ?: awaitProvider().also { provider = it }
        val selector = CameraSelector.Builder()
            .addCameraFilter { infos -> infos.filter { it.cameraIdentifier?.cameraIds?.firstOrNull() == camera.id } }
            .build()
        input = into
        val analysis = analysis ?: makeAnalysis().also { analysis = it }
        val preview = previewSurface?.let { surface -> (preview ?: Preview.Builder().build().also { preview = it }).also { it.surfaceProvider = surface } }
        unbindOwn(provider)
        lifecycle.resume()
        val cases = listOfNotNull(analysis, preview)
        val bound = provider.bindToLifecycle(lifecycle, selector, *cases.toTypedArray())
        own = cases
        // A camera another app holds, or one the system takes away mid-call,
        // is said by the camera's state rather than by the bind.
        watched?.removeObservers(lifecycle)
        watched = bound.cameraInfo.cameraState
        bound.cameraInfo.cameraState.observe(lifecycle) { state ->
            val error = state.error ?: return@observe
            if (state.type == CameraState.Type.CLOSED || error.type == CameraState.ErrorType.CRITICAL) {
                Log.w(TAG, "the camera failed: code ${error.code}")
                onFailure?.invoke(IllegalStateException("the camera failed (${error.code})"))
            }
        }
        follow(analysis)
        device = camera
        preferred = camera.id
        Log.i(TAG, "camera ${camera.id} (${camera.facing}) on, sending ${if (into != null) "${width}x$height" else "nothing"}")
    }

    /**
     * Where one's own picture is drawn while the camera runs (a PreviewView's
     * surfaceProvider, or any of one's own); null stops drawing it. Taken by
     * the next start, and at once by a camera already running.
     */
    suspend fun showPreview(surface: Preview.SurfaceProvider?) {
        previewSurface = surface
        val camera = device ?: return
        start(camera.id, input)
    }

    fun stop() {
        input = null
        watched?.removeObservers(lifecycle)
        watched = null
        orientation?.disable()
        orientation = null
        provider?.let(::unbindOwn)
        lifecycle.pause()
        device = null
    }

    /**
     * Takes this capture's own use cases off the camera, and nothing else.
     * The provider is the process's, shared with every other capture: it
     * unbound everything once, and a join screen's picture of oneself
     * stopped a moment after the call's camera started took the call's
     * camera away with it. Two captures may hold the camera at once that
     * way, each on its own lifecycle; CameraX runs the one started last and
     * gives the camera back to the other when that one stops.
     */
    private fun unbindOwn(provider: ProcessCameraProvider) {
        if (own.isEmpty()) return
        provider.unbind(*own.toTypedArray())
        own = emptyList()
    }

    /** Stops for good: the call is over. */
    fun close() {
        stop()
        blurring = false
        lifecycle.destroy()
        worker.shutdown()
    }

    private suspend fun awaitProvider(): ProcessCameraProvider = suspendCancellableCoroutine { continuation ->
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            try {
                continuation.resume(future.get())
            } catch (fault: Exception) {
                continuation.resumeWithException(fault)
            }
        }, ContextCompat.getMainExecutor(context))
    }

    private fun makeAnalysis(): ImageAnalysis {
        val wanted = Size(width.toInt(), height.toInt())
        val made = ImageAnalysis.Builder()
            .setResolutionSelector(
                ResolutionSelector.Builder()
                    .setAspectRatioStrategy(AspectRatioStrategy.RATIO_16_9_FALLBACK_AUTO_STRATEGY)
                    .setResolutionStrategy(ResolutionStrategy(wanted, ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER))
                    .build(),
            )
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_YUV_420_888)
            .build()
        made.setAnalyzer(worker, Analyzer())
        return made
    }

    /**
     * Keeps the picture upright however the phone is held: the app does not
     * turn its window with the phone, so the frames' rotation follows the
     * phone's own orientation rather than the display's.
     */
    private fun follow(analysis: ImageAnalysis) {
        orientation?.disable()
        orientation = object : OrientationEventListener(context) {
            override fun onOrientationChanged(degrees: Int) {
                if (degrees == ORIENTATION_UNKNOWN) return
                analysis.targetRotation = when (degrees) {
                    in 45 until 135 -> Surface.ROTATION_270
                    in 135 until 225 -> Surface.ROTATION_180
                    in 225 until 315 -> Surface.ROTATION_90
                    else -> Surface.ROTATION_0
                }
            }
        }.also { if (it.canDetectOrientation()) it.enable() }
    }

    /** Each frame's planes to the core, no faster than the web's frame rate for this device. */
    private inner class Analyzer : ImageAnalysis.Analyzer {
        /** When the next frame is due, on the camera's own clock. */
        private var due = 0L
        private var frames = 0
        private val interval = 1_000_000_000L / size.frameRate.toLong().coerceAtLeast(1)
        private var y = ByteArray(0)
        private var u = ByteArray(0)
        private var v = ByteArray(0)
        private var whole = ByteArray(0)

        override fun analyze(image: ImageProxy) {
            try {
                val input = input ?: return
                val stamp = image.imageInfo.timestamp
                // Paced by a deadline that moves on by the interval rather
                // than by the gap since the last frame taken: a camera at
                // thirty taking every frame fifty milliseconds after the last
                // took every other one, and sent fifteen rather than twenty.
                // A little early is taken, for the camera's own jitter.
                if (stamp < due - interval / 10) return
                due = if (stamp - due > interval) stamp + interval else due + interval
                val (py, pu, pv) = image.planes
                val halve = image.width >= width.toInt() * 2 && image.height >= height.toInt() * 2
                val rotation = image.imageInfo.rotationDegrees
                // Blurred on packed planes, which the halving already makes;
                // without blur the planes go as the camera laid them out.
                val blur = blur
                if (halve) {
                    val w = image.width / 2
                    val h = image.height / 2
                    y = halved(py, image.width, image.height, 1, y)
                    u = halved(pu, (image.width + 1) / 2, (image.height + 1) / 2, pu.pixelStride, u)
                    v = halved(pv, (image.width + 1) / 2, (image.height + 1) / 2, pv.pixelStride, v)
                    blur?.process(y, u, v, w, h, rotation, stamp)
                    input.pushPlanes(y, u, v, w.toUInt(), ((w + 1) / 2).toUInt(), 1u, w.toUInt(), h.toUInt(),
                        stamp / 1000, rotation.toUInt())
                } else if (blur != null) {
                    val w = image.width
                    val h = image.height
                    y = packed(py, w, h, y)
                    u = packed(pu, (w + 1) / 2, (h + 1) / 2, u)
                    v = packed(pv, (w + 1) / 2, (h + 1) / 2, v)
                    blur.process(y, u, v, w, h, rotation, stamp)
                    input.pushPlanes(y, u, v, w.toUInt(), ((w + 1) / 2).toUInt(), 1u, w.toUInt(), h.toUInt(),
                        stamp / 1000, rotation.toUInt())
                } else {
                    y = copied(py, y)
                    u = copied(pu, u)
                    v = copied(pv, v)
                    input.pushPlanes(y, u, v, py.rowStride.toUInt(), pu.rowStride.toUInt(), pu.pixelStride.toUInt(),
                        image.width.toUInt(), image.height.toUInt(), stamp / 1000, image.imageInfo.rotationDegrees.toUInt())
                }
                sent = frames + 1
                if (++frames % 150 == 0) {
                    Log.i(TAG, "camera sent $frames frames ${image.width}x${image.height}${if (halve) " halved" else ""} turned ${image.imageInfo.rotationDegrees}")
                }
            } catch (fault: Exception) {
                Log.w(TAG, "a camera frame was not sent: ${fault.message}")
            } finally {
                image.close()
            }
        }

        /** The plane's bytes into a buffer kept between frames: the core copies them across, so one buffer serves. */
        private fun copied(plane: ImageProxy.PlaneProxy, reuse: ByteArray): ByteArray {
            val buffer = plane.buffer
            buffer.rewind()
            val out = if (reuse.size == buffer.remaining()) reuse else ByteArray(buffer.remaining())
            buffer.get(out)
            return out
        }

        /** A plane as it is, packed (stride = width, step one), for the blur to work on. */
        private fun packed(plane: ImageProxy.PlaneProxy, planeWidth: Int, planeHeight: Int, reuse: ByteArray): ByteArray {
            whole = copied(plane, whole)
            val source = whole
            val stride = plane.rowStride
            val step = plane.pixelStride
            val out = if (reuse.size == planeWidth * planeHeight) reuse else ByteArray(planeWidth * planeHeight)
            for (row in 0 until planeHeight) {
                val from = row * stride
                val to = row * planeWidth
                if (step == 1) {
                    System.arraycopy(source, from, out, to, minOf(planeWidth, source.size - from))
                } else {
                    for (column in 0 until planeWidth) {
                        val at = from + column * step
                        if (at < source.size) out[to + column] = source[at]
                    }
                }
            }
            return out
        }

        /**
         * A plane at half its width and height, each sample the mean of the
         * four it stands for, packed (stride = width, step one): a camera
         * that offers 720p to a phone that sends 360p.
         */
        private fun halved(plane: ImageProxy.PlaneProxy, planeWidth: Int, planeHeight: Int, step: Int, reuse: ByteArray): ByteArray {
            // Read out of the direct buffer once, and worked on as an array:
            // a read per sample through the buffer was most of the frame's time.
            whole = copied(plane, whole)
            val source = whole
            val stride = plane.rowStride
            val w = planeWidth / 2
            val h = planeHeight / 2
            val out = if (reuse.size == w * h) reuse else ByteArray(w * h)
            for (row in 0 until h) {
                val top = row * 2 * stride
                val bottom = top + stride
                for (column in 0 until w) {
                    val left = column * 2 * step
                    val right = minOf(left + step, source.size - 1 - bottom)
                    val sum = (source[top + left].toInt() and 0xff) + (source[top + right].toInt() and 0xff) +
                        (source[bottom + left].toInt() and 0xff) + (source[bottom + right].toInt() and 0xff)
                    out[row * w + column] = ((sum + 2) / 4).toByte()
                }
            }
            return out
        }
    }

    /** A lifecycle resumed exactly while the camera is wanted. */
    private class Owner : LifecycleOwner {
        private val registry = LifecycleRegistry(this)
        override val lifecycle: Lifecycle get() = registry

        init {
            registry.currentState = Lifecycle.State.CREATED
        }

        fun resume() {
            registry.currentState = Lifecycle.State.RESUMED
        }

        fun pause() {
            if (registry.currentState.isAtLeast(Lifecycle.State.CREATED)) registry.currentState = Lifecycle.State.CREATED
        }

        fun destroy() {
            registry.currentState = Lifecycle.State.DESTROYED
        }
    }
}
