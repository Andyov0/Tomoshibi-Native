using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// A column of parts that come and go by name (Android's
/// <c>keepDeparting</c> with AnimatedVisibility): a part that appears
/// arrives on the settle curve, one that is no longer there leaves on the
/// leave curve, and one that stays is made again only when what it says
/// has changed, so that a panel told something new does not flash every
/// part it already showed.
/// </summary>
internal sealed class Keyed : StackPanel
{
    private readonly Dictionary<string, (Border Holder, string Said)> _parts = new();
    private string _order = "";

    public Keyed(double spacing)
    {
        Spacing = spacing;
    }

    /// <summary>
    /// The parts in order, each with what it says and how to make it; a part
    /// whose words are the same as last time keeps what it drew, and those
    /// not named go.
    /// </summary>
    public void Show(IReadOnlyList<(string Key, string Said, Func<UIElement> Make)> parts)
    {
        // Where each part stood, so that those a part arriving or leaving
        // pushes along can slide there rather than jump.
        var stood = IsLoaded ? Children.OfType<Border>().ToDictionary(b => b, b => b.ActualOffset.Y) : null;
        var keys = parts.Select(p => p.Key).ToHashSet();
        foreach (var gone in _parts.Keys.Where(k => !keys.Contains(k)).ToList())
        {
            Children.Remove(_parts[gone].Holder);
            _parts.Remove(gone);
        }
        foreach (var (key, said, make) in parts)
        {
            if (_parts.TryGetValue(key, out var part) && part.Said == said) continue;
            var holder = part.Holder;
            if (holder == null)
            {
                holder = new Border();
                holder.Arrives();
            }
            holder.Child = make();
            _parts[key] = (holder, said);
        }
        var order = string.Join("\n", parts.Select(p => p.Key));
        if (order == _order) return;
        _order = order;
        if (stood != null) Slide(stood);
        // In the order asked for, moving only what moved.
        for (var i = 0; i < parts.Count; i++)
        {
            var holder = _parts[parts[i].Key].Holder;
            var at = Children.IndexOf(holder);
            if (at == i) continue;
            if (at >= 0) Children.RemoveAt(at);
            Children.Insert(Math.Min(i, Children.Count), holder);
        }
    }

    /// <summary>
    /// The parts that stayed, slid from where they stood to where the new
    /// layout puts them, on the settle curve. Done here rather than by
    /// XAML's RepositionThemeTransition, which, under the arriving parts'
    /// own implicit show and hide animations, left the parts it moved faded
    /// for good: the host panel's people and its ending stood at a tenth of
    /// their opacity once the door's card had arrived above them.
    /// </summary>
    private void Slide(Dictionary<Border, float> stood)
    {
        void Laid(object? sender, object e)
        {
            LayoutUpdated -= Laid;
            foreach (var (holder, was) in stood)
            {
                if (!ReferenceEquals(holder.Parent, this)) continue;
                var by = was - holder.ActualOffset.Y;
                if (Math.Abs(by) < 0.5f) continue;
                ElementCompositionPreview.SetIsTranslationEnabled(holder, true);
                var visual = ElementCompositionPreview.GetElementVisual(holder);
                var compositor = visual.Compositor;
                var slide = compositor.CreateVector3KeyFrameAnimation();
                slide.InsertKeyFrame(0f, new Vector3(0, by, 0));
                slide.InsertKeyFrame(1f, Vector3.Zero, Motion.SettleCurve(compositor));
                slide.Duration = Motion.Settle;
                visual.StartAnimation("Translation", slide);
            }
        }
        LayoutUpdated += Laid;
    }
}

/// <summary>
/// Which rows of a list drawn again have been seen before (the keys of
/// Compose's <c>items</c> under AnimatedVisibility): a row arrives the first
/// time it is drawn and is simply there every time after, rather than every
/// row fading in again whenever the list is drawn.
/// </summary>
internal sealed class Arrivals
{
    private HashSet<string> _before = new();
    private HashSet<string> _now = new();

    /// <summary>Starts a drawing of the list.</summary>
    public void Begin() => _now = new HashSet<string>();

    /// <summary>The row, arriving if it was not in the last drawing.</summary>
    public T Row<T>(string key, T row) where T : FrameworkElement
    {
        _now.Add(key);
        return _before.Contains(key) ? row : row.Arrives();
    }

    /// <summary>Ends a drawing: what was drawn is what the next one compares with.</summary>
    public void End() => _before = _now;
}
