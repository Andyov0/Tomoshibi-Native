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
/// The lobby's picture of yourself with the camera off: a card with little
/// animals drawn on it, one after another, stroke by stroke, the way the
/// iPhone writes its first "hello" (the iPhone's PreviewGlass; the owner
/// asked for the same card here, 2026-10-27). On the narrow lobby it is the
/// card itself, drawn as art/README.md's "The card" says to draw the
/// iPhone's clear glass elsewhere: solid #202020 with a point of white at 40
/// per cent just inside its outline by night, solid white with no edge by
/// day (Tomoshibi.Basics.GlassInk; Android's PreviewGlass.kt). On the wide
/// one there is no card, the camera filling the window behind the sidebar,
/// and the animals stand on the stage beside it, where the picture would be
/// (PortraitStage).
///
/// Who is drawn and when is art/portraits.json, which every platform plays
/// as it is (Tomoshibi.Basics.PortraitArt); the lines are thin single lines
/// of the text colour (GlassInk).
///
/// Drawn by the compositor. Each piece of a stroke is a path whose TrimEnd
/// follows one clock, a property the compositor moves itself, through the
/// README's formulas written as expressions; so while an animal is written
/// or fades the window's thread does nothing. The clock moves only while
/// something on the card does: in the gap and the hold, and whenever the
/// card cannot be seen (the window minimised or hidden, a dialog or sheet
/// over the lobby, the card scrolled away, the card gone), it stands still
/// and nothing is drawn again, and it goes on from where it stood. With
/// Windows' animation effects off the first animal stands finished.
///
/// The lines are every stroke drawn so far united into one shape, so lines
/// that cross are one line rather than a darker crossing (the line is
/// translucent). Direct2D strokes a path but has no union of growing
/// strokes, so every stroke is drawn opaque in pure red into a layer, where
/// crossings are no redder than the rest, and one colour matrix turns the
/// red into the line's colour at its strength.
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
                Log.Line($"portraits: {_art.Portraits.Count} portraits, a round of {_art.Round:0.##} s");
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
    private readonly GlassInk _ink = GlassInk.Of(Ink.Scheme);
    private readonly bool _still = !new UISettings().AnimationsEnabled;
    private Drawing? _drawing;
    private bool _loaded;
    private bool _inView = true;

    /// <param name="radius">The corner the drawing is cut to.</param>
    /// <param name="pane">Whether this is a card of its own (the narrow lobby's), or portraits standing on what is already there (the wide stage).</param>
    /// <param name="pose">TOMOSHIBI_DEBUG_PORTRAIT in a debug build: one moment, held, for a picture of it.</param>
    public PreviewGlass(double radius, bool pane, string? pose = null)
    {
        _radius = radius;
        _pose = pose;
        IsHitTestVisible = false;
        CornerRadius = new CornerRadius(radius);
        if (pane)
        {
            // The README's card, solid: nothing behind it shows through, and
            // the rim, where there is one, is drawn inside the outline, as a
            // Grid's border is.
            Background = Ink.Brush(_ink.Card);
            if (_ink.CardEdge is { } edge)
            {
                BorderBrush = Ink.Brush(edge);
                BorderThickness = new Thickness(_ink.CardEdgeWidth);
            }
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
            _drawing = new Drawing(ElementCompositionPreview.GetElementVisual(_canvas).Compositor, art, _ink, DispatcherQueue);
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
        // Somebody who turned animation off sees the first portrait whole.
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

    /// <summary>The composition tree of the portrait on the card, the clock it is drawn by, and what moves the clock.</summary>
    private sealed class Drawing
    {
        public readonly ContainerVisual Root;
        private readonly Compositor _compositor;
        private readonly PortraitArt _art;
        private readonly CompositionPropertySet _clock;
        private readonly CompositionRoundedRectangleGeometry _cut;
        private readonly CompositionEffectFactory _effects;
        private readonly CompositionColorBrush _red;
        private readonly DispatcherQueueTimer _timer;
        private readonly CanvasDevice _device = CanvasDevice.GetSharedDevice();
        private Vector2 _size;
        private Picture? _shown;
        // Every portrait once made, kept for the next round: making one is a
        // few hundred milliseconds of Direct2D paths and shapes, which made
        // every change of portrait a stutter.
        private readonly Dictionary<int, Picture> _made = new();
        private bool _running;
        private bool _ticking;
        // Seconds of the cycle played before the present run, and when the
        // present run began: the cycle's time stands still while it does not run.
        private double _banked;
        private long? _since;

        /// <summary>One portrait: the visual they fade in, their layer, their shapes in the order they are drawn, and their strokes.</summary>
        private sealed record Picture(int Index, LayerVisual Layer, CompositionEffectBrush Glass, CompositionContainerShape Fit, List<CompositionSpriteShape> Shapes, Line[] Lines, ExpressionAnimation Shown)
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

        public Drawing(Compositor compositor, PortraitArt art, GlassInk ink, DispatcherQueue ui)
        {
            _compositor = compositor;
            _art = art;
            Root = compositor.CreateContainerVisual();
            _cut = compositor.CreateRoundedRectangleGeometry();
            Root.Clip = compositor.CreateGeometricClip(_cut);
            _clock = compositor.CreatePropertySet();
            _clock.InsertScalar("t", 0);
            // The fade is the effect's last step, after the matrix: an opacity
            // on the layer, or on a visual holding it, reached the pure red
            // before the matrix read it.
            _effects = compositor.CreateEffectFactory(new OpacityEffect
            {
                Name = "fade",
                Opacity = 1,
                Source = new ColorMatrixEffect
                {
                    Source = new CompositionEffectSourceParameter("layer"),
                    ColorMatrix = Matrix(ink.Line),
                    // Applied to the layer's premultiplied pixels as they
                    // are, so a pixel half covered by a line becomes the
                    // line's colour at half its strength.
                    AlphaMode = CanvasAlphaMode.Straight,
                    ClampOutput = true,
                },
            }, new[] { "fade.Opacity" });
            _red = compositor.CreateColorBrush(Windows.UI.Color.FromArgb(255, 255, 0, 0));
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
        /// At each moment the card starts or stops moving: the portrait whose
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

        /// <summary>The portrait at <paramref name="index"/> on the card, the one before taken off it.</summary>
        private void Show(int index)
        {
            if (_shown?.Index == index) return;
            if (_shown != null) Root.Children.Remove(_shown.Layer);
            if (!_made.TryGetValue(index, out var portrait))
            {
                portrait = Make(index);
                _made[index] = portrait;
            }
            // From nothing again: what they were left at last round is not
            // where their turn begins.
            portrait.Fading = false;
            portrait.Glass.StopAnimation("fade.Opacity");
            foreach (var line in portrait.Lines) line.State = -1;
            portrait.Layer.IsVisible = false;
            portrait.Layer.Size = _size;
            _shown = portrait;
            Root.Children.InsertAtTop(portrait.Layer);
            Fit(portrait);
        }

        /// <summary>
        /// Only what moves follows the clock: the strokes being written and
        /// the fade. A stroke not yet begun stands at nothing and one done at
        /// whole, and the portrait's opacity stands too outside the fade, so the
        /// compositor has one or two paths to draw again in a frame rather
        /// than every path of the portrait. Asked at each moment the card starts
        /// or stops moving, which every stroke's start and end is.
        /// </summary>
        private void Follow(double now)
        {
            if (_shown is not { } portrait) return;
            var moment = _art.At(now);
            if (moment.Portrait != portrait.Index) return;
            var tau = moment.Drawing;
            foreach (var line in portrait.Lines)
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
            var person = _art.Portraits[portrait.Index];
            var fading = tau >= person.Writing + person.Hold && tau < person.Writing + person.Hold + person.Fade;
            if (fading && !portrait.Fading)
            {
                portrait.Glass.StartAnimation("fade.Opacity", portrait.Shown);
            }
            else if (!fading)
            {
                if (portrait.Fading) portrait.Glass.StopAnimation("fade.Opacity");
                portrait.Glass.Properties.InsertScalar("fade.Opacity", (float)person.Shown(tau));
            }
            portrait.Fading = fading;
            // Nothing of them is drawn in the gap, so the layer is not either.
            portrait.Layer.IsVisible = tau >= 0;
        }

        /// <summary>
        /// One portrait, every piece of every stroke a path trimmed by the clock:
        /// the README's <c>drawn(stroke)</c>, smoothstep of the share of the
        /// stroke's time gone, and of that the share of this piece's length,
        /// the pen going through a stroke's pieces in order. The layer's
        /// opacity is the README's too: none before the gap ends, whole until
        /// the hold ends, then down the smoothstep over the fade.
        /// </summary>
        private Picture Make(int index)
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
            var sprites = new List<CompositionSpriteShape>();
            // Seconds into this portrait's drawing, read round the cycle.
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
                    var shape = _compositor.CreateSpriteShape(geometry);
                    shape.StrokeBrush = _red;
                    shape.StrokeStartCap = CompositionStrokeCap.Round;
                    shape.StrokeEndCap = CompositionStrokeCap.Round;
                    shape.StrokeLineJoin = CompositionStrokeLineJoin.Round;
                    sprites.Add(shape);
                    fit.Shapes.Add(shape);
                    before += length;
                }
            }
            shapes.Shapes.Add(fit);
            layer.Children.InsertAtTop(shapes);
            var fading = person.Writing + person.Hold;
            var shown = _compositor.CreateExpressionAnimation(person.Fade > 0
                ? $"1 - {Smoothstep($"({drawing} - {N(fading)}) / {N(person.Fade)}")}"
                : $"{drawing} >= {N(fading)} ? 0 : 1");
            shown.SetReferenceParameter("c", _clock);
            return new Picture(index, layer, glass, fit, sprites, lines.ToArray(), shown);
        }

        /// <summary>smoothstep(u) = u²(3 − 2u), u held to nought and one, as an expression.</summary>
        private static string Smoothstep(string u) => $"(Clamp({u}, 0, 1) * Clamp({u}, 0, 1) * (3 - 2 * Clamp({u}, 0, 1)))";

        private static string N(double value) => value.ToString("0.######", CultureInfo.InvariantCulture);

        /// <summary>The card's size and corner, and the portrait fitted to it.</summary>
        public void Lay(Vector2 size, float radius)
        {
            _size = size;
            Root.Size = size;
            _cut.Size = size;
            _cut.CornerRadius = new Vector2(radius, radius);
            if (_shown != null)
            {
                _shown.Layer.Size = size;
                Fit(_shown);
            }
        }

        /// <summary>
        /// The README's placement and width: the line <c>w</c> is the file's
        /// width, in the canvas's units, which the fit scales to the card.
        /// </summary>
        private void Fit(Picture portrait)
        {
            var fit = _art.Fit(_size.X, _size.Y);
            var scale = (float)fit.Scale;
            if (scale <= 0)
            {
                portrait.Fit.Scale = Vector2.Zero;
                return;
            }
            portrait.Fit.Scale = new Vector2(scale, scale);
            portrait.Fit.Offset = new Vector2((float)fit.X, (float)fit.Y);
            var line = (float)_art.LineWidth;
            foreach (var shape in portrait.Shapes) shape.StrokeThickness = line;
        }

        /// <summary>
        /// The matrix from the pure red to the line, applied to premultiplied
        /// pixels: the red's strength is how much of the pixel the line
        /// covers, so its row is the line's colour premultiplied.
        /// </summary>
        private static Matrix5x4 Matrix(Shade line) => new()
        {
            M11 = line.Red * line.Alpha,
            M12 = line.Green * line.Alpha,
            M13 = line.Blue * line.Alpha,
            M14 = line.Alpha,
        };

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
