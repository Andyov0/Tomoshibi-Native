using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;

namespace Tomoshibi.UI.Kit;

/// <summary>Which side of its control a popover opens on, and how it lines up along that side.</summary>
internal enum Edge { Below, Above, Left }
internal enum Align { Start, Centre, End }

/// <summary>
/// What lies over the screen: one popover at a time, and the dialogs and
/// sheets (Android's FloatingPopoverLayer, DialogHost and SheetHost). Drawn
/// in the window's own tree rather than in Popup windows, so that each
/// arrives and leaves on the apps' curves, a press outside closes it with
/// its leaving motion instead of at once, and Escape closes the topmost.
/// </summary>
internal sealed class Overlay : Grid
{
    public static Overlay? Current { get; private set; }

    private readonly Canvas _popoverLayer = new();
    private readonly Grid _dialogLayer = new();
    private Pop? _pop;
    private readonly List<Held> _dialogs = new();

    private sealed record Pop(string Id, Border Catcher, FrameworkElement Panel, Action? Closed);
    private sealed record Held(Border Dim, FrameworkElement Panel, Action Dismiss, bool Sheet);

    public Overlay()
    {
        Current = this;
        Children.Add(_dialogLayer);
        Children.Add(_popoverLayer);
        // Lets presses through wherever nothing is open.
        Background = null;
    }

    /// <summary>The id of the popover open, if any.</summary>
    public string? Open => _pop?.Id;

    public bool Any => _pop != null || _dialogs.Count > 0;

    /// <summary>Escape: the popover on top goes first, then the topmost dialog. Whether anything was closed.</summary>
    public bool Back()
    {
        if (_pop != null)
        {
            ClosePopover();
            return true;
        }
        if (_dialogs.Count > 0)
        {
            _dialogs[^1].Dismiss();
            return true;
        }
        return false;
    }

    /// <summary>Opens a popover from a control, or closes it when that one is already open (Popovers.toggle).</summary>
    public void Toggle(string id, FrameworkElement anchor, Func<UIElement> content, double width = 260, Edge edge = Edge.Below, Align align = Align.End, Action? closed = null)
    {
        if (_pop?.Id == id)
        {
            ClosePopover();
            return;
        }
        Show(id, anchor, content(), width, edge, align, closed);
    }

    public void Show(string id, FrameworkElement anchor, UIElement content, double width = 260, Edge edge = Edge.Below, Align align = Align.End, Action? closed = null)
    {
        ClosePopover();
        var catcher = new Border { Background = K.Clear };
        catcher.Width = ActualWidth;
        catcher.Height = ActualHeight;
        catcher.PointerPressed += (_, e) =>
        {
            e.Handled = true;
            ClosePopover();
        };
        var limit = Math.Min(width, Math.Max(120, ActualWidth - 20));
        var scroller = new ScrollViewer
        {
            Content = content,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollMode = ScrollMode.Disabled,
            MaxHeight = Math.Max(120, ActualHeight - 20),
        };
        var panel = K.Glass(scroller, 18);
        panel.Width = limit;
        panel.TabFocusNavigation = KeyboardNavigationMode.Cycle;
        panel.Measure(new Windows.Foundation.Size(limit, ActualHeight - 20));
        var size = panel.DesiredSize;

        var bounds = anchor.TransformToVisual(this).TransformBounds(new Windows.Foundation.Rect(0, 0, anchor.ActualWidth, anchor.ActualHeight));
        const double gap = 8;
        double x, y;
        var side = edge;
        if (side == Edge.Left)
        {
            x = bounds.Left - gap - size.Width;
            y = align == Align.End ? bounds.Bottom - size.Height : bounds.Top + bounds.Height / 2 - size.Height / 2;
        }
        else
        {
            // Opens on its side unless that side has no room, then on the other.
            if (side == Edge.Below && bounds.Bottom + gap + size.Height > ActualHeight - 10 && bounds.Top - gap - size.Height >= 10) side = Edge.Above;
            else if (side == Edge.Above && bounds.Top - gap - size.Height < 10 && bounds.Bottom + gap + size.Height <= ActualHeight - 10) side = Edge.Below;
            y = side == Edge.Below ? bounds.Bottom + gap : bounds.Top - gap - size.Height;
            x = align switch
            {
                Align.Start => bounds.Left,
                Align.Centre => bounds.Left + bounds.Width / 2 - size.Width / 2,
                _ => bounds.Right - size.Width,
            };
        }
        x = Math.Clamp(x, 10, Math.Max(10, ActualWidth - 10 - size.Width));
        y = Math.Clamp(y, 10, Math.Max(10, ActualHeight - 10 - size.Height));
        Canvas.SetLeft(panel, x);
        Canvas.SetTop(panel, y);
        _popoverLayer.Children.Add(catcher);
        _popoverLayer.Children.Add(panel);
        _pop = new Pop(id, catcher, panel, closed);

        // Grows from the control's side, over the settle curve.
        var anchorPoint = side switch
        {
            Edge.Below => new Vector2(align == Align.Start ? 0 : align == Align.End ? 1 : 0.5f, 0),
            Edge.Above => new Vector2(align == Align.Start ? 0 : align == Align.End ? 1 : 0.5f, 1),
            _ => new Vector2(1, align == Align.End ? 1 : 0.5f),
        };
        Animate(panel, 0.94f, 1f, 0f, 1f, Motion.Settle, true, anchorPoint, new Vector2((float)size.Width, (float)size.Height));
        panel.Loaded += (_, _) => FocusInside(panel);
    }

    public void ClosePopover()
    {
        if (_pop is not { } pop) return;
        _pop = null;
        _popoverLayer.Children.Remove(pop.Catcher);
        var size = new Vector2((float)pop.Panel.ActualWidth, (float)pop.Panel.ActualHeight);
        Animate(pop.Panel, 1f, 0.97f, 1f, 0f, Motion.Leave, false, new Vector2(0.5f, 0.5f), size, () => _popoverLayer.Children.Remove(pop.Panel));
        pop.Closed?.Invoke();
    }

    /// <summary>
    /// A dialog over the screen (DialogHost): a dimmed ground a press on
    /// which dismisses, and the content on the apps' surface in the middle,
    /// at most <paramref name="width"/> wide. With <paramref name="sheet"/>,
    /// the narrow window's sheet instead (SheetHost): the content rising from
    /// the foot, the window's full width, the top corners rounded; with
    /// <paramref name="cover"/> as well, the whole window, as the phone's
    /// start sheet takes the whole screen, its content laying itself out.
    /// </summary>
    public Action ShowDialog(UIElement content, double width, double? maxHeight, Action dismiss, bool sheet = false, bool scrolls = true, bool cover = false)
    {
        var dim = new Border { Background = Ink.Black(0.45f) };
        dim.PointerPressed += (_, e) =>
        {
            e.Handled = true;
            dismiss();
        };
        FrameworkElement panel;
        // A page that scrolls its own parts (the settings: the sidebar stays,
        // the page moves) is given the height and does its own scrolling.
        UIElement Held() => scrolls
            ? new ScrollViewer { Content = content, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollMode = ScrollMode.Disabled }
            : content;
        if (sheet && cover)
        {
            panel = new Border
            {
                Child = content,
                Background = Ink.Background,
                VerticalAlignment = VerticalAlignment.Stretch,
                HorizontalAlignment = HorizontalAlignment.Stretch,
            };
        }
        else if (sheet)
        {
            var body = new Border
            {
                Child = Held(),
                Background = Ink.Background,
                CornerRadius = new CornerRadius(22, 22, 0, 0),
                // A page that scrolls itself takes the sheet's whole height.
                VerticalAlignment = scrolls ? VerticalAlignment.Bottom : VerticalAlignment.Stretch,
                Margin = new Thickness(0, 42, 0, 0),
                Shadow = new ThemeShadow(),
                Translation = new Vector3(0, 0, 32),
            };
            if (maxHeight is { } most) body.MaxHeight = most;
            panel = body;
        }
        else
        {
            var body = new Border
            {
                Child = Held(),
                Background = Ink.Surface,
                CornerRadius = new CornerRadius(26),
                MaxWidth = width,
                Margin = new Thickness(24, 56, 24, 24),
                // As wide as it may be: stretched and held to its most, XAML
                // centres it, where centred alone it would be as narrow as
                // its content asked and a wide page would lay itself out narrow.
                HorizontalAlignment = HorizontalAlignment.Stretch,
                VerticalAlignment = VerticalAlignment.Center,
                Shadow = new ThemeShadow(),
                Translation = new Vector3(0, 0, 48),
            };
            if (maxHeight is { } most) body.MaxHeight = most;
            if (!scrolls && maxHeight is { } tall)
            {
                void Fit() => body.Height = Math.Max(320, Math.Min(tall, ActualHeight - 80));
                Fit();
                SizeChanged += (_, _) => Fit();
            }
            panel = body;
        }
        panel.TabFocusNavigation = KeyboardNavigationMode.Cycle;
        // Presses inside are the dialog's own.
        panel.PointerPressed += (_, e) => e.Handled = true;
        _dialogLayer.Children.Add(dim);
        _dialogLayer.Children.Add(panel);
        var held = new Held(dim, panel, dismiss, sheet);
        _dialogs.Add(held);
        Motion.Hide(dim);
        Motion.FadeTo(dim, 1);
        if (sheet) Slide(panel, true);
        else
        {
            panel.SizeChanged += First;
            void First(object _, SizeChangedEventArgs e)
            {
                panel.SizeChanged -= First;
                Animate(panel, 0.96f, 1f, 0f, 1f, Motion.Settle, true, new Vector2(0.5f, 0.5f), new Vector2((float)e.NewSize.Width, (float)e.NewSize.Height), lift: (float)e.NewSize.Height / 12);
            }
            Motion.Hide(panel);
        }
        panel.Loaded += (_, _) => FocusInside(panel);
        return () => CloseDialog(held);
    }

    private void CloseDialog(Held held)
    {
        if (!_dialogs.Remove(held)) return;
        ClosePopover();
        Motion.FadeTo(held.Dim, 0);
        var remove = () =>
        {
            _dialogLayer.Children.Remove(held.Dim);
            _dialogLayer.Children.Remove(held.Panel);
        };
        if (held.Sheet) Slide(held.Panel, false, remove);
        else Animate(held.Panel, 1f, 0.98f, 1f, 0f, Motion.Leave, false, new Vector2(0.5f, 0.5f), new Vector2((float)held.Panel.ActualWidth, (float)held.Panel.ActualHeight), remove);
    }

    /// <summary>
    /// The keyboard's focus into the dialog, once it is shown, on its first
    /// control: unless what it holds put the focus somewhere of its own (the
    /// start sheet's field), which a focus put on the first control after
    /// it took away again.
    /// </summary>
    private void FocusInside(FrameworkElement panel)
    {
        DispatcherQueue.TryEnqueue(() =>
        {
            if (panel.XamlRoot is not { } root) return;
            for (var at = FocusManager.GetFocusedElement(root) as DependencyObject; at != null; at = VisualTreeHelper.GetParent(at))
            {
                if (ReferenceEquals(at, panel)) return;
            }
            if (FocusManager.FindFirstFocusableElement(panel) is Control control) control.Focus(FocusState.Programmatic);
        });
    }

    private static void Animate(FrameworkElement element, float fromScale, float toScale, float fromOpacity, float toOpacity, TimeSpan duration, bool arriving, Vector2 anchor, Vector2 size, Action? done = null, float lift = 0)
    {
        var visual = ElementCompositionPreview.GetElementVisual(element);
        var compositor = visual.Compositor;
        var curve = arriving ? Motion.SettleCurve(compositor) : Motion.LeaveCurve(compositor);
        visual.CenterPoint = new Vector3(size.X * anchor.X, size.Y * anchor.Y, 0);
        var batch = compositor.CreateScopedBatch(CompositionBatchTypes.Animation);
        var scale = compositor.CreateVector3KeyFrameAnimation();
        scale.InsertKeyFrame(0f, new Vector3(fromScale, fromScale, 1));
        scale.InsertKeyFrame(1f, new Vector3(toScale, toScale, 1), curve);
        scale.Duration = duration;
        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(0f, fromOpacity);
        fade.InsertKeyFrame(1f, toOpacity, curve);
        fade.Duration = duration;
        visual.StartAnimation("Scale", scale);
        visual.StartAnimation("Opacity", fade);
        if (lift != 0)
        {
            ElementCompositionPreview.SetIsTranslationEnabled(element, true);
            var rise = compositor.CreateVector3KeyFrameAnimation();
            rise.InsertKeyFrame(0f, new Vector3(0, lift, 0));
            rise.InsertKeyFrame(1f, Vector3.Zero, curve);
            rise.Duration = duration;
            visual.StartAnimation("Translation", rise);
        }
        batch.End();
        if (done != null) batch.Completed += (_, _) => done();
    }

    /// <summary>A sheet rising from the foot by its own height, or going back down.</summary>
    private void Slide(FrameworkElement panel, bool arriving, Action? done = null)
    {
        ElementCompositionPreview.SetIsTranslationEnabled(panel, true);
        var visual = ElementCompositionPreview.GetElementVisual(panel);
        var compositor = visual.Compositor;
        var height = (float)Math.Max(ActualHeight, 400);
        var batch = compositor.CreateScopedBatch(CompositionBatchTypes.Animation);
        var move = compositor.CreateVector3KeyFrameAnimation();
        move.InsertKeyFrame(0f, arriving ? new Vector3(0, height, 0) : Vector3.Zero);
        move.InsertKeyFrame(1f, arriving ? Vector3.Zero : new Vector3(0, height, 0), arriving ? Motion.SettleCurve(compositor) : Motion.LeaveCurve(compositor));
        move.Duration = arriving ? Motion.Settle : Motion.Leave;
        visual.StartAnimation("Translation", move);
        batch.End();
        if (done != null) batch.Completed += (_, _) => done();
    }

    /// <summary>Keeps the catchers as large as the window.</summary>
    protected override Windows.Foundation.Size ArrangeOverride(Windows.Foundation.Size finalSize)
    {
        if (_pop is { } pop)
        {
            pop.Catcher.Width = finalSize.Width;
            pop.Catcher.Height = finalSize.Height;
        }
        return base.ArrangeOverride(finalSize);
    }
}
