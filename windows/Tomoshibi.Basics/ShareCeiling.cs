namespace Tomoshibi.Basics;

/// <summary>
/// The deployment's ceiling over a screen share (Remote's params, the core's
/// api/remote.rs), as Android's Sharing.kt and the Apple apps'
/// ShareCeiling.swift read it. Ceilings and nothing else: what is published
/// keeps the web's track names, sizes and simulcast ladders (CLAUDE.md), and
/// only goes out at a lower rate or a smaller picture than the person chose.
///
/// The deployment's screen_share_hot_fps is for a device that tells the app
/// it is hot; Windows tells an app nothing it can rely on about that, so the
/// Windows app reads no such ceiling.
/// </summary>
/// <param name="Fps">Frames a second at most (screen_share_max_fps).</param>
/// <param name="Height">The captured picture's height at most, in pixels, its width brought down with it (screen_share_max_height).</param>
public sealed record ShareCeiling(int? Fps = null, int? Height = null)
{
    public static readonly ShareCeiling None = new();

    /// <summary>
    /// From the deployment's numbers. A number that is not a positive count
    /// (nought, which the deployment writes for "do not lower", a negative, a
    /// fraction under one) is no ceiling: a share is never asked for no
    /// frames at all because a field was left at zero.
    /// </summary>
    public static ShareCeiling Of(IReadOnlyDictionary<string, double> numbers) =>
        new(Count(numbers, "screen_share_max_fps"), Count(numbers, "screen_share_max_height"));

    private static int? Count(IReadOnlyDictionary<string, double> numbers, string name)
    {
        if (!numbers.TryGetValue(name, out var number) || !double.IsFinite(number) || number < 1) return null;
        return (int)Math.Min(number, 1_000_000);
    }

    /// <summary>
    /// The rate a share is published at, for a rate chosen at a size (the
    /// share menu's shorter side, nought for automatic): the size's own clamp
    /// first (ShareRates), then the highest of its rates under the ceiling,
    /// or its lowest where every one is over it.
    ///
    /// One of the size's rates and never another number: the core takes a
    /// rate a size does not offer for its highest (share.rs
    /// <c>share_profile</c>), so a ceiling of 24 handed on as it was would
    /// have published 1080p at 240. Automatic has the one rate, and keeps it;
    /// its frames are held to the ceiling at the capture (<see cref="Captured"/>).
    /// </summary>
    public int Published(int wanted, int shorterSide)
    {
        var chosen = ShareRates.Clamp(wanted, shorterSide);
        if (Fps is not { } fps || fps >= chosen) return chosen;
        var rates = ShareRates.Allowed(shorterSide);
        var under = rates.Where(rate => rate <= fps).ToArray();
        return under.Length > 0 ? under.Max() : rates.Min();
    }

    /// <summary>The frames a second a share published at <paramref name="published"/> is captured at: no more than that, and no more than the ceiling.</summary>
    public int Captured(int published) => Math.Max(1, Fps is { } fps ? Math.Min(published, fps) : published);

    /// <summary>A capture's size under the height, its width scaled with it and both even, as the encoder needs them (ScreenSource.Fit).</summary>
    public (int Width, int Height) Size(int width, int height)
    {
        if (Height is not { } ceiling || height <= ceiling) return (width, height);
        var scaled = (int)((double)width * ceiling / height) & ~1;
        return (Math.Max(2, scaled), Math.Max(2, ceiling & ~1));
    }
}
