namespace Tomoshibi.Basics;

/// <summary>
/// A length across and down, in device-independent pixels (the points of
/// the other apps). Single precision, as Compose's Size is on Android, so
/// that the planner's arithmetic rounds where Android's does and the three
/// platforms put a picture on the same pixel.
/// </summary>
public readonly record struct Size(float Width, float Height)
{
    public static readonly Size Zero = new(0, 0);

    public override string ToString() => $"{Width}x{Height}";
}

/// <summary>A place on the screen: its left and top edges and its size.</summary>
public readonly record struct Rect(float Left, float Top, float Width, float Height)
{
    public float Right => Left + Width;
    public float Bottom => Top + Height;

    /// <summary>The whole of a container, from its corner.</summary>
    public static Rect Of(Size size) => new(0, 0, size.Width, size.Height);

    public override string ToString() => $"({Left}, {Top}, {Width}x{Height})";
}
