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
        if (width > 0 && height > 0) VideoHub.of(call).want(this, sid, width, height)
    }

    private fun release() {
        val call = call
        if (attached && call != null) VideoHub.existing(call)?.detach(this)
        attached = false
        shape = Triple(0, 0, 0)
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        val call = call
        val sid = sid
        if (attached && call != null && sid != null && w > 0 && h > 0) VideoHub.existing(call)?.want(this, sid, w, h)
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
        release()
        super.onDetachedFromWindow()
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
