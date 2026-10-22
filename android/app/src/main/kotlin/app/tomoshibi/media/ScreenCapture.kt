package app.tomoshibi.media

import android.content.Context
import android.graphics.SurfaceTexture
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.projection.MediaProjection
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
import app.tomoshibi.core.CoreNative
import app.tomoshibi.core.SharedEgl
import java.util.concurrent.Callable
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import livekit.org.webrtc.EglBase
import livekit.org.webrtc.GlUtil
import livekit.org.webrtc.RendererCommon
import livekit.org.webrtc.TextureBufferImpl
import livekit.org.webrtc.VideoFrame
import livekit.org.webrtc.YuvConverter
import uniffi.tomoshibi_core.VideoInput
import uniffi.tomoshibi_protocol.ShareQuality
import uniffi.tomoshibi_protocol.shareCaptureRequest

private const val TAG = "Tomoshibi"

/**
 * The screen, mirrored through a projection into a video input (the Apple
 * apps' ScreenCapture, ScreenCaptureKit's part of it).
 *
 * Each frame goes to the core as the GPU texture the display drew it into,
 * and from there to the hardware encoders, which draw it into their codecs'
 * input surfaces: the GPU scales it to each rung and the codec converts its
 * colour, and nothing of the picture passes through the processor (ScreenFrames
 * says how, livekit.org.webrtc.DefaultVideoEncoderFactory why). Until then the
 * shader here turned each frame into Y, U and V planes, which were read back
 * and copied four times on their way to three encoders' input buffers, sixty
 * times a second; that was the heat of a Fold sharing its inner screen.
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
    private var reader: ScreenFrames? = null
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
            onThread(gl) { ScreenFrames(gl, width, height, captured, video) }
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
            gl.post { reader.release { thread.quitSafely() } }
            throw fault
        } ?: run {
            gl.post { reader.release { thread.quitSafely() } }
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
    internal fun picture(): ScreenFrames.Picture? {
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
            if (reader != null) reader.release { thread?.quitSafely() } else thread?.quitSafely()
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
 * The GPU's half of the screen capture: an EGL context in the encoders'
 * share group (SharedEgl), the SurfaceTexture the display draws into, and
 * each frame handed to the core as that texture (CoreNative.pushFrame), which
 * the hardware encoders draw into their codecs' surfaces themselves. On its
 * thread alone.
 *
 * The texture holds one picture at a time, so a frame is not taken from the
 * display while one sent before it is still being drawn by an encoder: the
 * next would be drawn into the texture under it, and the display's buffer
 * handed back while an encoder was still reading it. Frames that arrive
 * meanwhile wait in the display's queue, and are taken, all of them, the
 * moment the last encoder lets go, so that the picture taken is the newest
 * rather than the oldest waiting. That is how WebRTC's own SurfaceTextureHelper
 * paces a camera; it is not used here because the share's deadline, its held
 * frame and its still-screen repeats need the texture between frames too.
 */
internal class ScreenFrames(
    private val handler: Handler,
    width: Int,
    height: Int,
    fps: Int,
    video: VideoInput?,
) : SurfaceTexture.OnFrameAvailableListener {
    private var width = width
    private var height = height
    private var interval = 1_000_000_000L / fps.coerceAtLeast(1)

    private val egl: EglBase = EglBase.create(SharedEgl.context(), EglBase.CONFIG_PIXEL_BUFFER).also {
        it.createDummyPbufferSurface()
        it.makeCurrent()
    }
    private val texture = GlUtil.generateTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES)
    private val frames = SurfaceTexture(texture).also {
        it.setDefaultBufferSize(width, height)
        it.setOnFrameAvailableListener(this, handler)
    }
    val surface = Surface(frames)
    // For a frame wanted as bytes after all: an encoder without an input
    // surface, the planes fallback below, a debug build's picture.
    private val converter = YuvConverter()
    private val transform = FloatArray(16)

    private var video: VideoInput? = video
    private var input: Long = video?.frameHandle()?.toLong() ?: 0L

    /** Frames sent that some part of WebRTC still holds. */
    private var inFlight = 0
    /** Frames the display has drawn that have not been taken off its queue. */
    private var queued = 0
    private var hasFrame = false
    private var quitting = false
    private var onReleased: (() -> Unit)? = null

    /** When the next frame is due, on the display's clock. */
    private var due = 0L
    private var held = false
    private var lastSent = 0L
    private var sent = 0
    private var fellBack = false
    private var spare = arrayOf(ByteArray(0), ByteArray(0), ByteArray(0))

    /** Told on any thread when WebRTC lets go of a frame; counted on this one. */
    private val released = Runnable { handler.post(::returned) }

    fun retune(fps: Int, video: VideoInput) {
        interval = 1_000_000_000L / fps.coerceAtLeast(1)
        if (input != 0L) CoreNative.releaseFrameInput(input)
        this.video = video
        input = video.frameHandle().toLong()
    }

    /** Frames taken at [fps] a second from now, into the same track. */
    fun pace(fps: Int) {
        interval = 1_000_000_000L / fps.coerceAtLeast(1)
    }

    /** One picture read back as planes: luma rows of [stride], chroma rows of half that. */
    class Picture(val width: Int, val height: Int, val stride: Int, val y: ByteArray, val u: ByteArray, val v: ByteArray)

    /** The texture as it is now, read back for a debug build (MediaDebug `sharedpicture`). */
    fun picture(): Picture? {
        if (!hasFrame) return null
        val buffer = wrap() ?: return null
        val i420 = try {
            buffer.toI420()
        } finally {
            buffer.release()
        } ?: return null
        try {
            fun bytes(of: java.nio.ByteBuffer) = ByteArray(of.remaining()).also { of.duplicate().get(it) }
            return Picture(i420.width, i420.height, i420.strideY, bytes(i420.dataY), bytes(i420.dataU), bytes(i420.dataV))
        } finally {
            i420.release()
        }
    }

    fun resize(width: Int, height: Int) {
        this.width = width
        this.height = height
        frames.setDefaultBufferSize(width, height)
        hasFrame = false
    }

    override fun onFrameAvailable(texture: SurfaceTexture) {
        queued++
        if (inFlight == 0) take()
    }

    /** Every frame the display has drawn taken off its queue, the newest kept, and sent if it is due. */
    private fun take() {
        if (queued == 0 || quitting) return
        try {
            synchronized(EglBase.lock) {
                repeat(queued) { frames.updateTexImage() }
            }
        } catch (fault: Exception) {
            Log.w(TAG, "a screen frame could not be taken: ${fault.message}")
            return
        } finally {
            queued = 0
        }
        hasFrame = true
        val stamp = frames.timestamp
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
        push(stamp)
    }

    /** The texture as a frame buffer of its own, which [released] is told of once WebRTC is done with it. */
    private fun wrap(): VideoFrame.TextureBuffer? {
        if (quitting) return null
        frames.getTransformMatrix(transform)
        inFlight++
        return TextureBufferImpl(
            width, height, VideoFrame.TextureBuffer.Type.OES, texture,
            RendererCommon.convertMatrixToAndroidGraphicsMatrix(transform), handler, converter, released,
        )
    }

    private fun push(stamp: Long) {
        if (input == 0L) return
        val buffer = wrap() ?: return
        lastSent = System.nanoTime()
        try {
            if (!CoreNative.pushFrame(input, buffer, width, height, stamp / 1000)) planes(buffer, stamp)
        } finally {
            buffer.release()
        }
        if (++sent % 300 == 0) Log.i(TAG, "screen sent $sent frames ${width}x$height")
    }

    /**
     * The frame as planes in memory, for a core that would not take it as a
     * texture: read back through the GPU, and sent the way a camera's frames
     * are. Slower, and a picture rather than none.
     */
    private fun planes(buffer: VideoFrame.TextureBuffer, stamp: Long) {
        if (!fellBack) {
            fellBack = true
            Log.e(TAG, "the core would not take the screen as a texture; sending it as planes")
        }
        val video = video ?: return
        val i420 = buffer.toI420() ?: return
        try {
            val sources = arrayOf(i420.dataY, i420.dataU, i420.dataV)
            for (index in 0..2) {
                val source = sources[index].duplicate()
                if (spare[index].size != source.remaining()) spare[index] = ByteArray(source.remaining())
                source.get(spare[index])
            }
            video.pushPlanes(spare[0], spare[1], spare[2], i420.strideY.toUInt(), i420.strideU.toUInt(), 1u,
                i420.width.toUInt(), i420.height.toUInt(), stamp / 1000, 0u)
        } finally {
            i420.release()
        }
    }

    /** A frame let go by WebRTC: once none is held, whatever the display drew meanwhile is taken. */
    private fun returned() {
        inFlight--
        if (inFlight > 0) return
        if (quitting) finish() else take()
    }

    /**
     * Stops taking frames and lets the GPU's things go once no encoder is
     * still drawing the texture, then [done]: the thread is not to quit
     * under a frame on its way back. A second is as long as that is waited
     * for; an encoder that held a frame longer has stopped encoding.
     */
    fun release(done: () -> Unit) {
        quitting = true
        onReleased = done
        handler.removeCallbacks(still)
        handler.removeCallbacks(late)
        if (input != 0L) CoreNative.releaseFrameInput(input)
        input = 0L
        video = null
        frames.setOnFrameAvailableListener(null)
        if (inFlight == 0) finish() else handler.postDelayed(::finish, 1_000L)
    }

    private fun finish() {
        val done = onReleased ?: return
        onReleased = null
        surface.release()
        frames.release()
        converter.release()
        GLES20.glDeleteTextures(1, intArrayOf(texture), 0)
        egl.release()
        done()
    }

    companion object {
        const val STILL_NANOS = 500_000_000L
    }
}
