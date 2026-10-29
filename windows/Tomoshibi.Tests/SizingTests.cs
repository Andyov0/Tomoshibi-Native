using System.Text.RegularExpressions;
using Tomoshibi.Basics;
using Xunit;
using static Tomoshibi.Tests.LooksTests;

namespace Tomoshibi.Tests;

/// <summary>
/// The sizes the Windows app draws to, held to what the owner decided for
/// the four apps and the web (2026-10-29, UI sync): the bar, the name on a
/// picture, the stage's buttons, the menus and the lobby. And the few rules
/// that are only in the drawing code, read from its source: no gradients,
/// a switch that is off never red, a speaker's name never the tally.
/// </summary>
public class SizingTests
{
    private static string Root()
    {
        var at = new DirectoryInfo(AppContext.BaseDirectory);
        while (at != null && !File.Exists(Path.Combine(at.FullName, "apple", "i18n", "native.json"))) at = at.Parent;
        return at?.FullName ?? throw new InvalidOperationException("the repository is not above " + AppContext.BaseDirectory);
    }

    private static string Source(string path) => File.ReadAllText(Path.Combine(Root(), "windows", "Tomoshibi", path));

    [Fact]
    public void TheSizesInUseAreOnTheScale()
    {
        foreach (var height in new[] { Sizing.Menu.Row, Sizing.Lobby.CornerButton, Sizing.Stage.Button, Sizing.Bar.Button, Sizing.Close })
            Assert.Contains(height, Sizing.PointerHeights);
        foreach (var radius in new[] { Sizing.Menu.Radius, Sizing.Menu.RowRadius, Sizing.Lobby.CardRadius })
            Assert.Contains(radius, Sizing.Radii);
        foreach (var space in new[] { Sizing.Menu.Inset, Sizing.Stage.Inset, Sizing.Stage.Gap, Sizing.NamePlate.Inset, Sizing.Bar.Spacing, Sizing.Bar.CompactSpacing })
            Assert.Contains(space, Sizing.Spacing);
        Assert.Equal(0.40f, Sizing.IconRatio);
        Assert.Equal(17.6, Sizing.Icon(44), 3);
    }

    [Fact]
    public void AMenusRowsSitInsideItsCorner()
    {
        Assert.Equal(14f, Sizing.Menu.Radius);
        Assert.Equal(10f, Sizing.Menu.RowRadius);
        // Concentric: the row's corner is the menu's less the inset between them.
        Assert.Equal(Sizing.Menu.Radius - Sizing.Menu.Inset, Sizing.Menu.RowRadius);
        Assert.Equal(32f, Sizing.Menu.Row);
        // Leaving's two answers are pressed in a hurry.
        Assert.True(Sizing.Menu.AnswerRow >= 44);
    }

    [Fact]
    public void LeavingIsTheBarsOneCapsuleAndTheReactionsArrowCanBeHit()
    {
        Assert.Equal(56f, Sizing.Bar.LeaveWidth);
        Assert.Equal(44f, Sizing.Bar.Button);
        Assert.True(Sizing.Bar.ReactionsArrow >= 32, "the reactions' arrow was a 20 sliver beside the hand");
        Assert.Equal(40f, Sizing.Bar.Hand(44));
        Assert.Equal(36f, Sizing.Bar.Hand(40));
        Assert.True(Sizing.Bar.Reaction >= 40);
        // Six reactions, five gaps and the inset each side: the tray is as wide as what it holds.
        Assert.Equal(6 * 40 + 5 * 2 + 2 * 4, Sizing.Bar.ReactionsWidth(6));
    }

    [Theory]
    // A full bar: the microphone, the camera, the share, the messages, the sound and the devices.
    [InlineData(false, 6)]
    // With watching together, the music and the host's crown as well.
    [InlineData(false, 9)]
    // A compact bar: the four and More.
    [InlineData(true, 5)]
    public void TheBarIsAsLongAsWhatIsOnIt(bool compact, int round)
    {
        var size = compact ? 40f : 44f;
        var parts = new List<float>();
        for (var i = 0; i < round; i++) parts.Add(size);
        parts.Add(Sizing.Bar.Hand(size) + Sizing.Bar.ReactionsArrow);
        if (!compact) parts.Add(1 + 4);
        parts.Add(compact ? 52 : 56);
        var expected = 2 * (compact ? 5 : 7) + parts.Sum() + (compact ? 4 : 6) * (parts.Count - 1);
        Assert.Equal((double)expected, Sizing.Bar.Length(compact, round), 3);
    }

    /// <summary>
    /// No gradient anywhere the owner looks (2026-10-29), the glass's rim
    /// among them, which faded from white at 26 per cent to 7. The one left is
    /// the scrim under the video watched together's own controls, which keeps
    /// them readable over the film and was not part of this round.
    /// </summary>
    [Fact]
    public void ThereAreNoGradients()
    {
        var ui = Path.Combine(Root(), "windows", "Tomoshibi", "UI");
        var allowed = new[] { Path.Combine("Watch", "WatchStage.cs") };
        var found = Directory.EnumerateFiles(ui, "*.cs", SearchOption.AllDirectories)
            .Where(f => !allowed.Any(a => f.EndsWith(a, StringComparison.Ordinal)))
            .Where(f => Regex.IsMatch(File.ReadAllText(f), @"\b(Linear|Radial)GradientBrush\b"))
            .Select(f => Path.GetRelativePath(ui, f))
            .ToList();
        Assert.Empty(found);
    }

    [Fact]
    public void ASwitchThatIsOffIsNeverRed()
    {
        // Before a call (Shared.DeviceSwitches) and in it (ControlBar):
        // the plate of the foreground, as the bar's.
        var shared = Source(Path.Combine("UI", "Shared.cs"));
        var switches = shared[shared.IndexOf("public static StackPanel DeviceSwitches", StringComparison.Ordinal)..];
        switches = switches[..switches.IndexOf("public sealed class CameraStage", StringComparison.Ordinal)];
        Assert.DoesNotContain("Look.Destructive", switches);
        Assert.Contains("Look.Off", switches);
        Assert.Contains("Look.Off", Source(Path.Combine("UI", "Room", "ControlBar.cs")));
    }

    [Fact]
    public void NoPanelsButtonIsLitInTheTally()
    {
        // Lit is for a share going out and a hand up; an open panel holds its button.
        var bar = Source(Path.Combine("UI", "Room", "ControlBar.cs"));
        var lit = Regex.Matches(bar, @"(_\w+)\.Lit\s*=").Select(m => m.Groups[1].Value).ToList();
        Assert.Equal(new[] { "_hand" }, lit);
        Assert.DoesNotMatch(new Regex(@"lit:\s*(chat|open)\b"), bar);
    }
}
