package app.tomoshibi.ui.lobby

import android.content.Context
import android.graphics.PathMeasure
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Theme
import app.tomoshibi.ui.InAppearance
import app.tomoshibi.ui.TestAppearance
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * The camera card's portraits as the app draws them: the shared file is in the
 * app, a stroke is cut along its length, the canvas is placed on the card as
 * the file says, and pictures of the card at each stage of a drawing and of
 * everybody finished, recorded with `./gradlew :app:recordRoborazziDebug`
 * (`-Pappearance=light` for the day) into src/test/screenshots/portraits.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h892dp-hdpi")
@OptIn(ExperimentalRoborazziApi::class)
class PreviewGlassTest {
    @get:Rule
    val compose = createComposeRule()

    private val context: Context get() = ApplicationProvider.getApplicationContext()

    @After
    fun tearDown() {
        PortraitDebug.said = null
    }

    @Test
    fun theSharedFileIsInTheApp() {
        val drawings = Portraits.load(context)
        assertNotNull("the app has no portraits.json in its assets", drawings)
        drawings!!
        val shared = PortraitBook.parse(File("../../art/portraits.json").readText())
        assertEquals(shared.portraits.map { it.name to it.strokes.size }, drawings.book.portraits.map { it.name to it.strokes.size })
        assertEquals(shared.round(), drawings.timeline.round, 1e-9)
    }

    private fun PortraitBook.round() = PortraitTimeline(this).round

    @Test
    fun aStrokeIsCutAlongItsLength() {
        val measure = PathMeasure()
        val line = StrokePath(PathData.parse("M 0 0 L 100 0"))
        assertEquals(100f, line.length, 0.01f)
        line.cut(0.25f, measure)
        assertEquals(25f, PathMeasure(line.partial, false).length, 0.01f)
        line.cut(1f, measure)
        assertEquals(100f, PathMeasure(line.partial, false).length, 0.01f)

        // A stroke lifted inside its `d` is cut across its pieces in their order.
        val lifted = StrokePath(PathData.parse("M 0 0 L 10 0 M 0 10 L 30 10"))
        assertEquals(40f, lifted.length, 0.01f)
        lifted.cut(0.5f, measure)
        val pieces = PathMeasure(lifted.partial, false)
        assertEquals(10f, pieces.length, 0.01f)
        pieces.nextContour()
        assertEquals(10f, pieces.length, 0.01f)
        lifted.cut(0.1f, measure)
        val first = PathMeasure(lifted.partial, false)
        assertEquals(4f, first.length, 0.01f)
        assertEquals(false, first.nextContour())
    }

    @Test
    fun theCanvasStandsOnTheCardsFootInTheMiddle() {
        // The README's sums: on a phone's card, wider than tall, the canvas
        // fills the card's height.
        val square = PortraitDrawings(PortraitBook.parse(oneStroke(100, 100)))
        assertEquals(PortraitDrawings.Fit(2.92f, 44f, 0f), square.fit(380f, 292f, 12f))
        // On a card taller than wide it fills the width less the inset at
        // either side, and leaves empty glass above the head.
        val tall = square.fit(200f, 400f, 12f)
        assertEquals(1.76f, tall.scale, 1e-5f)
        assertEquals(12f, tall.left, 1e-4f)
        assertEquals(224f, tall.top, 1e-3f)
        // A canvas wider than the card is fitted across it all the same.
        val wide = PortraitDrawings(PortraitBook.parse(oneStroke(300, 100))).fit(200f, 100f, 0f)
        assertEquals(2f / 3f, wide.scale, 1e-6f)
        assertEquals(0f, wide.left, 1e-4f)
        assertEquals(100f / 3f, wide.top, 1e-4f)
    }

    private fun oneStroke(width: Int, height: Int) = """
        {"format": 1, "canvas": {"width": $width, "height": $height}, "lineWidth": 2, "placement": "bottom-center", "gap": 0,
         "portraits": [{"name": "a", "hold": 1, "fade": 1, "strokes": [{"d": "M 0 0 L 1 1", "start": 0, "duration": 1}]}]}
    """.trimIndent()

    /** The card as the phone's lobby has it, 380 by 292, on the page's ground, each portrait finished. */
    @Test
    fun everybodyFinished() {
        val drawings = Portraits.load(context)!!
        card()
        for (art in drawings.book.portraits) {
            PortraitDebug.said = art.name
            compose.waitForIdle()
            compose.onNodeWithTag("card").captureRoboImage(file("finished-${art.name}"))
        }
    }

    /**
     * Where lines cross (a shell over a chick, a wing over a penguin) the
     * line is laid down once: no pixel of the card is further from its face
     * than the line's colour over it. Each stroke drawn on its own laid the
     * translucent line down twice there, a brighter knot at night and a
     * darker one by day.
     */
    @Test
    fun crossingLinesAreNoBrighterThanOneLine() {
        val drawings = Portraits.load(context)!!
        card()
        val palette = TestAppearance.palette
        val ink = GlassInk.of(palette)
        val line = ink.line.compositeOver(ink.card.compositeOver(palette.ground))
        for (art in drawings.book.portraits) {
            PortraitDebug.said = art.name
            compose.waitForIdle()
            val pixels = compose.onNodeWithTag("card").captureToImage().toPixelMap()
            var furthest = if (palette.dark) 0f else 1f
            for (x in 0 until pixels.width) for (y in 0 until pixels.height) {
                val red = pixels[x, y].red
                furthest = if (palette.dark) max(furthest, red) else min(furthest, red)
            }
            assertEquals(art.name, line.red, furthest, 2f / 255)
        }
    }

    /**
     * The card is what art/README.md gives for a platform without Liquid
     * Glass: by night #202020 with white at 40 per cent along the inside of
     * its outline, by day #FFFFFF with no edge, on the page's own ground
     * (#000000, #F2F2F7). Looked at with nothing drawn on it yet, half way
     * down its left side, where the outline is straight. The card is solid:
     * by night the thinnest material it was before is a step darker over
     * the black page, and lighter over the wide lobby's backdrop.
     */
    @Test
    fun theCardIsTheColoursTheReadmeGives() {
        PortraitDebug.said = Portraits.load(context)!!.book.portraits.first().name + "@0"
        card()
        val palette = TestAppearance.palette
        val pixels = compose.onNodeWithTag("card").captureToImage().toPixelMap()
        val inset = (16 * context.resources.displayMetrics.density).roundToInt()
        val middle = pixels.height / 2
        // A flat fill is drawn exactly; the edge, a stroke, to within a step or two.
        fun near(expected: Color, actual: Color, what: String, within: Float = 0.6f / 255) {
            assertEquals("$what red", expected.red, actual.red, within)
            assertEquals("$what green", expected.green, actual.green, within)
            assertEquals("$what blue", expected.blue, actual.blue, within)
        }
        near(palette.ground, pixels[inset / 2, middle], "page")
        near(if (palette.dark) Color(0xFF202020) else Color.White, pixels[pixels.width / 2, middle], "card")
        val edge = if (palette.dark) Color.White.copy(alpha = 0.4f).compositeOver(Color(0xFF202020)) else Color.White
        near(edge, pixels[inset, middle], "edge", within = 2f / 255)
    }

    /** One portrait at each stage of its drawing, for the strip of frames beside the iPhone's. */
    @Test
    fun oneBeingDrawn() {
        val drawings = Portraits.load(context)!!
        val art = drawings.book.portraits.first()
        val steps = 10
        card()
        for (step in 0..steps) {
            val at = art.writing * step / steps
            PortraitDebug.said = "${art.name}@$at"
            compose.waitForIdle()
            compose.onNodeWithTag("card").captureRoboImage(file("stage-${art.name}-%02d".format(step)))
        }
    }

    private fun card() {
        PortraitDebug.said = PortraitDebug.said ?: Portraits.load(context)!!.book.portraits.first().name
        compose.setContent {
            InAppearance {
                Box(Modifier.testTag("card").background(Theme.ground).padding(16.dp)) {
                    PreviewGlass(30.dp, held = false, Modifier.size(380.dp, 292.dp))
                }
            }
        }
        compose.waitForIdle()
    }

    private fun file(name: String) = "src/test/screenshots/portraits/$name-${if (TestAppearance.light) "light" else "dark"}.png"
}
