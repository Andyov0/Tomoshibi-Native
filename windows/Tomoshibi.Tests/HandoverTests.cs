using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * Links handed to the running app by later launches. The app registers to
 * be handed them before it has a window or a model to open them in; a link
 * clicked while it was starting was dropped, and its window never came
 * forward.
 */
public class HandoverTests
{
    [Fact]
    public void WhatIsHandedBeforeAnybodyListensIsGivenInOrderOnceSomebodyDoes()
    {
        var handover = new Handover<string?>();
        handover.Hand("tomoshibi://example.com/#/first");
        handover.Hand(null);
        handover.Hand("tomoshibi://example.com/#/second");
        var given = new List<string?>();
        handover.Listen(given.Add);
        Assert.Equal(new string?[] { "tomoshibi://example.com/#/first", null, "tomoshibi://example.com/#/second" }, given);
    }

    [Fact]
    public void WhatIsHandedAfterIsGivenAtOnceAndNothingTwice()
    {
        var handover = new Handover<string>();
        var given = new List<string>();
        handover.Hand("early");
        handover.Listen(given.Add);
        handover.Hand("late");
        Assert.Equal(new[] { "early", "late" }, given);
    }

    [Fact]
    public void HandedFromAnotherThreadWhileTheWaitingAreGivenComesAfterThem()
    {
        var handover = new Handover<int>();
        for (var i = 0; i < 1000; i++) handover.Hand(i);
        var given = new List<int>();
        var handing = new Thread(() => { for (var i = 1000; i < 2000; i++) handover.Hand(i); });
        handover.Listen(item =>
        {
            given.Add(item);
            if (item == 0) handing.Start();
        });
        handing.Join();
        Assert.Equal(Enumerable.Range(0, 2000), given);
    }
}
