using System.Diagnostics;
using System.Globalization;
using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Effects;
using Microsoft.Graphics.Canvas.Geometry;
using Microsoft.UI.Composition;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.UI.Kit;
using Windows.UI.ViewManagement;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// The lobby's picture of yourself with the camera off: a pane of glass
/// with children drawn on it in lines of glass, one after another, stroke
/// by stroke, the way the iPhone writes its first "hello" (the iPhone's
/// PreviewGlass; the owner asked for the same card here, 2026-10-27). On
/// the narrow lobby it is the card itself, in-app acrylic over the page
/// with a hairline border, as the phone's is glass; on the wide one there
/// is no card, the camera filling the window behind the sidebar, and the
/// children stand on the stage beside it, where the picture would be.
///
/// Who is drawn and when is art/portraits.json, which every platform plays
/// as it is (Tomoshibi.Basics.PortraitArt); how a line of glass looks is
/// the README beside it (PortraitGlass), the numbers for a platform without
/// Liquid Glass.
///
/// Drawn by the compositor. Each piece of a stroke is a path whose TrimEnd
/// follows one clock, a property the compositor moves itself, through the
/// README's formulas written as expressions; so while a child is written
/// or fades the window's thread does nothing. The clock moves only while
/// something on the card does: in the gap and the hold, and whenever the
/// card cannot be seen (the window minimised or hidden, a dialog or sheet
/// over the lobby, the card scrolled away, the card gone), it stands still
/// and nothing is drawn again, and it goes on from where it stood. With
/// Windows' animation effects off the first child stands finished.
///
/// A line of glass is the README's: every stroke drawn so far, united into
/// one shape, filled faintly with the text colour, its outline drawn in the
/// text colour at a third, and the light along the strokes' centre lines.
/// Direct2D can stroke a path but not stroke the outline of a union of
/// growing strokes, so each stroke is drawn four times in four pure colours
/// into a layer: wider than the line by the edge (the outline's outer half),
/// as wide as the line (its inner half, over the fill), narrower by the
/// edge (the fill), and the light. Every stroke's widest goes down first and
/// its light last, so where two lines cross the later fill covers the
/// earlier edge and they are one shape, as the README asks, rather than
/// each showing its edges through the other. One colour matrix then turns
/// each pure colour into its part of the glass, every part its own colour
/// at its own strength wherever it lies.
/// </summary>
internal sealed class PreviewGlass : Grid
{
    private static PortraitArt? _art;
    private static bool _read;

    /// <summary>
    /// The shared file, as the app carries it (art\portraits.json beside the
    /// program), read once. A file that cannot be read is said in the log and
    /// leaves the glass empty, which is still the card it was before.
    /// </summary>
    public static PortraitArt? Art
    {
        get
        {
            if (_read) return _art;
            _read = true;
            try
            {
                _art = PortraitArt.Load(Path.Combine(AppContext.BaseDirectory, "art", "portraits.json"));
                Log.Line($"portraits: {_art.Portraits.Count} children, a round of {_art.Round:0.##} s");
            }
            catch (Exception fault)
            {
                Log.Line($"portraits not drawn: {fault.Message}");
            }
            return _art;
        }
    }

    private readonly Border _canvas = new() { IsHitTestVisible = false };
    private readonly double _radius;
    private readonly string? _pose;
    private readonly GlassLine _look = PortraitGlass.Of(Ink.Scheme);
    private readonly bool _still = !new UISettings().AnimationsEnabled;
    private Drawing? _drawing;
    private bool _loaded;
    private bool _inView = true;

    /// <param name="radius">The corner the drawing is cut to.</param>
    /// <param name="pane">Whether this is a card of its own (the narrow lobby's), or children standing on what is already there (the wide stage).</param>
    /// <param name="pose">TOMOSHIBI_DEBUG_PORTRAIT in a debug build: one moment, held, for a picture of it.</param>
    public PreviewGlass(double radius, bool pane, string? pose = null)
    {
        _radius = radius;
        _pose = pose;
        IsHitTestVisible = false;
        CornerRadius = new CornerRadius(radius);
        if (pane)
        {
            // The README's card: the thinnest frosted material there is and a
            // one-point border of the text colour, flat, with no lit edge.
            Background = new Microsoft.UI.Xaml.Media.AcrylicBrush
            {
                TintColor = Ink.Of(Ink.Palette.Surface),
                TintOpacity = 0.3,
                TintLuminosityOpacity = 0.75,
                FallbackColor = Ink.Of(Ink.Palette.Surface),
            };
            BorderBrush = Ink.Brush(_look.PaneEdge);
            BorderThickness = new Thickness(_look.PaneEdgeWidth);
        }
        Children.Add(_canvas);
        // Decoration, which says nothing a screen reader should read.
        this.Quiet();
        Loaded += (_, _) =>
        {
            _loaded = true;
            if (Overlay.Current is { } overlay) overlay.CoveredChanged += Run;
            if (MainWindow.Current is { } window) window.SightChanged += Run;
            Build();
            Run();
        };
        Unloaded += (_, _) =>
        {
            _loaded = false;
            if (Overlay.Current is { } overlay) overlay.CoveredChanged -= Run;
            if (MainWindow.Current is { } window) window.SightChanged -= Run;
            Run();
        };
        // The first size is the first moment there is anything to draw on.
        SizeChanged += (_, _) =>
        {
            Build();
            Lay();
            Run();
        };
        // Scrolled out of the narrow lobby's list: nothing of it shows, so
        // nothing of it is drawn.
        EffectiveViewportChanged += (_, e) =>
        {
            var seen = e.EffectiveViewport;
            var now = seen.Width > 0 && seen.Height > 0
                && seen.Right > 0 && seen.Bottom > 0 && seen.Left < ActualWidth && seen.Top < ActualHeight;
            if (now == _inView) return;
            _inView = now;
            Run();
        };
    }

    /// <summary>Whether the drawing moves now: on the screen, in a window that can be seen, with nothing over it.</summary>
    private bool Running =>
        _loaded && _inView && MainWindow.Current?.InSight != false && Overlay.Current?.Covered != true;

    private void Build()
    {
        if (_drawing != null || Art is not { } art || ActualWidth <= 0 || ActualHeight <= 0) return;
        try
        {
            _drawing = new Drawing(ElementCompositionPreview.GetElementVisual(_canvas).Compositor, art, _look, DispatcherQueue);
        }
        catch (Exception fault)
        {
            // Win2D or the compositor refused: the glass stays an empty card.
            Log.Line($"portraits not drawn: {fault.Message}");
            return;
        }
        ElementCompositionPreview.SetElementChildVisual(_canvas, _drawing.Root);
        Lay();
        if (Pose() is { } at) _drawing.Hold(at);
    }

    /// <summary>A moment the drawing is held at rather than played: asked for by a test, or the only one with animation effects off.</summary>
    private double? Pose()
    {
        if (Art is not { } art) return null;
        if (_pose is { } pose)
        {
            // "<name>" finished, "<name>@<seconds into their drawing>", or
            // "@<seconds into the cycle>".
            var at = pose.IndexOf('@');
            var name = at < 0 ? pose : pose[..at];
            var seconds = at < 0 ? double.PositiveInfinity
                : double.TryParse(pose[(at + 1)..], NumberStyles.Float, CultureInfo.InvariantCulture, out var s) ? s : 0;
            if (name.Length == 0) return seconds;
            var index = Math.Max(art.IndexOf(name), 0);
            var person = art.Portraits[index];
            // Into the fade if asked, but not past the turn.
            return art.StartOf(index) + art.Gap + Math.Min(double.IsFinite(seconds) ? seconds : person.Writing, person.Writing + person.Hold + person.Fade);
        }
        // Somebody who turned animation off sees the first child whole.
        if (_still) return art.Gap + art.Portraits[0].Writing;
        return null;
    }

    private void Lay()
    {
        if (_drawing == null) return;
        _drawing.Lay(new Vector2((float)ActualWidth, (float)ActualHeight), (float)_radius);
    }

    private void Run()
    {
        if (_drawing == null || Pose() != null) return;
        _drawing.Run(Running);
    }

    /// <summary>The composition tree of the child on the card, the clock it is drawn by, and what moves the clock.</summary>
    private sealed class Drawing
    {
        public readonly ContainerVisual Root;
        private readonly Compositor _compositor;
        private readonly PortraitArt _art;
        private readonly GlassLine _look;
        private readonly CompositionPropertySet _clock;
        private readonly CompositionRoundedRectangleGeometry _cut;
        private readonly CompositionEffectFactory _effects;
        private readonly CompositionColorBrush[] _inks;
        private readonly DispatcherQueueTimer _timer;
        private readonly CanvasDevice _device = CanvasDevice.GetSharedDevice();
        private Vector2 _size;
        private Child? _child;
        // Every child once made, kept for the next round: making one is a
        // few hundred milliseconds of Direct2D paths and shapes, which made
        // every change of child a stutter.
        private readonly Dictionary<int, Child> _made = new();
        private bool _running;
        private bool _ticking;
        // Seconds of the cycle played before the present run, and when the
        // present run began: the cycle's time stands still while it does not run.
        private double _banked;
        private long? _since;

        /// <summary>One child: the visual they fade in, their layer, their shapes in the order they are drawn, and their strokes.</summary>
        private sealed record Child(int Index, LayerVisual Layer, CompositionEffectBrush Glass, CompositionContainerShape Fit, List<CompositionSpriteShape>[] Parts, Line[] Lines, ExpressionAnimation Shown)
        {
            /// <summary>Whether the fade's expression is on the glass, or it stands at a value of its own.</summary>
            public bool Fading { get; set; }
        }

        /// <summary>A stroke's pieces and the expressions that write them, and whether they are on them now.</summary>
        private sealed class Line(PortraitStroke stroke, List<CompositionPathGeometry> pieces, List<ExpressionAnimation> trims)
        {
            public PortraitStroke Stroke { get; } = stroke;
            public List<CompositionPathGeometry> Pieces { get; } = pieces;
            public List<ExpressionAnimation> Trims { get; } = trims;
            /// <summary>Nought before the pen reaches it, one while it is written, two once it is whole.</summary>
            public int State { get; set; } = -1;
        }

        // The four parts of a line, in the order they go down, and the pure
        // colour each is drawn in for the matrix (Matrix): the outline's
        // outer half, its inner half over the fill, the fill, the light.
        private const int Outer = 0, Inner = 1, Fill = 2, Light = 3;

        public Drawing(Compositor compositor, PortraitArt art, GlassLine look, DispatcherQueue ui)
        {
            _compositor = compositor;
            _art = art;
            _look = look;
            Root = compositor.CreateContainerVisual();
            _cut = compositor.CreateRoundedRectangleGeometry();
            Root.Clip = compositor.CreateGeometricClip(_cut);
            _clock = compositor.CreatePropertySet();
            _clock.InsertScalar("t", 0);
            // The fade is the effect's last step, after the matrix. An opacity
            // on the layer, or on a visual holding it, reached the pure
            // colours before the matrix read them, and a fading child lost
            // its edges and its light and went a flat dark grey.
            _effects = compositor.CreateEffectFactory(new OpacityEffect
            {
                Name = "fade",
                Opacity = 1,
                Source = new ColorMatrixEffect
                {
                    Source = new CompositionEffectSourceParameter("layer"),
                    ColorMatrix = Matrix(look),
                    // Applied to the layer's premultiplied pixels as they
                    // are, so a pixel half covered by the edge becomes the
                    // edge's colour at half its strength.
                    AlphaMode = CanvasAlphaMode.Straight,
                    ClampOutput = true,
                },
            }, new[] { "fade.Opacity" });
            // The inner half of the outline is the edge over the fill: as much
            // of the fill as the edge lets through, and all of the edge.
            var through = (byte)Math.Round((1 - look.Edge.Alpha) * 255);
            _inks = new[]
            {
                compositor.CreateColorBrush(Windows.UI.Color.FromArgb(255, 0, 255, 0)),
                compositor.CreateColorBrush(Windows.UI.Color.FromArgb(255, through, 255, 0)),
                compositor.CreateColorBrush(Windows.UI.Color.FromArgb(255, 255, 0, 0)),
                compositor.CreateColorBrush(Windows.UI.Color.FromArgb(255, 0, 0, 255)),
            };
            _timer = ui.CreateTimer();
            _timer.IsRepeating = false;
            _timer.Tick += (_, _) => Tick();
        }

        /// <summary>Seconds into the cycle now.</summary>
        private double Now => _banked + (_since is { } since ? Stopwatch.GetElapsedTime(since).TotalSeconds : 0);

        public void Run(bool running)
        {
            if (running == _running) return;
            _running = running;
            if (running)
            {
                _since = Stopwatch.GetTimestamp();
                Tick();
            }
            else
            {
                _banked = Now;
                _since = null;
                _timer.Stop();
                Stand(_banked);
            }
        }

        /// <summary>Held still <paramref name="seconds"/> into the cycle, for good.</summary>
        public void Hold(double seconds)
        {
            Run(false);
            _banked = seconds;
            Show(_art.At(seconds).Portrait);
            Stand(seconds);
            Follow(seconds);
        }

        /// <summary>
        /// At each moment the card starts or stops moving: the child whose
        /// turn it is on the card, the clock let go while something moves
        /// and stopped while nothing does, and the next such moment awaited.
        /// A pause shorter than a fifth of a second (a pen lifted between two
        /// strokes) is not worth stopping for.
        /// </summary>
        private void Tick()
        {
            _timer.Stop();
            if (!_running) return;
            var now = Now;
            Show(_art.At(now).Portrait);
            var (moving, until) = _art.Motion(now);
            var wait = until - now;
            if (moving || wait < 0.2) Go(now);
            else Stand(now);
            Follow(now);
            _timer.Interval = TimeSpan.FromSeconds(Math.Max(wait, 0.005));
            _timer.Start();
        }

        /// <summary>The clock moving on its own from <paramref name="now"/>, at a second a second, round and round.</summary>
        private void Go(double now)
        {
            if (_ticking) return;
            _ticking = true;
            var round = (float)_art.Round;
            var from = (float)(now % _art.Round);
            var time = _compositor.CreateScalarKeyFrameAnimation();
            time.InsertKeyFrame(0, from);
            time.InsertKeyFrame(1, from + round, _compositor.CreateLinearEasingFunction());
            time.Duration = TimeSpan.FromSeconds(round);
            // Each round starts again where this one began; the expressions
            // read the clock round the cycle, so a round later is the same.
            time.IterationBehavior = AnimationIterationBehavior.Forever;
            _clock.StartAnimation("t", time);
        }

        /// <summary>The clock standing at <paramref name="now"/>, and nothing drawn again until it moves.</summary>
        private void Stand(double now)
        {
            if (_ticking) _clock.StopAnimation("t");
            _ticking = false;
            _clock.InsertScalar("t", (float)(now % _art.Round));
        }

        /// <summary>The child at <paramref name="index"/> on the card, the one before taken off it.</summary>
        private void Show(int index)
        {
            if (_child?.Index == index) return;
            if (_child != null) Root.Children.Remove(_child.Layer);
            if (!_made.TryGetValue(index, out var child))
            {
                child = Make(index);
                _made[index] = child;
            }
            // From nothing again: what they were left at last round is not
            // where their turn begins.
            child.Fading = false;
            child.Glass.StopAnimation("fade.Opacity");
            foreach (var line in child.Lines) line.State = -1;
            child.Layer.IsVisible = false;
            child.Layer.Size = _size;
            _child = child;
            Root.Children.InsertAtTop(child.Layer);
            Fit(child);
        }

        /// <summary>
        /// Only what moves follows the clock: the strokes being written and
        /// the fade. A stroke not yet begun stands at nothing and one done at
        /// whole, and the child's opacity stands too outside the fade, so the
        /// compositor has one or two paths to draw again in a frame rather
        /// than every path of the child. Asked at each moment the card starts
        /// or stops moving, which every stroke's start and end is.
        /// </summary>
        private void Follow(double now)
        {
            if (_child is not { } child) return;
            var moment = _art.At(now);
            if (moment.Portrait != child.Index) return;
            var tau = moment.Drawing;
            foreach (var line in child.Lines)
            {
                var state = tau >= line.Stroke.End ? 2 : tau >= line.Stroke.Start ? 1 : 0;
                if (state == line.State) continue;
                line.State = state;
                for (var i = 0; i < line.Pieces.Count; i++)
                {
                    var piece = line.Pieces[i];
                    if (state == 1)
                    {
                        piece.StartAnimation("TrimEnd", line.Trims[i]);
                        continue;
                    }
                    piece.StopAnimation("TrimEnd");
                    piece.TrimEnd = state == 2 ? 1 : 0;
                }
            }
            var person = _art.Portraits[child.Index];
            var fading = tau >= person.Writing + person.Hold && tau < person.Writing + person.Hold + person.Fade;
            if (fading && !child.Fading)
            {
                child.Glass.StartAnimation("fade.Opacity", child.Shown);
            }
            else if (!fading)
            {
                if (child.Fading) child.Glass.StopAnimation("fade.Opacity");
                child.Glass.Properties.InsertScalar("fade.Opacity", (float)person.Shown(tau));
            }
            child.Fading = fading;
            // Nothing of them is drawn in the gap, so the layer is not either.
            child.Layer.IsVisible = tau >= 0;
        }

        /// <summary>
        /// One child, every piece of every stroke a path trimmed by the clock:
        /// the README's <c>drawn(stroke)</c>, smoothstep of the share of the
        /// stroke's time gone, and of that the share of this piece's length,
        /// the pen going through a stroke's pieces in order. The layer's
        /// opacity is the README's too: none before the gap ends, whole until
        /// the hold ends, then down the smoothstep over the fade.
        /// </summary>
        private Child Make(int index)
        {
            var person = _art.Portraits[index];
            var layer = _compositor.CreateLayerVisual();
            layer.Size = _size;
            layer.IsVisible = false;
            var glass = _effects.CreateBrush();
            layer.Effect = glass;
            var lines = new List<Line>();
            var shapes = _compositor.CreateShapeVisual();
            shapes.RelativeSizeAdjustment = Vector2.One;
            var fit = _compositor.CreateContainerShape();
            var parts = new[] { new List<CompositionSpriteShape>(), new List<CompositionSpriteShape>(), new List<CompositionSpriteShape>(), new List<CompositionSpriteShape>() };
            // Seconds into this child's drawing, read round the cycle.
            var drawing = $"(Mod(c.t, {N(_art.Round)}) - {N(_art.StartOf(index) + _art.Gap)})";
            foreach (var stroke in person.Strokes)
            {
                var drawn = stroke.Duration > 0
                    ? Smoothstep($"({drawing} - {N(stroke.Start)}) / {N(stroke.Duration)}")
                    : $"({drawing} >= {N(stroke.Start)} ? 1 : 0)";
                var before = 0.0;
                var line = new Line(stroke, new(), new());
                lines.Add(line);
                foreach (var (piece, length) in stroke.Pieces)
                {
                    var geometry = _compositor.CreatePathGeometry(new CompositionPath(Path(_device, piece)));
                    geometry.TrimEnd = 0;
                    var trim = _compositor.CreateExpressionAnimation(stroke.Pieces.Count == 1 || stroke.Length <= 0
                        ? drawn
                        : $"Clamp(({drawn} * {N(stroke.Length)} - {N(before)}) / {N(Math.Max(length, 1e-6))}, 0, 1)");
                    trim.SetReferenceParameter("c", _clock);
                    line.Pieces.Add(geometry);
                    line.Trims.Add(trim);
                    for (var part = 0; part < parts.Length; part++)
                    {
                        // A single line, or a tube without an edge or a light,
                        // has no shapes for them.
                        if ((part is Outer or Inner && !_look.Edged) || (part == Light && !_look.Lit)) continue;
                        var shape = _compositor.CreateSpriteShape(geometry);
                        shape.StrokeBrush = _inks[part];
                        shape.StrokeStartCap = CompositionStrokeCap.Round;
                        shape.StrokeEndCap = CompositionStrokeCap.Round;
                        shape.StrokeLineJoin = CompositionStrokeLineJoin.Round;
                        parts[part].Add(shape);
                    }
                    before += length;
                }
            }
            foreach (var part in parts)
            {
                foreach (var shape in part) fit.Shapes.Add(shape);
            }
            shapes.Shapes.Add(fit);
            layer.Children.InsertAtTop(shapes);
            var fading = person.Writing + person.Hold;
            var shown = _compositor.CreateExpressionAnimation(person.Fade > 0
                ? $"1 - {Smoothstep($"({drawing} - {N(fading)}) / {N(person.Fade)}")}"
                : $"{drawing} >= {N(fading)} ? 0 : 1");
            shown.SetReferenceParameter("c", _clock);
            return new Child(index, layer, glass, fit, parts, lines.ToArray(), shown);
        }

        /// <summary>smoothstep(u) = u²(3 − 2u), u held to nought and one, as an expression.</summary>
        private static string Smoothstep(string u) => $"(Clamp({u}, 0, 1) * Clamp({u}, 0, 1) * (3 - 2 * Clamp({u}, 0, 1)))";

        private static string N(double value) => value.ToString("0.######", CultureInfo.InvariantCulture);

        /// <summary>The card's size and corner: the child fitted to it, and every width that is not the drawing's own made again for the new scale.</summary>
        public void Lay(Vector2 size, float radius)
        {
            _size = size;
            Root.Size = size;
            _cut.Size = size;
            _cut.CornerRadius = new Vector2(radius, radius);
            if (_child != null)
            {
                _child.Layer.Size = size;
                Fit(_child);
            }
        }

        /// <summary>
        /// The README's placement and widths: the line <c>w</c> is the file's
        /// width at the card's scale; the outline is 0.75 points across,
        /// centred on the line's edge; the light is 0.16 of the line, at
        /// least 0.8 points, moved up and to the left by a fifth of it.
        /// </summary>
        private void Fit(Child child)
        {
            var fit = _art.Fit(_size.X, _size.Y);
            var scale = (float)fit.Scale;
            if (scale <= 0)
            {
                child.Fit.Scale = Vector2.Zero;
                return;
            }
            child.Fit.Scale = new Vector2(scale, scale);
            child.Fit.Offset = new Vector2((float)fit.X, (float)fit.Y);
            // In the canvas's units, which the fit scales to points.
            var line = _look.Width is { } points ? (float)points / scale : (float)_art.LineWidth;
            var edge = (float)_look.EdgeWidth / scale;
            var widths = new[]
            {
                line + edge,
                line,
                _look.Edged ? Math.Max(line - edge, line / 4) : line,
                Math.Max(line * (float)_look.LightWidth, (float)_look.LightLeast / scale),
            };
            var moved = new Vector2(line * (float)_look.LightX, line * (float)_look.LightY);
            for (var part = 0; part < child.Parts.Length; part++)
            {
                foreach (var shape in child.Parts[part])
                {
                    shape.StrokeThickness = widths[part];
                    if (part == Light) shape.Offset = moved;
                }
            }
        }

        /// <summary>
        /// The matrix from the pure colours to the glass, applied to
        /// premultiplied pixels: each colour's strength is how much of the
        /// pixel its part covers, so each row is its part's colour
        /// premultiplied. Red is the fill, green the edge, blue the light,
        /// whose row is the light over the fill.
        /// </summary>
        private static Matrix5x4 Matrix(GlassLine look)
        {
            static (float R, float G, float B, float A) Pre(Shade c) => (c.Red * c.Alpha, c.Green * c.Alpha, c.Blue * c.Alpha, c.Alpha);
            var fill = Pre(look.Fill);
            var edge = Pre(look.Edge);
            var light = Pre(look.Light);
            var under = 1 - look.Light.Alpha;
            return new Matrix5x4
            {
                M11 = fill.R, M12 = fill.G, M13 = fill.B, M14 = fill.A,
                M21 = edge.R, M22 = edge.G, M23 = edge.B, M24 = edge.A,
                M31 = light.R + fill.R * under, M32 = light.G + fill.G * under, M33 = light.B + fill.B * under, M34 = light.A + fill.A * under,
            };
        }

        /// <summary>A piece of a stroke in the canvas's units, as Direct2D draws it.</summary>
        private static CanvasGeometry Path(CanvasDevice device, IReadOnlyList<PathSegment> path)
        {
            using var builder = new CanvasPathBuilder(device);
            var open = false;
            Vector2 start = default, at = default;
            void Begin()
            {
                if (open) return;
                builder.BeginFigure(at);
                open = true;
            }
            foreach (var segment in path)
            {
                var to = new Vector2((float)segment.X, (float)segment.Y);
                switch (segment.Verb)
                {
                    case PathVerb.Move:
                        if (open) builder.EndFigure(CanvasFigureLoop.Open);
                        open = false;
                        start = at = to;
                        Begin();
                        break;
                    case PathVerb.Line:
                        Begin();
                        builder.AddLine(to);
                        at = to;
                        break;
                    case PathVerb.Cubic:
                        Begin();
                        builder.AddCubicBezier(new Vector2((float)segment.X1, (float)segment.Y1), new Vector2((float)segment.X2, (float)segment.Y2), to);
                        at = to;
                        break;
                    case PathVerb.Quad:
                        Begin();
                        builder.AddQuadraticBezier(new Vector2((float)segment.X1, (float)segment.Y1), to);
                        at = to;
                        break;
                    case PathVerb.Close:
                        if (open) builder.EndFigure(CanvasFigureLoop.Closed);
                        open = false;
                        at = start;
                        break;
                }
            }
            if (open) builder.EndFigure(CanvasFigureLoop.Open);
            return CanvasGeometry.CreatePath(builder);
        }
    }
}
