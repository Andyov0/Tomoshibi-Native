using System.Text.RegularExpressions;
using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The call's controls for somebody without a mouse: they step aside only
/// where and when they may, never while the keyboard is on them or a screen
/// reader listens, and the Mac's two shortcuts reach them. What these guard
/// is controls collapsed out of the keyboard's reach and Narrator's, as they
/// were, so that nobody without a mouse could mute or leave.
/// </summary>
public class ControlsAwayTests
{
    private static string Root()
    {
        var at = new DirectoryInfo(AppContext.BaseDirectory);
        while (at != null && !File.Exists(Path.Combine(at.FullName, "apple", "i18n", "native.json"))) at = at.Parent;
        return at?.FullName ?? throw new InvalidOperationException("the repository is not above " + AppContext.BaseDirectory);
    }

    [Fact]
    public void TheControlsStepAsideOnlyWhereTheyMayAndOnlyWhenNobodyIsThere()
    {
        Assert.True(ControlsAway.Away(true, 3.1, false, false, false, false));
        // Not where they are always out (the placement), and not before the three seconds.
        Assert.False(ControlsAway.Away(false, 60, false, false, false, false));
        Assert.False(ControlsAway.Away(true, Plan.IdleAfter, false, false, false, false));
        Assert.False(ControlsAway.Away(true, 1, false, false, false, false));
    }

    [Theory]
    [InlineData(true, false, false, false)]
    [InlineData(false, true, false, false)]
    // The keyboard on one of the controls: the web keeps them out while a focus it shows is in them.
    [InlineData(false, false, true, false)]
    // A screen reader listening: nobody using one has a pointer to bring them back with.
    [InlineData(false, false, false, true)]
    public void NothingInUseStepsAside(bool menu, bool panel, bool focus, bool reader) =>
        Assert.False(ControlsAway.Away(true, 600, menu, panel, focus, reader));

    [Fact]
    public void TheShortcutsAreTheMacs()
    {
        // ⌘D and ⌘E on the Mac (apple/App/TomoshibiApp.swift), Control here.
        var mac = File.ReadAllText(Path.Combine(Root(), "apple", "App", "TomoshibiApp.swift"));
        var microphone = Regex.Match(mac, @"Mute microphone.*?keyboardShortcut\(""(\w)""", RegexOptions.Singleline);
        var camera = Regex.Match(mac, @"Turn camera off.*?keyboardShortcut\(""(\w)""", RegexOptions.Singleline);
        Assert.True(microphone.Success && camera.Success, "the Mac's Call menu has moved");
        Assert.Equal(char.ToUpperInvariant(microphone.Groups[1].Value[0]), CallKeys.Microphone);
        Assert.Equal(char.ToUpperInvariant(camera.Groups[1].Value[0]), CallKeys.Camera);
        // Upper case: the letters are the virtual keys' own codes.
        Assert.Equal(0x44, (int)CallKeys.Microphone);
        Assert.Equal(0x45, (int)CallKeys.Camera);
    }
}
