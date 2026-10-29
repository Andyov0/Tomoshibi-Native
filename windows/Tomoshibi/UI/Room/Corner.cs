using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// What the call says in its corner (Android's kit/Toasts.kt; Notices.tsx):
/// at most three notices that go by themselves, piled with the newest in
/// front and the others peeking above it, spread into a column while the
/// pointer is over them, every countdown held while they are read. Each
/// says what it is about by the rail at its edge: the edge is the whole of
/// the severity system.
/// </summary>
internal sealed class ToastStack : Canvas
{
    private readonly CallModel _call;
    private readonly Dictionary<long, Border> _cards = new();
    private readonly Dictionary<long, CallToast> _drawn = new();
    private bool _spread;
    private double _lift;

    public ToastStack(CallModel call)
    {
        _call = call;
        Width = 300;
        HorizontalAlignment = HorizontalAlignment.Right;
        VerticalAlignment = VerticalAlignment.Bottom;
        Background = K.Clear;
        PointerEntered += (_, _) =>
        {
            _spread = true;
            call.HoldToasts(true);
            Lay();
        };
        PointerExited += (_, _) =>
        {
            _spread = false;
            call.HoldToasts(false);
            Lay();
        };
        Update();
    }

    /// <summary>How far the pile rises to keep clear of the messages in the other corner (CornerLanes).</summary>
    public double Lift
    {
        set
        {
            if (Math.Abs(value - _lift) < 0.5) return;
            _lift = value;
            Lay();
        }
    }

    public void Update()
    {
        var toasts = _call.Toasts;
        var ids = toasts.Select(t => t.Id).ToHashSet();
        foreach (var gone in _cards.Keys.Where(id => !ids.Contains(id)).ToList())
        {
            var card = _cards[gone];
            _cards.Remove(gone);
            _drawn.Remove(gone);
            Children.Remove(card);
        }
        foreach (var toast in toasts)
        {
            if (_cards.TryGetValue(toast.Id, out var card))
            {
                // Said again under its key: the words change inside the
                // card's own glass, which stays where it is. Only when they
                // did change: drawn again on every change to the call, the
                // door's notice made its list afresh every two seconds.
                if (_drawn.TryGetValue(toast.Id, out var drawn) && drawn == toast) continue;
                _drawn[toast.Id] = toast;
                if (card.Child is Border kept) kept.Child = Line(toast);
                continue;
            }
            card = new Border { Width = 300, Child = Line(toast) };
            var glass = K.Glass(null, 14, shadow: true);
            card.Child = null;
            glass.Child = Line(toast);
            card.Child = glass;
            card.Arrives(0.96f, new Vector2(0.5f, 1f), fromY: 18);
            _cards[toast.Id] = card;
            _drawn[toast.Id] = toast;
            Children.Add(card);
        }
        Lay();
    }

    /// <summary>Puts each card in its place in the pile, newest in front.</summary>
    private void Lay()
    {
        var newestFirst = _call.Toasts.Reverse().Where(t => _cards.ContainsKey(t.Id)).ToList();
        var heights = newestFirst.Select(t =>
        {
            var card = _cards[t.Id];
            card.Measure(new Windows.Foundation.Size(300, 400));
            return (float)Math.Max(44, card.DesiredSize.Height);
        }).ToList();
        var lifts = NoticePile.Lifts(heights, _spread);
        var total = NoticePile.Height(heights, _spread);
        Height = Math.Max(1, total + _lift);
        for (var place = 0; place < newestFirst.Count; place++)
        {
            var card = _cards[newestFirst[place].Id];
            var bottom = total - lifts[place];
            var top = bottom - heights[place];
            // A card behind the front, piled, shows only its edge.
            if (!_spread && place > 0) top = total - lifts[place] - heights[0];
            Canvas.SetTop(card, Math.Max(0, top));
            Canvas.SetZIndex(card, newestFirst.Count - place);
            var visual = ElementCompositionPreview.GetElementVisual(card);
            visual.CenterPoint = new Vector3(150, 0, 0);
            var scale = NoticePile.Scale(place, _spread);
            var compositor = visual.Compositor;
            var grow = compositor.CreateVector3KeyFrameAnimation();
            grow.InsertKeyFrame(1f, new Vector3(scale, scale, 1), Motion.SettleCurve(compositor));
            grow.Duration = Motion.Settle;
            visual.StartAnimation("Scale", grow);
            if (card.Child is Border glass && glass.Child is UIElement inside)
            {
                inside.Opacity = _spread || place == 0 ? 1 : 0;
            }
        }
    }

    /// <summary>One notice's line (Toasts.kt <c>ToastLine</c>): the rail, the words, its button and its close mark, read out as it arrives (Spoken).</summary>
    private Grid Line(CallToast toast)
    {
        var rail = new Border
        {
            Width = 3,
            CornerRadius = new CornerRadius(1.5),
            Margin = new Thickness(0, 1, 0, 1),
            Background = toast.Kind switch
            {
                CallToast.Kinds.Info => Ink.Wash(0.22f),
                CallToast.Kinds.Failure => Ink.Danger,
                _ => Ink.Tally,
            },
        };
        var words = K.Column(3, K.Text(toast.Title, 13, K.Medium));
        if (toast.Detail != null) words.Children.Add(K.Text(toast.Detail, 11.5, ink: Ink.Muted));
        if (toast.Attachment is { } attachment && _call.Attachments.TryGetValue(attachment, out var draw))
        {
            var drawn = draw();
            if (drawn is FrameworkElement element) element.Margin = new Thickness(0, 6, 0, 0);
            words.Children.Add(drawn);
        }
        var parts = new List<(UIElement, double)> { K.Fixed(rail), K.Flex(words) };
        if (toast.ActionTitle != null)
        {
            var action = new Tap(K.Text(toast.ActionTitle, 11.5, K.Semibold, Ink.Background, lines: 1), Look.Plain, 26, padding: 10);
            action.Resting = Ink.Palette.Foreground;
            action.VerticalAlignment = VerticalAlignment.Center;
            action.Click += (_, _) => _call.Act(toast.Id);
            parts.Add(K.Fixed(action));
        }
        if (toast.CanClose)
        {
            var close = Tap.Icon("xmark", T("Dismiss"), Look.Clear, 20, 8.5);
            close.VerticalAlignment = VerticalAlignment.Top;
            close.Click += (_, _) => _call.Dismiss(toast.Id);
            parts.Add(K.Fixed(close));
        }
        var line = K.Line(10, parts.ToArray());
        line.Padding = new Thickness(9, 10, toast.CanClose ? 7 : 12, 10);
        return new Spoken(line, toast.Detail != null ? toast.Title + ". " + toast.Detail : toast.Title);
    }
}

/// <summary>
/// What others said while the messages were closed (kit/Said.kt), in the
/// other corner for six seconds each: the last three, a run by one person
/// under their monogram once.
/// </summary>
internal sealed class SaidColumn : StackPanel
{
    private readonly CallModel _call;
    private readonly Dictionary<string, Border> _cards = new();

    public SaidColumn(CallModel call)
    {
        _call = call;
        Spacing = 6;
        MaxWidth = 290;
        VerticalAlignment = VerticalAlignment.Bottom;
        HorizontalAlignment = HorizontalAlignment.Left;
        IsHitTestVisible = true;
        Update();
    }

    public void Update()
    {
        var lines = _call.Arriving.TakeLast(3).ToList();
        var ids = lines.Select(l => l.Id).ToHashSet();
        foreach (var gone in _cards.Keys.Where(id => !ids.Contains(id)).ToList())
        {
            Children.Remove(_cards[gone]);
            _cards.Remove(gone);
        }
        for (var i = 0; i < lines.Count; i++)
        {
            var line = lines[i];
            if (_cards.ContainsKey(line.Id)) continue;
            var run = i > 0 && lines[i - 1].Identity == line.Identity;
            var monogram = Faces.Monogram(line.Name, line.Identity, 24);
            if (run) monogram.Opacity = 0;
            var words = K.Column(2);
            if (!run) words.Children.Add(K.Text(Refusals.Named(line.Name, line.Identity), 11, K.Semibold, Ink.Muted, lines: 1));
            words.Children.Add(Panels.Linked(line.Text, 13, lines: 5));
            var card = K.Holding(K.Glass(K.Line(9, K.Fixed(monogram.At(HorizontalAlignment.Left, VerticalAlignment.Top)), K.Flex(words)), 16, new Thickness(8, run ? 7 : 8, 12, run ? 7 : 8)));
            card.Arrives(0.94f, new Vector2(0, 1), fromY: 8);
            _cards[line.Id] = card;
            Children.Add(card);
        }
    }
}
