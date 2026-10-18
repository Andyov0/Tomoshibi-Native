package app.tomoshibi.ui.kit

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp

/** SwiftUI's `Capsule()`: ends as round as the shape is high. */
val Capsule: Shape = RoundedCornerShape(percent = 50)

/**
 * SwiftUI's `RoundedRectangle(cornerRadius:style: .continuous)`, which every
 * card, pane and field of the Apple apps is drawn in.
 *
 * A continuous corner is not a quarter circle: the curve starts further
 * along each edge and bends gradually, so the edge flows into the corner
 * with no visible join. Compose's rounded rectangle is the circular kind,
 * and beside the iPhone its panes read as a different, stiffer shape. The
 * path is the one UIKit draws for `UIBezierPath(roundedRect:cornerRadius:)`
 * since iOS 7, whose curve reaches 1.528665 radii along each edge. Where a
 * side is too short for that, the corner falls back to a circular one of the
 * radius that fits, as UIKit does.
 */
class ContinuousShape(private val radius: Dp) : Shape {
    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline {
        val r = with(density) { radius.toPx() }
        val shortest = minOf(size.width, size.height)
        if (r <= 0f) return Outline.Rectangle(size.toRect())
        if (r * SPAN > shortest / 2) {
            val fitted = minOf(r, shortest / 2)
            return Outline.Rounded(RoundRect(size.toRect(), fitted, fitted))
        }
        return Outline.Generic(continuousPath(size, r))
    }

    override fun equals(other: Any?) = other is ContinuousShape && other.radius == radius
    override fun hashCode() = radius.hashCode()
    override fun toString() = "ContinuousShape($radius)"

    private companion object {
        const val SPAN = 1.528665f

        /**
         * The corner's three curves, each as two control points and an end,
         * every point as fractions of the radius back along the edge
         * arriving at the corner and on along the edge leaving it; between
         * the first two, a line to [joint].
         */
        val corner = floatArrayOf(
            1.08849323f, 0f, 0.86840689f, 0f, 0.66993427f, 0.06549600f,
            0.37282392f, 0.16905899f, 0.16905883f, 0.37282401f, 0.07491176f, 0.63149399f,
            0f, 0.86840701f, 0f, 1.08849299f, 0f, 1.52866483f,
        )
        val joint = floatArrayOf(0.63149399f, 0.07491100f)

        fun continuousPath(size: Size, r: Float): Path = Path().apply {
            val w = size.width
            val h = size.height
            moveTo(SPAN * r, 0f)
            // Each corner as a point and the two directions of its edges:
            // the one arriving at it and the one leaving it.
            turn(this, Offset(w, 0f), Offset(1f, 0f), Offset(0f, 1f), r)
            turn(this, Offset(w, h), Offset(0f, 1f), Offset(-1f, 0f), r)
            turn(this, Offset(0f, h), Offset(-1f, 0f), Offset(0f, -1f), r)
            turn(this, Offset(0f, 0f), Offset(0f, -1f), Offset(1f, 0f), r)
            close()
        }

        fun turn(path: Path, at: Offset, arriving: Offset, leaving: Offset, r: Float) {
            fun point(back: Float, on: Float) = at - arriving * (back * r) + leaving * (on * r)
            val start = point(SPAN, 0f)
            path.lineTo(start.x, start.y)
            var i = 0
            while (i < corner.size) {
                val a = point(corner[i], corner[i + 1])
                val b = point(corner[i + 2], corner[i + 3])
                val c = point(corner[i + 4], corner[i + 5])
                path.cubicTo(a.x, a.y, b.x, b.y, c.x, c.y)
                if (i == 0) point(joint[0], joint[1]).let { path.lineTo(it.x, it.y) }
                i += 6
            }
        }

        fun Size.toRect() = androidx.compose.ui.geometry.Rect(Offset.Zero, this)
    }
}

/** A continuous rounded rectangle of `radius` points. */
fun rounded(radius: Float): Shape = ContinuousShape(radius.dp)
