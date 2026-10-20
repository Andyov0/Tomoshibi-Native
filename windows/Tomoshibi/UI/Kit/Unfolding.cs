using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// A space that opens to what it holds and closes when it holds nothing,
/// what it says changing with a cross-fade while its height follows (Compose's
/// AnimatedContent with a SizeTransform, and expandVertically and
/// shrinkVertically about it): the old content fades out over the leave
/// curve as the new fades in over the settle curve, laid over each other,
/// and the height moves between theirs. Never a cut, in or out.
///
/// The height is the content's measured at the space's own width, which is
/// not known until the space is laid out: content shown before that is
/// measured again once it is, and the height moves on from wherever it has
/// got to. A space measured too early left the last line of a card, its
/// buttons, cut off below.
/// </summary>
internal sealed class Unfolding : Grid
{
    private UIElement? _shown;
    private Storyboard? _moving;
    private double _target;

    public Unfolding()
    {
        SizeChanged += (_, e) =>
        {
            // Content wider or taller than the space while it moves is cut at
            // its edge rather than drawn over what is below.
            Clip = new RectangleGeometry { Rect = new Windows.Foundation.Rect(0, 0, e.NewSize.Width, e.NewSize.Height) };
        };
        Loaded += (_, _) => Fit(animate: _moving != null);
        Height = 0;
    }

    private bool _refitting;

    /// <summary>
    /// The content measured without a limit on its height, whatever the
    /// space's is now; where it asks for other than the space is going to, the
    /// space follows on the next turn. That is what catches a new width
    /// wrapping the words again, and content inside that grew of its own
    /// accord (a card whose own body opened).
    /// </summary>
    protected override Windows.Foundation.Size MeasureOverride(Windows.Foundation.Size available)
    {
        var size = base.MeasureOverride(new Windows.Foundation.Size(available.Width, double.PositiveInfinity));
        var asked = _shown?.DesiredSize.Height ?? 0;
        if (Math.Abs(asked - _target) > 0.5 && !_refitting && IsLoaded)
        {
            _refitting = true;
            DispatcherQueue.TryEnqueue(() =>
            {
                _refitting = false;
                Fit(animate: true);
            });
        }
        return size;
    }

    /// <summary>What it holds now, or nothing, which closes it.</summary>
    public void Show(UIElement? content)
    {
        if (ReferenceEquals(content, _shown)) return;
        if (_shown != null) Children.Remove(_shown);
        _shown = content;
        if (content != null)
        {
            content.Arrives(1f);
            if (content is FrameworkElement framework) framework.VerticalAlignment = VerticalAlignment.Top;
            Children.Add(content);
        }
        Fit(animate: true);
    }

    /// <summary>What the content asks for at this width, or nothing.</summary>
    private double Measured()
    {
        if (_shown == null) return 0;
        var width = ActualWidth > 0 ? ActualWidth : double.PositiveInfinity;
        _shown.Measure(new Windows.Foundation.Size(width, double.PositiveInfinity));
        return _shown.DesiredSize.Height;
    }

    /// <summary>Moves the height to the content's, over the settle or leave curve, or at once.</summary>
    private void Fit(bool animate)
    {
        var target = Measured();
        if (_moving != null && Math.Abs(target - _target) < 0.5) return;
        _target = target;
        var from = ActualHeight;
        _moving?.Stop();
        _moving = null;
        if (!animate || !IsLoaded || Math.Abs(from - target) < 0.5)
        {
            Height = target;
            return;
        }
        Height = from;
        var opening = target > from;
        var move = new DoubleAnimation
        {
            From = from,
            To = target,
            Duration = opening ? Motion.Settle : Motion.Leave,
            EnableDependentAnimation = true,
            // The settle and leave curves of Theme, as a XAML easing: out for
            // what arrives, in for what leaves.
            EasingFunction = new CubicEase { EasingMode = opening ? EasingMode.EaseOut : EasingMode.EaseIn },
        };
        Storyboard.SetTarget(move, this);
        Storyboard.SetTargetProperty(move, "Height");
        var storyboard = new Storyboard { Children = { move } };
        storyboard.Completed += (_, _) =>
        {
            if (!ReferenceEquals(_moving, storyboard)) return;
            _moving = null;
            // Held at what it measures now, which a late layout may have changed.
            Height = Measured();
        };
        _moving = storyboard;
        storyboard.Begin();
    }

    /// <summary>The content changed size of its own accord (more lines): the space follows.</summary>
    public void Refit() => Fit(animate: true);
}
