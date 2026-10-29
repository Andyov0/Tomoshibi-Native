using System.Numerics;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_core;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// One remote picture drawn by the core (panel.rs): a SwapChainPanel the
/// core draws into, told its place whenever the tile changes size, both to
/// cut the picture to it and to ask the server for the layer that answers
/// it.
///
/// Whether a camera fills its tile or is shown whole is the shared framing
/// rule (Framing.Fills), decided from the picture's shape as the core
/// reports it, turned, and the tile's; a shared screen is read, and is never
/// cut. Shown whole, the rest of the tile is a blurred copy of the picture
/// (RemoteVideoView.kt): a glimpse of a few dozen pixels the core keeps of
/// the last frame, taken about once a second, blurred and darkened here,
/// crossfaded from the last. A change of framing after the first is seen to
/// settle: the panel's visual starts at the old scale and eases to the new
/// over the settle curve.
/// </summary>
internal sealed class RemotePicture : Grid
{
    public readonly string Sid;
    private readonly Call _call;
    private readonly bool _camera;
    private readonly SwapChainPanel _panel = new();
    private readonly Grid _behind = new() { IsHitTestVisible = false };
    private readonly Image _glimpseA = new() { Stretch = Stretch.UniformToFill };
    private readonly Image _glimpseB = new() { Stretch = Stretch.UniformToFill };
    private bool _showingA;
    private ulong _id;
    private (float Width, float Height) _picture;
    private bool? _fill;
    private Microsoft.UI.Dispatching.DispatcherQueueTimer? _glimpses;
    private bool _behindShown;
    private bool _glimpseSaid;

    private sealed class Shape(RemotePicture tile) : PictureShape
    {
        // On a core thread, with nothing in the core locked.
        public void Changed(uint width, uint height, uint rotation) =>
            tile.DispatcherQueue.TryEnqueue(() => tile.Reshaped(width, height, rotation));
    }

    public RemotePicture(Call call, string sid, bool camera)
    {
        _call = call;
        Sid = sid;
        _camera = camera;
        _behind.Children.Add(_glimpseA);
        _behind.Children.Add(_glimpseB);
        // Darkened by a third, so that the picture in front reads as the picture.
        _behind.Children.Add(new Rectangle { Fill = Ink.Black(Theme.GlimpseDarken) });
        Motion.Hide(_behind);
        Motion.Hide(_glimpseA);
        Motion.Hide(_glimpseB);
        Children.Add(_behind);
        Children.Add(_panel);
        SizeChanged += (_, _) => Place();
        Loaded += (_, _) => Attach();
        Unloaded += (_, _) => Detach();
    }

    public ulong FramesDrawn => _id == 0 ? 0 : _call.VideoPanelFrames(Sid, _id);

    public bool Whole => _fill == false;

    /// <summary>Starts drawing the picture into the panel; UI thread.</summary>
    private void Attach()
    {
        if (_id != 0) return;
        var chain = _call.AttachVideoPanel(Sid, new Shape(this));
        if (chain == 0)
        {
            Log.Line($"no Direct3D device for {Sid}");
            return;
        }
        _id = chain;
        ComCalls.SetSwapChain(_panel, (IntPtr)(long)chain);
        // The core keeps its own reference; this one was handed over for the panel.
        Marshal.Release((IntPtr)(long)chain);
        Place();
    }

    /// <summary>
    /// Stops the core drawing at once; the panel lets its swap chain go a
    /// little later, after the tile has faded out with its last picture.
    /// </summary>
    private void Detach()
    {
        // Moved rather than taken away: XAML may raise the new place's
        // Loaded before the old place's Unloaded, and then the Attach finds
        // the panel still attached and does nothing, and this would let the
        // picture go for good, leaving the tile black for the rest of the
        // call.
        if (IsLoaded) return;
        _glimpses?.Stop();
        // Shown again from the start when it is attached again.
        _behindShown = false;
        if (_id == 0) return;
        _call.DetachVideoPanel(Sid, _id);
        _id = 0;
        var timer = DispatcherQueue.CreateTimer();
        timer.Interval = TimeSpan.FromMilliseconds(300);
        timer.IsRepeating = false;
        // Not over a swap chain attached again meanwhile.
        timer.Tick += (_, _) =>
        {
            if (_id == 0) ComCalls.SetSwapChain(_panel, IntPtr.Zero);
        };
        timer.Start();
    }

    private void Reshaped(uint width, uint height, uint rotation)
    {
        _picture = Framing.Shown(width, height, rotation);
        Log.Line($"picture {Sid} is {width}x{height} turned {rotation}");
        Place();
    }

    private void Place()
    {
        if (_id == 0 || ActualWidth <= 0 || ActualHeight <= 0) return;
        var (w, h) = ((float)ActualWidth, (float)ActualHeight);
        var (placeW, placeH) = (w, h);
        // A shared screen is read, and is shown whole.
        var fill = _camera && Framing.Fills(_picture.Width, _picture.Height, w, h);
        var was = _fill;
        _fill = fill;
        // Shown whole, the panel is the picture's own shape in the middle of
        // the place, and the blurred glimpse is what is around it: the swap
        // chain is opaque, and a panel the size of the place drew black
        // bars over the glimpse behind it.
        if (!fill && _picture.Width > 0 && _picture.Height > 0)
        {
            var fit = Math.Min(w / _picture.Width, h / _picture.Height);
            (w, h) = (MathF.Max(1, MathF.Round(_picture.Width * fit)), MathF.Max(1, MathF.Round(_picture.Height * fit)));
        }
        _panel.Width = fill ? double.NaN : w;
        _panel.Height = fill ? double.NaN : h;
        _call.PlaceVideoPanel(Sid, _id, w, h, fill);
        var scale = XamlRoot?.RasterizationScale ?? 1.0;
        try
        {
            // The place's size, whatever the framing: a picture framed whole
            // is filled again with a press, and asked for at the smaller
            // size it would come back blurred until the sender's next layer.
            _call.RequestVideoSize(Sid, (uint)(placeW * scale), (uint)(placeH * scale));
        }
        catch (CallException fault)
        {
            // A track gone between the last layout and this one.
            Log.Line($"no size asked for {Sid}: {fault.Message}");
        }
        if (was != null && was != fill) Settle(fill, placeW, placeH, w, h);
        if (was != fill) Log.Line($"framing {Sid}: {(fill ? "fills" : "whole")}");
        ShowBehind(!fill && _camera);
    }

    /// <summary>
    /// The change of framing animated: the core draws the new framing with
    /// the next frame, and the panel's visual eases from the old scale to it
    /// over 0.32 s about the middle, so that the picture is seen to settle.
    /// </summary>
    private void Settle(bool fill, float placeW, float placeH, float panelW, float panelH)
    {
        var ratio = Framing.FillOverWhole(_picture.Width, _picture.Height, placeW, placeH);
        if (ratio <= 1.001f) return;
        var visual = ElementCompositionPreview.GetElementVisual(_panel);
        var compositor = visual.Compositor;
        // About the panel's own middle, which is the place's: the panel is
        // the place filled, or the picture's shape centred in it.
        visual.CenterPoint = new Vector3(panelW / 2, panelH / 2, 0);
        // To whole: it was larger. To fill: it was smaller.
        var from = fill ? 1 / ratio : ratio;
        var scale = compositor.CreateVector3KeyFrameAnimation();
        scale.InsertKeyFrame(0f, new Vector3(from, from, 1));
        scale.InsertKeyFrame(1f, Vector3.One, Motion.SettleCurve(compositor));
        scale.Duration = Motion.Settle;
        visual.StartAnimation("Scale", scale);
    }

    private void ShowBehind(bool shown)
    {
        if (shown == _behindShown) return;
        _behindShown = shown;
        if (shown)
        {
            _glimpses ??= MakeGlimpseTimer();
            _glimpses.Start();
            TakeGlimpse();
        }
        else
        {
            _glimpses?.Stop();
            Motion.FadeTo(_behind, 0);
        }
    }

    private Microsoft.UI.Dispatching.DispatcherQueueTimer MakeGlimpseTimer()
    {
        var timer = DispatcherQueue.CreateTimer();
        timer.Interval = TimeSpan.FromMilliseconds(Theme.GlimpseEveryMs);
        timer.Tick += (_, _) => TakeGlimpse();
        return timer;
    }

    /// <summary>A new glimpse, blurred, crossfaded over the last over 0.6 s; the layer fades in over the settle curve the first time.</summary>
    private void TakeGlimpse()
    {
        Glimpse? glimpse;
        try
        {
            glimpse = _call.VideoPanelGlimpse(Sid, 48);
        }
        catch (Exception fault)
        {
            if (!_glimpseSaid) Log.Line($"no glimpse of {Sid}: {fault.Message}");
            _glimpseSaid = true;
            glimpse = null;
        }
        if (glimpse == null) return;
        if (!_glimpseSaid) Log.Line($"glimpse of {Sid}: {glimpse.Width}x{glimpse.Height}");
        _glimpseSaid = true;
        var bitmap = new WriteableBitmap((int)glimpse.Width, (int)glimpse.Height);
        var pixels = Blur.Box(glimpse.Bgra, (int)glimpse.Width, (int)glimpse.Height, 2);
        using (var stream = bitmap.PixelBuffer.AsStream()) stream.Write(pixels, 0, pixels.Length);
        bitmap.Invalidate();
        var (next, last) = _showingA ? (_glimpseB, _glimpseA) : (_glimpseA, _glimpseB);
        _showingA = !_showingA;
        next.Source = bitmap;
        var fade = TimeSpan.FromMilliseconds(Theme.GlimpseFadeMs);
        Motion.FadeTo(next, 1, fade);
        Motion.FadeTo(last, 0, fade);
        Motion.FadeTo(_behind, 1);
    }
}

/// <summary>A box blur on a small BGRA picture, three passes of it, which is near enough a Gaussian for a backdrop.</summary>
internal static class Blur
{
    public static byte[] Box(byte[] source, int width, int height, int radius)
    {
        var a = (byte[])source.Clone();
        var b = new byte[a.Length];
        for (var pass = 0; pass < 3; pass++)
        {
            Run(a, b, width, height, radius, horizontal: true);
            Run(b, a, width, height, radius, horizontal: false);
        }
        return a;
    }

    private static void Run(byte[] from, byte[] to, int width, int height, int radius, bool horizontal)
    {
        var (lines, length) = horizontal ? (height, width) : (width, height);
        for (var line = 0; line < lines; line++)
        {
            for (var at = 0; at < length; at++)
            {
                int b = 0, g = 0, r = 0, n = 0;
                for (var k = -radius; k <= radius; k++)
                {
                    var p = Math.Clamp(at + k, 0, length - 1);
                    var index = horizontal ? (line * width + p) * 4 : (p * width + line) * 4;
                    b += from[index];
                    g += from[index + 1];
                    r += from[index + 2];
                    n++;
                }
                var target = horizontal ? (line * width + at) * 4 : (at * width + line) * 4;
                to[target] = (byte)(b / n);
                to[target + 1] = (byte)(g / n);
                to[target + 2] = (byte)(r / n);
                to[target + 3] = 255;
            }
        }
    }
}

/// <summary>
/// One place in the room (Android's ui/room/Tile.kt <c>TileFrame</c>): the
/// picture or, with the camera off, the person's initials; their name at
/// the foot with the marks that go with it (microphone off, a raised hand);
/// the ring that lights while they speak; and the reactions that float up
/// over it. Placed by coordinates, so that it is made once for as long as
/// its owner is in the call and moving it is an animation.
///
/// No mark in a corner saying that a press makes it larger. There was one,
/// under the pointer, and it met the stage's own controls in the same corner
/// however its showing was conditioned: the owner had it taken away
/// (2026-10-26), the whole picture being what is pressed.
/// </summary>
internal sealed class Tile : Grid
{
    /// <summary>
    /// What fills a place other than a person's own picture: a screen shared
    /// but not on the stage, this participant's own share, or something a
    /// feature draws there (the video watched together).
    /// </summary>
    public abstract record Face
    {
        public sealed record Picture : Face;
        public sealed record ShareCard : Face;
        public sealed record MyShare(Action Stop) : Face;
        public sealed record Drawn(string Key, Func<UIElement> Make) : Face;
    }

    public readonly string Id;
    private string? _faceKey;
    /// <summary>The stage's buttons over the picture (Sizing.Stage), out while the pointer is on it or the keyboard in them.</summary>
    private readonly Grid _controls = new() { HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(Sizing.Stage.Inset) };
    private string? _controlsKey;
    private bool _pointerOver;
    private bool _barOut = true;
    private bool _focusInControls;
    private bool _controlsOut;
    private readonly RoomScreen _room;
    private readonly Grid _content = new();
    private readonly Border _ring;
    private readonly Border _plate;
    private readonly StackPanel _label;
    private readonly TextBlock _name;
    private readonly Border _hand;
    private readonly Canvas _reactions = new() { IsHitTestVisible = false };
    private readonly HashSet<long> _floating = new();
    private string? _sid;
    private bool _speaking;
    private DateTime _quietSince;
    private RemotePicture? _picture;
    private SelfView? _self;
    private Rect _spot;
    private bool _placed;
    private readonly CompositionRoundedRectangleGeometry _clipShape;

    public RemotePicture? Picture => _picture;

    public Tile(RoomScreen room, string id)
    {
        _room = room;
        Id = id;
        Background = Ink.Surface;
        var visual = ElementCompositionPreview.GetElementVisual(this);
        var compositor = visual.Compositor;
        _clipShape = compositor.CreateRoundedRectangleGeometry();
        _clipShape.CornerRadius = new Vector2(Theme.TileRadius, Theme.TileRadius);
        visual.Clip = compositor.CreateGeometricClip(_clipShape);
        SizeChanged += (_, e) => _clipShape.Size = new Vector2((float)e.NewSize.Width, (float)e.NewSize.Height);

        _ring = new Border { BorderThickness = new Thickness(2), CornerRadius = new CornerRadius(Theme.TileRadius), IsHitTestVisible = false };
        Motion.Hide(_ring);
        // The name on its plate (Sizing.NamePlate): a capsule of black at
        // seven tenths, the name in the dark scheme's white whether or not
        // its owner speaks, since the ring says who is speaking and the
        // tally on a bright picture read at about 2:1. It was a rounded
        // square at half black on Windows, a capsule on the other platforms.
        _name = K.Text("", 12.5, K.Medium, Ink.Brush(Sizing.NamePlate.Ink), lines: 1);
        _label = K.Row(5, _name);
        _label.VerticalAlignment = VerticalAlignment.Center;
        _plate = new Border
        {
            Child = _label,
            Background = Ink.Brush(Sizing.NamePlate.Fill),
            Height = Sizing.NamePlate.Height,
            CornerRadius = new CornerRadius(Sizing.NamePlate.Height / 2),
            Padding = new Thickness(10, 0, 10, 0),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            Margin = new Thickness(Sizing.NamePlate.Inset),
            IsHitTestVisible = false,
        };
        _hand = new Border
        {
            Width = 28,
            Height = 28,
            CornerRadius = new CornerRadius(14),
            Background = Ink.Tally,
            Child = new TextBlock { Text = "✋", FontSize = 15, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center },
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(8),
            Visibility = Visibility.Collapsed,
            IsHitTestVisible = false,
        };
        _hand.Arrives(0.5f);
        // A name that grows or shrinks (a mark beside it, a name changed)
        // may come to reach the bar, or no longer reach it.
        _plate.SizeChanged += (_, _) => room.LiftNames();
        Children.Add(_content);
        Children.Add(_reactions);
        Children.Add(_plate);
        Children.Add(_hand);
        Children.Add(_controls);
        Children.Add(_ring);
        // Out of sight and out of reach together until wanted: faded to
        // nothing they would still take presses over the picture.
        Motion.Hide(_controls);
        _controls.IsHitTestVisible = false;
        // The keyboard on one of them brings them out, as the web's
        // focus-within does, so that they are not reached unseen.
        _controls.GotFocus += (_, _) => { _focusInControls = true; ShowControls(); };
        _controls.LostFocus += (_, _) => { _focusInControls = false; ShowControls(); };

        // A press selects it (on to the stage or back), a second within the
        // web's 300 ms fills the screen with it (Room.tsx `Presses`).
        Tapped += (_, e) =>
        {
            e.Handled = true;
            room.Pressed(id);
        };
        DoubleTapped += (_, e) =>
        {
            e.Handled = true;
            room.Expand(id);
        };
        RightTapped += (_, e) =>
        {
            e.Handled = true;
            room.MenuFor(id, this);
        };
        IsTabStop = false;
        this.Arrives(anchor: Vector2.Zero);
    }

    /// <summary>Where it goes; the first place is taken at once, later ones are moved to over the settle curve.</summary>
    public void Place(Rect spot)
    {
        if (_placed && spot == _spot) return;
        var was = _spot;
        _spot = spot;
        Canvas.SetLeft(this, spot.Left);
        Canvas.SetTop(this, spot.Top);
        Width = Math.Max(1, spot.Width);
        Height = Math.Max(1, spot.Height);
        if (_placed) Motion.Settles(this, was, spot);
        _placed = true;
        var compact = spot.Height < 140;
        var plate = compact ? Sizing.NamePlate.CompactHeight : Sizing.NamePlate.Height;
        _plate.Margin = new Thickness(compact ? Sizing.NamePlate.CompactInset : Sizing.NamePlate.Inset);
        _plate.Height = plate;
        _plate.CornerRadius = new CornerRadius(plate / 2);
        _plate.Padding = new Thickness(compact ? 7 : 10, 0, compact ? 7 : 10, 0);
        _name.FontSize = compact ? 10.5 : 12.5;
        _hand.Margin = new Thickness(compact ? 5 : 8);
        _hand.Width = _hand.Height = compact ? 22 : 28;
    }

    public Rect Spot => _spot;

    /// <summary>
    /// The pointer is on this picture and not on another drawn over it
    /// (Sizing.Stage.Under, worked out by the room from where the pointer
    /// is): the stage's buttons come out for it, as on the Mac and the web.
    /// </summary>
    public bool PointerOver
    {
        set
        {
            if (_pointerOver == value) return;
            _pointerOver = value;
            ShowControls();
        }
    }

    /// <summary>The bar is out: the stage's buttons go away with it when nobody has moved.</summary>
    public bool BarOut
    {
        set
        {
            if (_barOut == value) return;
            _barOut = value;
            ShowControls();
        }
    }

    /// <summary>The stage's buttons out or away, fading either way over the arriving and the leaving curves.</summary>
    private void ShowControls()
    {
        var shown = _controls.Children.Count > 0 && Sizing.Stage.Shown(_pointerOver, _barOut, _focusInControls);
        if (shown == _controlsOut) return;
        _controlsOut = shown;
        _controls.IsHitTestVisible = shown;
        Motion.FadeTo(_controls, shown ? 1 : 0);
    }

    /// <summary>How far in from the picture's left the name's plate stands (Plan.NameUnderBar).</summary>
    public float PlateInset => (float)_plate.Margin.Left;

    /// <summary>
    /// How wide the name's plate was last laid out, or nought where there is
    /// no name: what decides, with where it stands, whether the bar covers it
    /// (Plan.NameUnderBar). Read from the layout, not measured here, since it
    /// is asked from the plate's own size change too; a name changed this
    /// moment is asked about again once it is laid out (the plate's
    /// SizeChanged above).
    /// </summary>
    public float PlateWidth => _plate.Visibility == Visibility.Visible ? (float)_plate.ActualWidth : 0;

    private float _lift;

    /// <summary>
    /// The name raised by this much above the foot of the picture
    /// (RoomView.kt <c>lift</c>): above the controls while
    /// they are out, for a picture that runs under them, and back down after
    /// them, over the settle curve up and the leave curve down. The person
    /// filling a call of two runs under the bar, and their name was drawn
    /// below it.
    /// </summary>
    public void Lift(float by)
    {
        if (Math.Abs(by - _lift) < 0.5f) return;
        var up = by > _lift;
        _lift = by;
        ElementCompositionPreview.SetIsTranslationEnabled(_plate, true);
        var visual = ElementCompositionPreview.GetElementVisual(_plate);
        var compositor = visual.Compositor;
        var move = compositor.CreateVector3KeyFrameAnimation();
        move.InsertKeyFrame(1f, new Vector3(0, -by, 0), up ? Motion.SettleCurve(compositor) : Motion.LeaveCurve(compositor));
        move.Duration = up ? Motion.Settle : Motion.Leave;
        visual.StartAnimation("Translation", move);
    }

    /// <summary>Brings the tile up to date: whose, what picture, which marks.</summary>
    public void Show(
        string label, string identity, string name, string? sid, bool camera, bool mine, bool speaking, bool muted, bool hand,
        bool selected, bool onStage, bool filled, IEnumerable<ShownReaction> reactions,
        Face? face = null, bool silenced = false, (string Key, Func<UIElement> Make)? controls = null, float? controlsFoot = null)
    {
        face ??= new Face.Picture();
        _name.Text = label;
        // The video watched together carries no name: its own stage says
        // what it is (RoomView.kt `TileMarks("", unlabelled = true)`).
        _plate.Visibility = label.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
        // The ring lights at once and is held 700 ms after speech stops, so
        // that it does not flicker between two words (Tile.kt `steady`).
        if (speaking) _quietSince = DateTime.MaxValue;
        else if (_speaking && _quietSince == DateTime.MaxValue) _quietSince = DateTime.UtcNow;
        var lit = speaking || (_quietSince != DateTime.MaxValue && DateTime.UtcNow - _quietSince < TimeSpan.FromMilliseconds(700));
        if (lit != _speaking)
        {
            _speaking = lit;
            Motion.FadeTo(_ring, lit ? 1 : 0);
        }
        _ring.BorderBrush = Ink.Tally;
        var radius = filled && onStage ? 0 : Theme.TileRadius;
        _clipShape.CornerRadius = new Vector2(radius, radius);
        _ring.CornerRadius = new CornerRadius(radius);
        Background = filled && onStage ? Ink.Brush(Shade.Black) : Ink.Surface;

        // The marks beside the name.
        while (_label.Children.Count > 1) _label.Children.RemoveAt(1);
        if (muted)
        {
            var mark = K.Symbol("mic.slash.fill", _name.FontSize - 2, Ink.Brush(Sizing.NamePlate.Ink));
            mark.Named(T("microphone off"));
            _label.Children.Add(mark);
        }
        // Somebody this listener turned off, said on their picture so that
        // nobody wonders why a person talking makes no sound (Tile.kt `silenced`).
        if (silenced) _label.Children.Add(K.Symbol("speaker.slash.fill", _name.FontSize - 2, Ink.Brush(Sizing.NamePlate.Ink)));
        // The stage's own controls over whatever is on it, arriving and
        // leaving with it.
        // At the picture's top corner on the stage; at its foot in a call
        // of two, lifted over the bar where the bar reaches under them
        // (Sizing.Stage.FootLift), the top corner there holding one's own
        // picture.
        var controlsKey = controls?.Key;
        if (controlsKey != _controlsKey)
        {
            _controlsKey = controlsKey;
            _controls.Children.Clear();
            if (controls is { } shown) _controls.Children.Add(shown.Make());
            ShowControls();
        }
        _controls.VerticalAlignment = controlsFoot != null ? VerticalAlignment.Bottom : VerticalAlignment.Top;
        _controls.Margin = controlsFoot is { } lift
            ? new Thickness(Sizing.Stage.Inset, Sizing.Stage.Inset, Sizing.Stage.Inset, Sizing.Stage.Inset + lift)
            : new Thickness(Sizing.Stage.Inset);
        _hand.Visibility = hand ? Visibility.Visible : Visibility.Collapsed;
        this.Named(selected ? T("Show everybody") : T("Show {name} larger", ("name", name)));

        // What fills the place, made again only when it changes.
        var key = face switch
        {
            Face.ShareCard => "card:" + name + (_spot.Height < 140 ? ":compact" : ""),
            Face.MyShare => "mine:screen" + (_spot.Height < 140 ? ":compact" : ""),
            Face.Drawn drawn => "drawn:" + drawn.Key,
            _ => null,
        };
        if (key != null)
        {
            if (key != _faceKey)
            {
                _faceKey = key;
                _content.Children.Clear();
                _picture = null;
                _self = null;
                _sid = null;
                var compact = _spot.Height < 140;
                UIElement made = face switch
                {
                    Face.ShareCard => SharePieces.ShareCard(name, compact),
                    Face.MyShare share => SharePieces.MyShare(compact, share.Stop),
                    Face.Drawn drawn => drawn.Make(),
                    _ => new Grid(),
                };
                _content.Children.Add(made.Arrives(1f));
            }
        }
        else if (_faceKey != null)
        {
            // Back to a picture from a card: made again below.
            _faceKey = null;
            _content.Children.Clear();
            _sid = null;
            _self = null;
            _picture = null;
        }
        // Otherwise the picture, or the initials with the camera off; a
        // place filled by a face above is left as it is.
        if (key != null)
        {
            // Nothing more to fill.
        }
        else if (mine)
        {
            if (camera && _self == null)
            {
                _content.Children.Clear();
                _picture = null;
                _self = new SelfView(_room.App.Media).Arrives(1f);
                _content.Children.Add(_self);
            }
            else if (!camera && (_self != null || _content.Children.Count == 0))
            {
                _self = null;
                _content.Children.Clear();
                _content.Children.Add(Avatar(name, identity));
            }
        }
        else if (sid != _sid || _content.Children.Count == 0)
        {
            _sid = sid;
            _content.Children.Clear();
            _picture = null;
            if (sid != null)
            {
                _picture = new RemotePicture(_room.Call.Core, sid, camera);
                _picture.Arrives(1f);
                _content.Children.Add(_picture);
            }
            else
            {
                _content.Children.Add(Avatar(name, identity));
            }
        }

        // Reactions float up over the picture for four seconds each (Reactions.kt `FloatingReactions`).
        var place = 0;
        foreach (var reaction in reactions)
        {
            if (_floating.Add(reaction.Id)) Float(reaction, place);
            place++;
        }
    }

    private void Float(ShownReaction reaction, int place)
    {
        var compact = _spot.Height < 140;
        var glyph = new TextBlock { Text = reaction.What, FontSize = compact ? 22 : 30, IsHitTestVisible = false };
        glyph.Measure(new Windows.Foundation.Size(200, 200));
        var x = _spot.Width / 2 + (place % 3) * 28 - 28 - glyph.DesiredSize.Width / 2;
        var y = _spot.Height - Math.Min(98, _spot.Height * 0.3) - glyph.DesiredSize.Height;
        Canvas.SetLeft(glyph, x);
        Canvas.SetTop(glyph, y);
        _reactions.Children.Add(glyph);
        ElementCompositionPreview.SetIsTranslationEnabled(glyph, true);
        var visual = ElementCompositionPreview.GetElementVisual(glyph);
        var compositor = visual.Compositor;
        var total = TimeSpan.FromMilliseconds(4000);
        var settle = Motion.SettleCurve(compositor);
        var easeIn = compositor.CreateCubicBezierEasingFunction(new Vector2(0.42f, 0), new Vector2(1, 1));
        // Arrives over the first 12 per cent, rises and fades over the last 30.
        var opacity = compositor.CreateScalarKeyFrameAnimation();
        opacity.InsertKeyFrame(0f, 0f);
        opacity.InsertKeyFrame(0.12f, 1f, settle);
        opacity.InsertKeyFrame(0.70f, 1f);
        opacity.InsertKeyFrame(1f, 0f, easeIn);
        opacity.Duration = total;
        var rise = compositor.CreateVector3KeyFrameAnimation();
        rise.InsertKeyFrame(0f, new Vector3(0, 6, 0));
        rise.InsertKeyFrame(0.12f, Vector3.Zero, settle);
        rise.InsertKeyFrame(0.70f, Vector3.Zero);
        rise.InsertKeyFrame(1f, new Vector3(0, -64, 0), easeIn);
        rise.Duration = total;
        var grow = compositor.CreateVector3KeyFrameAnimation();
        grow.InsertKeyFrame(0f, new Vector3(0.6f, 0.6f, 1));
        grow.InsertKeyFrame(0.12f, Vector3.One, settle);
        grow.Duration = total;
        visual.CenterPoint = new Vector3((float)glyph.DesiredSize.Width / 2, (float)glyph.DesiredSize.Height / 2, 0);
        var batch = compositor.CreateScopedBatch(CompositionBatchTypes.Animation);
        visual.StartAnimation("Opacity", opacity);
        visual.StartAnimation("Translation", rise);
        visual.StartAnimation("Scale", grow);
        batch.End();
        batch.Completed += (_, _) =>
        {
            _reactions.Children.Remove(glyph);
            _floating.Remove(reaction.Id);
        };
    }

    /// <summary>A camera that is off (Tile.kt <c>Avatar</c>): initials in a circle a little over a third of the tile's shorter side.</summary>
    private Grid Avatar(string name, string identity)
    {
        var grid = new Grid();
        var circle = new Grid { HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        var disc = new Ellipse { Fill = new SolidColorBrush(Faces.AvatarColour(string.IsNullOrEmpty(name) ? identity : name)) };
        var letters = K.Text(Faces.Initials(string.IsNullOrEmpty(name) ? identity : name), 20, K.Semibold, align: TextAlignment.Center).At(HorizontalAlignment.Center, VerticalAlignment.Center);
        circle.Children.Add(disc);
        circle.Children.Add(letters);
        grid.Children.Add(circle);
        grid.SizeChanged += (_, e) =>
        {
            var side = Math.Min(e.NewSize.Width, e.NewSize.Height) * 0.36;
            circle.Width = circle.Height = side;
            letters.FontSize = Math.Max(8, side * 0.38);
        };
        return grid.Quiet();
    }
}
