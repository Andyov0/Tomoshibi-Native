namespace Tomoshibi.Basics;

/// <summary>
/// The arithmetic of blurring what is behind somebody, on a camera frame in
/// three packed planes (I420), apart from finding the person (the app's
/// BackgroundBlur): pure, so a test can hold it to its numbers. Android's
/// BlurMath.kt, line for line, so the two blur alike.
///
/// The web's pipeline, step for step (livekit track-processors,
/// webgl/index.ts, which the web's blur.ts runs): the frame is taken down to a
/// quarter of its size, blurred there with a Gaussian of a quarter of the
/// radius, and laid under the frame through the person's mask, softened at its
/// edge so that the outline of hair does not step. The radius is the Apple
/// apps' rule (BackgroundBlur.swift): the web's 12 at a picture 720 rows high,
/// in proportion below that.
///
/// A quarter is also what keeps it cheap: the blur itself runs over a
/// sixteenth of the pixels, and the full-size work is one blend per pixel.
/// </summary>
public sealed class BlurMath
{
    /// <summary>The web's radius, at a picture 720 rows high (blur.ts <c>RADIUS</c>).</summary>
    public const double SigmaAt720 = 12.0;

    public int Width { get; }
    public int Height { get; }

    /// <summary>The quarter-size grid the blur and the mask live on.</summary>
    public int GridWidth { get; }
    public int GridHeight { get; }

    private readonly float[] _lowY;
    private readonly float[] _lowU;
    private readonly float[] _lowV;
    private readonly float[] _scratch;
    private readonly float[] _kernel;

    // Where each full-size column and row falls on the grid, worked out once:
    // the grid cell before it and how far towards the next.
    private readonly Taps _lumaX;
    private readonly Taps _lumaY;
    private readonly Taps _chromaX;
    private readonly Taps _chromaY;

    public BlurMath(int width, int height)
    {
        Width = width;
        Height = height;
        GridWidth = Math.Max(1, (int)Math.Ceiling(width / 4.0));
        GridHeight = Math.Max(1, (int)Math.Ceiling(height / 4.0));
        var cells = GridWidth * GridHeight;
        _lowY = new float[cells];
        _lowU = new float[cells];
        _lowV = new float[cells];
        _scratch = new float[cells];
        _kernel = Gaussian(SigmaAt720 * height / 720.0 / 4.0);
        _lumaX = new Taps(width, 4, GridWidth);
        _lumaY = new Taps(height, 4, GridHeight);
        _chromaX = new Taps((width + 1) / 2, 2, GridWidth);
        _chromaY = new Taps((height + 1) / 2, 2, GridHeight);
    }

    /// <summary>
    /// Blurs the background of the packed planes in place: <paramref name="y"/>
    /// of Width by Height, <paramref name="u"/> and <paramref name="v"/> at
    /// half that each way. <paramref name="person"/> is the grid's share of
    /// person in each cell, 0 to 1, or null where nobody has been found yet,
    /// in which case the whole frame is blurred: somebody who asked for their
    /// room to be hidden is shown nothing of it while the segmenter is still
    /// starting.
    /// </summary>
    public void Blur(byte[] y, byte[] u, byte[] v, float[]? person)
    {
        var cw = (Width + 1) / 2;
        var ch = (Height + 1) / 2;
        Shrink(y, Width, Height, 4, _lowY);
        Shrink(u, cw, ch, 2, _lowU);
        Shrink(v, cw, ch, 2, _lowV);
        Smooth(_lowY);
        Smooth(_lowU);
        Smooth(_lowV);
        Blend(y, Width, Height, _lumaX, _lumaY, _lowY, person);
        Blend(u, cw, ch, _chromaX, _chromaY, _lowU, person);
        Blend(v, cw, ch, _chromaX, _chromaY, _lowV, person);
    }

    /// <summary>A plane averaged down onto the grid, <paramref name="factor"/> of its pixels to a cell each way.</summary>
    private void Shrink(byte[] plane, int planeWidth, int planeHeight, int factor, float[] output)
    {
        Array.Clear(output);
        var counts = _scratch;
        Array.Clear(counts);
        for (var row = 0; row < planeHeight; row++)
        {
            var cell = Math.Min(row / factor, GridHeight - 1) * GridWidth;
            var at = row * planeWidth;
            for (var column = 0; column < planeWidth; column++)
            {
                var index = cell + Math.Min(column / factor, GridWidth - 1);
                output[index] += plane[at + column];
                counts[index] += 1f;
            }
        }
        for (var i = 0; i < output.Length; i++)
        {
            if (counts[i] > 0f) output[i] /= counts[i];
        }
    }

    /// <summary>The Gaussian on the grid, across and then down, with the edges held.</summary>
    private void Smooth(float[] plane)
    {
        var radius = _kernel.Length / 2;
        if (radius == 0) return;
        var gw = GridWidth;
        var gh = GridHeight;
        for (var row = 0; row < gh; row++)
        {
            var at = row * gw;
            for (var column = 0; column < gw; column++)
            {
                var sum = 0f;
                for (var k = -radius; k <= radius; k++) sum += _kernel[k + radius] * plane[at + Math.Clamp(column + k, 0, gw - 1)];
                _scratch[at + column] = sum;
            }
        }
        for (var row = 0; row < gh; row++)
        {
            for (var column = 0; column < gw; column++)
            {
                var sum = 0f;
                for (var k = -radius; k <= radius; k++) sum += _kernel[k + radius] * _scratch[Math.Clamp(row + k, 0, gh - 1) * gw + column];
                plane[row * gw + column] = sum;
            }
        }
    }

    /// <summary>
    /// Each pixel of a plane between itself and the blurred grid under it, by
    /// how much of a person the mask says is there: kept where it is all
    /// person, the blur where it is none.
    /// </summary>
    private void Blend(byte[] plane, int planeWidth, int planeHeight, Taps xs, Taps ys, float[] low, float[]? person)
    {
        var gw = GridWidth;
        for (var row = 0; row < planeHeight; row++)
        {
            var r0 = ys.Cell[row] * gw;
            var r1 = ys.Next[row] * gw;
            var fy = ys.Toward[row];
            var at = row * planeWidth;
            for (var column = 0; column < planeWidth; column++)
            {
                var c0 = xs.Cell[column];
                var c1 = xs.Next[column];
                var fx = xs.Toward[column];
                var alpha = person == null ? 0f : Bilinear(person, r0, r1, c0, c1, fx, fy);
                if (alpha >= 0.996f) continue;
                var behind = Bilinear(low, r0, r1, c0, c1, fx, fy);
                float pixel = plane[at + column];
                plane[at + column] = (byte)Math.Clamp((int)MathF.Round(behind + alpha * (pixel - behind), MidpointRounding.AwayFromZero), 0, 255);
            }
        }
    }

    private static float Bilinear(float[] grid, int r0, int r1, int c0, int c1, float fx, float fy)
    {
        var top = grid[r0 + c0] + (grid[r0 + c1] - grid[r0 + c0]) * fx;
        var bottom = grid[r1 + c0] + (grid[r1 + c1] - grid[r1 + c0]) * fx;
        return top + (bottom - top) * fy;
    }

    /// <summary>For each pixel along one side of a plane <paramref name="factor"/> times the grid's <paramref name="cells"/>: its cell, the next one, and how far between.</summary>
    private sealed class Taps
    {
        public readonly int[] Cell;
        public readonly int[] Next;
        public readonly float[] Toward;

        public Taps(int count, int factor, int cells)
        {
            Cell = new int[count];
            Next = new int[count];
            Toward = new float[count];
            for (var i = 0; i < count; i++)
            {
                var position = Math.Clamp((i + 0.5f) / factor - 0.5f, 0f, cells - 1);
                var at = (int)position;
                Cell[i] = at;
                Next[i] = Math.Min(at + 1, cells - 1);
                Toward[i] = position - at;
            }
        }
    }

    /// <summary>A normalised Gaussian of <paramref name="sigma"/> cells, out to three sigma.</summary>
    public static float[] Gaussian(double sigma)
    {
        if (sigma < 0.3) return [1f];
        var radius = (int)Math.Ceiling(sigma * 3);
        var output = new float[radius * 2 + 1];
        var sum = 0.0;
        var weights = new double[output.Length];
        for (var i = -radius; i <= radius; i++)
        {
            var w = Math.Exp(-(i * i) / (2 * sigma * sigma));
            weights[i + radius] = w;
            sum += w;
        }
        for (var i = 0; i < output.Length; i++) output[i] = (float)(weights[i] / sum);
        return output;
    }

    /// <summary>
    /// Where a pixel of the frame as captured (<paramref name="x"/>,
    /// <paramref name="y"/> of <paramref name="width"/> by
    /// <paramref name="height"/>) is in the picture turned upright by
    /// <paramref name="rotation"/> clockwise degrees, which is how the
    /// segmenter sees it: a person on their side is a person it does not know.
    /// </summary>
    public static (int X, int Y) Upright(int x, int y, int width, int height, int rotation) => rotation switch
    {
        90 => (height - 1 - y, x),
        180 => (width - 1 - x, height - 1 - y),
        270 => (y, width - 1 - x),
        _ => (x, y),
    };

    /// <summary>And back: where an upright picture's pixel came from in the frame.</summary>
    public static (int X, int Y) Captured(int ux, int uy, int width, int height, int rotation) => rotation switch
    {
        90 => (uy, height - 1 - ux),
        180 => (width - 1 - ux, height - 1 - uy),
        270 => (width - 1 - uy, ux),
        _ => (ux, uy),
    };

    /// <summary>
    /// The person's mask, upright at <paramref name="maskWidth"/> by
    /// <paramref name="maskHeight"/>, laid on the grid of a frame of
    /// <paramref name="width"/> by <paramref name="height"/> captured at
    /// <paramref name="rotation"/>, and its edge softened by a few cells (the
    /// web blurs its mask by eight pixels at full size, <c>maskBlurRadius</c>,
    /// which is two here).
    /// </summary>
    public static float[] PersonOnGrid(float[] mask, int maskWidth, int maskHeight, int width, int height, int rotation, int gridWidth, int gridHeight)
    {
        var turned = rotation is 90 or 270;
        var uprightWidth = turned ? height : width;
        var uprightHeight = turned ? width : height;
        var output = new float[gridWidth * gridHeight];
        for (var gy = 0; gy < gridHeight; gy++)
        {
            var y = Math.Min(gy * 4 + 2, height - 1);
            for (var gx = 0; gx < gridWidth; gx++)
            {
                var x = Math.Min(gx * 4 + 2, width - 1);
                var (ux, uy) = Upright(x, y, width, height, rotation);
                var mx = Math.Clamp(ux * maskWidth / uprightWidth, 0, maskWidth - 1);
                var my = Math.Clamp(uy * maskHeight / uprightHeight, 0, maskHeight - 1);
                output[gy * gridWidth + gx] = Math.Clamp(mask[my * maskWidth + mx], 0f, 1f);
            }
        }
        // A box of five cells, twice, across and down: a soft edge of about
        // two cells either side.
        var scratch = new float[output.Length];
        for (var pass = 0; pass < 2; pass++)
        {
            Box(output, scratch, gridWidth, gridHeight, across: true);
            Box(scratch, output, gridWidth, gridHeight, across: false);
        }
        return output;
    }

    /// <summary>
    /// The frame upright and stretched to the model's square of
    /// <paramref name="side"/>, as RGB floats from 0 to 1, which is the
    /// selfie model's input: MediaPipe's own run of it gives the same mask for
    /// the same floats, and stretches a picture that is not square rather than
    /// bordering it (windows/Blur/convert.py measured both). Sampled at the
    /// middle of each cell rather than averaged: the model sees a picture 256
    /// across, and a person at that size is the same either way.
    /// </summary>
    public static void ModelInput(byte[] y, byte[] u, byte[] v, int width, int height, int rotation, int side, float[] output)
    {
        var turned = rotation is 90 or 270;
        var uw = turned ? height : width;
        var uh = turned ? width : height;
        var cw = (width + 1) / 2;
        var at = 0;
        for (var sy = 0; sy < side; sy++)
        {
            var uy = Math.Min(uh - 1, (int)((sy + 0.5) * uh / side));
            for (var sx = 0; sx < side; sx++)
            {
                var ux = Math.Min(uw - 1, (int)((sx + 0.5) * uw / side));
                var (x, yy) = Captured(ux, uy, width, height, rotation);
                var (r, g, b) = Planes.Rgb(y[yy * width + x], u[yy / 2 * cw + x / 2], v[yy / 2 * cw + x / 2]);
                output[at++] = r / 255f;
                output[at++] = g / 255f;
                output[at++] = b / 255f;
            }
        }
    }

    private static void Box(float[] from, float[] to, int w, int h, bool across)
    {
        for (var row = 0; row < h; row++)
        {
            for (var column = 0; column < w; column++)
            {
                var sum = 0f;
                for (var k = -2; k <= 2; k++)
                {
                    var r = across ? row : Math.Clamp(row + k, 0, h - 1);
                    var c = across ? Math.Clamp(column + k, 0, w - 1) : column;
                    sum += from[r * w + c];
                }
                to[row * w + column] = sum / 5f;
            }
        }
    }
}
