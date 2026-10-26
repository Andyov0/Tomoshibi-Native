package app.tomoshibi.ui.lobby

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

// The people the lobby's camera card draws while the camera is off
// (PreviewGlass.kt), as every platform reads them: art/portraits.json at the
// repository's root, which the build copies into the app's assets unchanged
// (app/build.gradle.kts `portraitAssets`). The Apple apps draw the same lines
// at the same pace from the same file, so nothing about a person is written
// here: this reads the file and says where the pen is at a given moment, and
// PreviewGlass draws it.
//
// The format and its timing are art/README.md's, which every platform
// follows; in short:
//
//     { "format": 1, "canvas": {"width": W, "height": H}, "lineWidth": n,
//       "placement": "bottom-center", "gap": seconds,
//       "portraits": [{ "name": ..., "hold": s, "fade": s,
//                       "strokes": [{ "d": "M .. C .. Q .. L .. Z", "start": s, "duration": s }] }] }
//
// The canvas's origin is its top left corner, y down. A person's turn is the
// gap (empty glass), their drawing, their hold and their fade, in the order
// of the array, round and round. Each stroke is drawn along its length from
// nothing to all of it between `start` and `start + duration` seconds after
// the gap, eased in and out (smoothstep); the finished person stands for
// `hold` seconds and fades over `fade` on the same curve. `d` holds absolute
// M, L, C, Q and Z only.

/** The whole file: the canvas the people are drawn on and the people in turn. */
class PortraitBook(
    val width: Float,
    val height: Float,
    /** How wide a line is, in the canvas's units. */
    val lineWidth: Float,
    val placement: Placement,
    /** Seconds of empty glass before each person. */
    val gap: Double,
    val portraits: List<PortraitArt>,
) {
    /**
     * How the canvas is laid on the card: `bottom-center`, scaled to the
     * card with its proportions kept, standing on the card's foot in the
     * middle across (PortraitDrawings `fit` has the README's sums).
     */
    enum class Placement { BottomCenter }

    companion object {
        const val FORMAT = 1

        /**
         * Reads the file. A file that says anything this cannot draw as
         * written (another format, a placement or a path command it does not
         * know, a stroke that takes no time) is refused whole rather than
         * drawn as something else: the drawing would then differ from the
         * iPhone's beside it with nothing to say so.
         */
        fun parse(json: String): PortraitBook = try {
            read(JSONObject(json))
        } catch (fault: JSONException) {
            throw PortraitFormatException("not the portraits' JSON: ${fault.message}")
        }

        private fun read(root: JSONObject): PortraitBook {
            val format = root.getInt("format")
            if (format != FORMAT) throw PortraitFormatException("format $format, where this reads $FORMAT")
            val canvas = root.getJSONObject("canvas")
            val width = positive(canvas.getDouble("width"), "canvas width")
            val height = positive(canvas.getDouble("height"), "canvas height")
            val placement = when (val said = root.getString("placement")) {
                "bottom-center" -> Placement.BottomCenter
                else -> throw PortraitFormatException("placement \"$said\"")
            }
            val people = root.getJSONArray("portraits")
            if (people.length() == 0) throw PortraitFormatException("nobody to draw")
            return PortraitBook(
                width = width.toFloat(),
                height = height.toFloat(),
                lineWidth = positive(root.getDouble("lineWidth"), "lineWidth").toFloat(),
                placement = placement,
                gap = notNegative(root.getDouble("gap"), "gap"),
                portraits = people.objects().map(::person),
            )
        }

        private fun person(it: JSONObject): PortraitArt {
            val name = it.getString("name")
            val strokes = it.getJSONArray("strokes")
            if (strokes.length() == 0) throw PortraitFormatException("$name has no strokes")
            return PortraitArt(
                name = name,
                hold = notNegative(it.getDouble("hold"), "$name's hold"),
                fade = notNegative(it.getDouble("fade"), "$name's fade"),
                strokes = strokes.objects().mapIndexed { index, stroke ->
                    PortraitStroke(
                        path = PathData.parse(stroke.getString("d")),
                        start = notNegative(stroke.getDouble("start"), "$name's stroke $index start"),
                        duration = positive(stroke.getDouble("duration"), "$name's stroke $index duration"),
                    )
                },
            )
        }

        private fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }

        private fun positive(value: Double, what: String): Double =
            value.takeIf { it.isFinite() && it > 0 } ?: throw PortraitFormatException("$what is $value")

        private fun notNegative(value: Double, what: String): Double =
            value.takeIf { it.isFinite() && it >= 0 } ?: throw PortraitFormatException("$what is $value")
    }
}

/** One person: their strokes, and how long they stand finished and take to fade. */
class PortraitArt(
    /** What a debug switch calls them by (`portrait` on a debug launch). */
    val name: String,
    val hold: Double,
    val fade: Double,
    val strokes: List<PortraitStroke>,
) {
    /** How long the drawing takes: until the last stroke is finished, whichever that is. */
    val writing: Double = strokes.maxOf { it.start + it.duration }
}

/** One stroke of the pen and when it is drawn, in seconds after its person's gap. */
class PortraitStroke(val path: List<PathData.Segment>, val start: Double, val duration: Double) {
    /**
     * How much of the stroke is down `seconds` after the gap, from nought to
     * one along its length, slow out of its start and into its end.
     */
    fun drawn(seconds: Double): Float = smoothstep((seconds - start) / duration)

    companion object {
        /** The README's ease, for the pen and the fade alike: u * u * (3 - 2u), u clamped to nought to one. */
        fun smoothstep(u: Double): Float {
            if (u.isNaN() || u <= 0) return 0f
            if (u >= 1) return 1f
            return (u * u * (3 - 2 * u)).toFloat()
        }
    }
}

/** The file says something this cannot draw as written. */
class PortraitFormatException(message: String) : IllegalArgumentException(message)

/**
 * The absolute path commands the drawings use (M, L, C, Q, Z) read out of
 * an SVG path's `d`, with SVG's own allowances: commas or spaces between
 * numbers, a command's numbers repeated for another of the same (pairs after
 * an M being lines). Relative commands and the others (H, V, S, T, A) are
 * refused: the format promises they are not there, and a drawing quietly
 * missing a stroke is worse than one refused.
 */
object PathData {
    sealed interface Segment {
        data class Move(val x: Float, val y: Float) : Segment
        data class Line(val x: Float, val y: Float) : Segment
        data class Cubic(val x1: Float, val y1: Float, val x2: Float, val y2: Float, val x: Float, val y: Float) : Segment
        data class Quad(val x1: Float, val y1: Float, val x: Float, val y: Float) : Segment
        data object Close : Segment
    }

    fun parse(d: String): List<Segment> {
        val tokens = tokens(d)
        val out = ArrayList<Segment>()
        var i = 0
        var command: Char? = null
        fun number(): Float {
            val token = tokens.getOrNull(i) ?: throw PortraitFormatException("\"$d\" ends inside a command")
            i++
            return (token as? Token.Number)?.value ?: throw PortraitFormatException("\"$d\": a number expected where ${(token as Token.Command).letter} is")
        }
        while (i < tokens.size) {
            val token = tokens[i]
            if (token is Token.Command) {
                command = token.letter
                i++
                if (command == 'Z') {
                    out += Segment.Close
                    continue
                }
            } else if (command == null || command == 'Z') {
                throw PortraitFormatException("\"$d\": a number with no command before it")
            }
            when (command) {
                'M' -> {
                    out += Segment.Move(number(), number())
                    // Further pairs after a move are lines (SVG 1.1, 8.3.2).
                    command = 'L'
                }
                'L' -> out += Segment.Line(number(), number())
                'C' -> out += Segment.Cubic(number(), number(), number(), number(), number(), number())
                'Q' -> out += Segment.Quad(number(), number(), number(), number())
            }
        }
        if (out.isEmpty()) throw PortraitFormatException("an empty stroke")
        if (out.first() !is Segment.Move) throw PortraitFormatException("\"$d\" does not start with M")
        return out
    }

    private sealed interface Token {
        data class Command(val letter: Char) : Token
        data class Number(val value: Float) : Token
    }

    private fun tokens(d: String): List<Token> {
        val out = ArrayList<Token>()
        var i = 0
        while (i < d.length) {
            val c = d[i]
            when {
                c.isWhitespace() || c == ',' -> i++
                c in "MLCQZ" -> {
                    out += Token.Command(c)
                    i++
                }
                c.isLetter() -> throw PortraitFormatException("path command '$c', where only absolute M, L, C, Q and Z are drawn")
                else -> {
                    val start = i
                    if (d[i] == '-' || d[i] == '+') i++
                    var dot = false
                    while (i < d.length && (d[i].isDigit() || (d[i] == '.' && !dot))) {
                        if (d[i] == '.') dot = true
                        i++
                    }
                    if (i < d.length && (d[i] == 'e' || d[i] == 'E')) {
                        i++
                        if (i < d.length && (d[i] == '-' || d[i] == '+')) i++
                        while (i < d.length && d[i].isDigit()) i++
                    }
                    val value = d.substring(start, i).toFloatOrNull()
                        ?: throw PortraitFormatException("\"${d.substring(start, minOf(d.length, start + 8))}\" in a path is not a number")
                    if (!value.isFinite()) throw PortraitFormatException("a path coordinate is $value")
                    out += Token.Number(value)
                }
            }
        }
        return out
    }
}

/**
 * Who is on the card at a moment of the cycle, and how far they are drawn:
 * each person's turn is the gap, their drawing, their hold and their fade,
 * one after another in the file's order, round and round.
 */
class PortraitTimeline(val book: PortraitBook) {
    private val turns = book.portraits.map { book.gap + it.writing + it.hold + it.fade }
    private val starts = turns.runningFold(0.0) { at, turn -> at + turn }.dropLast(1)

    /** One round, everybody once. */
    val round: Double = turns.sum()

    /** The moment `t` seconds into the cycle. */
    fun at(t: Double): Moment {
        val (person, into, start) = place(t)
        val art = book.portraits[person]
        // Seconds after the gap, which is what the strokes' times count
        // from: below nought during the gap, when nothing is drawn.
        val tau = into - book.gap
        val shown = if (art.fade > 0) 1f - PortraitStroke.smoothstep((tau - art.writing - art.hold) / art.fade)
        else if (tau < art.writing + art.hold) 1f else 0f
        val (moving, until) = motion(art, tau)
        return Moment(person, tau, shown, moving, start + book.gap + until)
    }

    /**
     * Whether anything on the card moves `tau` seconds after this person's
     * gap, and until when (in the same seconds) that stays so: a stroke
     * being drawn and the fade move; the gap, the moments between strokes
     * and the finished person held do not, and nothing is drawn then
     * (PortraitCycle).
     */
    private fun motion(art: PortraitArt, tau: Double): Pair<Boolean, Double> {
        val writing = art.writing
        val fadeStart = writing + art.hold
        val turnEnd = fadeStart + art.fade
        if (tau < 0) return false to (art.strokes.minOf { it.start }.takeIf { it > 0 } ?: 0.0)
        if (tau < writing) {
            val drawing = art.strokes.filter { tau >= it.start && tau < it.start + it.duration }
            if (drawing.isNotEmpty()) {
                // Moving until a stroke now being drawn is finished; the next
                // look says whether another keeps it moving.
                return true to drawing.minOf { it.start + it.duration }
            }
            // Between strokes, the pen lifted: still until the next starts.
            return false to (art.strokes.map { it.start }.filter { it > tau }.minOrNull() ?: writing)
        }
        if (tau < fadeStart) return false to fadeStart
        return true to turnEnd
    }

    /** Whose turn it is `t` seconds into the cycle, how far into it, and when it began. */
    private fun place(t: Double): Triple<Int, Double, Double> {
        val clamped = if (t.isFinite()) maxOf(t, 0.0) else 0.0
        val base = Math.floor(clamped / round) * round
        val inRound = clamped - base
        val person = starts.indexOfLast { it <= inRound }.coerceAtLeast(0)
        // A moment at the very end of a round, by rounding, is the start of
        // the next one rather than past the end of the last person's turn.
        if (inRound - starts[person] >= turns[person]) return Triple(0, 0.0, base + round)
        val start = base + starts[person]
        return Triple(person, (clamped - start).coerceAtLeast(0.0), start)
    }

    /**
     * A moment of the cycle. [person] indexes the book's portraits, [tau] is
     * seconds after their gap (below nought during it), [shown] how much of
     * them is left to see as they fade, and [moving] whether anything on the
     * card is moving now, which stays so until [until] seconds into the cycle.
     */
    data class Moment(val person: Int, val tau: Double, val shown: Float, val moving: Boolean, val until: Double)
}
