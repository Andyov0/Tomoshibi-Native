namespace Tomoshibi.Basics;

/// <summary>
/// A picture in three packed planes (I420: luma at full size, the two chroma
/// at half each way), to and from BGRA rows, with BT.601 at full range, which
/// is what cameras deliver and what the core is handed. For the frames the
/// app changes itself (the background blur, the generated camera's picture)
/// and the picture of yourself drawn from them.
/// </summary>
public static class Planes
{
    /// <summary>One pixel's colour from its luma and chroma.</summary>
    public static (byte R, byte G, byte B) Rgb(byte y, byte u, byte v)
    {
        float luma = y;
        var cb = u - 128f;
        var cr = v - 128f;
        return (Clamp(luma + 1.402f * cr), Clamp(luma - 0.344136f * cb - 0.714136f * cr), Clamp(luma + 1.772f * cb));
    }

    private static byte Clamp(float value) => (byte)Math.Clamp((int)MathF.Round(value, MidpointRounding.AwayFromZero), 0, 255);

    /// <summary>
    /// The planes of a BGRA picture <paramref name="width"/> by
    /// <paramref name="height"/> (both even), its chroma the average of each
    /// two by two.
    /// </summary>
    public static (byte[] Y, byte[] U, byte[] V) FromBgra(byte[] bgra, int width, int height)
    {
        var y = new byte[width * height];
        var cw = width / 2;
        var u = new byte[cw * (height / 2)];
        var v = new byte[cw * (height / 2)];
        for (var row = 0; row < height; row++)
        {
            for (var column = 0; column < width; column++)
            {
                var at = (row * width + column) * 4;
                float b = bgra[at], g = bgra[at + 1], r = bgra[at + 2];
                y[row * width + column] = Clamp(0.299f * r + 0.587f * g + 0.114f * b);
            }
        }
        for (var row = 0; row < height / 2; row++)
        {
            for (var column = 0; column < cw; column++)
            {
                float r = 0, g = 0, b = 0;
                for (var dy = 0; dy < 2; dy++)
                {
                    for (var dx = 0; dx < 2; dx++)
                    {
                        var at = ((row * 2 + dy) * width + column * 2 + dx) * 4;
                        b += bgra[at];
                        g += bgra[at + 1];
                        r += bgra[at + 2];
                    }
                }
                r /= 4;
                g /= 4;
                b /= 4;
                u[row * cw + column] = Clamp(128 - 0.168736f * r - 0.331264f * g + 0.5f * b);
                v[row * cw + column] = Clamp(128 + 0.5f * r - 0.418688f * g - 0.081312f * b);
            }
        }
        return (y, u, v);
    }

    /// <summary>
    /// BGRA rows of the planes, one pixel for every <paramref name="step"/>
    /// each way (a picture of yourself needs less than is sent), into
    /// <paramref name="output"/>, which is (width / step) by (height / step).
    /// </summary>
    public static void ToBgra(byte[] y, byte[] u, byte[] v, int width, int height, int step, byte[] output)
    {
        var cw = (width + 1) / 2;
        var ow = width / step;
        var oh = height / step;
        for (var row = 0; row < oh; row++)
        {
            var sy = row * step;
            for (var column = 0; column < ow; column++)
            {
                var sx = column * step;
                var c = sy / 2 * cw + sx / 2;
                var (r, g, b) = Rgb(y[sy * width + sx], u[c], v[c]);
                var at = (row * ow + column) * 4;
                output[at] = b;
                output[at + 1] = g;
                output[at + 2] = r;
                output[at + 3] = 255;
            }
        }
    }
}
