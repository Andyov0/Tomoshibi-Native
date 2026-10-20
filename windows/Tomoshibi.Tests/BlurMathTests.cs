using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The arithmetic of the background blur (Android's BlurMathTest, case for
/// case, and what Windows adds: the model's input). What these guard is a
/// person blurred along with their room, a room left sharp, a mask laid on
/// the frame turned the wrong way, the radius not following the picture's
/// size, and the model handed colours in a range or an order it was not
/// trained on, which finds nobody and blurs everyone.
/// </summary>
public class BlurMathTests
{
    private const int Width = 64;
    private const int Height = 36;

    /// <summary>A frame of fine vertical stripes in luma, and flat chroma.</summary>
    private static (byte[] Y, byte[] U, byte[] V) Stripes()
    {
        var y = new byte[Width * Height];
        for (var i = 0; i < y.Length; i++) y[i] = (byte)(i % Width % 2 == 0 ? 40 : 200);
        var u = Enumerable.Repeat((byte)100, Width / 2 * (Height / 2)).ToArray();
        var v = Enumerable.Repeat((byte)150, Width / 2 * (Height / 2)).ToArray();
        return (y, u, v);
    }

    private static int Contrast(byte[] y, int from, int until)
    {
        var most = 0;
        for (var row = 0; row < Height; row++)
        {
            for (var column = from; column < until - 1; column++)
            {
                most = Math.Max(most, Math.Abs(y[row * Width + column] - y[row * Width + column + 1]));
            }
        }
        return most;
    }

    [Fact]
    public void WhereThereIsAllPersonTheFrameIsKeptAsItIs()
    {
        var (y, u, v) = Stripes();
        var before = (byte[])y.Clone();
        var math = new BlurMath(Width, Height);
        math.Blur(y, u, v, Enumerable.Repeat(1f, math.GridWidth * math.GridHeight).ToArray());
        Assert.Equal(before, y);
    }

    [Fact]
    public void WithNobodyFoundYetTheWholeFrameIsBlurred()
    {
        var (y, u, v) = Stripes();
        new BlurMath(Width, Height).Blur(y, u, v, null);
        Assert.True(Contrast(y, 0, Width) < 20, $"stripes 160 apart are smoothed to {Contrast(y, 0, Width)}");
        // Flat chroma stays as it was.
        Assert.All(u, value => Assert.InRange(value, 99, 101));
    }

    [Fact]
    public void TheBackgroundIsBlurredAndThePersonKept()
    {
        var (y, u, v) = Stripes();
        var math = new BlurMath(Width, Height);
        // The person on the left half of the grid.
        var person = new float[math.GridWidth * math.GridHeight];
        for (var i = 0; i < person.Length; i++) person[i] = i % math.GridWidth < math.GridWidth / 2 ? 1f : 0f;
        math.Blur(y, u, v, person);
        Assert.Equal(160, Contrast(y, 0, Width / 2 - 8));
        Assert.True(Contrast(y, Width / 2 + 8, Width) < 20, "the room's side is smoothed");
    }

    [Fact]
    public void TheRadiusFollowsTheSize()
    {
        // 12 at 720 rows, a quarter of it on the quarter grid: 3 cells at
        // 720, 1.5 at 360, out to three sigma.
        Assert.Equal(19, BlurMath.Gaussian(12.0 * 720 / 720 / 4).Length);
        Assert.Equal(11, BlurMath.Gaussian(12.0 * 360 / 720 / 4).Length);
        Assert.Equal(1f, BlurMath.Gaussian(1.5).Sum(), 5);
    }

    [Fact]
    public void TurningUprightAndBackComesBackToTheSamePixel()
    {
        foreach (var rotation in new[] { 0, 90, 180, 270 })
        {
            foreach (var (x, y) in new[] { (0, 0), (63, 0), (0, 35), (17, 9) })
            {
                var (ux, uy) = BlurMath.Upright(x, y, Width, Height, rotation);
                Assert.Equal((x, y), BlurMath.Captured(ux, uy, Width, Height, rotation));
            }
        }
        // A camera turned 90 degrees: the frame's top left is the upright
        // picture's top right.
        Assert.Equal((Height - 1, 0), BlurMath.Upright(0, 0, Width, Height, 90));
    }

    [Fact]
    public void AnUprightMaskIsLaidOnTheFrameItWasFoundIn()
    {
        // Upright, the person fills the top half of the picture.
        const int turned = 90;
        const int uw = Height;
        const int uh = Width;
        var mask = new float[uw * uh];
        for (var i = 0; i < mask.Length; i++) mask[i] = i / uw < uh / 2 ? 1f : 0f;
        var math = new BlurMath(Width, Height);
        var grid = BlurMath.PersonOnGrid(mask, uw, uh, Width, Height, turned, math.GridWidth, math.GridHeight);
        // The upright top is the captured frame's left, for a turn of 90.
        var left = grid[math.GridHeight / 2 * math.GridWidth + 1];
        var right = grid[math.GridHeight / 2 * math.GridWidth + math.GridWidth - 2];
        Assert.True(left > 0.9f, $"left is person ({left})");
        Assert.True(right < 0.1f, $"right is room ({right})");
    }

    /// <summary>A BGRA picture whose left half is one colour and right half another.</summary>
    private static byte[] Halves(int width, int height, (byte R, byte G, byte B) left, (byte R, byte G, byte B) right)
    {
        var bgra = new byte[width * height * 4];
        for (var row = 0; row < height; row++)
        {
            for (var column = 0; column < width; column++)
            {
                var (r, g, b) = column < width / 2 ? left : right;
                var at = (row * width + column) * 4;
                bgra[at] = b;
                bgra[at + 1] = g;
                bgra[at + 2] = r;
                bgra[at + 3] = 255;
            }
        }
        return bgra;
    }

    [Fact]
    public void TheModelIsHandedRedGreenBlueFromNoughtToOne()
    {
        // Red on the left, blue on the right, stretched to a square of 8.
        var (y, u, v) = Planes.FromBgra(Halves(Width, Height, (230, 20, 20), (20, 20, 230)), Width, Height);
        var input = new float[8 * 8 * 3];
        BlurMath.ModelInput(y, u, v, Width, Height, 0, 8, input);
        // The first pixel is red, in [0, 1], in the order R, G, B.
        Assert.InRange(input[0], 0.85f, 0.95f);
        Assert.InRange(input[1], 0.03f, 0.13f);
        Assert.InRange(input[2], 0.03f, 0.13f);
        // The last is blue.
        var last = input.Length - 3;
        Assert.InRange(input[last], 0.03f, 0.13f);
        Assert.InRange(input[last + 2], 0.85f, 0.95f);
        Assert.All(input, value => Assert.InRange(value, 0f, 1f));
    }

    [Fact]
    public void TheModelSeesTheFrameUpright()
    {
        // Captured turned 90 degrees clockwise: the frame's left half is the
        // upright picture's top.
        var (y, u, v) = Planes.FromBgra(Halves(Width, Height, (230, 20, 20), (20, 20, 230)), Width, Height);
        var input = new float[8 * 8 * 3];
        BlurMath.ModelInput(y, u, v, Width, Height, 90, 8, input);
        Assert.True(input[0] > 0.8f, "the upright top is red");
        Assert.True(input[(7 * 8) * 3 + 2] > 0.8f, "the upright bottom is blue");
    }

    [Fact]
    public void PlanesComeBackAsTheColoursTheyWereMadeFrom()
    {
        var bgra = Halves(16, 8, (200, 120, 40), (30, 90, 210));
        var (y, u, v) = Planes.FromBgra(bgra, 16, 8);
        var back = new byte[16 * 8 * 4];
        Planes.ToBgra(y, u, v, 16, 8, 1, back);
        for (var i = 0; i < back.Length; i++) Assert.InRange(back[i] - bgra[i], -3, 3);
    }
}
