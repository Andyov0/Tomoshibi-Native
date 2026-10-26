package app.tomoshibi.ui.lobby

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.BlendMode
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import android.os.SystemClock
import android.util.Log
import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.layout.onVisibilityChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import app.tomoshibi.BuildConfig
import app.tomoshibi.basics.LocalPalette
import app.tomoshibi.basics.Palette
import app.tomoshibi.ui.kit.ContinuousShape
import app.tomoshibi.ui.kit.GlassSupport
import app.tomoshibi.ui.kit.LocalGlassBackdrop
import dev.chrisbanes.haze.HazeInput
import dev.chrisbanes.haze.blur.HazeBlurStyle
import dev.chrisbanes.haze.blur.HazeColorEffect
import dev.chrisbanes.haze.blur.hazeBlur
import kotlinx.coroutines.delay
import kotlin.math.max
import kotlin.math.min

// The lobby's camera card with the camera off: a pane of glass with children
// drawn on it in lines of glass, one after another (PreviewGlass.swift).
//
// It was the grey of a camera-off tile, an empty card the size of the
// picture that was not there. The owner asked for a large piece of glass
// with people drawn on it in frosted-glass lines, written the way the iPhone
// writes its first "hello" (2026-10-26), and for Android to have what the
// iPhone has. Who is drawn, their lines and their pace are the shared file's
// (art/portraits.json, read by PortraitArt.kt), and how the glass looks
// without Liquid Glass is the file's README's; what is here is the drawing.
//
// Android has no Liquid Glass. The README draws its glass the way it looks
// without its depth, which the Apple apps draw before iOS 26: the lines as
// one shape, filled with the text colour at 12 per cent and edged at 34, a
// thin line of light along their upper side, and no shadow; the card the
// system's thinnest material with a hairline of the text colour. Where there
// is something behind the card to see through it (the wide lobby's stage,
// which is the camera's picture as it comes on), the card blurs it with a
// RenderEffect on Android 12 and later; on the phone's page, one flat colour,
// a blur would show the same colour and cost a pass on every frame of the
// drawing, so the card is the material's tint there, as it is before 12
// everywhere. No gradients: the owner finds them cheap.
//
// It is decoration and nothing else: hidden from screen readers, never
// pressed (a tap on it is the page's, which puts the keyboard away), the
// first child standing finished where animations are switched off, and
// moving only while it can be seen. It holds still while the app is not in
// front, while the card is scrolled out of sight or covered by a sheet, and
// in the gap and the hold nothing on it is drawn again at all: the owner
// found an Android phone hot in a call (2026-10-17), and a lobby left open on
// a desk should cost nothing while nothing moves.

/**
 * The camera card's glass with the children on it, filling what it is put
 * in, clipped to a rounded rectangle of [cornerRadius]. [held]: the card is
 * covered (a sheet over the lobby), and the drawing holds still.
 * [seeThrough]: something may be behind the card to be seen through it
 * blurred (the screen's glass backdrop, which the card must not be part of).
 */
@Composable
fun PreviewGlass(cornerRadius: Dp, held: Boolean, modifier: Modifier = Modifier, seeThrough: Boolean = false) {
    val shape = ContinuousShape(cornerRadius)
    val palette = LocalPalette.current
    val ink = GlassInk.of(palette)
    val backdrop = LocalGlassBackdrop.current?.takeIf { seeThrough && GlassSupport.blurs }
    val frost = remember(palette) { CardFrost(palette, ink) }
    Box(
        modifier
            .clearAndSetSemantics {}
            .clip(shape)
            .background(ink.card)
            .then(if (backdrop != null) Modifier.hazeBlur(HazeInput.Sources(backdrop), frost) else Modifier)
            .border(1.dp, ink.cardEdge, shape),
    ) {
        PortraitCycle(held, Modifier.fillMaxSize())
    }
}

/**
 * The thinnest material over a picture: blurred, and tinted as the card is
 * over the page, so that the card is the same colour wherever what is behind
 * it is plain.
 */
private fun CardFrost(palette: Palette, ink: GlassInk) = HazeBlurStyle {
    blurRadius(18.dp)
    noiseFactor(0f)
    backgroundColor(palette.ground)
    colorEffects(listOf(HazeColorEffect.tint(ink.card)))
    fallbackColorEffect(HazeColorEffect.tint(ink.card))
}

/**
 * The children in turn, without the card: placed in what they are drawn in
 * as the shared file says (`bottom-center`).
 */
@Composable
fun PortraitCycle(held: Boolean, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val drawings = remember { Portraits.load(context) } ?: return Spacer(modifier)
    val cycle = remember(drawings) { CycleState(drawings.timeline) }
    var inSight by remember { mutableStateOf(true) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val stage by lifecycle.currentStateFlow.collectAsState()
    val still = PortraitDebug.held(drawings) ?: if (animationsOff()) PortraitDebug.Still(0, Double.MAX_VALUE) else null
    // In front and resumed, as the iPhone's runs only while its scene is
    // active; not while another app or the system's own screen is over it.
    val running = still == null && !held && inSight && stage.isAtLeast(Lifecycle.State.RESUMED)

    if (still != null) SideEffect { cycle.stand(still) }
    LaunchedEffect(cycle, running) {
        if (running) cycle.run()
    }
    PortraitLines(
        drawings,
        cycle,
        modifier.onVisibilityChanged(minFractionVisible = 0.05f) { inSight = it },
    )
}

/** Whether the system's animations are off (Settings, Accessibility, Remove animations): the first child, standing still. */
private fun animationsOff(): Boolean = !ValueAnimator.areAnimatorsEnabled()

/**
 * Where the cycle is: whose turn, how far into their drawing (only the part
 * of it that changes what is drawn, so that it stands still while they are
 * held and fade), and how much of them is left to see. Each is its own
 * state, read only while drawing: a fade changes nothing but the layer's
 * alpha, and the lines are not drawn again for it.
 */
@Stable
internal class CycleState(
    private val timeline: PortraitTimeline,
    /** The clock the display's frame times are read on: the system's monotonic one, as the Choreographer's are. */
    private val now: () -> Long = System::nanoTime,
) {
    private val book = timeline.book

    var person by mutableIntStateOf(0)

    /** Seconds after the person's gap, no further than their drawing goes; below nought in the gap. */
    var written by mutableDoubleStateOf(0.0)
    var shown by mutableFloatStateOf(1f)

    /** How far into the cycle it had got when it last stopped, in seconds. */
    private var stoppedAt = 0.0

    /** One child standing still, as far into their drawing as said. */
    fun stand(still: PortraitDebug.Still) {
        person = still.person
        written = min(still.written, book.portraits[still.person].writing)
        shown = 1f
    }

    private fun show(moment: PortraitTimeline.Moment) {
        person = moment.person
        written = min(moment.tau, book.portraits[moment.person].writing)
        shown = moment.shown
    }

    /**
     * Moves the drawing on by the display's frames while something on it
     * moves, at no more than sixty a second, and sleeps through the moments
     * when nothing does; carries on from where it stopped when started again.
     *
     * Its frames are an infinite animation's, which a test of the screens
     * around the card stops at once (the test's InfiniteAnimationPolicy): with
     * the display's own frames the test's clock never found the lobby idle,
     * and every lobby test ran until it was killed.
     */
    suspend fun run() {
        val from = stoppedAt
        val began = withInfiniteAnimationFrameNanos { it }
        // When a frame was last shown; none yet. Not Long.MIN_VALUE: a frame
        // time less that overflowed below nought, every frame counted as too
        // soon after the last, and the card stood empty while the loop ran.
        var lastShown: Long? = null
        var at = from
        try {
            while (true) {
                val frame = withInfiniteAnimationFrameNanos { it }
                at = from + (frame - began) / 1e9
                // A phone drawing 120 frames a second draws this at sixty, as
                // the iPhone's timeline does: a line growing by a hair a frame
                // is not seen to move any more smoothly, and every frame is
                // the lines worked out again.
                if (lastShown != null && frame - lastShown < MIN_FRAME_NANOS) continue
                lastShown = frame
                val moment = timeline.at(at)
                show(moment)
                if (!moment.moving) {
                    // The settled frame is the one just shown; nothing changes
                    // until the next stroke, the fade or the next child.
                    delay(((moment.until - at) * 1000).toLong().coerceAtLeast(0) + 1)
                }
            }
        } finally {
            // Counting the still moment it was stopped in, as far as it got:
            // started again, it goes on rather than holding that moment anew.
            stoppedAt = max(at, from + (now() - began) / 1e9)
        }
    }

    companion object {
        /** Sixty a second, less a little, so that a display at exactly sixty is not held to thirty. */
        const val MIN_FRAME_NANOS = 15_500_000L
    }
}

/**
 * The lines on a layer of their own, faded as a whole by the layer's alpha,
 * so that a fade draws nothing again.
 */
@Composable
private fun PortraitLines(drawings: PortraitDrawings, cycle: CycleState, modifier: Modifier) {
    val ink = GlassInk.of(LocalPalette.current)
    val painter = remember(drawings) { LinePainter(drawings) }
    Spacer(
        modifier
            .graphicsLayer {
                alpha = cycle.shown
                // Its own pixels, for the passes inside it to replace one
                // another rather than lie over the card (LinePainter).
                compositingStrategy = CompositingStrategy.Offscreen
            }
            .drawBehind { painter.draw(this, cycle.person, cycle.written, ink) },
    )
}

/**
 * The README's glass for a platform without Liquid Glass, in one appearance:
 * the card's face and its edge, and the lines' fill, edge and light. The
 * text colour is the palette's own (night #F5F5F7, day #1C1C1E).
 */
internal class GlassInk(
    val card: Color,
    val cardEdge: Color,
    val fill: Color,
    val edge: Color,
    val light: Color,
) {
    /** The edge laid over the fill: what the inner half of the edge is, the fill showing through it. */
    val edgeOverFill: Color = edge.compositeOver(fill)

    companion object {
        /** The edge's width, laid along the lines' outline, half inside and half out. */
        val EdgeWidth = 0.75.dp

        /** The light's width, as a share of the line's, and never under [LightLeast]. */
        const val LIGHT_WIDTH = 0.16f
        val LightLeast = 0.8.dp

        /** Where the light lies off the line's middle, in line widths: up and to the left. */
        const val LIGHT_X = -0.17f
        const val LIGHT_Y = -0.2f

        private fun of(text: Color, card: Color) = GlassInk(
            card = card,
            cardEdge = text.copy(alpha = 0.16f),
            fill = text.copy(alpha = 0.12f),
            edge = text.copy(alpha = 0.34f),
            light = Color.White.copy(alpha = 0.62f),
        )

        /**
         * The thinnest material's colour over the page: at night the iPhone's
         * card drawn without Liquid Glass measures 31 over its black page,
         * white at twelve per cent; by day white at nearly three quarters
         * over the page's grey, as a raised plate is (Palette.plate).
         */
        private val Night = of(Palette.Dark.foreground, Color.White.copy(alpha = 0.12f))
        private val Day = of(Palette.Light.foreground, Color.White.copy(alpha = 0.72f))

        /** The ink of the appearance the card is drawn in: the dark's or the light's whole, never a blend of them. */
        fun of(palette: Palette): GlassInk = if (palette.dark) Night else Day
    }
}

/**
 * Draws a child's strokes as far as they are written, as the README's lines
 * of glass: one shape, every stroke drawn so far united.
 *
 * Into the layer's own pixels, three passes, each replacing what is there
 * rather than laid over it: every stroke a hair wider than the line in the
 * edge's colour, every stroke at the line's width in the edge over the fill,
 * every stroke a hair narrower in the fill. What is left is the shape filled
 * and edged along its outline, as the Apple apps fill the shape and stroke
 * it; where two strokes cross (a cheek and the hair, the two lines of an
 * eye) they are one shape with one edge, where laid one over another they
 * were two lines with a double edge and a darker crossing. Then the light.
 *
 * The strokes already finished are drawn as one path, made again only when
 * another is finished, so that a frame is a handful of paths rather than
 * every stroke of a child of twenty four.
 */
private class LinePainter(private val drawings: PortraitDrawings) {
    private val measure = PathMeasure()
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }

    /** The finished strokes of [finishedPerson] as one path, [finishedCount] of them. */
    private val finished = Path()
    private var finishedPerson = -1
    private var finishedCount = -1
    private val shown = ArrayList<Path>()

    fun draw(scope: DrawScope, person: Int, written: Double, ink: GlassInk) = with(scope) {
        val fit = drawings.fit(size.width, size.height, PortraitDrawings.Inset.toPx())
        val paths = visible(person, written)
        if (paths.isEmpty()) return@with
        val width = drawings.book.lineWidth
        // The hairlines are the README's points, whatever the scale.
        val edge = GlassInk.EdgeWidth.toPx() / fit.scale
        val light = max(width * GlassInk.LIGHT_WIDTH, GlassInk.LightLeast.toPx() / fit.scale)
        drawIntoCanvas {
            val canvas = it.nativeCanvas
            canvas.save()
            canvas.clipRect(0f, 0f, size.width, size.height)
            canvas.translate(fit.left, fit.top)
            canvas.scale(fit.scale, fit.scale)
            fun pass(stroke: Float, colour: Color) {
                paint.strokeWidth = stroke
                paint.color = colour.toArgb()
                for (path in paths) canvas.drawPath(path, paint)
            }
            paint.blendMode = BlendMode.SRC
            pass(width + edge, ink.edge)
            pass(width, ink.edgeOverFill)
            pass(max(width - edge, 0f), ink.fill)
            // The light: the middle of the lines again, thin, moved up and to
            // the left. At these proportions it lies inside the shape
            // everywhere, its round ends included (it is never more than
            // 0.34 of a width from the middle, the shape's edge half a width
            // away, while the line is wider than 1.7 points), and needs no
            // mask to keep it there.
            paint.blendMode = BlendMode.SRC_OVER
            paint.strokeWidth = light
            paint.color = ink.light.toArgb()
            canvas.translate(width * GlassInk.LIGHT_X, width * GlassInk.LIGHT_Y)
            for (path in paths) canvas.drawPath(path, paint)
            canvas.restore()
        }
    }

    /**
     * The strokes as far as they are drawn: the finished ones as one path,
     * each one being drawn cut at its length so far, those not begun left out.
     */
    private fun visible(person: Int, written: Double): List<Path> {
        val art = drawings.book.portraits[person]
        val strokes = drawings.people[person]
        shown.clear()
        var done = 0
        for ((index, stroke) in art.strokes.withIndex()) if (stroke.drawn(written) >= 1f) done++
        if (person != finishedPerson || done != finishedCount) {
            // The finished strokes only grow while a child is drawn, so their
            // number says which they are.
            finished.rewind()
            for ((index, stroke) in art.strokes.withIndex()) if (stroke.drawn(written) >= 1f) finished.addPath(strokes[index].path)
            finishedPerson = person
            finishedCount = done
        }
        if (done > 0) shown += finished
        for ((index, stroke) in art.strokes.withIndex()) {
            val drawn = stroke.drawn(written)
            if (drawn <= 0f || drawn >= 1f) continue
            strokes[index].cut(drawn, measure)
            shown += strokes[index].partial
        }
        return shown
    }
}

/** The shared file's children as the platform's paths, read once for the life of the process. */
class PortraitDrawings(val book: PortraitBook) {
    val timeline = PortraitTimeline(book)
    internal val people: List<List<StrokePath>> = book.portraits.map { art -> art.strokes.map { StrokePath(it.path) } }

    /** Where the canvas lies on a card: its scale, and where its top left corner is. */
    data class Fit(val scale: Float, val left: Float, val top: Float)

    /**
     * The README's `bottom-center`: the canvas scaled to the card, no wider
     * than the card less [inset] at either side, and no taller than it,
     * centred across and standing on the card's foot. A phone's card is
     * wider than tall and the canvas fills its height; a card taller than
     * wide leaves empty glass above the head.
     */
    fun fit(width: Float, height: Float, inset: Float): Fit {
        val scale = when (book.placement) {
            PortraitBook.Placement.BottomCenter -> min(height / book.height, (width - 2 * inset) / book.width).coerceAtLeast(0f)
        }
        return Fit(scale, (width - book.width * scale) / 2, height - book.height * scale)
    }

    companion object {
        /** The README's inset at the card's left and right. */
        val Inset = 12.dp
    }
}

/** A stroke's path, its length, and the path of as much of it as is drawn. */
internal class StrokePath(segments: List<PathData.Segment>) {
    val path = Path().apply {
        for (segment in segments) when (segment) {
            is PathData.Segment.Move -> moveTo(segment.x, segment.y)
            is PathData.Segment.Line -> lineTo(segment.x, segment.y)
            is PathData.Segment.Cubic -> cubicTo(segment.x1, segment.y1, segment.x2, segment.y2, segment.x, segment.y)
            is PathData.Segment.Quad -> quadTo(segment.x1, segment.y1, segment.x, segment.y)
            PathData.Segment.Close -> close()
        }
    }

    /** The length of each of its pieces (a stroke may lift and set down again inside its `d`), and of all of them. */
    private val pieces: FloatArray
    val length: Float

    init {
        val measure = PathMeasure(path, false)
        val lengths = ArrayList<Float>()
        do lengths += measure.length while (measure.nextContour())
        pieces = lengths.toFloatArray()
        length = pieces.sum()
    }

    /** As much of it as is drawn, made by [cut]. */
    val partial = Path()

    /** Cuts [partial] to the first [fraction] of the stroke's length, through its pieces in their order. */
    fun cut(fraction: Float, measure: PathMeasure) {
        partial.rewind()
        var left = length * fraction
        measure.setPath(path, false)
        var piece = 0
        do {
            if (left <= 0f || piece >= pieces.size) break
            measure.getSegment(0f, min(left, pieces[piece]), partial, true)
            left -= pieces[piece]
            piece++
        } while (measure.nextContour())
    }
}

/** The shared drawings, read from the app's assets the first time a card asks. */
object Portraits {
    const val ASSET = "portraits.json"

    @Volatile
    private var read: Result<PortraitDrawings>? = null

    /**
     * The drawings, or null where the file is missing or says something this
     * cannot draw (said in the log once): the card is then the glass alone,
     * which is what it is in the gap between two children.
     */
    fun load(context: Context): PortraitDrawings? {
        read?.let { return it.getOrNull() }
        val began = SystemClock.elapsedRealtime()
        val result = runCatching {
            val json = context.applicationContext.assets.open(ASSET).use { it.readBytes().toString(Charsets.UTF_8) }
            PortraitDrawings(PortraitBook.parse(json))
        }
        result.exceptionOrNull()?.let { Log.w("Tomoshibi", "the portraits could not be read: ${it.javaClass.simpleName}: ${it.message}") }
        if (BuildConfig.DEBUG && result.isSuccess) Log.i("Tomoshibi", "portraits read in ${SystemClock.elapsedRealtime() - began} ms")
        read = result
        return result.getOrNull()
    }
}

/**
 * The debug switch the iPhone's TOMOSHIBI_DEBUG_PORTRAIT is: a debug launch's
 * `-e portrait <name>[@<seconds>]` stands that child still on the card,
 * finished or that many seconds after the gap into their drawing, to
 * photograph each of them and the stages of one. Tests set [said] the same way.
 */
object PortraitDebug {
    var said by mutableStateOf<String?>(null)

    data class Still(val person: Int, val written: Double)

    internal fun held(drawings: PortraitDrawings): Still? {
        val said = said ?: return null
        val name = said.substringBefore('@')
        val person = drawings.book.portraits.indexOfFirst { it.name == name }.takeIf { it >= 0 } ?: return null
        val seconds = said.substringAfter('@', "").toDoubleOrNull() ?: Double.MAX_VALUE
        return Still(person, seconds)
    }
}
