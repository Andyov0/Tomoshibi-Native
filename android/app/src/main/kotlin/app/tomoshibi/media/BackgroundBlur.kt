package app.tomoshibi.media

import android.content.Context
import android.util.Log
import com.google.mediapipe.framework.image.ByteBufferExtractor
import com.google.mediapipe.framework.image.ByteBufferImageBuilder
import com.google.mediapipe.framework.image.MPImage
import com.google.mediapipe.tasks.core.BaseOptions
import com.google.mediapipe.tasks.core.Delegate
import com.google.mediapipe.tasks.vision.core.RunningMode
import com.google.mediapipe.tasks.vision.imagesegmenter.ImageSegmenter
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

private const val TAG = "Tomoshibi"

/**
 * The background behind somebody blurred, frame by frame, before the frame is
 * sent (live/blur.ts; the Apple apps' BackgroundBlur).
 *
 * The person is found by the web's own segmenter: MediaPipe's selfie model,
 * the very file the web serves from its deployment (web/public/segment), run
 * by MediaPipe's tasks library, so what is kept and what is blurred is the
 * same on a phone as in a browser. The blur and the blend are BlurMath's.
 *
 * Finding the person takes longer than a frame has on a slow phone, so it
 * runs on a thread of its own and never holds a frame up: each frame is
 * blended through the newest mask there is, and the segmenter is handed the
 * next frame whenever it is free. A mask a frame or two old trails a fast
 * movement by that much at the edge; a camera that drops to the segmenter's
 * pace would be worse everywhere. Until the first mask, the whole frame is
 * blurred rather than none of it.
 *
 * Made when blur is turned on and closed when it is turned off, so that a
 * camera without blur pays nothing for it, not even the model in memory.
 * [process] is for the camera's thread; the rest for any.
 */
internal class BackgroundBlur(context: Context, private val delegate: Delegate = Delegate.CPU) {
    private val context = context.applicationContext
    private val worker: ExecutorService = Executors.newSingleThreadExecutor { Thread(it, "tomoshibi-segment") }
    /** Made on the worker, used only there. */
    private var segmenter: ImageSegmenter? = null
    private val busy = AtomicBoolean(false)
    @Volatile private var closed = false
    @Volatile private var failed = false

    /** The newest mask, on the grid of the frame it was found in. */
    private class Person(val width: Int, val height: Int, val rotation: Int, val grid: FloatArray)
    @Volatile private var person: Person? = null

    /** Camera thread only. */
    private var math: BlurMath? = null
    private var small: ByteBuffer? = null
    private var lastStamp = 0L
    private var frames = 0
    private var spent = 0L

    /** Told, on the segmenter's thread, when the segmenter would not start: the frame cannot be kept. */
    var onFailure: ((Throwable) -> Unit)? = null

    /**
     * Blurs what is behind the person in the packed planes, in place: [y]
     * of [width] by [height], [u] and [v] at half that, captured
     * [rotation] clockwise degrees from upright, at [stampNanos].
     */
    fun process(y: ByteArray, u: ByteArray, v: ByteArray, width: Int, height: Int, rotation: Int, stampNanos: Long) {
        val began = System.nanoTime()
        val math = math?.takeIf { it.width == width && it.height == height } ?: BlurMath(width, height).also { math = it }
        if (!failed && !closed && busy.compareAndSet(false, true)) {
            val (sw, sh, buffer) = uprightSmall(y, u, v, width, height, rotation)
            val stamp = maxOf(stampNanos / 1_000_000L, lastStamp + 1)
            lastStamp = stamp
            val gridWidth = math.gridWidth
            val gridHeight = math.gridHeight
            try {
                worker.execute {
                    try {
                        segment(buffer, sw, sh, stamp)?.let { mask ->
                            person = Person(width, height, rotation,
                                BlurMath.personOnGrid(mask, sw, sh, width, height, rotation, gridWidth, gridHeight))
                        }
                    } finally {
                        busy.set(false)
                    }
                }
            } catch (_: java.util.concurrent.RejectedExecutionException) {
                // Turned off between the look and the hand-over.
                busy.set(false)
            }
        }
        val found = person?.takeIf { it.width == width && it.height == height && it.rotation == rotation }
        math.blur(y, u, v, found?.grid)
        spent += System.nanoTime() - began
        if (++frames % 150 == 0) {
            Log.i(TAG, "blur: ${"%.1f".format(spent / 150 / 1e6)} ms a frame on the camera's thread, ${if (found != null) "a person found" else "nobody found yet"}")
            spent = 0
        }
    }

    /**
     * The frame upright and small, as RGBA, for the segmenter: 256 on its
     * longer side, which is the model's own size, so nothing more is
     * converted than it looks at. One buffer, reused: a frame is handed over
     * only while the segmenter is free, and it is done with this one by then.
     */
    private fun uprightSmall(y: ByteArray, u: ByteArray, v: ByteArray, width: Int, height: Int, rotation: Int): Triple<Int, Int, ByteBuffer> {
        val turned = rotation == 90 || rotation == 270
        val uw = if (turned) height else width
        val uh = if (turned) width else height
        val scale = MODEL_SIDE.toDouble() / maxOf(uw, uh)
        val sw = maxOf(1, (uw * scale).toInt())
        val sh = maxOf(1, (uh * scale).toInt())
        val buffer = small?.takeIf { it.capacity() == sw * sh * 4 }
            ?: ByteBuffer.allocateDirect(sw * sh * 4).order(ByteOrder.nativeOrder()).also { small = it }
        buffer.clear()
        val cw = (width + 1) / 2
        for (sy in 0 until sh) {
            val uy = sy * uh / sh
            for (sx in 0 until sw) {
                val ux = sx * uw / sw
                val (x, yy) = BlurMath.captured(ux, uy, width, height, rotation)
                val luma = (y[yy * width + x].toInt() and 0xff).toFloat()
                val c = (yy / 2) * cw + x / 2
                val cb = (u[c].toInt() and 0xff) - 128f
                val cr = (v[c].toInt() and 0xff) - 128f
                // A camera's YUV is full range (JFIF), BT.601.
                buffer.put((luma + 1.402f * cr).toInt().coerceIn(0, 255).toByte())
                buffer.put((luma - 0.344136f * cb - 0.714136f * cr).toInt().coerceIn(0, 255).toByte())
                buffer.put((luma + 1.772f * cb).toInt().coerceIn(0, 255).toByte())
                buffer.put(255.toByte())
            }
        }
        buffer.rewind()
        return Triple(sw, sh, buffer)
    }

    /** The person's share of each pixel of the small picture, or null where it could not be found. On the worker. */
    private fun segment(buffer: ByteBuffer, width: Int, height: Int, stamp: Long): FloatArray? {
        if (closed || failed) return null
        if (segmenter == null && delegate == Delegate.CPU && emulatedWithoutSve()) {
            failed = true
            Log.e(TAG, "blur: not on an emulator whose processor claims SVE it cannot run")
            onFailure?.invoke(UnsupportedOperationException("segmentation on this emulator"))
            return null
        }
        val segmenter = segmenter ?: try {
            ImageSegmenter.createFromOptions(context, ImageSegmenter.ImageSegmenterOptions.builder()
                .setBaseOptions(BaseOptions.builder().setModelAssetPath(MODEL).setDelegate(delegate).build())
                .setRunningMode(RunningMode.VIDEO)
                .setOutputConfidenceMasks(true)
                .setOutputCategoryMask(false)
                .build()).also {
                segmenter = it
                Log.i(TAG, "blur: the segmenter is ready")
            }
        } catch (fault: Throwable) {
            failed = true
            Log.e(TAG, "blur: the segmenter would not start: ${fault.message}")
            onFailure?.invoke(fault)
            return null
        }
        val image = ByteBufferImageBuilder(buffer, width, height, MPImage.IMAGE_FORMAT_RGBA).build()
        return try {
            val result = segmenter.segmentForVideo(image, stamp)
            val masks = result.confidenceMasks().orElse(null) ?: return null
            // The selfie model's one output is the person's confidence.
            val mask = masks.firstOrNull() ?: return null
            val floats = ByteBufferExtractor.extract(mask).order(ByteOrder.nativeOrder()).asFloatBuffer()
            val out = FloatArray(minOf(floats.remaining(), mask.width * mask.height))
            floats.get(out)
            for (m in masks) m.close()
            if (mask.width != width || mask.height != height) {
                Log.w(TAG, "blur: a mask of ${mask.width}x${mask.height} for a picture of ${width}x$height")
                return null
            }
            out
        } catch (fault: Exception) {
            Log.w(TAG, "blur: a frame was not segmented: ${fault.message}")
            null
        } finally {
            image.close()
        }
    }

    /** Lets the segmenter and its model go, once it has finished with any frame it has. */
    fun close() {
        if (closed) return
        closed = true
        person = null
        worker.execute {
            segmenter?.close()
            segmenter = null
        }
        worker.shutdown()
    }

    companion object {
        /**
         * An emulator on an Apple M4 or later, whose virtual processor says it
         * has SVE because the host has SME, and cannot run SVE outside SME's
         * streaming mode: MediaPipe's CPU kernels pick SVE from what the
         * processor says, and the whole app died of SIGILL in the segmenter's
         * thread the moment blur was turned on (an M5 Max, 2026-10-18). A
         * phone that says SVE has it.
         */
        private fun emulatedWithoutSve(): Boolean {
            if (android.os.Build.HARDWARE != "ranchu") return false
            // Listed as sve2, svei8mm and the rest; there is no plain "sve".
            return try {
                java.io.File("/proc/cpuinfo").readLines().firstOrNull { it.startsWith("Features") }
                    ?.split(' ', '\t')?.any { it.startsWith("sve") } == true
            } catch (_: Exception) {
                false
            }
        }

        /** The web's model, at the same path under the app's assets as under the deployment's (`/segment/...`). */
        const val MODEL = "segment/selfie_segmenter.tflite"
        const val MODEL_SIDE = 256
    }
}
