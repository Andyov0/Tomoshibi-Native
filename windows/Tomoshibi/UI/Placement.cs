using System.Text.Json;
using Microsoft.UI.Windowing;
using Tomoshibi.Basics;
using Windows.Graphics;

namespace Tomoshibi.UI;

/// <summary>
/// Where the window opens and how large (the Mac app's window does the same):
/// the first time a desktop window of about 1280 by 820 at the monitor's own
/// scale, never more than nine tenths of the work area, in the middle of it;
/// after that where it was left, as large as it was left, maximised if it was.
///
/// The phone's layout is what a window dragged narrow shows, past the other
/// apps' 600-point line (Screen.Wide); a window as it opens is a desktop one.
/// Kept with the other choices, so that a test, which keeps nothing, never
/// moves the owner's window.
/// </summary>
internal static class Placement
{
    public const string Key = "tomoshibi.window";

    /// <summary>The Mac app's first size, in device-independent pixels.</summary>
    public const int Width = 1280;
    public const int Height = 820;

    /// <summary>The narrowest a phone is held, and tall enough for a join screen's pane.</summary>
    public const int MinimumWidth = 360;
    public const int MinimumHeight = 560;

    private sealed record Kept(int X, int Y, int Width, int Height, bool Maximised);

    public static void Restore(AppWindow window, IDefaults defaults, double scale)
    {
        var area = DisplayArea.GetFromWindowId(window.Id, DisplayAreaFallback.Primary).WorkArea;
        if (Read(defaults) is { } kept && OnSomeDisplay(kept))
        {
            window.MoveAndResize(new RectInt32(kept.X, kept.Y, kept.Width, kept.Height));
            if (kept.Maximised && window.Presenter is OverlappedPresenter presenter) presenter.Maximize();
            return;
        }
        var width = Math.Min((int)Math.Round(Width * scale), (int)(area.Width * 0.9));
        var height = Math.Min((int)Math.Round(Height * scale), (int)(area.Height * 0.9));
        window.MoveAndResize(new RectInt32(area.X + (area.Width - width) / 2, area.Y + (area.Height - height) / 2, width, height));
    }

    /// <summary>Writes down where the window is: its restored bounds, and whether it is maximised.</summary>
    public static void Keep(AppWindow window, IDefaults defaults)
    {
        if (window.Presenter is not OverlappedPresenter presenter) return;
        // A full screen (a stage filled) or a minimised window is not where it was left.
        if (presenter.State == OverlappedPresenterState.Minimized) return;
        var maximised = presenter.State == OverlappedPresenterState.Maximized;
        var previous = Read(defaults);
        // While maximised the bounds are the monitor's: the size it comes back
        // down to is the one it had before.
        var (x, y, w, h) = maximised && previous != null
            ? (previous.X, previous.Y, previous.Width, previous.Height)
            : (window.Position.X, window.Position.Y, window.Size.Width, window.Size.Height);
        defaults.Set(Key, JsonSerializer.Serialize(new Kept(x, y, w, h, maximised)));
    }

    private static Kept? Read(IDefaults defaults)
    {
        try
        {
            return defaults.String(Key) is { } text ? JsonSerializer.Deserialize<Kept>(text) : null;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    /// <summary>Whether a window kept there would show on a monitor still attached: a monitor unplugged since leaves it nowhere.</summary>
    private static bool OnSomeDisplay(Kept kept)
    {
        if (kept.Width < 200 || kept.Height < 200) return false;
        try
        {
            // By index: the list FindAll hands back cannot be enumerated
            // from C# (an InvalidCastException out of the projection's
            // enumerator), which ended the app at its second start, the
            // first with a place to come back to.
            var displays = DisplayArea.FindAll();
            for (var i = 0; i < displays.Count; i++)
            {
                var area = displays[i].WorkArea;
                var visibleX = Math.Min(kept.X + kept.Width, area.X + area.Width) - Math.Max(kept.X, area.X);
                var visibleY = Math.Min(kept.Y + kept.Height, area.Y + area.Height) - Math.Max(kept.Y, area.Y);
                if (visibleX >= 120 && visibleY >= 80) return true;
            }
            return false;
        }
        catch (Exception)
        {
            // Opened in the middle of the screen instead.
            return false;
        }
    }
}
