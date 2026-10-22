using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// Draws a <see cref="Pace"/> frame by frame while it is behind what it
/// shows, from CompositionTarget.Rendering, and not at all once it has
/// caught up or while the element it belongs to is out of the window: one
/// handler on every frame of the app for a bar that is standing still is a
/// cost for nothing. Each frame moves the pace by the frame's own time, which
/// every handler of that frame is given alike, so two views of one bar (the
/// banner and the settings' row) do not move it twice as fast.
///
/// What is drawn is set on the element each frame rather than animated
/// towards: a composition animation started at every report (a tenth of a
/// second apart, from the core) eased out over the settle curve each time,
/// and the bar went across in surges.
/// </summary>
internal sealed class Paced
{
    private readonly Pace _pace;
    private readonly FrameworkElement _owner;
    private readonly Action<double> _draw;
    private bool _ticking;

    /// <param name="owner">What the drawing is in: frames are taken only while it is loaded.</param>
    /// <param name="draw">Draws what is shown, from nought to one; at once, and then on every frame it moves.</param>
    public Paced(FrameworkElement owner, Pace pace, Action<double> draw)
    {
        _owner = owner;
        _pace = pace;
        _draw = draw;
        owner.Loaded += (_, _) => Wake();
        owner.Unloaded += (_, _) => Rest();
        draw(pace.Shown);
    }

    /// <summary>Told whenever what has arrived may have moved: draws it as it stands, and takes frames again if it is behind.</summary>
    public void Wake()
    {
        _draw(_pace.Shown);
        if (_ticking || !_pace.Moving || !_owner.IsLoaded) return;
        _ticking = true;
        CompositionTarget.Rendering += Frame;
    }

    private void Frame(object? sender, object e)
    {
        var moving = _pace.Advance(((RenderingEventArgs)e).RenderingTime);
        _draw(_pace.Shown);
        if (!moving) Rest();
    }

    private void Rest()
    {
        if (!_ticking) return;
        _ticking = false;
        CompositionTarget.Rendering -= Frame;
    }
}
