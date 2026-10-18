package app.tomoshibi.media

import android.content.Context
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.view.TextureView
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import app.tomoshibi.core.CoreNative
import uniffi.tomoshibi_core.Call
import uniffi.tomoshibi_core.PictureShape

/**
 * A remote picture, drawn by the core into this view's surface
 * (core/src/call/window.rs); the view only turns and fits it, and tells the
 * hub how large it is drawn, which is the layer the server is asked for.
 *
 * Keyed by the track rather than rebuilt per layout: a picture that is
 * rebuilt resubscribes and waits for a keyframe, the black tile the web's
 * plan.ts exists to avoid.
 */
class RemoteVideoView(context: Context) : TextureView(context), TextureView.SurfaceTextureListener {
    private var call: Call? = null
    private var sid: String? = null
    private var attached = false
    private var shape = Triple(0, 0, 0)
    private val main = Handler(Looper.getMainLooper())
    /** Asks again when the device grows hot or cools, while the view is in a window. */
    private var thermal: AutoCloseable? = null

    /** Whether the picture fills the view, cropping, rather than fitting whole inside it. */
    var fill = false
        set(value) {
            field = value
            fit()
        }

    /** Told on the main thread when the picture's size or turn changes: width, height, clockwise degrees. */
    var onShape: ((Int, Int, Int) -> Unit)? = null

    init {
        surfaceTextureListener = this
        isOpaque = false
    }

    /** Shows [sid] of [call], or nothing. */
    fun show(call: Call?, sid: String?) {
        if (this.call === call && this.sid == sid) return
        release()
        this.call = call
        this.sid = sid
        attach()
    }

    private fun attach() {
        val call = call ?: return
        val sid = sid ?: return
        val texture = surfaceTexture ?: return
        if (attached) return
        val surface = Surface(texture)
        val window = CoreNative.windowFromSurface(surface)
        surface.release()
        if (window == 0L) return
        attached = true
        VideoHub.of(call).attach(this, sid, window, object : PictureShape {
            override fun changed(width: UInt, height: UInt, rotation: UInt) {
                main.post {
                    shape = Triple(width.toInt(), height.toInt(), rotation.toInt())
                    fit()
                    onShape?.invoke(shape.first, shape.second, shape.third)
                }
            }
        })
        ask()
    }

    /**
     * Tells the hub the size this view draws its picture at, which is the
     * layer the server is asked for, by [PictureDensity]'s rule.
     */
    private fun ask() {
        val call = call ?: return
        val sid = sid ?: return
        if (!attached || width <= 0 || height <= 0) return
        val metrics = resources.displayMetrics
        val (w, h) = PictureDensity.asked(width, height, metrics.density,
            phone = resources.configuration.smallestScreenWidthDp < 600, hot = Thermal.hot)
        VideoHub.existing(call)?.want(this, sid, w, h)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        Thermal.install(context)
        thermal = Thermal.watch { ask() }
    }

    private fun release() {
        val call = call
        if (attached && call != null) VideoHub.existing(call)?.detach(this)
        attached = false
        shape = Triple(0, 0, 0)
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        ask()
        fit()
    }

    /** The picture upright, whole or filling, in the middle of the view. */
    private fun fit() {
        val (w, h, turn) = shape
        if (w == 0 || h == 0 || width == 0 || height == 0) return
        val turned = turn == 90 || turn == 270
        val (pw, ph) = if (turned) h.toFloat() to w.toFloat() else w.toFloat() to h.toFloat()
        val scale = if (fill) maxOf(width / pw, height / ph) else minOf(width / pw, height / ph)
        val m = Matrix()
        val cx = width / 2f
        val cy = height / 2f
        // The surface's buffer is stretched to the view first; undo that,
        // turn it, and scale it to fit.
        m.postScale(w / width.toFloat(), h / height.toFloat(), cx, cy)
        m.postRotate(turn.toFloat(), cx, cy)
        m.postScale(scale, scale, cx, cy)
        setTransform(m)
    }

    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) = attach()
    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = fit()
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
        release()
        return true
    }
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) {}

    override fun onDetachedFromWindow() {
        thermal?.close()
        thermal = null
        release()
        super.onDetachedFromWindow()
    }
}

/**
 * The size a picture is asked of the server at, from the size its view is
 * drawn at in device pixels.
 *
 * The web asks at the view's points times the screen's density (room.ts
 * `adaptiveStream: { pixelDensity: "screen" }`), which on a phone of density
 * three asked a stage of 440 by 285 points for 1320 by 854: more than the
 * 1112 by 720 rung, so the phone was sent the 1080 rung at sixty frames,
 * decoded and drew all of it, and grew hot (the owner's iPhone,
 * 2026-10-17). So a phone asks at twice its points at most, which is also
 * livekit-client's own default where no density is named
 * (RemoteVideoTrack.ts `getPixelDensity`); a tablet keeps the screen's. Any
 * device the system calls severely hot asks at its points alone, a quarter
 * of the pixels again, until it cools (Thermal).
 */
object PictureDensity {
    fun asked(widthPx: Int, heightPx: Int, density: Float, phone: Boolean, hot: Boolean): Pair<Int, Int> {
        if (density <= 0f) return widthPx to heightPx
        var wanted = density
        if (phone) wanted = minOf(wanted, 2f)
        if (hot) wanted = minOf(wanted, 1f)
        val scale = wanted / density
        return Math.round(widthPx * scale) to Math.round(heightPx * scale)
    }
}

/**
 * A remote picture in Compose: [sid] of [call], drawn by the core. The view is
 * made once for the composable's life and handed whatever it is to show.
 */
@Composable
fun RemoteVideo(call: Call, sid: String?, modifier: Modifier = Modifier, fill: Boolean = false) {
    AndroidView(
        factory = { RemoteVideoView(it) },
        update = {
            it.fill = fill
            it.show(call, sid)
        },
        onRelease = { it.show(null, null) },
        modifier = modifier,
    )
}
