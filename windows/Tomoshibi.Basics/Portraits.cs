using System.Globalization;
using System.Text.Json;

namespace Tomoshibi.Basics;

/// <summary>
/// The animals the lobby's camera card draws while the camera is off, one
/// after another, in thin lines: read from art/portraits.json at the
/// repository's top, the one file every platform plays as it is, so that
/// every platform draws the same lines at the same moments (art/README.md,
/// which this follows formula for formula).
///
/// The file says what is drawn and when; the README also says how the lines
/// and the card look (<see cref="GlassInk"/>) and where the drawing stands on
/// a desktop lobby (<see cref="PortraitStage"/>). Paths are SVG path data in
/// absolute M, L, C, Q and Z alone, on a canvas whose origin is its top left
/// with y going down. Each portrait's turn is the gap of empty glass, then the
/// strokes, each written along its length between its start and its end on
/// the smoothstep curve, then the finished drawing held, then its fade; the
/// next portrait's gap follows, round and round in the file's order.
///
/// Parsed strictly: the file ships inside the app, so anything in it this
/// cannot draw as written is a mistake for the tests that read it
/// (PortraitsTests) to catch, not a portrait to be drawn some other way.
/// </summary>
public sealed record PortraitArt(
    int Format,
    double CanvasWidth,
    double CanvasHeight,
    double LineWidth,
    string Placement,
    double Gap,
    IReadOnlyList<Portrait> Portraits)
{
    /// <summary>The one version of the format this reads; a reader that does not know the number draws nothing.</summary>
    public const int KnownFormat = 1;

    /// <summary>Scaled evenly to the card and standing on its foot in the middle (the README's formula, <see cref="Fit"/>).</summary>
    public const string BottomCentre = "bottom-center";

    /// <summary>The card's margin on the left and the right that the canvas keeps clear of when the card's width is what limits it.</summary>
    public const double Inset = 12;

    /// <summary>How long one round of everybody takes.</summary>
    public double Round => Portraits.Sum(Turn);

    /// <summary>How long a portrait's turn is: the gap, the drawing, the hold and the fade.</summary>
    public double Turn(Portrait person) => Gap + person.Writing + person.Hold + person.Fade;

    /// <summary>When the portrait at <paramref name="index"/> begins their turn (their gap), seconds into a round.</summary>
    public double StartOf(int index) => Portraits.Take(index).Sum(Turn);

    /// <summary>
    /// Who is on the card <paramref name="seconds"/> into the cycle, how far
    /// into their turn and into their drawing (after the gap; less than
    /// nought during it), and how much of them is there to see. Before the
    /// cycle's start is its start.
    /// </summary>
    public PortraitMoment At(double seconds)
    {
        var round = Round;
        var t = double.IsFinite(seconds) && seconds > 0 ? seconds : 0;
        if (round > 0) t -= Math.Floor(t / round) * round;
        var at = 0.0;
        for (var i = 0; i < Portraits.Count; i++)
        {
            var turn = Turn(Portraits[i]);
            // The last turn takes whatever the rounding left at the round's end.
            if (t < at + turn || i == Portraits.Count - 1)
            {
                var into = Math.Max(t - at, 0);
                return new PortraitMoment(i, into, into - Gap, Portraits[i].Shown(into - Gap));
            }
            at += turn;
        }
        return new PortraitMoment(0, 0, -Gap, 1);
    }

    /// <summary>
    /// Whether anything on the card moves <paramref name="seconds"/> into the
    /// cycle, and until when (seconds into the cycle, not wrapped) that stays
    /// so. A stroke being written and the fade move; the gap, the hold and a
    /// pen lifted between strokes do not, and nothing is redrawn then.
    /// </summary>
    public (bool Moving, double Until) Motion(double seconds)
    {
        var round = Round;
        var t = double.IsFinite(seconds) && seconds > 0 ? seconds : 0;
        var rounds = round > 0 ? Math.Floor(t / round) : 0;
        var moment = At(t);
        var person = Portraits[moment.Portrait];
        var start = rounds * round + StartOf(moment.Portrait);
        var tau = moment.Drawing;
        var fading = person.Writing + person.Hold;
        // The moments of the drawing at which it starts or stops moving.
        var marks = person.Strokes.SelectMany(s => new[] { s.Start, s.End })
            .Append(person.Writing).Append(fading).Append(fading + person.Fade)
            .Where(m => m > tau).Order().ToList();
        var moving = tau >= 0 && (person.Strokes.Any(s => tau >= s.Start && tau < s.End)
            || (tau >= fading && tau < fading + person.Fade));
        var next = marks.Count > 0 ? Gap + marks[0] : Turn(person);
        return (moving, start + next);
    }

    /// <summary>
    /// Where the canvas goes on a card <paramref name="width"/> by
    /// <paramref name="height"/>: scaled evenly to the card's height, or to
    /// its width less <see cref="Inset"/> at each side where that is the
    /// smaller, in the middle and standing on the card's foot.
    /// </summary>
    public PortraitFit Fit(double width, double height)
    {
        var scale = Math.Max(0, Math.Min(height / CanvasHeight, (width - 2 * Inset) / CanvasWidth));
        return new PortraitFit(scale, (width - CanvasWidth * scale) / 2, height - CanvasHeight * scale);
    }

    /// <summary>The portrait called <paramref name="name"/>, or none (-1).</summary>
    public int IndexOf(string name)
    {
        for (var i = 0; i < Portraits.Count; i++)
        {
            if (Portraits[i].Name == name) return i;
        }
        return -1;
    }

    public static PortraitArt Load(string path) => Parse(File.ReadAllText(path));

    public static PortraitArt Parse(string json)
    {
        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(json, new JsonDocumentOptions { CommentHandling = JsonCommentHandling.Skip, AllowTrailingCommas = false });
        }
        catch (JsonException fault)
        {
            throw new PortraitArtException($"not JSON: {fault.Message}");
        }
        using (document)
        {
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object) throw new PortraitArtException("the file is not an object");
            var format = Number(root, "format", "format");
            if (format != KnownFormat) throw new PortraitArtException($"format: {format} is not a format this reads ({KnownFormat})");
            var canvas = Member(root, "canvas", "canvas", JsonValueKind.Object);
            var width = Positive(canvas, "width", "canvas.width");
            var height = Positive(canvas, "height", "canvas.height");
            var lineWidth = Positive(root, "lineWidth", "lineWidth");
            var placement = Member(root, "placement", "placement", JsonValueKind.String).GetString()!;
            if (placement != BottomCentre) throw new PortraitArtException($"placement: \"{placement}\" is not a placement this draws");
            var gap = NotNegative(root, "gap", "gap");
            var list = Member(root, "portraits", "portraits", JsonValueKind.Array);
            var portraits = new List<Portrait>();
            var names = new HashSet<string>(StringComparer.Ordinal);
            var index = 0;
            foreach (var item in list.EnumerateArray())
            {
                var at = $"portraits[{index}]";
                if (item.ValueKind != JsonValueKind.Object) throw new PortraitArtException($"{at}: not an object");
                var name = Member(item, "name", $"{at}.name", JsonValueKind.String).GetString()!;
                if (name.Trim().Length == 0) throw new PortraitArtException($"{at}.name: empty");
                if (!names.Add(name)) throw new PortraitArtException($"{at}.name: \"{name}\" is the name of an earlier portrait too");
                var hold = NotNegative(item, "hold", $"{at}.hold");
                var fade = NotNegative(item, "fade", $"{at}.fade");
                var strokes = new List<PortraitStroke>();
                var s = 0;
                foreach (var stroke in Member(item, "strokes", $"{at}.strokes", JsonValueKind.Array).EnumerateArray())
                {
                    var where = $"{at}.strokes[{s}]";
                    if (stroke.ValueKind != JsonValueKind.Object) throw new PortraitArtException($"{where}: not an object");
                    var d = Member(stroke, "d", $"{where}.d", JsonValueKind.String).GetString()!;
                    IReadOnlyList<PathSegment> path;
                    try
                    {
                        path = PathData.Parse(d);
                    }
                    catch (PortraitArtException fault)
                    {
                        throw new PortraitArtException($"{where}.d: {fault.Message}");
                    }
                    strokes.Add(new PortraitStroke(d, NotNegative(stroke, "start", $"{where}.start"), NotNegative(stroke, "duration", $"{where}.duration"), path, PathData.Length(path)));
                    s++;
                }
                if (strokes.Count == 0) throw new PortraitArtException($"{at}.strokes: nobody is drawn in no strokes");
                portraits.Add(new Portrait(name, hold, fade, strokes));
                index++;
            }
            if (portraits.Count == 0) throw new PortraitArtException("portraits: there is nobody to draw");
            var art = new PortraitArt(KnownFormat, width, height, lineWidth, placement, gap, portraits);
            if (!(art.Round > 0)) throw new PortraitArtException("a round of everybody takes no time");
            return art;
        }
    }

    private static JsonElement Member(JsonElement owner, string name, string at, JsonValueKind kind)
    {
        if (!owner.TryGetProperty(name, out var value)) throw new PortraitArtException($"{at}: missing");
        if (value.ValueKind != kind) throw new PortraitArtException($"{at}: a {value.ValueKind.ToString().ToLowerInvariant()} where a {kind.ToString().ToLowerInvariant()} belongs");
        return value;
    }

    private static double Number(JsonElement owner, string name, string at)
    {
        var value = Member(owner, name, at, JsonValueKind.Number).GetDouble();
        if (!double.IsFinite(value)) throw new PortraitArtException($"{at}: not a finite number");
        return value;
    }

    private static double Positive(JsonElement owner, string name, string at)
    {
        var value = Number(owner, name, at);
        if (value <= 0) throw new PortraitArtException($"{at}: {value.ToString(CultureInfo.InvariantCulture)} is not more than nought");
        return value;
    }

    private static double NotNegative(JsonElement owner, string name, string at)
    {
        var value = Number(owner, name, at);
        if (value < 0) throw new PortraitArtException($"{at}: {value.ToString(CultureInfo.InvariantCulture)} is less than nought");
        return value;
    }
}

/// <summary>One portrait: the strokes they are drawn in, how long they stand finished, and how long they take to fade.</summary>
public sealed record Portrait(string Name, double Hold, double Fade, IReadOnlyList<PortraitStroke> Strokes)
{
    /// <summary>How long the drawing takes: until the last stroke is whole.</summary>
    public double Writing => Strokes.Count == 0 ? 0 : Strokes.Max(s => s.End);

    /// <summary>
    /// The drawing's opacity <paramref name="drawing"/> seconds after the gap:
    /// whole until the hold ends, then down the smoothstep curve over the
    /// fade. It does not fade in; it arrives by being drawn.
    /// </summary>
    public double Shown(double drawing)
    {
        var fading = drawing - Writing - Hold;
        if (Fade <= 0) return fading < 0 ? 1 : 0;
        return 1 - Easing.Smoothstep(fading / Fade);
    }
}

/// <summary>
/// A line of a portrait: its path, when it is written, and the pieces of it
/// that begin with a move, which the pen goes through in order.
/// </summary>
public sealed record PortraitStroke(string Data, double Start, double Duration, IReadOnlyList<PathSegment> Path, double Length)
{
    public double End => Start + Duration;

    /// <summary>The pieces, each from its move, with their lengths.</summary>
    public IReadOnlyList<(IReadOnlyList<PathSegment> Path, double Length)> Pieces { get; } =
        PathData.Pieces(Path).Select(p => (p, PathData.Length(p))).ToList();

    /// <summary>
    /// The share of the stroke's length that is down <paramref name="drawing"/>
    /// seconds after the gap, from nought to one, on the smoothstep curve.
    /// </summary>
    public double Drawn(double drawing)
    {
        if (Duration <= 0) return drawing >= Start ? 1 : 0;
        return Easing.Smoothstep((drawing - Start) / Duration);
    }

    /// <summary>
    /// How much of piece <paramref name="piece"/> is down when
    /// <paramref name="drawn"/> of the whole stroke is: the length is the sum
    /// of its pieces, and the pen finishes one before it starts the next.
    /// </summary>
    public double PieceDrawn(int piece, double drawn)
    {
        var before = 0.0;
        for (var i = 0; i < piece; i++) before += Pieces[i].Length;
        var length = Pieces[piece].Length;
        var down = drawn * Length - before;
        if (length <= 0) return down >= 0 && drawn > 0 ? 1 : 0;
        return Math.Clamp(down / length, 0, 1);
    }
}

/// <summary>
/// Who is on the card at a moment: which portrait, how many seconds into their
/// turn, how many into their drawing (less than nought during the gap), and
/// the drawing's opacity.
/// </summary>
public readonly record struct PortraitMoment(int Portrait, double Into, double Drawing, double Shown);

/// <summary>The canvas on a card: its scale, and where its top left goes.</summary>
public readonly record struct PortraitFit(double Scale, double X, double Y);

/// <summary>The file could not be read as written, and why, said where in it.</summary>
public sealed class PortraitArtException(string message) : Exception(message);

/// <summary>The one curve the drawings move on, the README's: smoothstep, u²(3 − 2u) for u held to nought to one.</summary>
public static class Easing
{
    public static double Smoothstep(double u)
    {
        u = Math.Clamp(u, 0, 1);
        return u * u * (3 - 2 * u);
    }
}

/// <summary>One step of a path: a move, a line, a cubic or a quadratic curve to a point, or a close.</summary>
public readonly record struct PathSegment(PathVerb Verb, double X1 = 0, double Y1 = 0, double X2 = 0, double Y2 = 0, double X = 0, double Y = 0);

public enum PathVerb { Move, Line, Cubic, Quad, Close }

/// <summary>SVG path data in the absolute M, L, C, Q and Z the shared file is written in, and its length.</summary>
public static class PathData
{
    public static IReadOnlyList<PathSegment> Parse(string data)
    {
        var segments = new List<PathSegment>();
        var i = 0;
        char? command = null;
        var any = false;
        while (true)
        {
            Skip(data, ref i);
            if (i >= data.Length) break;
            var c = data[i];
            if (char.IsLetter(c))
            {
                i++;
                if (c is not ('M' or 'L' or 'C' or 'Q' or 'Z'))
                {
                    throw new PortraitArtException(char.IsLower(c) && "mlcqz".Contains(c)
                        ? $"'{c}' is relative; the file is written in absolute commands"
                        : $"'{c}' is not one of M, L, C, Q and Z");
                }
                if (!any && c != 'M') throw new PortraitArtException("a path begins with M");
                any = true;
                command = c;
                if (c == 'Z')
                {
                    segments.Add(new PathSegment(PathVerb.Close));
                    command = null;
                    continue;
                }
                Read(data, ref i, c, segments);
                continue;
            }
            // Numbers with no letter before them repeat the last command,
            // a move's as lines (SVG's implicit commands).
            if (command is null) throw new PortraitArtException(any ? $"a number at {i} follows a close with no command" : "a path begins with M");
            Read(data, ref i, command == 'M' ? 'L' : command.Value, segments);
            if (command == 'M') command = 'L';
        }
        if (!any) throw new PortraitArtException("empty");
        return segments;
    }

    private static void Read(string data, ref int i, char command, List<PathSegment> segments)
    {
        switch (command)
        {
            case 'M':
                segments.Add(new PathSegment(PathVerb.Move, X: Number(data, ref i), Y: Number(data, ref i)));
                break;
            case 'L':
                segments.Add(new PathSegment(PathVerb.Line, X: Number(data, ref i), Y: Number(data, ref i)));
                break;
            case 'C':
                segments.Add(new PathSegment(PathVerb.Cubic, Number(data, ref i), Number(data, ref i), Number(data, ref i), Number(data, ref i), Number(data, ref i), Number(data, ref i)));
                break;
            case 'Q':
                var x1 = Number(data, ref i);
                var y1 = Number(data, ref i);
                segments.Add(new PathSegment(PathVerb.Quad, x1, y1, X: Number(data, ref i), Y: Number(data, ref i)));
                break;
        }
    }

    private static void Skip(string data, ref int i)
    {
        while (i < data.Length && (char.IsWhiteSpace(data[i]) || data[i] == ',')) i++;
    }

    private static double Number(string data, ref int i)
    {
        Skip(data, ref i);
        var start = i;
        if (i < data.Length && data[i] is '+' or '-') i++;
        var digits = 0;
        while (i < data.Length && char.IsAsciiDigit(data[i])) { i++; digits++; }
        if (i < data.Length && data[i] == '.')
        {
            i++;
            while (i < data.Length && char.IsAsciiDigit(data[i])) { i++; digits++; }
        }
        if (digits == 0) throw new PortraitArtException(i < data.Length ? $"'{data[i]}' at {i} where a number belongs" : "a command is missing its numbers");
        if (i < data.Length && data[i] is 'e' or 'E')
        {
            var mark = i;
            i++;
            if (i < data.Length && data[i] is '+' or '-') i++;
            var exponent = 0;
            while (i < data.Length && char.IsAsciiDigit(data[i])) { i++; exponent++; }
            if (exponent == 0) i = mark;
        }
        var value = double.Parse(data.AsSpan(start, i - start), NumberStyles.Float, CultureInfo.InvariantCulture);
        if (!double.IsFinite(value)) throw new PortraitArtException($"{data[start..i]} is not a finite number");
        return value;
    }

    /// <summary>The path cut at each move: the pieces a stroke of several moves is drawn in, one after another.</summary>
    public static IReadOnlyList<IReadOnlyList<PathSegment>> Pieces(IReadOnlyList<PathSegment> path)
    {
        var pieces = new List<IReadOnlyList<PathSegment>>();
        List<PathSegment>? piece = null;
        foreach (var segment in path)
        {
            if (segment.Verb == PathVerb.Move || piece == null)
            {
                piece = new List<PathSegment>();
                pieces.Add(piece);
            }
            piece.Add(segment);
        }
        return pieces;
    }

    /// <summary>The path's length in the canvas's units, its curves walked in short straight steps, a close counted as its line home.</summary>
    public static double Length(IReadOnlyList<PathSegment> path) => Walk(path).Sum(step => step.Length);

    /// <summary>
    /// Where the pen is <paramref name="fraction"/> of the way along the path
    /// by length: the tip of a stroke written that far. What every
    /// platform's trimming of a path by its length draws to.
    /// </summary>
    public static (double X, double Y) PointAt(IReadOnlyList<PathSegment> path, double fraction)
    {
        var steps = Walk(path).ToList();
        var total = steps.Sum(s => s.Length);
        if (steps.Count == 0) return path.Count > 0 ? (path[0].X, path[0].Y) : (0, 0);
        var want = Math.Clamp(fraction, 0, 1) * total;
        foreach (var step in steps)
        {
            if (want <= step.Length)
            {
                var k = step.Length > 0 ? want / step.Length : 0;
                return (step.FromX + (step.ToX - step.FromX) * k, step.FromY + (step.ToY - step.FromY) * k);
            }
            want -= step.Length;
        }
        var last = steps[^1];
        return (last.ToX, last.ToY);
    }

    private readonly record struct Step(double FromX, double FromY, double ToX, double ToY)
    {
        public double Length => Math.Sqrt((ToX - FromX) * (ToX - FromX) + (ToY - FromY) * (ToY - FromY));
    }

    private const int CurveSteps = 64;

    private static IEnumerable<Step> Walk(IReadOnlyList<PathSegment> path)
    {
        double x = 0, y = 0, startX = 0, startY = 0;
        foreach (var segment in path)
        {
            switch (segment.Verb)
            {
                case PathVerb.Move:
                    x = startX = segment.X;
                    y = startY = segment.Y;
                    break;
                case PathVerb.Line:
                    yield return new Step(x, y, segment.X, segment.Y);
                    x = segment.X;
                    y = segment.Y;
                    break;
                case PathVerb.Cubic:
                case PathVerb.Quad:
                {
                    double lastX = x, lastY = y;
                    for (var k = 1; k <= CurveSteps; k++)
                    {
                        var t = (double)k / CurveSteps;
                        var u = 1 - t;
                        double px, py;
                        if (segment.Verb == PathVerb.Cubic)
                        {
                            px = u * u * u * x + 3 * u * u * t * segment.X1 + 3 * u * t * t * segment.X2 + t * t * t * segment.X;
                            py = u * u * u * y + 3 * u * u * t * segment.Y1 + 3 * u * t * t * segment.Y2 + t * t * t * segment.Y;
                        }
                        else
                        {
                            px = u * u * x + 2 * u * t * segment.X1 + t * t * segment.X;
                            py = u * u * y + 2 * u * t * segment.Y1 + t * t * segment.Y;
                        }
                        yield return new Step(lastX, lastY, px, py);
                        lastX = px;
                        lastY = py;
                    }
                    x = segment.X;
                    y = segment.Y;
                    break;
                }
                case PathVerb.Close:
                    yield return new Step(x, y, startX, startY);
                    x = startX;
                    y = startY;
                    break;
            }
        }
    }
}

/// <summary>
/// The camera card's colours in one appearance where there is no Liquid
/// Glass, as art/README.md gives them ("The lines", "The card"): the card's
/// face, the lit rim just inside its outline where it has one, and the
/// lines, every stroke drawn so far united into one shape at the line's
/// width (the file's lineWidth at the card's scale), with round caps and
/// joins, in the text colour at an opacity, so that two lines that cross are
/// one line and not a darker crossing. Nothing else: no fill, no light along
/// the lines, no shadow, no gradient.
///
/// The card is what the iPhone's clear glass comes to on the phone's lobby,
/// measured from its pixels: #202020 with a point of white at 40 per cent by
/// night, #FFFFFF with no edge by day (Android's GlassInk, the same numbers).
/// It was the thinnest frosted material with a hairline of the text colour at
/// 16 per cent, which is what Apple before 26 draws and not what the README
/// gives Windows. The lines were once tubes of glass, with a faint fill, an
/// outline and a line of light, until the owner settled on thin single lines
/// for the animals (2026-10-27).
/// </summary>
public sealed record GlassInk(Shade Card, Shade? CardEdge, double CardEdgeWidth, Shade Line)
{
    /// <summary>The lines' share of the text colour, by night (#F5F5F7) and by day (#1C1C1E).</summary>
    public const float LineNight = 0.60f;
    public const float LineDay = 0.50f;

    public static readonly GlassInk Night = new(
        Card: Theme.Hex(0x202020),
        CardEdge: Shade.White.WithAlpha(0.40f),
        CardEdgeWidth: 1,
        Line: Theme.Dark.Foreground.WithAlpha(LineNight));

    public static readonly GlassInk Day = new(
        Card: Theme.Hex(0xFFFFFF),
        CardEdge: null,
        CardEdgeWidth: 0,
        Line: Theme.Light.Foreground.WithAlpha(LineDay));

    /// <summary>The ink of the appearance the card is drawn in: the dark's or the light's whole, never a blend.</summary>
    public static GlassInk Of(Scheme scheme) => scheme == Scheme.Light ? Day : Night;
}

/// <summary>
/// Where the animals stand on a desktop lobby with no card (art/README.md,
/// "On a desktop lobby with no card"; the Mac's StageDrawing): on the stage's
/// own ground, the part of it beside the sidebar, in the band between the
/// greeting at its head and the switches at its foot, a square canvas whose
/// own drawing (y from 8 to 76 of its 100) has its middle in the band's.
/// </summary>
public static class PortraitStage
{
    /// <summary>What the greeting and the switches take, with a margin.</summary>
    public const double Head = 130;
    public const double Foot = 96;

    /// <summary>The largest the canvas is drawn, so that a tall window does not make giants of the animals.</summary>
    public const double Most = 480;

    /// <summary>
    /// The canvas on a stage this large: its side, and where its middle is,
    /// from the stage's top left. Nothing where the stage has no band.
    /// </summary>
    public static (double Side, double CentreX, double CentreY) Place(double stageWidth, double stageHeight)
    {
        var band = Math.Max(0, stageHeight - Head - Foot);
        var side = Math.Max(0, Math.Min(Math.Min(band / 0.75, stageWidth * 0.5), Most));
        return (side, stageWidth / 2, Head + band / 2 + side * 0.08);
    }
}
