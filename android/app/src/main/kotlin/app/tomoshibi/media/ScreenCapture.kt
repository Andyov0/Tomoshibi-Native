package app.tomoshibi.media

import android.content.Context
import android.graphics.SurfaceTexture
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.projection.MediaProjection
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLSurface
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.Process
import android.util.Log
import android.view.Display
import android.view.Surface
import android.view.WindowManager
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.Callable
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import uniffi.tomoshibi_core.VideoInput
import uniffi.tomoshibi_protocol.ShareQuality
import uniffi.tomoshibi_protocol.shareCaptureRequest

private const val TAG = "Tomoshibi"

/**
 * The screen, mirrored through a projection into a video input (the Apple
 * apps' ScreenCapture, ScreenCaptureKit's part of it).
 *
 * The core takes a picture as Y, U and V planes in memory (VideoInput
 * `pushPlanes`), which its encoder copies into an I420 picture: on Android
 * WebRTC's encoder factory is made without an EGL context, so no texture can
 * be handed to it. A virtual display gives RGBA, and turning RGBA into I420
 * on the CPU is the expensive part: 2.5 million pixels a frame for a phone's
 * screen at sixty frames, nine bytes read and written for each. So the
 * conversion is done by the GPU, as WebRTC's own YuvConverter does it: the
 * display draws into a SurfaceTexture, a shader writes the frame's luma and
 * its two half-size chroma planes into RGBA framebuffers, four samples to
 * a pixel, and the three planes are read back as they are, a byte and a half
 * a pixel, straight into the arrays handed to the core. The colour is
 * BT.601 at video range, which is what WebRTC takes an I420 picture to be.
 *
 * A virtual display draws only when the screen changes, and as often as the
 * screen refreshes when it does, up to 120 times a second. Frames are taken
 * no faster than the share's rate, on a deadline that moves on by the
 * interval; one that arrives early is held back and taken when it falls due
 * if nothing newer has come, so the last change before the screen goes still
 * is never lost. A still screen is sent again twice a second, so that a
 * person who arrives while nothing moves gets a picture to start from rather
 * than a black tile: a key frame is only ever made from a frame that comes.
 *
 * Each frame's work is on a thread of its own; [start], [retune], [resize]
 * and [stop] are for the main thread.
 */
internal class ScreenCapture(context: Context) {
    private val context = context.applicationContext
    private var thread: HandlerThread? = null
    private var gl: Handler? = null
    private var reader: PlaneReader? = null
    private var display: VirtualDisplay? = null
    private var displayListener: DisplayManager.DisplayListener? = null
    private val main = Handler(Looper.getMainLooper())

    /** Told on the main thread when the display stops under the share: the projection went away. */
    var onStopped: (() -> Unit)? = null

    /** The size being captured now, for a change of setting to start from. */
    var size: Pair<Int, Int> = 0 to 0
        private set
    /** What was being shared, at its own size, before it was fitted to a setting. */
    private var content: Pair<Int, Int> = 0 to 0
    private var quality: ShareQuality = ShareQuality.AUTO
    /** The rate the share is published at, which decides the size it is captured at (share.rs `share_capture_request`). */
    private var fps = 30
    /** The deployment's ceiling over the share (ShareCeiling), held to whatever size the capture is made at. */
    private var limits = ShareCeiling.NONE

    val running: Boolean get() = display != null

    /**
     * Mirrors the screen through [projection] into [video], at the size a
     * share published at [fps] is captured at, held to [limits], and no more
     * than [captured] frames a second, which is [fps] or less (Sharing
     * `plan`). Throws where the display or the GPU would not start; nothing
     * is left running then.
     */
    fun start(projection: MediaProjection, quality: ShareQuality, fps: Int, video: VideoInput, captured: Int = fps, limits: ShareCeiling = ShareCeiling.NONE) {
        stop()
        this.quality = quality
        this.fps = fps
        this.limits = limits
        content = screenSize(context)
        val (width, height) = bounded(content, quality, fps, limits)
        val thread = HandlerThread("tomoshibi-screen", Process.THREAD_PRIORITY_DISPLAY).also { it.start() }
        val gl = Handler(thread.looper)
        val reader = try {
            onThread(gl) { PlaneReader(gl, width, height, captured, video) }
        } catch (fault: Exception) {
            thread.quitSafely()
            throw fault
        }
        val display = try {
            projection.createVirtualDisplay(
                "Tomoshibi", width, height, context.resources.displayMetrics.densityDpi,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, reader.surface,
                object : VirtualDisplay.Callback() {
                    override fun onStopped() {
                        Log.i(TAG, "the shared screen's display stopped")
                        onStopped?.invoke()
                    }
                },
                main,
            )
        } catch (fault: Exception) {
            gl.post { reader.release() }
            thread.quitSafely()
            throw fault
        } ?: run {
            gl.post { reader.release() }
            thread.quitSafely()
            throw IllegalStateException("the screen's display was not made")
        }
        this.thread = thread
        this.gl = gl
        this.reader = reader
        this.display = display
        size = width to height
        followTurns()
        Log.i(TAG, "screen capture ${width}x$height at $captured fps from ${content.first}x${content.second}")
    }

    /**
     * The running capture at another setting, its pictures going to [video]
     * from the next frame: the share menu's size and rate changed mid-share,
     * and the uplink card's switch. The display is resized in place rather
     * than made again, because Android 14 allows one per consent, and a share
     * changed that way would need the person to be asked again.
     */
    fun retune(quality: ShareQuality, fps: Int, video: VideoInput, captured: Int = fps, limits: ShareCeiling = this.limits) {
        this.quality = quality
        this.fps = fps
        this.limits = limits
        val target = bounded(content, quality, fps, limits)
        resizeTo(target)
        val reader = reader ?: return
        gl?.post { reader.retune(captured, video) }
        Log.i(TAG, "screen capture now ${target.first}x${target.second} at $captured fps")
    }

    /**
     * The running capture's frames taken at [captured] a second from now,
     * into the same picture track at the same size: the device got hot or
     * cooled down (the deployment's screen_share_hot_fps), and nobody
     * watching is to see the share go and come back for it.
     */
    fun pace(captured: Int) {
        val reader = reader ?: return
        gl?.post { reader.pace(captured) }
        Log.i(TAG, "screen capture now at $captured fps")
    }

    /**
     * What is shared changed size: the phone was turned, or a shared app's
     * window was resized (Android 14 says so; before it the display is
     * watched). The capture follows it at the same setting, so a phone
     * turned on its side is sent on its side rather than as a small upright
     * picture in the middle of a black one.
     */
    fun resize(width: Int, height: Int) {
        if (width <= 0 || height <= 0 || display == null) return
        content = width to height
        resizeTo(bounded(content, quality, fps, limits))
    }

    /** What the setting asks for of what is shared, held to [limits]: the size the capture would be made at now. */
    fun fitted(quality: ShareQuality, fps: Int, limits: ShareCeiling = this.limits): Pair<Int, Int> = bounded(content, quality, fps, limits)

    private fun bounded(content: Pair<Int, Int>, quality: ShareQuality, fps: Int, limits: ShareCeiling): Pair<Int, Int> {
        val (width, height) = fit(content, quality, fps)
        return limits.size(width, height)
    }

    /** The last picture sent, as the core was handed it, for a debug build to look at. */
    internal fun picture(): PlaneReader.Picture? {
        val reader = reader ?: return null
        return onThread(gl ?: return null) { reader.picture() }
    }

    private fun resizeTo(target: Pair<Int, Int>) {
        val display = display ?: return
        if (target == size) return
        size = target
        val (width, height) = target
        val reader = reader
        if (reader != null) onThread(gl ?: return) { reader.resize(width, height) }
        display.resize(width, height, context.resources.displayMetrics.densityDpi)
    }

    fun stop() {
        displayListener?.let { context.getSystemService(DisplayManager::class.java)?.unregisterDisplayListener(it) }
        displayListener = null
        display?.release()
        display = null
        val reader = reader
        val thread = thread
        this.reader = null
        this.gl?.post {
            reader?.release()
            thread?.quitSafely()
        }
        this.gl = null
        this.thread = null
        size = 0 to 0
    }

    /**
     * Before Android 14 nothing says the screen turned, so the display it
     * mirrors is watched; from 14 the projection says so itself, for an
     * app's window as well as the screen (ScreenProjection).
     */
    private fun followTurns() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) return
        val manager = context.getSystemService(DisplayManager::class.java) ?: return
        val listener = object : DisplayManager.DisplayListener {
            override fun onDisplayAdded(displayId: Int) {}
            override fun onDisplayRemoved(displayId: Int) {}
            override fun onDisplayChanged(displayId: Int) {
                if (displayId != Display.DEFAULT_DISPLAY) return
                val now = screenSize(context)
                if (now != content) resize(now.first, now.second)
            }
        }
        manager.registerDisplayListener(listener, main)
        displayListener = listener
    }

    companion object {
        /**
         * The capture size for [content] at a setting: the content's own
         * pixels, scaled down so its shorter side is no more than the setting
         * asks (ScreenCapture.swift `size`), the setting's ask being the
         * core's (share.rs `share_capture_request`). Even sizes: the encoder
         * works in two-by-two blocks of chroma.
         */
        fun fit(content: Pair<Int, Int>, quality: ShareQuality, fps: Int): Pair<Int, Int> {
            val ask = shareCaptureRequest(fps.toUInt(), quality)
            val limit = minOf(ask.width, ask.height).toDouble()
            var (width, height) = content.first.toDouble() to content.second.toDouble()
            val shorter = minOf(width, height)
            if (shorter > limit) {
                width = width * limit / shorter
                height = height * limit / shorter
            }
            return (width.toInt() and 1.inv()).coerceAtLeast(2) to (height.toInt() and 1.inv()).coerceAtLeast(2)
        }

        /** The whole screen's pixels as it is held now, whatever window the app has. */
        fun screenSize(context: Context): Pair<Int, Int> {
            val windows = context.getSystemService(WindowManager::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && windows != null) {
                val bounds = windows.maximumWindowMetrics.bounds
                return bounds.width() to bounds.height()
            }
            val metrics = android.util.DisplayMetrics()
            @Suppress("DEPRECATION")
            context.getSystemService(DisplayManager::class.java)?.getDisplay(Display.DEFAULT_DISPLAY)?.getRealMetrics(metrics)
            return metrics.widthPixels to metrics.heightPixels
        }

        /** Runs [work] on [handler]'s thread and waits for it, its failure thrown here. */
        private fun <T> onThread(handler: Handler, work: () -> T): T {
            if (Looper.myLooper() == handler.looper) return work()
            val task = FutureTask(Callable { work() })
            handler.post(task)
            return try {
                task.get(5, TimeUnit.SECONDS)
            } catch (fault: java.util.concurrent.ExecutionException) {
                throw fault.cause ?: fault
            }
        }
    }
}

/**
 * The GPU's half of the screen capture: an EGL context of its own, the
 * SurfaceTexture the display draws into, and the shader that turns each
 * frame into three planes. On its thread alone.
 */
internal class PlaneReader(
    private val handler: Handler,
    width: Int,
    height: Int,
    fps: Int,
    @Volatile private var video: VideoInput?,
) : SurfaceTexture.OnFrameAvailableListener {
    private var width = width
    private var height = height
    private var interval = 1_000_000_000L / fps.coerceAtLeast(1)

    private val eglDisplay: EGLDisplay
    private val eglContext: EGLContext
    private val eglSurface: EGLSurface
    private val texture: Int
    private val texture2: SurfaceTexture
    val surface: Surface
    private val program: Int
    private val transform = FloatArray(16)
    /** Luma's framebuffer and texture, then chroma's. */
    private val framebuffers = IntArray(2)
    private val targets = IntArray(2)

    // The planes, read straight into the arrays that are handed to the core,
    // through wrappers made once for each array rather than three a frame.
    private var stride = 0
    private var y = ByteArray(0)
    private var u = ByteArray(0)
    private var v = ByteArray(0)
    private var yBuffer: ByteBuffer = ByteBuffer.wrap(y)
    private var uBuffer: ByteBuffer = ByteBuffer.wrap(u)
    private var vBuffer: ByteBuffer = ByteBuffer.wrap(v)
    private var hasFrame = false

    // Where the shaders' inputs are, looked up once: by name, every frame,
    // was six questions to the driver a frame for answers that never change.
    private var pictureAt = 0
    private var transformAt = 0
    private var widthScaleAt = 0
    private var stepAt = 0
    private var coefficientsAt = 0
    private var positionAt = 0

    /** When the next frame is due, on the display's clock. */
    private var due = 0L
    private var held = false
    private var lastSent = 0L
    private var sent = 0
    private val quad: ByteBuffer = ByteBuffer.allocateDirect(8 * 4).order(ByteOrder.nativeOrder()).also {
        it.asFloatBuffer().put(floatArrayOf(-1f, -1f, 1f, -1f, -1f, 1f, 1f, 1f))
    }

    init {
        eglDisplay = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
        check(eglDisplay != EGL14.EGL_NO_DISPLAY) { "no EGL display" }
        val version = IntArray(2)
        check(EGL14.eglInitialize(eglDisplay, version, 0, version, 1)) { "EGL would not start" }
        val configs = arrayOfNulls<EGLConfig>(1)
        val count = IntArray(1)
        val attributes = intArrayOf(
            EGL14.EGL_RED_SIZE, 8, EGL14.EGL_GREEN_SIZE, 8, EGL14.EGL_BLUE_SIZE, 8, EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT, EGL14.EGL_SURFACE_TYPE, EGL14.EGL_PBUFFER_BIT,
            EGL14.EGL_NONE,
        )
        check(EGL14.eglChooseConfig(eglDisplay, attributes, 0, configs, 0, 1, count, 0) && count[0] > 0) { "no EGL configuration" }
        eglContext = EGL14.eglCreateContext(eglDisplay, configs[0], EGL14.EGL_NO_CONTEXT,
            intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE), 0)
        check(eglContext != EGL14.EGL_NO_CONTEXT) { "no EGL context" }
        eglSurface = EGL14.eglCreatePbufferSurface(eglDisplay, configs[0], intArrayOf(EGL14.EGL_WIDTH, 1, EGL14.EGL_HEIGHT, 1, EGL14.EGL_NONE), 0)
        check(EGL14.eglMakeCurrent(eglDisplay, eglSurface, eglSurface, eglContext)) { "EGL would not make its context current" }

        val names = IntArray(1)
        GLES20.glGenTextures(1, names, 0)
        texture = names[0]
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, texture)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
        texture2 = SurfaceTexture(texture)
        texture2.setDefaultBufferSize(width, height)
        texture2.setOnFrameAvailableListener(this, handler)
        surface = Surface(texture2)
        program = link(VERTEX, FRAGMENT)
        pictureAt = GLES20.glGetUniformLocation(program, "uPicture")
        transformAt = GLES20.glGetUniformLocation(program, "uTransform")
        widthScaleAt = GLES20.glGetUniformLocation(program, "uWidthScale")
        stepAt = GLES20.glGetUniformLocation(program, "uStep")
        coefficientsAt = GLES20.glGetUniformLocation(program, "uCoefficients")
        positionAt = GLES20.glGetAttribLocation(program, "aPosition")
        allocate()
    }

    fun retune(fps: Int, video: VideoInput) {
        interval = 1_000_000_000L / fps.coerceAtLeast(1)
        this.video = video
    }

    /** Frames taken at [fps] a second from now, into the same track. */
    fun pace(fps: Int) {
        interval = 1_000_000_000L / fps.coerceAtLeast(1)
    }

    /** One picture's planes as they were sent: luma rows of [stride], chroma rows of half that. */
    class Picture(val width: Int, val height: Int, val stride: Int, val y: ByteArray, val u: ByteArray, val v: ByteArray)

    fun picture(): Picture? = if (!hasFrame) null else Picture(width, height, stride, y.copyOf(), u.copyOf(), v.copyOf())

    fun resize(width: Int, height: Int) {
        this.width = width
        this.height = height
        texture2.setDefaultBufferSize(width, height)
        allocate()
        hasFrame = false
    }

    /**
     * The framebuffers the planes are drawn into, and the arrays they are
     * read into: luma at full size in rows of [stride] bytes, a multiple of
     * eight so that the chroma planes' rows are a whole number of pixels of
     * four samples each; and the two chroma planes side by side in a second.
     * Two rather than one stacked: a phone's screen held upright is over
     * three thousand rows, and half as many again would pass the 4096 that
     * older GPUs allow a texture.
     */
    private fun allocate() {
        if (framebuffers[0] != 0) {
            GLES20.glDeleteFramebuffers(2, framebuffers, 0)
            GLES20.glDeleteTextures(2, targets, 0)
        }
        stride = (width + 7) / 8 * 8
        val chromaHeight = (height + 1) / 2
        GLES20.glGenTextures(2, targets, 0)
        GLES20.glGenFramebuffers(2, framebuffers, 0)
        for ((index, rows) in listOf(height, chromaHeight).withIndex()) {
            GLES20.glBindTexture(GLES20.GL_TEXTURE_2D, targets[index])
            GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_NEAREST)
            GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_NEAREST)
            GLES20.glTexImage2D(GLES20.GL_TEXTURE_2D, 0, GLES20.GL_RGBA, stride / 4, rows, 0,
                GLES20.GL_RGBA, GLES20.GL_UNSIGNED_BYTE, null)
            GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffers[index])
            GLES20.glFramebufferTexture2D(GLES20.GL_FRAMEBUFFER, GLES20.GL_COLOR_ATTACHMENT0, GLES20.GL_TEXTURE_2D, targets[index], 0)
            val status = GLES20.glCheckFramebufferStatus(GLES20.GL_FRAMEBUFFER)
            check(status == GLES20.GL_FRAMEBUFFER_COMPLETE) { "the planes' framebuffer is incomplete ($status)" }
        }
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0)
        y = ByteArray(stride * height)
        u = ByteArray(stride / 2 * chromaHeight)
        v = ByteArray(stride / 2 * chromaHeight)
        yBuffer = ByteBuffer.wrap(y)
        uBuffer = ByteBuffer.wrap(u)
        vBuffer = ByteBuffer.wrap(v)
    }

    override fun onFrameAvailable(texture: SurfaceTexture) {
        // Every frame is taken off the queue, or the display stops drawing
        // once its few buffers are full; only those that fall due are read.
        try {
            texture.updateTexImage()
        } catch (fault: Exception) {
            Log.w(TAG, "a screen frame could not be taken: ${fault.message}")
            return
        }
        val stamp = texture.timestamp
        if (stamp < due - interval / 10) {
            if (!held) {
                held = true
                handler.postDelayed(late, ((due - stamp) / 1_000_000L).coerceAtLeast(1))
            }
            return
        }
        due = if (stamp - due > interval) stamp + interval else due + interval
        send(stamp)
    }

    /** The frame held back as too early, taken now it is due, if nothing newer took its place. */
    private val late = Runnable {
        if (!held) return@Runnable
        held = false
        val stamp = System.nanoTime()
        due = stamp + interval
        send(stamp)
    }

    /** The last picture again, while the screen is still. */
    private val still = object : Runnable {
        override fun run() {
            val now = System.nanoTime()
            if (hasFrame && now - lastSent >= STILL_NANOS) push(now)
            handler.postDelayed(this, STILL_NANOS / 1_000_000L)
        }
    }

    init {
        handler.postDelayed(still, STILL_NANOS / 1_000_000L)
    }

    private fun send(stamp: Long) {
        held = false
        handler.removeCallbacks(late)
        texture2.getTransformMatrix(transform)
        draw()
        hasFrame = true
        push(stamp)
    }

    private fun push(stamp: Long) {
        val video = video ?: return
        lastSent = System.nanoTime()
        video.pushPlanes(y, u, v, stride.toUInt(), (stride / 2).toUInt(), 1u, width.toUInt(), height.toUInt(), stamp / 1000, 0u)
        if (++sent % 300 == 0) Log.i(TAG, "screen sent $sent frames ${width}x$height")
    }

    /**
     * One frame into the three planes, and the planes into their arrays.
     *
     * All three planes are drawn before any is read. A read waits for the GPU
     * to finish everything asked of it so far; read between the passes, as
     * luma once was before chroma had been drawn, the thread waited for the
     * GPU twice a frame, sixty times a second, and the GPU sat idle while the
     * first plane was copied out.
     */
    private fun draw() {
        val chromaHeight = (height + 1) / 2
        GLES20.glUseProgram(program)
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, texture)
        GLES20.glUniform1i(pictureAt, 0)
        GLES20.glUniformMatrix4fv(transformAt, 1, false, transform, 0)
        // The planes are rows of `stride` samples drawn across a picture
        // `width` wide: what the last few samples of a row read past the
        // picture's edge is padding nobody reads.
        GLES20.glUniform1f(widthScaleAt, stride.toFloat() / width)
        quad.position(0)
        GLES20.glVertexAttribPointer(positionAt, 2, GLES20.GL_FLOAT, false, 0, quad)
        GLES20.glEnableVertexAttribArray(positionAt)

        // Luma: each pixel of the framebuffer is four samples, a source pixel apart.
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffers[0])
        GLES20.glViewport(0, 0, stride / 4, height)
        GLES20.glUniform1f(stepAt, 1f / width)
        GLES20.glUniform4fv(coefficientsAt, 1, Y_COEFFICIENTS, 0)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
        // Chroma: four samples two source pixels apart, each sampled between
        // two pixels and two rows, which the texture's linear filter averages.
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffers[1])
        GLES20.glViewport(0, 0, stride / 8, chromaHeight)
        GLES20.glUniform1f(stepAt, 2f / width)
        GLES20.glUniform4fv(coefficientsAt, 1, U_COEFFICIENTS, 0)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
        GLES20.glViewport(stride / 8, 0, stride / 8, chromaHeight)
        GLES20.glUniform4fv(coefficientsAt, 1, V_COEFFICIENTS, 0)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
        GLES20.glDisableVertexAttribArray(positionAt)

        // Read into the arrays themselves (Android's GL bindings take an
        // array-backed buffer), so nothing is copied again on the way. The
        // first read waits for all three passes; the others find them done.
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffers[0])
        GLES20.glReadPixels(0, 0, stride / 4, height, GLES20.GL_RGBA, GLES20.GL_UNSIGNED_BYTE, yBuffer.rewind())
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffers[1])
        GLES20.glReadPixels(0, 0, stride / 8, chromaHeight, GLES20.GL_RGBA, GLES20.GL_UNSIGNED_BYTE, uBuffer.rewind())
        GLES20.glReadPixels(stride / 8, 0, stride / 8, chromaHeight, GLES20.GL_RGBA, GLES20.GL_UNSIGNED_BYTE, vBuffer.rewind())
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0)
        val error = GLES20.glGetError()
        if (error != GLES20.GL_NO_ERROR) Log.w(TAG, "the screen's planes were drawn with GL error $error")
    }

    fun release() {
        handler.removeCallbacks(still)
        handler.removeCallbacks(late)
        video = null
        texture2.setOnFrameAvailableListener(null)
        surface.release()
        texture2.release()
        GLES20.glDeleteFramebuffers(2, framebuffers, 0)
        GLES20.glDeleteTextures(2, targets, 0)
        GLES20.glDeleteTextures(1, intArrayOf(texture), 0)
        GLES20.glDeleteProgram(program)
        EGL14.eglMakeCurrent(eglDisplay, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)
        EGL14.eglDestroySurface(eglDisplay, eglSurface)
        EGL14.eglDestroyContext(eglDisplay, eglContext)
        EGL14.eglReleaseThread()
    }

    private fun link(vertex: String, fragment: String): Int {
        fun compile(type: Int, source: String): Int {
            val shader = GLES20.glCreateShader(type)
            GLES20.glShaderSource(shader, source)
            GLES20.glCompileShader(shader)
            val ok = IntArray(1)
            GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, ok, 0)
            check(ok[0] != 0) { "a screen shader would not compile: ${GLES20.glGetShaderInfoLog(shader)}" }
            return shader
        }
        val program = GLES20.glCreateProgram()
        GLES20.glAttachShader(program, compile(GLES20.GL_VERTEX_SHADER, vertex))
        GLES20.glAttachShader(program, compile(GLES20.GL_FRAGMENT_SHADER, fragment))
        GLES20.glLinkProgram(program)
        val ok = IntArray(1)
        GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, ok, 0)
        check(ok[0] != 0) { "the screen's shaders would not link: ${GLES20.glGetProgramInfoLog(program)}" }
        return program
    }

    companion object {
        const val STILL_NANOS = 500_000_000L

        // BT.601 at video range, as WebRTC's YuvConverter writes it: a
        // weight for each of red, green and blue, and an offset.
        val Y_COEFFICIENTS = floatArrayOf(0.256788f, 0.504129f, 0.0979059f, 0.0627451f)
        val U_COEFFICIENTS = floatArrayOf(-0.148223f, -0.290993f, 0.439216f, 0.501961f)
        val V_COEFFICIENTS = floatArrayOf(0.439216f, -0.367788f, -0.0714274f, 0.501961f)

        /**
         * Where in the picture each pixel of a plane falls, before the
         * SurfaceTexture's own transform: across from the left edge, and up
         * from the top, because the framebuffer's first row (the one read
         * first) is its bottom one and has to be the picture's top.
         */
        const val VERTEX = """
            attribute vec2 aPosition;
            uniform float uWidthScale;
            varying vec2 vPlace;
            void main() {
                vPlace = vec2((aPosition.x + 1.0) * 0.5 * uWidthScale, 1.0 - (aPosition.y + 1.0) * 0.5);
                gl_Position = vec4(aPosition, 0.0, 1.0);
            }
        """

        const val FRAGMENT = """
            #extension GL_OES_EGL_image_external : require
            #ifdef GL_FRAGMENT_PRECISION_HIGH
            precision highp float;
            #else
            precision mediump float;
            #endif
            varying vec2 vPlace;
            uniform samplerExternalOES uPicture;
            uniform mat4 uTransform;
            uniform float uStep;
            uniform vec4 uCoefficients;
            float sampleAt(float offset) {
                vec2 at = (uTransform * vec4(vPlace.x + offset, vPlace.y, 0.0, 1.0)).xy;
                return uCoefficients.a + dot(uCoefficients.rgb, texture2D(uPicture, at).rgb);
            }
            void main() {
                gl_FragColor = vec4(sampleAt(-1.5 * uStep), sampleAt(-0.5 * uStep), sampleAt(0.5 * uStep), sampleAt(1.5 * uStep));
            }
        """
    }
}
