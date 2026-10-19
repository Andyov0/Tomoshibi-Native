using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * The web's cases for finding addresses in a message (Linked.test.tsx), and
 * the corner's rules: how long notices stay, which one a fourth pushes out,
 * where a pile of them sits, and the chat box's limit; Android's CornerTest
 * and the Apple apps' CornerTests, case for case.
 */
public class CornerTests
{
    [Fact]
    public void FindsACompleteAddress()
    {
        Assert.Equal(new[] { "https://example.com/a?b=1" }, Linked.Links("see https://example.com/a?b=1"));
        Assert.Equal(new[] { "http://a.test", "https://b.test" }, Linked.Links("http://a.test and https://b.test"));
    }

    // A false link is worse than a missed one: it is clickable, it goes
    // somewhere, and it looks deliberate.
    [Fact]
    public void DoesNotGuessAtAnythingThatIsNotOne()
    {
        Assert.Empty(Linked.Links("see example.com or www.example.com"));
        Assert.Empty(Linked.Links("port 4443 on the box"));
        Assert.Empty(Linked.Links("ftp://files.test"));
        Assert.Empty(Linked.Links("xhttps://example.com"));
    }

    [Fact]
    public void LeavesTrailingPunctuationOutOfTheAddress()
    {
        Assert.Equal(new[] { "https://example.com/docs" }, Linked.Links("it is at https://example.com/docs."));
        Assert.Equal(new[] { "https://example.com" }, Linked.Links("(https://example.com)"));
        Assert.Equal(new[] { "https://example.com" }, Linked.Links("https://example.com, then"));
        // JavaScript's \s, not .NET's: an ideographic space ends the address.
        Assert.Equal(new[] { "https://example.com/x" }, Linked.Links("https://example.com/x　次"));
        Assert.Equal(new[] { "https://example.com/x" }, Linked.Links("https://example.com/x　好的"));
        // And no wider than JavaScript's: NEL is not a space there, so it
        // does not end an address in a browser and must not here.
        Assert.Equal(new[] { "https://example.com/x\u0085y" }, Linked.Links("https://example.com/x\u0085y"));
    }

    [Fact]
    public void KeepsTheWordsAroundIt()
    {
        var pieces = Linked.Pieces("go to https://example.com now");
        Assert.Equal(new Linked.Piece[] { new Linked.Piece.Text("go to "), new Linked.Piece.Link("https://example.com"), new Linked.Piece.Text(" now") }, pieces);
        Assert.Equal("go to https://example.com now", string.Concat(pieces.Select(p => p is Linked.Piece.Text t ? t.Words : ((Linked.Piece.Link)p).Href)));
    }

    [Fact]
    public void FindsAnAddressStraightAfterChineseOrJapanese()
    {
        Assert.Equal(new[] { "https://example.com/x" }, Linked.Links("看这个https://example.com/x"));
        Assert.Equal(new[] { "https://example.jp" }, Linked.Links("これhttps://example.jp を"));
        Assert.Equal(new[] { "HTTPS://Example.com/A" }, Linked.Links("HTTPS://Example.com/A"));
    }

    [Fact]
    public void TheWebsDurations()
    {
        Assert.Equal(2.2, NoticeTimes.Briefly);
        Assert.Equal(3.0, NoticeTimes.AMoment);
        Assert.Equal(10.0, NoticeTimes.AtMost);
        Assert.Equal(3, NoticeTimes.Shown);
    }

    [Fact]
    public void TheFourthPushesOutTheOldestThatGoesByItself()
    {
        Assert.Empty(NoticeTimes.Overflow(new[] { false, false, false }));
        Assert.Equal(new[] { 0 }, NoticeTimes.Overflow(new[] { false, false, false, false }));
        // Somebody at the door stays; the next oldest goes instead.
        Assert.Equal(new[] { 1 }, NoticeTimes.Overflow(new[] { true, false, false, false }));
        Assert.Equal(new[] { 0, 1 }, NoticeTimes.Overflow(new[] { false, false, false, false, false }));
        // Nothing that stays is ever dropped, even past the limit.
        Assert.Empty(NoticeTimes.Overflow(new[] { true, true, true, true }));
    }

    [Fact]
    public void AClockWaitsWhileHeldAndKeepsWhatWasLeft()
    {
        var clock = new NoticeClock(lasts: 3.0, now: 100.0, running: true);
        Assert.Equal(2.0, clock.Due(101.0)!.Value, 9);
        clock = clock.Hold(101.0);
        Assert.Null(clock.Due(150.0));
        Assert.Equal(2.0, clock.Left, 9);
        clock = clock.Run(150.0);
        Assert.Equal(2.0, clock.Due(150.0)!.Value, 9);
        Assert.Equal(0.5, clock.Due(151.5)!.Value, 9);
    }

    [Fact]
    public void ANoticeReadToItsLastMomentIsGivenASecondMore()
    {
        var clock = new NoticeClock(lasts: 2.2, now: 0.0, running: true).Hold(2.15).Run(9.0);
        Assert.Equal(NoticeTimes.AfterHolding, clock.Due(9.0)!.Value, 9);
    }

    [Fact]
    public void AClockStartedHeldDoesNotRun()
    {
        var clock = new NoticeClock(lasts: 2.2, now: 0.0, running: false);
        Assert.Null(clock.Due(5.0));
        Assert.False(clock.Running);
    }

    [Fact]
    public void APilePeeksAndASpreadColumnStacks()
    {
        var heights = new[] { 44f, 60f, 52f };
        Assert.Equal(new[] { 0f, NoticePile.Peek, 2 * NoticePile.Peek }, NoticePile.Lifts(heights, spread: false));
        Assert.Equal(44 + 2 * NoticePile.Peek, NoticePile.Height(heights, spread: false));
        Assert.Equal(new[] { 0f, 44 + NoticePile.Gap, 44 + 60 + 2 * NoticePile.Gap }, NoticePile.Lifts(heights, spread: true));
        Assert.Equal(44 + 60 + 52 + 2 * NoticePile.Gap, NoticePile.Height(heights, spread: true));
        Assert.Empty(NoticePile.Lifts(Array.Empty<float>(), spread: true));
        Assert.Equal(0f, NoticePile.Height(Array.Empty<float>(), spread: false));
        Assert.Equal(1f, NoticePile.Scale(0, spread: false));
        Assert.Equal(0.9f, NoticePile.Scale(2, spread: false), 4);
        Assert.Equal(1f, NoticePile.Scale(2, spread: true));
    }

    [Fact]
    public void NoticesRiseOverMessagesOnlyWhereTheyShareWidth()
    {
        // Messages at the left, 320 wide from 12, 90 high.
        Assert.Equal(0f, CornerLanes.Lift(1200f, 332f, 90f));
        Assert.Equal(90 + CornerLanes.Gap, CornerLanes.Lift(60f, 332f, 90f));
        // Closer than the gap counts as touching.
        Assert.Equal(90 + CornerLanes.Gap, CornerLanes.Lift(336f, 332f, 90f));
        // Measured in the window's pixels, the gap is given in pixels too.
        Assert.Equal(0f, CornerLanes.Lift(336f, 332f, 90f, gap: 2f));
        Assert.Equal(0f, CornerLanes.Lift(60f, null, null));
        Assert.Equal(0f, CornerLanes.Lift(60f, 0f, 0f));
    }

    [Fact]
    public void TheBoxStopsAtTwoThousandWithoutSplittingACharacter()
    {
        Assert.Equal(2000, ChatRules.Clamp(new string('a', 2100)).Length);
        Assert.Equal("short", ChatRules.Clamp("short"));
        // One short of the limit, then an emoji of two UTF-16 units.
        Assert.Equal(new string('a', 1999), ChatRules.Clamp(new string('a', 1999) + "😀"));
        Assert.Equal(2000, ChatRules.Clamp(string.Concat(Enumerable.Repeat("你", 2001))).Length);
    }

    [Fact]
    public void ARunFromOnePersonNamesThemOnce()
    {
        var speakers = new[] { "a", "a", "b", "a", "a", "a" };
        Assert.Equal(new[] { false, true, false, false, true, true }, speakers.Select((_, i) => ChatRules.Continues(speakers, i)));
        Assert.False(ChatRules.Continues(Array.Empty<string>(), 0));
    }
}
