namespace Tomoshibi.Basics;

/// <summary>
/// Whether a camera's picture fills its tile, cut to the tile's shape, or is
/// shown whole inside it (TomoshibiBasics/Framing.swift and Android's
/// basics/Framing.kt, case for case).
///
/// A face is fine cut at the edges, so a picture of nearly the tile's shape
/// fills it, and so does one that is only longer the same way round: a
/// telephone held upright, shown in a window held upright, loses a little of
/// each side and nobody misses it. A picture the other way round is another
/// matter: a camera held on its side, filling an upright place, kept little
/// more than its middle. Those are shown whole, the rest of the tile given
/// to a blurred copy of the picture.
/// </summary>
public static class Framing
{
    /// <summary>As close as this, a fill loses so little that it fills whichever way round the two are.</summary>
    public const double CloseEnough = 0.75;

    /// <summary>The same way round, a fill keeps at least this much of the picture before it is shown whole.</summary>
    public const double SameWay = 0.5;

    /// <summary>The picture as it is shown, that is turned, and the tile, each as width and height.</summary>
    public static bool Fills(float pictureWidth, float pictureHeight, float viewWidth, float viewHeight)
    {
        if (pictureWidth <= 0f || pictureHeight <= 0f || viewWidth <= 0f || viewHeight <= 0f) return true;
        // In doubles, as the other apps work it out, so that a shape on the
        // line (a square picture in a three-by-four tile keeps exactly 0.75)
        // comes out the same on all of them.
        var picture = (double)pictureWidth / pictureHeight;
        var view = (double)viewWidth / viewHeight;
        // The share of the picture a fill keeps along the side it cuts.
        var kept = Math.Min(picture, view) / Math.Max(picture, view);
        if (kept >= CloseEnough) return true;
        // A square is neither way round.
        var sameWay = (picture > 1.0 && view > 1.0) || (picture < 1.0 && view < 1.0);
        return sameWay && kept >= SameWay;
    }

    /// <summary>
    /// The picture's shape as it is shown: a quarter turn swaps its sides
    /// (the core turns a phone's camera upright before drawing it).
    /// </summary>
    public static (float Width, float Height) Shown(uint width, uint height, uint rotation) =>
        rotation is 90 or 270 ? (height, width) : (width, height);

    /// <summary>
    /// How much larger the picture is drawn filling a place than shown whole
    /// in it: the scale a change between the two animates through, so that
    /// the picture is seen to settle into its new framing rather than jump.
    /// One when either shape is not known yet.
    /// </summary>
    public static float FillOverWhole(float pictureWidth, float pictureHeight, float viewWidth, float viewHeight)
    {
        if (pictureWidth <= 0f || pictureHeight <= 0f || viewWidth <= 0f || viewHeight <= 0f) return 1f;
        var fill = MathF.Max(viewWidth / pictureWidth, viewHeight / pictureHeight);
        var whole = MathF.Min(viewWidth / pictureWidth, viewHeight / pictureHeight);
        return fill / whole;
    }
}
