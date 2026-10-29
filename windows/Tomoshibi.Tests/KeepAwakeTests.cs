using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The display and the machine kept awake through a call and let go after
/// it, asked only when that changes: a display that went dark in a call, or
/// one kept on after it, is what these guard.
/// </summary>
public class KeepAwakeTests
{
    [Fact]
    public void ACallKeepsTheDisplayAndTheMachineAwakeAndLeavingLetsThemGo()
    {
        var asked = new List<uint>();
        var awake = new KeepAwake(flags =>
        {
            asked.Add(flags);
            return 0x80000000;
        });
        Assert.True(awake.Hold(true));
        // ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED.
        Assert.Equal(new uint[] { 0x80000003 }, asked);
        Assert.True(awake.Held);
        // A call joined again on its way to another relay asks nothing more.
        awake.Hold(true);
        Assert.Single(asked);
        awake.Hold(false);
        // ES_CONTINUOUS alone clears what the call asked.
        Assert.Equal(new uint[] { 0x80000003, 0x80000000 }, asked);
        Assert.False(awake.Held);
        awake.Hold(false);
        Assert.Equal(2, asked.Count);
    }

    [Fact]
    public void NothingIsAskedBeforeTheFirstCallAndARefusalIsAskedAgain()
    {
        var asked = new List<uint>();
        var refuse = true;
        var awake = new KeepAwake(flags =>
        {
            asked.Add(flags);
            return refuse ? 0u : 0x80000000;
        });
        awake.Hold(false);
        Assert.Empty(asked);
        Assert.False(awake.Hold(true));
        Assert.False(awake.Held);
        refuse = false;
        Assert.True(awake.Hold(true));
        Assert.True(awake.Held);
        Assert.Equal(new uint[] { KeepAwake.InCall, KeepAwake.InCall }, asked);
    }
}
