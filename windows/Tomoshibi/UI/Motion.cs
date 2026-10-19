using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;

namespace Tomoshibi.UI;

/// <summary>
/// The apps' motion (Theme.swift, Android's Theme.kt): arrivals settle in
/// over 320 ms, fading up from 97 per cent of their size; departures fade out
/// in 200 ms. Every view that appears or disappears goes through these; the
/// person who owns the product does not accept one that simply pops in or
/// out.
///
/// They are XAML's implicit show and hide animations, which run when an
/// element is made visible or collapsed and also when it is added to or
/// taken out of the tree, so a tile removed with its participant still fades.
/// </summary>
internal static class Motion
{
    private static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(320);
    private static readonly TimeSpan Leave = TimeSpan.FromMilliseconds(200);

    public static T Arrives<T>(this T element) where T : UIElement
    {
        var visual = ElementCompositionPreview.GetElementVisual(element);
        var compositor = visual.Compositor;
        // --ease-settle and --ease-leave.
        var settle = compositor.CreateCubicBezierEasingFunction(new Vector2(0.2f, 0.9f), new Vector2(0.3f, 1f));
        var leave = compositor.CreateCubicBezierEasingFunction(new Vector2(0.4f, 0f), new Vector2(1f, 1f));

        var fadeIn = compositor.CreateScalarKeyFrameAnimation();
        fadeIn.Target = "Opacity";
        fadeIn.InsertKeyFrame(0f, 0f);
        fadeIn.InsertKeyFrame(1f, 1f, settle);
        fadeIn.Duration = Settle;

        var grow = compositor.CreateVector3KeyFrameAnimation();
        grow.Target = "Scale";
        grow.InsertKeyFrame(0f, new Vector3(0.97f, 0.97f, 1f));
        grow.InsertKeyFrame(1f, Vector3.One, settle);
        grow.Duration = Settle;

        var show = compositor.CreateAnimationGroup();
        show.Add(fadeIn);
        show.Add(grow);

        var fadeOut = compositor.CreateScalarKeyFrameAnimation();
        fadeOut.Target = "Opacity";
        fadeOut.InsertKeyFrame(0f, 1f);
        fadeOut.InsertKeyFrame(1f, 0f, leave);
        fadeOut.Duration = Leave;

        ElementCompositionPreview.SetImplicitShowAnimation(element, show);
        ElementCompositionPreview.SetImplicitHideAnimation(element, fadeOut);

        // Scaled about its middle, wherever layout puts it.
        if (element is FrameworkElement framework)
        {
            framework.SizeChanged += (_, e) =>
                visual.CenterPoint = new Vector3((float)e.NewSize.Width / 2, (float)e.NewSize.Height / 2, 0);
        }
        return element;
    }
}
