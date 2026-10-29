using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// A name at the foot of a picture raised above the controls where they
/// would cover it (RoomView.kt <c>underBar</c>): the other person filling a
/// call of two had their name drawn under the bar in a window a little over
/// 900 across, as the iPhone had it.
/// </summary>
public class NameUnderBarTests
{
    /// <summary>A window this wide and 800 tall, its bar of this length in the middle 20 above the foot, and the other person of a call of two filling it.</summary>
    private static (Rect Spot, float BarLeft, float BarRight, float BarBottom) CallOfTwo(float width, float bar)
    {
        var spot = new Rect(0, 0, width, 800);
        return (spot, (width - bar) / 2, (width + bar) / 2, 800 - 20);
    }

    [Fact]
    public void ALongNameInACallOfTwoAWindowJustOver900AcrossIsUnderTheBar()
    {
        // The full bar with every button is 597 long; the plate of a name of
        // twenty letters is about 150 across, 8 in from the left.
        var (spot, left, right, bottom) = CallOfTwo(920, 597);
        Assert.True(Plan.NameUnderBar(spot, 150, 8, left, right, bottom));
        // Raised in a window narrower than 900, as it was.
        var (narrow, l2, r2, b2) = CallOfTwo(800, 597);
        Assert.True(Plan.NameUnderBar(narrow, 60, 8, l2, r2, b2));
    }

    [Fact]
    public void ANameClearOfTheBarStaysWhereItIs()
    {
        var (spot, left, right, bottom) = CallOfTwo(1280, 597);
        Assert.False(Plan.NameUnderBar(spot, 150, 8, left, right, bottom));
        // Nor where there is no name.
        Assert.False(Plan.NameUnderBar(spot, 0, 8, 0, 1280, bottom));
    }

    [Fact]
    public void OnlyAPictureWhoseFootIsInTheBarsBandIsLifted()
    {
        var bottom = 800f - 20;
        // A picture in the top row of a grid, its foot far above the bar.
        Assert.False(Plan.NameUnderBar(new Rect(400, 40, 300, 300), 120, 8, 300, 900, bottom));
        // One in the bottom row whose foot runs into the bar's band.
        Assert.True(Plan.NameUnderBar(new Rect(400, 420, 300, 300), 120, 8, 300, 900, bottom));
        // One in the bottom row to the side of the bar, its plate clear of the capsule.
        Assert.False(Plan.NameUnderBar(new Rect(8, 420, 260, 300), 120, 8, 300, 900, bottom));
    }
}
