using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * The web's own cases for where pictures go (web/src/live/plan.test.ts), as
 * Android's PlanTest and the Apple apps' PlanTests hold their ports to them,
 * case for case. The faults they guard are the ones the web found: a
 * filmstrip passing under the controls, a phone showing one person in a
 * letterbox, two strangers side by side where a conversation should be.
 */
public class PlanTests
{
    private static readonly Size Window = new(1600f, 900f);
    private static readonly Size Phone = new(393f, 852f);

    private static List<string> Ids(int count) => Enumerable.Range(0, count).Select(i => $"p{i}").ToList();

    private static List<string> With(params IEnumerable<string>[] parts) => parts.SelectMany(p => p).ToList();

    private static bool Overlaps(Rect a, Rect b)
    {
        const float tolerance = 0.5f;
        return a.Right - tolerance > b.Left && b.Right - tolerance > a.Left && a.Bottom - tolerance > b.Top && b.Bottom - tolerance > a.Top;
    }

    [Fact]
    public void GridPlacesEverybodyOnceWithoutOverlap()
    {
        for (var count = 1; count <= 9; count++)
        {
            var placed = Plan.Grid(Window, Ids(count)).Values.ToList();
            Assert.Equal(count, placed.Count);
            for (var i = 0; i < placed.Count; i++)
                for (var j = i + 1; j < placed.Count; j++)
                    Assert.False(Overlaps(placed[i], placed[j]), $"{count} people");
        }
    }

    /// <summary>Clear of the controls whole, and of the window's edges by the gap.</summary>
    [Fact]
    public void GridKeepsClearOfTheControlsAndInsideTheWindow()
    {
        foreach (var size in new[] { Window, new Size(1280f, 772f), Phone })
        {
            for (var count = 1; count <= 9; count++)
            {
                foreach (var spot in Plan.Grid(size, Ids(count)).Values)
                {
                    Assert.True(spot.Bottom <= size.Height - Plan.Island + 0.5f, $"{count} in {size}");
                    Assert.True(spot.Left >= Plan.Gap - 0.5f, $"{count} in {size}");
                    Assert.True(spot.Top >= 0);
                    Assert.True(spot.Right <= size.Width - Plan.Gap + 0.5f, $"{count} in {size}");
                }
            }
        }
    }

    /// <summary>The paging arrows sit beside the pictures, not over the first and last.</summary>
    [Fact]
    public void ARoomThatPagesLeavesItsArrowsRoom()
    {
        var grid = Plan.ForRoom(Window, With(new[] { "me" }, Ids(20)), "me", null);
        foreach (var spot in grid.Spots.Values)
        {
            Assert.True(spot.Left >= Plan.PagerReserve - 0.5f);
            Assert.True(spot.Right <= Window.Width - Plan.PagerReserve + 0.5f);
        }
        var strip = Plan.ForRoom(Window, With(new[] { "me", "stage" }, Ids(20)), "me", "stage");
        foreach (var id in strip.Shown)
        {
            Assert.True(strip.Spots[id].Left >= Plan.PagerReserve - 0.5f);
            Assert.True(strip.Spots[id].Right <= Window.Width - Plan.PagerReserve + 0.5f);
        }
    }

    [Fact]
    public void GridCentresAShortLastRow()
    {
        var plan = Plan.Grid(new Size(1200f, 900f), Ids(5));
        var lowest = plan.Values.Max(r => r.Top);
        var lastRow = plan.Values.Where(r => r.Top == lowest).ToList();
        Assert.Equal(lastRow.Min(r => r.Left), 1200f - lastRow.Max(r => r.Right), 3);
    }

    [Fact]
    public void GridPlansNothingUnmeasured()
    {
        Assert.Empty(Plan.Grid(Size.Zero, Ids(4)));
        Assert.Empty(Plan.Grid(Window, new List<string>()));
    }

    [Fact]
    public void FocusPutsTheStageAboveTheStripAndBothClearOfTheControls()
    {
        var plan = Plan.Focus(Window, "stage", Ids(4));
        var stage = plan["stage"];
        foreach (var id in Ids(4))
        {
            var spot = plan[id];
            Assert.True(spot.Top >= stage.Bottom);
            Assert.True(spot.Bottom <= Window.Height - Plan.Island + 0.5f);
        }
    }

    [Fact]
    public void FocusFillsTheScreenWhenAsked()
    {
        var plan = Plan.Focus(Window, "stage", Ids(3), fullscreen: true);
        Assert.Single(plan);
        Assert.Equal(Rect.Of(Window), plan["stage"]);
    }

    [Fact]
    public void StripCapacityHoldsSomebodyAlways()
    {
        Assert.Equal(1, Plan.StripCapacity(new Size(10f, 900f)));
        Assert.True(Plan.StripCapacity(new Size(1600f, 900f)) > Plan.StripCapacity(new Size(800f, 900f)));
    }

    [Fact]
    public void APhoneIsFilledRatherThanBanded()
    {
        // The web measured 0.28 before it learned to turn tiles upright.
        foreach (var count in new[] { 3, 4, 6 })
        {
            var used = Plan.Grid(Phone, Ids(count)).Values.Sum(r => (double)(r.Width * r.Height));
            Assert.True(used / (Phone.Width * (Phone.Height - Plan.Island)) > 0.45, $"{count} people");
        }
    }

    [Fact]
    public void UprightPicturesAreUpright()
    {
        var spot = Plan.Grid(Phone, Ids(4)).Values.First();
        Assert.True(spot.Height > spot.Width);
        var wide = Plan.Grid(Window, Ids(4)).Values.First();
        Assert.True(wide.Width > wide.Height);
    }

    [Fact]
    public void TwoPeopleAreAConversation()
    {
        var plan = Plan.Close(Phone, "them", new[] { "me" });
        Assert.Equal(Rect.Of(Phone), plan["them"]);
        var me = plan["me"];
        Assert.True(me.Right <= Phone.Width);
        Assert.True(me.Bottom <= Phone.Height - Plan.Island);
        Assert.True(me.Width < Phone.Width / 2);
    }

    [Fact]
    public void ACardThatWouldNotFitIsDropped()
    {
        var shortWindow = new Size(393f, 300f);
        foreach (var (id, spot) in Plan.Close(shortWindow, "them", Ids(6)))
        {
            if (id != "them") Assert.True(spot.Bottom <= shortWindow.Height - Plan.Island);
        }
    }

    // The whole room (Room.tsx)

    /// <summary>Whichever page is showing, your own picture is on it, and it takes one of the nine places rather than making a tenth.</summary>
    [Fact]
    public void YourOwnPictureNeverPagesAway()
    {
        var everybody = With(new[] { "me" }, Ids(20));
        var seen = new HashSet<string>();
        var first = Plan.ForRoom(Window, everybody, "me", null);
        Assert.Equal(3, first.Pages);
        for (var page = 0; page < first.Pages; page++)
        {
            var room = Plan.ForRoom(Window, everybody, "me", null, page: page);
            Assert.True(room.Spots.ContainsKey("me"), $"page {page}");
            Assert.Equal("me", room.Shown[0]);
            Assert.True(room.Spots.Count <= Plan.TilesPerPage);
            seen.UnionWith(room.Shown);
        }
        Assert.Equal(everybody.ToHashSet(), seen);
    }

    [Fact]
    public void YourOwnPictureStaysInTheStripToo()
    {
        var room = Plan.ForRoom(Window, With(new[] { "me", "stage" }, Ids(30)), "me", "stage", page: 2);
        Assert.True(room.Spots.ContainsKey("me"));
        Assert.True(room.Spots.ContainsKey("stage"));
        var places = Plan.StripCapacity(new Size(Window.Width - 2 * Plan.PagerReserve, Window.Height));
        Assert.Equal(places + 1, room.Spots.Count);
    }

    /// <summary>
    /// A strip that pages holds what fits beside the arrows at the window's
    /// own shape: counted at the width left between them, a window a little
    /// short of upright (840 by 1000) read as upright, its strip counted as
    /// narrow upright cards and then drawn as wide ones, off both sides.
    /// </summary>
    [Fact]
    public void AStripThatPagesStaysInsideItsWindow()
    {
        foreach (var size in new[] { new Size(840f, 1000f), new Size(700f, 820f), Window, Phone })
        {
            for (var count = 3; count <= 12; count++)
            {
                var room = Plan.ForRoom(size, With(new[] { "me", "stage" }, Ids(count)), "me", "stage");
                foreach (var id in room.Shown)
                {
                    var spot = room.Spots[id];
                    Assert.True(spot.Left >= -0.5f && spot.Right <= size.Width + 0.5f, $"{count} in {size}: {spot}");
                }
            }
        }
    }

    [Fact]
    public void APageThatNoLongerExistsShowsTheLastOne()
    {
        var room = Plan.ForRoom(Window, With(new[] { "me" }, Ids(10)), "me", null, page: 7);
        Assert.Equal(room.Pages - 1, room.Page);
        Assert.NotEmpty(room.Shown);
    }

    [Fact]
    public void TwoPeopleAreDrawnAsACallOfTwo()
    {
        var room = Plan.ForRoom(Phone, new[] { "me", "them" }, "me", null);
        Assert.Equal(Rect.Of(Phone), room.Spots["them"]);
        Assert.True(room.Spots["me"].Width < Phone.Width / 2);
    }

    [Fact]
    public void FillingTheScreenDrawsTheStageAlone()
    {
        var room = Plan.ForRoom(Window, With(new[] { "me", "stage" }, Ids(4)), "me", "stage", fullscreen: true);
        Assert.Single(room.Spots);
        Assert.Equal(Rect.Of(Window), room.Spots["stage"]);
    }

    [Fact]
    public void AStageThatIsNotInTheRoomIsTheGrid()
    {
        var room = Plan.ForRoom(Window, With(new[] { "me" }, Ids(4)), "me", "gone");
        Assert.Equal(5, room.Spots.Count);
        Assert.False(room.Spots.ContainsKey("gone"));
    }

    /// <summary>The strain card, stacked above the notices, is lifted clear of the filmstrip's pictures and of nothing else.</summary>
    [Fact]
    public void TheStrainCardEndsAboveTheFilmstrip()
    {
        var room = Plan.ForRoom(Window, With(new[] { "me", "stage" }, Ids(3)), "me", "stage");
        var clear = Plan.Clear(room.Spots, room.Shown, Window);
        var stripTop = room.Shown.Where(room.Spots.ContainsKey).Min(id => room.Spots[id].Top);
        Assert.Equal((float?)(Window.Height - stripTop + Plan.Gap), clear);
        var ends = Plan.Island + 8 + 10;
        var lift = Plan.Lift(clear, ends);
        Assert.True(lift > 0);
        Assert.True(Window.Height - (ends + lift) <= stripTop - Plan.Gap);
        Assert.Equal(0f, Plan.Lift(clear, clear!.Value + 40));
        var grid = Plan.ForRoom(Window, With(new[] { "me" }, Ids(3)), "me", null);
        Assert.Null(Plan.Clear(grid.Spots, new List<string>(), Window));
        Assert.Equal(0f, Plan.Lift(null, ends));
    }

    // Who has the stage (usePin.test.tsx)

    [Fact]
    public void AShareTakesTheStageWhenItStartsAndGivesItBack()
    {
        var pin = new StagePin();
        pin = pin.Update(new[] { "a", "b" }, Array.Empty<string>());
        Assert.Null(pin.Pinned);
        pin = pin.Update(new[] { "a", "b", "a/screen" }, new[] { "a/screen" });
        Assert.Equal("a/screen", pin.Pinned);
        pin = pin.Update(new[] { "a", "b" }, Array.Empty<string>());
        Assert.Null(pin.Pinned);
    }

    [Fact]
    public void AShareAlreadyRunningOnArrivalTakesTheStage()
    {
        Assert.Equal("a/screen", new StagePin().Update(new[] { "a", "a/screen" }, new[] { "a/screen" }).Pinned);
    }

    /// <summary>The way back to the grid: pressing the share on the stage shows everybody, and the share does not climb straight back on.</summary>
    [Fact]
    public void TheGridCanBeChosenWhileSomebodyShares()
    {
        var pin = new StagePin().Update(new[] { "a", "b", "a/screen" }, new[] { "a/screen" });
        pin = pin.Toggle("a/screen");
        pin = pin.Update(new[] { "a", "b", "a/screen" }, new[] { "a/screen" });
        Assert.Null(pin.Pinned);
        // Until a share that is new starts.
        pin = pin.Update(new[] { "a", "b", "a/screen", "b/screen" }, new[] { "a/screen", "b/screen" });
        Assert.Equal("b/screen", pin.Pinned);
    }

    [Fact]
    public void AShareDoesNotTakeAStagePinnedByHand()
    {
        var pin = new StagePin().Update(new[] { "a", "b" }, Array.Empty<string>()).Pin("b");
        pin = pin.Update(new[] { "a", "b", "a/screen" }, new[] { "a/screen" });
        Assert.Equal("b", pin.Pinned);
        pin = pin.Update(new[] { "a", "b" }, Array.Empty<string>());
        // An unrelated share ending keeps a pin made by hand.
        Assert.Equal("b", pin.Pinned);
    }

    [Fact]
    public void APinnedPersonLeavingEmptiesTheStage()
    {
        var pin = new StagePin().Update(new[] { "a", "b" }, Array.Empty<string>()).Pin("b").Update(new[] { "a" }, Array.Empty<string>());
        Assert.Null(pin.Pinned);
    }

    [Fact]
    public void TheSameTileTwiceTogglesOff()
    {
        var pin = new StagePin().Toggle("a");
        Assert.Equal("a", pin.Pinned);
        pin = pin.Toggle("a");
        Assert.Null(pin.Pinned);
    }

    [Fact]
    public void TheFirstShareKeepsTheStageAndHandsItOn()
    {
        var pin = new StagePin().Update(new[] { "a", "b", "a/screen" }, new[] { "a/screen" });
        pin = pin.Update(new[] { "a", "b", "a/screen", "b/screen" }, new[] { "a/screen", "b/screen" });
        // A second share does not fight the first for the stage.
        Assert.Equal("a/screen", pin.Pinned);
        pin = pin.Update(new[] { "a", "b", "b/screen" }, new[] { "b/screen" });
        // The remaining share takes over rather than the grid.
        Assert.Equal("b/screen", pin.Pinned);
    }

    // Who spoke last (useSpeakingOrder.test.tsx)

    [Fact]
    public void SomebodyTalkingComesForwardAndStaysThroughAPause()
    {
        var room = new[] { "a", "b", "c" };
        var record = new SpeakingRecord().Hear(Array.Empty<string>(), room, 1.0);
        Assert.Equal(new[] { "a", "b", "c" }, record.Order(room));
        record = record.Hear(new[] { "c" }, room, 2.0);
        Assert.Equal(new[] { "c", "a", "b" }, record.Order(room));
        record = record.Hear(Array.Empty<string>(), room, 3.0);
        Assert.Equal(new[] { "c", "a", "b" }, record.Order(room));
        record = record.Hear(new[] { "b" }, room, 4.0);
        Assert.Equal(new[] { "b", "c", "a" }, record.Order(room));
    }

    [Fact]
    public void SomebodyWhoLeftIsForgotten()
    {
        var record = new SpeakingRecord().Hear(new[] { "c" }, new[] { "a", "c" }, 1.0).Hear(Array.Empty<string>(), new[] { "a" }, 2.0);
        Assert.Equal(new[] { "a", "c" }, record.Order(new[] { "a", "c" }));
    }
}
