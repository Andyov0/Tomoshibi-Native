using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * When a camera fills its tile and when it is shown whole; Android's
 * FramingTest and the Apple apps' FramingTests, case for case.
 */
public class FramingTests
{
    private static bool Fills((int W, int H) picture, (int W, int H) view) =>
        Framing.Fills(picture.W, picture.H, view.W, view.H);

    [Fact]
    public void ACameraOnItsSideIsShownWholeOnAnUprightScreen()
    {
        Assert.False(Fills((640, 480), (300, 400)));
        // A Fold6 opened, and its cover screen.
        Assert.False(Fills((640, 480), (1856, 2160)));
        Assert.False(Fills((640, 480), (968, 2376)));
        // A telephone held upright, in a window of a computer.
        Assert.False(Fills((720, 960), (1440, 900)));
    }

    [Fact]
    public void ACameraTheSameWayRoundFills()
    {
        Assert.True(Fills((720, 960), (1856, 2160)));
        Assert.True(Fills((720, 960), (968, 2376)));
        Assert.True(Fills((720, 960), (1080, 2340)));
        Assert.True(Fills((720, 1280), (968, 2376)));
        Assert.True(Fills((1280, 720), (1440, 900)));
    }

    [Fact]
    public void ANearShapeFillsWhicheverWayRound()
    {
        Assert.True(Fills((480, 480), (300, 400)));
        Assert.True(Fills((640, 480), (1600, 900)));
    }

    [Fact]
    public void ASquareIsNeitherWayRound()
    {
        Assert.False(Fills((1280, 720), (500, 500)));
    }

    [Fact]
    public void TooLongEvenTheSameWayRoundIsShownWhole()
    {
        Assert.False(Fills((720, 1280), (400, 1600)));
    }

    [Fact]
    public void AShapeNotYetKnownFills()
    {
        Assert.True(Fills((0, 0), (300, 400)));
        Assert.True(Fills((640, 480), (0, 0)));
    }

    // What the Windows tile adds: the shape the core reports is the
    // decoded one and its turn, and the change between the two framings is
    // animated through the ratio of their scales.

    [Fact]
    public void AQuarterTurnSwapsTheSides()
    {
        Assert.Equal((480f, 640f), Framing.Shown(640, 480, 90));
        Assert.Equal((480f, 640f), Framing.Shown(640, 480, 270));
        Assert.Equal((640f, 480f), Framing.Shown(640, 480, 180));
        Assert.Equal((640f, 480f), Framing.Shown(640, 480, 0));
    }

    [Fact]
    public void FillingIsLargerThanWholeByTheRatioOfTheShapes()
    {
        // A wide picture in a square: whole is half the height, filling twice that.
        Assert.Equal(16f / 9f, Framing.FillOverWhole(1600, 900, 400, 400), 4);
        Assert.Equal(1f, Framing.FillOverWhole(1600, 900, 1600, 900), 4);
        Assert.Equal(1f, Framing.FillOverWhole(0, 0, 400, 400), 4);
    }
}
