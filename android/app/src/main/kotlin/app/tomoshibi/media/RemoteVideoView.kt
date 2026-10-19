package app.tomoshibi.media

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.view.TextureView
import android.view.animation.PathInterpolator
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import app.tomoshibi.basics.Framing
import app.tomoshibi.basics.Theme
import app.tomoshibi.core.CoreNative
import kotlinx.coroutines.delay
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

    /**
     * A camera, which fills the view when its shape is near enough the
     * view's (Framing) and is otherwise shown whole; anything else, a shared
     * screen, is always shown whole.
     */
    var camera = false
        set(value) {
            field = value
            fit()
        }

    /** Told on the main thread when the picture's size or turn changes: width, height, clockwise degrees. */
    var onShape: ((Int, Int, Int) -> Unit)? = null

    /** Told on the main thread when a camera starts or stops being shown whole, for what is drawn behind it. */
    var onWhole: ((Boolean) -> Unit)? = null

    /** Whether the picture is meant to fill, as last decided; null before its shape is known. */
    private var fills: Boolean? = null

    /** How far from whole (0) to filling (1) the picture is drawn, moved by [framing] when [fills] changes. */
    private var filling = 1f
    private var framing: ValueAnimator? = null

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
        framing?.cancel()
        framing = null
        if (fills == false) onWhole?.invoke(false)
        fills = null
        filling = 1f
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        ask()
        fit()
    }

    /**
     * Decides whether the picture fills the view or is shown whole, and
     * draws it so. A change of mind once the picture is showing is drawn as
     * a move from the one to the other, not a jump; the first decision, and
     * a change of the view's size, take effect at once.
     */
    private fun fit() {
        val (w, h, turn) = shape
        if (w == 0 || h == 0 || width == 0 || height == 0) return
        val turned = turn == 90 || turn == 270
        val (pw, ph) = if (turned) h.toFloat() to w.toFloat() else w.toFloat() to h.toFloat()
        val wanted = camera && Framing.fills(pw, ph, width.toFloat(), height.toFloat())
        val before = fills
        if (wanted != before) {
            fills = wanted
            framing?.cancel()
            val to = if (wanted) 1f else 0f
            if (before == null) {
                filling = to
            } else {
                framing = ValueAnimator.ofFloat(filling, to).apply {
                    duration = 320
                    interpolator = PathInterpolator(0.2f, 0.9f, 0.3f, 1f)
                    addUpdateListener {
                        filling = it.animatedValue as Float
                        draw()
                    }
                    start()
                }
            }
            if (!wanted || before == false) onWhole?.invoke(!wanted)
        }
        draw()
    }

    /** The picture upright in the middle of the view, [filling] of the way from whole to filling it. */
    private fun draw() {
        val (w, h, turn) = shape
        if (w == 0 || h == 0 || width == 0 || height == 0) return
        val turned = turn == 90 || turn == 270
        val (pw, ph) = if (turned) h.toFloat() to w.toFloat() else w.toFloat() to h.toFloat()
        val whole = minOf(width / pw, height / ph)
        val scale = whole + (maxOf(width / pw, height / ph) - whole) * filling
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

    /**
     * The picture as it is now, upright and a few dozen pixels across, for
     * the blur behind it: read back from the surface, which holds the frame
     * as decoded, before this view's turn and scale.
     */
    fun glimpse(): Bitmap? {
        val (w, h, turn) = shape
        if (!isAvailable || w == 0 || h == 0) return null
        val scale = GLIMPSE / maxOf(w, h).toFloat()
        val small = getBitmap(maxOf(1, Math.round(w * scale)), maxOf(1, Math.round(h * scale))) ?: return null
        if (turn == 0) return small
        val upright = Bitmap.createBitmap(small, 0, 0, small.width, small.height, Matrix().apply { postRotate(turn.toFloat()) }, true)
        small.recycle()
        return upright
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

    private companion object {
        /** The glimpse's longer side, in pixels: blurred over a whole tile, more would not show. */
        const val GLIMPSE = 48
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
 *
 * A [camera] shown whole (Framing) is not given bars: the rest of the tile is
 * the same picture, filling it, blurred and darkened, as a telephone shows a
 * caller who turned theirs on its side. It is a glimpse taken about once a
 * second, crossfaded from the last, not the video again, and it is there
 * only while the picture is whole.
 */
@Composable
fun RemoteVideo(call: Call, sid: String?, modifier: Modifier = Modifier, camera: Boolean = false) {
    var view by remember { mutableStateOf<RemoteVideoView?>(null) }
    var whole by remember { mutableStateOf(false) }
    var glimpse by remember { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(view, whole) {
        val shown = view ?: return@LaunchedEffect
        while (whole) {
            shown.glimpse()?.let { glimpse = it.asImageBitmap() }
            delay(GLIMPSE_EVERY)
        }
    }
    val behind by animateFloatAsState(if (whole && glimpse != null) 1f else 0f, if (whole) Theme.settle() else Theme.leave(), label = "behind")
    Box(modifier) {
        if (behind > 0f) {
            Box(Modifier.fillMaxSize().graphicsLayer { alpha = behind }) {
                Crossfade(glimpse, Modifier.fillMaxSize(), animationSpec = tween(GLIMPSE_FADE), label = "glimpse") { image ->
                    if (image != null) {
                        Image(
                            image, null,
                            Modifier.fillMaxSize().blur(28.dp, BlurredEdgeTreatment.Rectangle),
                            contentScale = ContentScale.Crop,
                            filterQuality = FilterQuality.Low,
                        )
                    }
                }
                Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.35f)))
            }
        }
        AndroidView(
            factory = { context -> RemoteVideoView(context).also { made -> made.onWhole = { whole = it }; view = made } },
            update = {
                it.camera = camera
                it.show(call, sid)
            },
            onRelease = {
                it.show(null, null)
                view = null
            },
            modifier = Modifier.fillMaxSize(),
        )
    }
}

/** How often the blur behind a picture shown whole is taken again, and how long one fades into the next. */
private const val GLIMPSE_EVERY = 1000L
private const val GLIMPSE_FADE = 600
