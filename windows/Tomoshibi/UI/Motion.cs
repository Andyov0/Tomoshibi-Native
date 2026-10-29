using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;
using Theme = Tomoshibi.Basics.Theme;

namespace Tomoshibi.UI;

/// <summary>
/// The apps' motion (Theme.swift, Android's Theme.kt): arrivals settle in
/// over 320 ms on --ease-settle, fading up from 97 per cent of their size;
/// departures fade out in 200 ms on --ease-leave. Every view that appears or
/// disappears goes through these; the person who owns the product does not
/// accept one that simply pops in or out.
///
/// They are composition animations on the element's own visual: implicit
/// show and hide animations, which run when an element is made visible or
/// collapsed and also when it is added to or taken out of the tree, so that
/// a tile taken away with its participant still fades; and one-off
/// animations for what moves while it stays.
/// </summary>
internal static class Motion
{
    public static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(Theme.SettleMs);
    public static readonly TimeSpan Leave = TimeSpan.FromMilliseconds(Theme.LeaveMs);
    /// <summary>A hover or a press, the colour and the scale together: 140 ms, the web's <c>pressable</c>.</summary>
    public static readonly TimeSpan Hover = TimeSpan.FromMilliseconds(140);

    private static Visual VisualOf(UIElement element) => ElementCompositionPreview.GetElementVisual(element);

    public static CompositionEasingFunction SettleCurve(Compositor compositor) =>
        compositor.CreateCubicBezierEasingFunction(new Vector2(Theme.SettleCurve.X1, Theme.SettleCurve.Y1), new Vector2(Theme.SettleCurve.X2, Theme.SettleCurve.Y2));

    public static CompositionEasingFunction LeaveCurve(Compositor compositor) =>
        compositor.CreateCubicBezierEasingFunction(new Vector2(Theme.LeaveCurve.X1, Theme.LeaveCurve.Y1), new Vector2(Theme.LeaveCurve.X2, Theme.LeaveCurve.Y2));

    /// <summary>Keeps the element's scale about its middle, wherever layout puts it.</summary>
    private static void Centre(FrameworkElement element, Visual visual, Vector2? anchor = null)
    {
        void Place(double width, double height)
        {
            var (ax, ay) = anchor is { } a ? (a.X, a.Y) : (0.5f, 0.5f);
            visual.CenterPoint = new Vector3((float)width * ax, (float)height * ay, 0);
        }
        element.SizeChanged += (_, e) => Place(e.NewSize.Width, e.NewSize.Height);
        Place(element.ActualWidth, element.ActualHeight);
    }

    /// <summary>
    /// Arrives fading up from <paramref name="from"/> of its size over the
    /// settle curve, and leaves fading over the leave curve; <paramref name="anchor"/>
    /// is the point it grows from, as fractions of its size (the middle unless told).
    /// </summary>
    public static T Arrives<T>(this T element, float from = Theme.ArriveScale, Vector2? anchor = null, float fromX = 0, float fromY = 0) where T : UIElement
    {
        var visual = VisualOf(element);
        var compositor = visual.Compositor;
        var settle = SettleCurve(compositor);
        var leave = LeaveCurve(compositor);

        var fadeIn = compositor.CreateScalarKeyFrameAnimation();
        fadeIn.Target = "Opacity";
        fadeIn.InsertKeyFrame(0f, 0f);
        fadeIn.InsertKeyFrame(1f, 1f, settle);
        fadeIn.Duration = Settle;

        var show = compositor.CreateAnimationGroup();
        show.Add(fadeIn);
        if (from != 1f)
        {
            var grow = compositor.CreateVector3KeyFrameAnimation();
            grow.Target = "Scale";
            grow.InsertKeyFrame(0f, new Vector3(from, from, 1f));
            grow.InsertKeyFrame(1f, Vector3.One, settle);
            grow.Duration = Settle;
            show.Add(grow);
        }
        if (fromX != 0 || fromY != 0)
        {
            ElementCompositionPreview.SetIsTranslationEnabled(element, true);
            var slide = compositor.CreateVector3KeyFrameAnimation();
            slide.Target = "Translation";
            slide.InsertKeyFrame(0f, new Vector3(fromX, fromY, 0));
            slide.InsertKeyFrame(1f, Vector3.Zero, settle);
            slide.Duration = Settle;
            show.Add(slide);
        }

        var fadeOut = compositor.CreateScalarKeyFrameAnimation();
        fadeOut.Target = "Opacity";
        fadeOut.InsertKeyFrame(0f, 1f);
        fadeOut.InsertKeyFrame(1f, 0f, leave);
        fadeOut.Duration = Leave;
        var hide = compositor.CreateAnimationGroup();
        hide.Add(fadeOut);
        if (fromX != 0 || fromY != 0)
        {
            var away = compositor.CreateVector3KeyFrameAnimation();
            away.Target = "Translation";
            away.InsertKeyFrame(0f, Vector3.Zero);
            away.InsertKeyFrame(1f, new Vector3(fromX, fromY, 0), leave);
            away.Duration = Leave;
            hide.Add(away);
        }

        ElementCompositionPreview.SetImplicitShowAnimation(element, show);
        ElementCompositionPreview.SetImplicitHideAnimation(element, hide);
        if (element is FrameworkElement framework) Centre(framework, visual, anchor);
        return element;
    }

    /// <summary>
    /// The screens before a call bring their pieces in in an order: each from
    /// fourteen points below, over 420 ms on the arriving curve, after
    /// <paramref name="delayMs"/> (Android's <c>rise</c>). Once, when it is
    /// first shown.
    /// </summary>
    public static T Rises<T>(this T element, int delayMs = 0) where T : FrameworkElement
    {
        ElementCompositionPreview.SetIsTranslationEnabled(element, true);
        var visual = VisualOf(element);
        visual.Opacity = 0;
        var started = false;
        element.Loaded += (_, _) =>
        {
            if (started) return;
            started = true;
            var compositor = visual.Compositor;
            var settle = SettleCurve(compositor);
            var duration = TimeSpan.FromMilliseconds(Theme.RiseMs);
            var fade = compositor.CreateScalarKeyFrameAnimation();
            fade.InsertKeyFrame(0f, 0f);
            fade.InsertKeyFrame(1f, 1f, settle);
            fade.Duration = duration;
            fade.DelayTime = TimeSpan.FromMilliseconds(delayMs);
            fade.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            var lift = compositor.CreateVector3KeyFrameAnimation();
            lift.InsertKeyFrame(0f, new Vector3(0, Theme.RiseLift, 0));
            lift.InsertKeyFrame(1f, Vector3.Zero, settle);
            lift.Duration = duration;
            lift.DelayTime = TimeSpan.FromMilliseconds(delayMs);
            lift.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            visual.StartAnimation("Opacity", fade);
            visual.StartAnimation("Translation", lift);
        };
        return element;
    }

    /// <summary>Starts the element's visual transparent, for a fade up to begin from.</summary>
    public static void Hide(UIElement element) => VisualOf(element).Opacity = 0;

    /// <summary>Fades the element's visual to <paramref name="opacity"/>, on the settle curve going up and the leave curve going down.</summary>
    public static void FadeTo(UIElement element, float opacity, TimeSpan? duration = null)
    {
        var visual = VisualOf(element);
        var compositor = visual.Compositor;
        var up = opacity >= visual.Opacity;
        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(1f, opacity, up ? SettleCurve(compositor) : LeaveCurve(compositor));
        fade.Duration = duration ?? (up ? Settle : Leave);
        visual.StartAnimation("Opacity", fade);
    }

    /// <summary>
    /// Moves a text's colour to another over the settle curve, rather than
    /// switching it: a brush of its own is animated, so that a brush the text
    /// shares with others is left as it is.
    /// </summary>
    public static void Recolour(Microsoft.UI.Xaml.Controls.TextBlock text, Windows.UI.Color to)
    {
        var from = text.Foreground is Microsoft.UI.Xaml.Media.SolidColorBrush now ? now.Color : to;
        var brush = new Microsoft.UI.Xaml.Media.SolidColorBrush(from);
        text.Foreground = brush;
        if (from == to) return;
        var change = new Microsoft.UI.Xaml.Media.Animation.ColorAnimation
        {
            To = to,
            Duration = Settle,
            EnableDependentAnimation = true,
            EasingFunction = new Microsoft.UI.Xaml.Media.Animation.CubicEase { EasingMode = Microsoft.UI.Xaml.Media.Animation.EasingMode.EaseOut },
        };
        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTarget(change, brush);
        Microsoft.UI.Xaml.Media.Animation.Storyboard.SetTargetProperty(change, "Color");
        var board = new Microsoft.UI.Xaml.Media.Animation.Storyboard();
        board.Children.Add(change);
        board.Begin();
    }

    /// <summary>Scales the element's visual about its middle, over the hover curve unless told.</summary>
    public static void ScaleTo(FrameworkElement element, float scale, TimeSpan? duration = null)
    {
        var visual = VisualOf(element);
        visual.CenterPoint = new Vector3((float)element.ActualWidth / 2, (float)element.ActualHeight / 2, 0);
        var compositor = visual.Compositor;
        var grow = compositor.CreateVector3KeyFrameAnimation();
        grow.InsertKeyFrame(1f, new Vector3(scale, scale, 1f), compositor.CreateCubicBezierEasingFunction(new Vector2(0f, 0f), new Vector2(0.58f, 1f)));
        grow.Duration = duration ?? Hover;
        visual.StartAnimation("Scale", grow);
    }

    /// <summary>Turns the element's visual about its middle to <paramref name="degrees"/>, on the settle curve.</summary>
    public static void TurnTo(FrameworkElement element, float degrees)
    {
        var visual = VisualOf(element);
        visual.CenterPoint = new Vector3((float)element.ActualWidth / 2, (float)element.ActualHeight / 2, 0);
        var compositor = visual.Compositor;
        var turn = compositor.CreateScalarKeyFrameAnimation();
        turn.InsertKeyFrame(1f, degrees, SettleCurve(compositor));
        turn.Duration = Settle;
        visual.StartAnimation("RotationAngleInDegrees", turn);
    }

    /// <summary>
    /// Moves a picture from where it was to where layout has just put it,
    /// over the settle curve: its visual starts offset and scaled back to the
    /// old place and settles into the new one, so that a tile going to the
    /// stage is seen to go there rather than to vanish from one place and
    /// appear in another (Android animates the spot; the swap chain cannot be
    /// laid out through an animation, so the visual is).
    /// </summary>
    public static void Settles(FrameworkElement element, Basics.Rect was, Basics.Rect now)
    {
        if (was.Width <= 0 || now.Width <= 0 || was.Height <= 0 || now.Height <= 0 || was == now) return;
        ElementCompositionPreview.SetIsTranslationEnabled(element, true);
        var visual = VisualOf(element);
        var compositor = visual.Compositor;
        var settle = SettleCurve(compositor);
        visual.CenterPoint = Vector3.Zero;
        var scale = compositor.CreateVector3KeyFrameAnimation();
        scale.InsertKeyFrame(0f, new Vector3(was.Width / now.Width, was.Height / now.Height, 1f));
        scale.InsertKeyFrame(1f, Vector3.One, settle);
        scale.Duration = Settle;
        var move = compositor.CreateVector3KeyFrameAnimation();
        move.InsertKeyFrame(0f, new Vector3(was.Left - now.Left, was.Top - now.Top, 0));
        move.InsertKeyFrame(1f, Vector3.Zero, settle);
        move.Duration = Settle;
        visual.StartAnimation("Scale", scale);
        visual.StartAnimation("Translation", move);
    }
}
