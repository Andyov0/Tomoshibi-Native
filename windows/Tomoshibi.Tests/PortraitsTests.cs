using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The people on the lobby's camera card (art/portraits.json): the shared
/// file read as every platform reads it, the paths in it, when each stroke
/// is written and each person fades, and where the drawing stands on a card.
/// What these guard is a drawing that differs between the platforms: a
/// stroke written at another pace, a person who fades early, a portrait
/// standing in the air above the card's foot, or a file one platform reads
/// and another refuses.
/// </summary>
public class PortraitsTests
{
    private static string Root()
    {
        var at = new DirectoryInfo(AppContext.BaseDirectory);
        while (at != null && !File.Exists(Path.Combine(at.FullName, "apple", "i18n", "native.json"))) at = at.Parent;
        return at?.FullName ?? throw new InvalidOperationException("the repository is not above " + AppContext.BaseDirectory);
    }

    /// <summary>Two people: one of two strokes with a lift between them, one of a single closed square.</summary>
    private const string Two = """
        {
          "format": 1,
          "canvas": {"width": 100, "height": 120},
          "lineWidth": 3,
          "placement": "bottom-center",
          "gap": 0.5,
          "portraits": [
            {"name": "first", "hold": 1, "fade": 0.6, "strokes": [
              {"d": "M 10 10 L 90 10", "start": 0.25, "duration": 1},
              {"d": "M 10 20 C 30 0 70 0 90 20", "start": 1.5, "duration": 0.5}
            ]},
            {"name": "second", "hold": 0.5, "fade": 1, "strokes": [
              {"d": "M 0 0 L 10 0 L 10 10 L 0 10 Z", "start": 0, "duration": 2}
            ]}
          ]
        }
        """;

    private static PortraitArt Art => PortraitArt.Parse(Two);

    [Fact]
    public void TheSharedFileIsOneEveryPlatformCanDraw()
    {
        var art = PortraitArt.Load(Path.Combine(Root(), "art", "portraits.json"));
        Assert.Equal(1, art.Format);
        Assert.Equal(PortraitArt.BottomCentre, art.Placement);
        Assert.NotEmpty(art.Portraits);
        Assert.True(art.Round > 0);
        foreach (var person in art.Portraits)
        {
            Assert.Matches("^[a-z0-9-]+$", person.Name);
            Assert.True(person.Writing > 0, $"{person.Name} takes no time to draw");
            foreach (var stroke in person.Strokes)
            {
                Assert.Equal(PathVerb.Move, stroke.Path[0].Verb);
                Assert.True(stroke.Duration > 0, $"a stroke of {person.Name} is written in no time");
                Assert.All(stroke.Pieces, piece => Assert.True(piece.Length > 0, $"a piece of a stroke of {person.Name} has no length"));
                // The phone's microphone and camera switches lie over the
                // canvas's top right corner, and nothing is drawn under them.
                Assert.DoesNotContain(stroke.Path, s => s.Verb != PathVerb.Close && s.X > 79 && s.Y < 18);
            }
        }
        // Every name is somebody's own: the debug switch picks a portrait by it.
        Assert.Equal(art.Portraits.Count, art.Portraits.Select(p => p.Name).Distinct().Count());
    }

    [Fact]
    public void TheFileIsReadAsWritten()
    {
        var art = Art;
        Assert.Equal(100, art.CanvasWidth);
        Assert.Equal(120, art.CanvasHeight);
        Assert.Equal(3, art.LineWidth);
        Assert.Equal(0.5, art.Gap);
        Assert.Equal(new[] { "first", "second" }, art.Portraits.Select(p => p.Name));
        var first = art.Portraits[0];
        Assert.Equal(1, first.Hold);
        Assert.Equal(0.6, first.Fade);
        Assert.Equal(0.25, first.Strokes[0].Start);
        Assert.Equal(1.25, first.Strokes[0].End);
        Assert.Equal(80, first.Strokes[0].Length, 6);
        Assert.Equal("M 10 20 C 30 0 70 0 90 20", first.Strokes[1].Data);
    }

    [Fact]
    public void ATurnIsTheGapTheDrawingTheHoldAndTheFade()
    {
        var art = Art;
        // Drawn by the end of its last stroke, 1.5 + 0.5.
        Assert.Equal(2, art.Portraits[0].Writing, 9);
        Assert.Equal(0.5 + 2 + 1 + 0.6, art.Turn(art.Portraits[0]), 9);
        Assert.Equal(0.5 + 2 + 0.5 + 1, art.Turn(art.Portraits[1]), 9);
        Assert.Equal(4.1 + 4, art.Round, 9);
        Assert.Equal(0, art.StartOf(0));
        Assert.Equal(4.1, art.StartOf(1), 9);
    }

    [Fact]
    public void EachStrokeIsWrittenAlongItsLengthBetweenItsStartAndItsEnd()
    {
        var stroke = Art.Portraits[0].Strokes[0];
        Assert.Equal(0, stroke.Drawn(-0.5));
        Assert.Equal(0, stroke.Drawn(0.25));
        Assert.Equal(0.5, stroke.Drawn(0.75), 9);
        Assert.Equal(1, stroke.Drawn(1.25));
        Assert.Equal(1, stroke.Drawn(30));
        // Smoothstep: slow from the pen's landing and into its lift.
        Assert.Equal(0.028, stroke.Drawn(0.35), 9);
        Assert.Equal(0.972, stroke.Drawn(1.15), 9);
        // The pen is up between strokes: the second has not begun when the first is whole.
        var second = Art.Portraits[0].Strokes[1];
        Assert.Equal(0, second.Drawn(1.4));
    }

    [Fact]
    public void AStrokeOfSeveralMovesIsDrawnPieceByPiece()
    {
        var stroke = PortraitArt.Parse(Two.Replace("M 10 10 L 90 10", "M 0 0 L 10 0 M 0 10 L 30 10")).Portraits[0].Strokes[0];
        Assert.Equal(2, stroke.Pieces.Count);
        Assert.Equal(40, stroke.Length, 9);
        // A quarter of forty is the first piece whole and nothing of the second.
        Assert.Equal(1, stroke.PieceDrawn(0, 0.25), 9);
        Assert.Equal(0, stroke.PieceDrawn(1, 0.25), 9);
        Assert.Equal(0.5, stroke.PieceDrawn(0, 0.125), 9);
        Assert.Equal(1.0 / 3, stroke.PieceDrawn(1, 0.5), 9);
        Assert.Equal(1, stroke.PieceDrawn(1, 1), 9);
    }

    [Fact]
    public void TheGapIsEmptyGlassThenTheChildIsDrawnHeldAndFaded()
    {
        var art = Art;
        // The turn begins with the gap, in which nothing is drawn yet.
        Assert.Equal(new PortraitMoment(0, 0, -0.5, 1), art.At(0));
        Assert.Equal(-0.1, art.At(0.4).Drawing, 9);
        Assert.Equal(0, art.Portraits[0].Strokes[0].Drawn(art.At(0.4).Drawing));
        // Whole through the hold.
        Assert.Equal(1, art.At(3.4).Shown);
        // Halfway through the fade, halfway gone, on the same curve.
        Assert.Equal(0.5, art.At(3.8).Shown, 9);
        Assert.Equal(1 - Easing.Smoothstep(0.5 / 0.6), art.At(4.0).Shown, 9);
        Assert.Equal(0, art.At(4.0).Portrait);
        // The next portrait, from the start of their own turn: their gap first.
        var next = art.At(4.1 + 1);
        Assert.Equal(1, next.Portrait);
        Assert.Equal(1, next.Into, 9);
        Assert.Equal(0.5, next.Drawing, 9);
        Assert.Equal(1, next.Shown);
        // Round and round: a round later is the same moment.
        var later = art.At(1.3 + art.Round);
        Assert.Equal(0, later.Portrait);
        Assert.Equal(1.3, later.Into, 9);
        Assert.Equal(1, art.At(art.Round * 3 + 5).Portrait);
        Assert.Equal(0.9, art.At(art.Round * 3 + 5).Into, 9);
        // Before the start is the start.
        Assert.Equal(art.At(0), art.At(-5));
    }

    [Fact]
    public void NothingMovesInTheGapTheHoldOrWhileThePenIsLifted()
    {
        var art = Art;
        void Still(double at, bool moving, double until)
        {
            var said = art.Motion(at);
            Assert.Equal(moving, said.Moving);
            Assert.Equal(until, said.Until, 9);
        }
        // The gap: still, until the first stroke begins.
        Still(0.1, false, 0.75);
        Still(1.0, true, 1.75);
        // The pen lifted between strokes.
        Still(1.8, false, 2.0);
        // Finished and held, until the fade.
        Still(2.7, false, 3.5);
        Still(3.6, true, 4.1);
        // The next portrait's gap, and their one stroke.
        Still(4.3, false, 4.6);
        Still(4.7, true, 6.6);
        // A round later, a round later.
        Still(art.Round + 2.7, false, art.Round + 3.5);
    }

    [Fact]
    public void TheDrawingStandsOnTheCardsFootByTheReadmesFormula()
    {
        var art = Art;
        // A phone's card, wider than tall: the canvas fills its height.
        var fit = art.Fit(380, 292);
        Assert.Equal(292.0 / 120, fit.Scale, 9);
        Assert.Equal((380 - 100 * fit.Scale) / 2, fit.X, 9);
        Assert.Equal(0, fit.Y, 9);
        // A card taller than wide: the width less twelve each side, standing
        // on the foot with empty glass above.
        var tall = art.Fit(150, 240);
        Assert.Equal((150 - 24) / 100.0, tall.Scale, 9);
        Assert.Equal(12, tall.X, 9);
        Assert.Equal(240 - 120 * tall.Scale, tall.Y, 9);
        // Nothing to stand on: nothing drawn, rather than a drawing turned over.
        Assert.Equal(0, art.Fit(10, 240).Scale);
    }

    [Fact]
    public void PathsAreReadInTheShapesSvgWritesThem()
    {
        // Commas or spaces, signs that start a number, a decimal point that
        // does, exponents, and a move's further pairs read as lines.
        var path = PathData.Parse("M10,20 30-40L.5.5 1e1 2E-1C1 2 3 4 5 6 7 8 9 10 11 12Q 1 1 2 2Z");
        Assert.Equal(new[] { PathVerb.Move, PathVerb.Line, PathVerb.Line, PathVerb.Line, PathVerb.Cubic, PathVerb.Cubic, PathVerb.Quad, PathVerb.Close }, path.Select(s => s.Verb));
        Assert.Equal(new PathSegment(PathVerb.Line, X: 30, Y: -40), path[1]);
        Assert.Equal(new PathSegment(PathVerb.Line, X: 0.5, Y: 0.5), path[2]);
        Assert.Equal(new PathSegment(PathVerb.Line, X: 10, Y: 0.2), path[3]);
        Assert.Equal(new PathSegment(PathVerb.Cubic, 7, 8, 9, 10, 11, 12), path[5]);
        Assert.Equal(new PathSegment(PathVerb.Quad, 1, 1, X: 2, Y: 2), path[6]);
    }

    [Fact]
    public void APathsLengthIsTheLengthOfItsLine()
    {
        // A square closed by Z: its fourth side is the close.
        Assert.Equal(40, PathData.Length(PathData.Parse("M 0 0 L 10 0 L 10 10 L 0 10 Z")), 9);
        // A quarter of a circle of radius 10 as the usual cubic.
        var k = 10 * 0.5523;
        Assert.Equal(Math.PI * 5, PathData.Length(PathData.Parse($"M 10 0 C 10 {k} {k} 10 0 10")), 2);
        // A quadratic whose control lies on its chord is the chord.
        Assert.Equal(10, PathData.Length(PathData.Parse("M 0 0 Q 5 0 10 0")), 6);
        // A second move lifts the pen: the jump is not drawn.
        Assert.Equal(20, PathData.Length(PathData.Parse("M 0 0 L 10 0 M 50 50 L 50 60")), 9);
    }

    [Fact]
    public void ThePenIsAsFarAlongAsTheLengthWritten()
    {
        // An L of two equal arms: half of it written is the corner.
        var path = PathData.Parse("M 0 0 L 10 0 L 10 10");
        Assert.Equal((10.0, 0.0), PathData.PointAt(path, 0.5));
        Assert.Equal((10.0, 5.0), PathData.PointAt(path, 0.75));
        Assert.Equal((0.0, 0.0), PathData.PointAt(path, 0));
        Assert.Equal((10.0, 10.0), PathData.PointAt(path, 1));
    }

    [Fact]
    public void TheCurveIsSmoothstep()
    {
        Assert.Equal(0, Easing.Smoothstep(0));
        Assert.Equal(1, Easing.Smoothstep(1));
        Assert.Equal(0.5, Easing.Smoothstep(0.5), 9);
        Assert.Equal(0.15625, Easing.Smoothstep(0.25), 9);
        // Held to nought and one outside them.
        Assert.Equal(0, Easing.Smoothstep(-2));
        Assert.Equal(1, Easing.Smoothstep(3));
    }

    [Theory]
    [InlineData("""{"format": 2}""", "format")]
    [InlineData("""[1, 2]""", "not an object")]
    [InlineData("not json", "not JSON")]
    public void AFileOfAnotherKindIsRefused(string json, string said)
    {
        var refused = Assert.Throws<PortraitArtException>(() => PortraitArt.Parse(json));
        Assert.Contains(said, refused.Message);
    }

    [Theory]
    [InlineData("\"canvas\": {\"width\": 100, \"height\": 120}", "\"canvas\": {\"width\": 100}", "canvas.height: missing")]
    [InlineData("\"lineWidth\": 3", "\"lineWidth\": 0", "lineWidth")]
    [InlineData("\"bottom-center\"", "\"top-left\"", "placement")]
    [InlineData("\"gap\": 0.5", "\"gap\": -1", "gap")]
    [InlineData("\"duration\": 0.5", "\"duration\": -0.5", "portraits[0].strokes[1].duration")]
    [InlineData("\"name\": \"second\"", "\"name\": \"first\"", "portraits[1].name")]
    [InlineData("M 10 10 L 90 10", "M 10 10 l 80 0", "portraits[0].strokes[0].d: 'l' is relative")]
    [InlineData("M 10 10 L 90 10", "M 10 10 A 5 5 0 0 1 90 10", "'A' is not one of")]
    [InlineData("M 10 10 L 90 10", "L 10 10 L 90 10", "begins with M")]
    [InlineData("M 10 10 L 90 10", "M 10 10 L 90", "missing its numbers")]
    [InlineData("\"hold\": 0.5", "\"hold\": \"long\"", "portraits[1].hold")]
    public void AMistakeInTheFileIsSaidWhereItIs(string was, string becomes, string said)
    {
        Assert.Contains(was, Two);
        var refused = Assert.Throws<PortraitArtException>(() => PortraitArt.Parse(Two.Replace(was, becomes)));
        Assert.Contains(said, refused.Message);
    }

    [Fact]
    public void NobodyToDrawIsRefused()
    {
        var empty = Two[..Two.IndexOf("\"portraits\"", StringComparison.Ordinal)] + "\"portraits\": []}";
        Assert.Contains("nobody to draw", Assert.Throws<PortraitArtException>(() => PortraitArt.Parse(empty)).Message);
    }

    private static string Hex(Shade shade)
    {
        var (a, r, g, b) = shade.Bytes();
        return $"#{r:X2}{g:X2}{b:X2}@{a}";
    }

    /// <summary>
    /// The rows of one of art/README.md's colour tables under its heading,
    /// each as its name and its two cells (night, day).
    /// </summary>
    private static Dictionary<string, (string Night, string Day)> ReadmeTable(string heading)
    {
        var lines = File.ReadAllLines(Path.Combine(Root(), "art", "README.md"));
        var at = Array.FindIndex(lines, l => l.Trim() == "## " + heading);
        Assert.True(at >= 0, $"art/README.md has no section \"{heading}\"");
        var rows = new Dictionary<string, (string, string)>();
        for (var i = at + 1; i < lines.Length && !lines[i].StartsWith("## ", StringComparison.Ordinal); i++)
        {
            var cells = lines[i].Split('|', StringSplitOptions.TrimEntries);
            // | name | night | day |, and not the header or its rule.
            if (cells.Length != 5 || cells[1].Length == 0 || cells[1].StartsWith("---", StringComparison.Ordinal)) continue;
            rows[cells[1]] = (cells[2], cells[3]);
        }
        return rows;
    }

    /// <summary>A cell's colour, `#RRGGBB` alone or followed by "at N %", as a shade.</summary>
    private static Shade ReadmeColour(string cell)
    {
        var said = System.Text.RegularExpressions.Regex.Match(cell, @"`#([0-9A-Fa-f]{6})`(?:\s+at\s+(\d+)\s*%)?");
        Assert.True(said.Success, $"no colour in \"{cell}\"");
        var alpha = said.Groups[2].Success ? int.Parse(said.Groups[2].Value) / 100f : 1f;
        return Theme.Hex(Convert.ToInt32(said.Groups[1].Value, 16), alpha);
    }

    [Fact]
    public void TheLinesAreTheReadmesThinLinesOfTheTextColour()
    {
        var table = ReadmeTable("The lines");
        // #F5F5F7 at 60 per cent by night, #1C1C1E at 50 by day: the palette's
        // own text colours, so the README and the palette cannot drift apart.
        Assert.Equal(Hex(ReadmeColour(table["colour"].Night)), Hex(GlassInk.Night.Line));
        Assert.Equal(Hex(ReadmeColour(table["colour"].Day)), Hex(GlassInk.Day.Line));
        Assert.Equal(Theme.Dark.Foreground.WithAlpha(GlassInk.LineNight), GlassInk.Night.Line);
        Assert.Equal(Theme.Light.Foreground.WithAlpha(GlassInk.LineDay), GlassInk.Day.Line);
        Assert.Same(GlassInk.Night, GlassInk.Of(Scheme.Dark));
        Assert.Same(GlassInk.Day, GlassInk.Of(Scheme.Light));
    }

    [Fact]
    public void TheCardIsTheReadmesForPlatformsWithoutLiquidGlass()
    {
        var table = ReadmeTable("The card");
        // #202020 by night and #FFFFFF by day, solid.
        Assert.Equal(Hex(ReadmeColour(table["card"].Night)), Hex(GlassInk.Night.Card));
        Assert.Equal(Hex(ReadmeColour(table["card"].Day)), Hex(GlassInk.Day.Card));
        Assert.Equal(1f, GlassInk.Night.Card.Alpha);
        Assert.Equal(1f, GlassInk.Day.Card.Alpha);
        // "1 pt of white at 40 %" by night, "none" by day.
        var edge = System.Text.RegularExpressions.Regex.Match(table["edge"].Night, @"(\d+) pt of white at (\d+) %");
        Assert.True(edge.Success, table["edge"].Night);
        Assert.Equal(double.Parse(edge.Groups[1].Value), GlassInk.Night.CardEdgeWidth);
        Assert.Equal(Hex(Shade.White.WithAlpha(int.Parse(edge.Groups[2].Value) / 100f)), Hex(GlassInk.Night.CardEdge!.Value));
        Assert.Equal("none", table["edge"].Day);
        Assert.Null(GlassInk.Day.CardEdge);
        Assert.Equal(0, GlassInk.Day.CardEdgeWidth);
        Assert.Equal(("none", "none"), table["shadow"]);
    }
}
