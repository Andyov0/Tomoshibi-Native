package app.tomoshibi.ui.lobby

import android.animation.ValueAnimator
import android.content.Context
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
import kotlinx.coroutines.delay
import kotlin.math.max
import kotlin.math.min

// The lobby's camera card with the camera off: a pane of glass with small
// animals drawn on it in thin lines, one after another (PreviewGlass.swift).
//
// It was the grey of a camera-off tile, an empty card the size of the
// picture that was not there. The owner asked for a large piece of glass
// with somebody drawn on it, written the way the iPhone writes its first
// "hello" (2026-10-26), and for Android to have what the iPhone has. Who is
// drawn, their lines and their pace are the shared file's (art/portraits.json,
// read by PortraitArt.kt); what is here is the drawing.
//
// The lines are single thin lines of the text colour, round at their ends
// and corners, with nothing else: no glass, no light along them and no
// shadow. They were tubes of glass round people's and then children's
// faces, a fill, an edge round the lines made one shape and a line of light,
// until the owner found the faces unsettling and settled on six small
// animals, a cat, a rabbit, a bear, a hamster, a penguin and a chick, with
// two dots for eyes, in thin lines (2026-10-27). The eyes are short strokes
// that the round ends make into dots.
//
// The card is what the iPhone's clear Liquid Glass comes to on the phone's
// lobby, measured from its pixels (art/README.md, "The card"): by night
// #202020 on the black page with the glass's lit rim, a point of white at
// 40 per cent just inside its outline; by day #FFFFFF on the page's #F2F2F7
// with no edge at all; no shadow either way. It was the thinnest material
// with a hairline of the text colour, which by night was a darker card than
// the iPhone's beside it and by day a grey ring round a white one; a blur of
// what was behind it, the wide lobby's picture coming on, is gone with it,
// since the card is now solid and nothing behind it could be seen. No
// gradients: the owner finds them cheap.
//
// It is decoration and nothing else: hidden from screen readers, never
// pressed (a tap on it is the page's, which puts the keyboard away), the
// first animal standing finished where animations are switched off, and
// moving only while it can be seen. It holds still while the app is not in
// front, while the card is scrolled out of sight or covered by a sheet, and
// in the gap and the hold nothing on it is drawn again at all: the owner
// found an Android phone hot in a call (2026-10-17), and a lobby left open on
// a desk should cost nothing while nothing moves. While it moves it is drawn
// thirty times a second (CycleState `run`).

/**
 * The camera card's glass with the animals on it, filling what it is put
 * in, clipped to a rounded rectangle of [cornerRadius]. [held]: the card is
 * covered (a sheet over the lobby), and the drawing holds still.
 */
@Composable
fun PreviewGlass(cornerRadius: Dp, held: Boolean, modifier: Modifier = Modifier) {
    val shape = ContinuousShape(cornerRadius)
    val ink = GlassInk.of(LocalPalette.current)
    Box(
        modifier
            .clearAndSetSemantics {}
            .clip(shape)
            .background(ink.card)
            // Drawn inside the outline, as the glass's rim is.
            .then(if (ink.cardEdge != null) Modifier.border(1.dp, ink.cardEdge, shape) else Modifier),
    ) {
        PortraitCycle(held, Modifier.fillMaxSize())
    }
}

/**
 * The animals in turn, without the card: placed in what they are drawn in
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

/** Whether the system's animations are off (Settings, Accessibility, Remove animations): the first animal, standing still. */
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

    /** Seconds after the portrait's gap, no further than its drawing goes; below nought in the gap. */
    var written by mutableDoubleStateOf(0.0)
    var shown by mutableFloatStateOf(1f)

    /** How far into the cycle it had got when it last stopped, in seconds. */
    private var stoppedAt = 0.0

    /** One animal standing still, as far into their drawing as said. */
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
     * moves, thirty a second, and sleeps through the moments when nothing
     * does; carries on from where it stopped when started again.
     *
     * Thirty, not the display's rate: every frame of the drawing is a frame
     * of the whole window, and on an emulator a lobby drawing at sixty took
     * a third of a core and its inner screen half of one, where at thirty
     * they took six and eight per cent. A line growing for a fraction of a
     * second is not seen to move any less smoothly at thirty. Between frames
     * it waits rather than letting the display's frames go by unused: each
     * of those woke the app to do nothing.
     *
     * Its frames are an infinite animation's, which a test of the screens
     * around the card stops at once (the test's InfiniteAnimationPolicy): with
     * the display's own frames the test's clock never found the lobby idle,
     * and every lobby test ran until it was killed.
     */
    suspend fun run() {
        val from = stoppedAt
        val began = withInfiniteAnimationFrameNanos { it }
        var at = from
        try {
            while (true) {
                val frame = withInfiniteAnimationFrameNanos { it }
                at = from + (frame - began) / 1e9
                val moment = timeline.at(at)
                show(moment)
                // The settled frame, when nothing moves, is the one just
                // shown; nothing changes until the next stroke, the fade or
                // the next animal.
                delay(if (moment.moving) FRAME_WAIT_MS else ((moment.until - at) * 1000).toLong().coerceAtLeast(0) + 1)
            }
        } finally {
            // Counting the still moment it was stopped in, as far as it got:
            // started again, it goes on rather than holding that moment anew.
            stoppedAt = max(at, from + (now() - began) / 1e9)
        }
    }

    companion object {
        /**
         * The wait after a frame while something moves: the next frame the
         * display draws after it is a thirtieth of a second after the last
         * at sixty, ninety or a hundred and twenty a second.
         */
        const val FRAME_WAIT_MS = 28L
    }
}

/**
 * The lines, faded as a whole by their layer's alpha, which a fade changes
 * without drawing them again. One path drawn once, so the alpha can be laid
 * on it directly rather than on a layer of pixels of its own.
 */
@Composable
private fun PortraitLines(drawings: PortraitDrawings, cycle: CycleState, modifier: Modifier) {
    val ink = GlassInk.of(LocalPalette.current)
    val painter = remember(drawings) { LinePainter(drawings) }
    Spacer(
        modifier
            .graphicsLayer {
                alpha = cycle.shown
                compositingStrategy = CompositingStrategy.ModulateAlpha
            }
            .drawBehind { painter.draw(this, cycle.person, cycle.written, ink) },
    )
}

/**
 * The card's colours in one appearance, as art/README.md gives them for
 * every platform without Liquid Glass: the card's face, its edge where it
 * has one, and the lines. The text colour is the palette's own (night
 * #F5F5F7, day #1C1C1E); the page round the card is the palette's ground
 * (night #000000, day #F2F2F7).
 */
internal class GlassInk(val card: Color, val cardEdge: Color?, val line: Color) {
    companion object {
        private val Night = GlassInk(
            card = Color(0xFF202020),
            cardEdge = Color.White.copy(alpha = 0.40f),
            line = Palette.Dark.foreground.copy(alpha = LINE_NIGHT),
        )
        private val Day = GlassInk(
            card = Color(0xFFFFFFFF),
            cardEdge = null,
            line = Palette.Light.foreground.copy(alpha = LINE_DAY),
        )

        /** The lines' share of the text colour, by night (#F5F5F7) and by day (#1C1C1E). */
        const val LINE_NIGHT = 0.60f
        const val LINE_DAY = 0.50f

        /** The ink of the appearance the card is drawn in: the dark's or the light's whole, never a blend of them. */
        fun of(palette: Palette): GlassInk = if (palette.dark) Night else Day
    }
}

/**
 * Draws an animal's strokes as far as they are written: every stroke drawn so
 * far in one path, stroked once at the file's line width with round caps
 * and joins. One draw, so that where two strokes cross the translucent line
 * is laid down once and the crossing is no stronger than the line.
 *
 * Nothing is made on a frame that need not be: the paths are read once, the
 * strokes already finished are one path made again only when another is
 * finished, the stroke being drawn is cut into a path kept for it, and the
 * paint is the painter's own.
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

    /** What is drawn this frame: the finished strokes and the ones being drawn. */
    private val drawn = Path()

    fun draw(scope: DrawScope, person: Int, written: Double, ink: GlassInk) = with(scope) {
        if (!gather(person, written)) return@with
        val fit = drawings.fit(size.width, size.height, PortraitDrawings.Inset.toPx())
        paint.strokeWidth = drawings.book.lineWidth
        paint.color = ink.line.toArgb()
        drawIntoCanvas {
            val canvas = it.nativeCanvas
            canvas.save()
            canvas.clipRect(0f, 0f, size.width, size.height)
            canvas.translate(fit.left, fit.top)
            canvas.scale(fit.scale, fit.scale)
            canvas.drawPath(drawn, paint)
            canvas.restore()
        }
    }

    /**
     * Gathers into [drawn] the strokes as far as they are drawn: the finished
     * ones, each one being drawn cut at its length so far, those not begun
     * left out. False where there is nothing yet.
     */
    private fun gather(person: Int, written: Double): Boolean {
        val art = drawings.book.portraits[person]
        val strokes = drawings.people[person]
        var done = 0
        for (stroke in art.strokes) if (stroke.drawn(written) >= 1f) done++
        if (person != finishedPerson || done != finishedCount) {
            // The finished strokes only grow while an animal is drawn, so their
            // number says which they are.
            finished.rewind()
            for ((index, stroke) in art.strokes.withIndex()) if (stroke.drawn(written) >= 1f) finished.addPath(strokes[index].path)
            finishedPerson = person
            finishedCount = done
        }
        drawn.set(finished)
        var any = done > 0
        for ((index, stroke) in art.strokes.withIndex()) {
            val part = stroke.drawn(written)
            if (part <= 0f || part >= 1f) continue
            strokes[index].cut(part, measure)
            drawn.addPath(strokes[index].partial)
            any = true
        }
        return any
    }
}

/** The shared file's animals as the platform's paths, read once for the life of the process. */
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
     * which is what it is in the gap between two animals.
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
 * `-e portrait <name>[@<seconds>]` stands that animal still on the card,
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
