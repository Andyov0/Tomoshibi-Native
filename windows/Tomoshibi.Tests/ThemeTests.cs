using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The two palettes, held to the numbers every platform shares (the daylight
/// palette of 2026-10-26): the dark one exactly as it always was, since
/// every picture of a dark screen is to be the same after the day mode as
/// before it, and the light one as written down, so that the four apps look
/// like one product in either.
/// </summary>
public class ThemeTests
{
    private static string Hex(Shade shade)
    {
        var (a, r, g, b) = shade.Bytes();
        return a == 255 ? $"#{r:X2}{g:X2}{b:X2}" : $"#{r:X2}{g:X2}{b:X2}@{a}";
    }

    [Fact]
    public void TheDarkPaletteIsWhatItAlwaysWas()
    {
        var dark = Theme.Of(Scheme.Dark);
        Assert.Equal("#0B0B0D", Hex(dark.Background));
        Assert.Equal("#1C1C1E", Hex(dark.Surface));
        Assert.Equal("#2C2C2E", Hex(dark.SurfaceHigh));
        Assert.Equal("#FFFFFF@26", Hex(dark.Border));
        Assert.Equal("#F5F5F7", Hex(dark.Foreground));
        Assert.Equal("#EBEBF5@153", Hex(dark.Muted));
        Assert.Equal("#30D158", Hex(dark.Good));
        Assert.Equal("#FF453A", Hex(dark.Danger));
        Assert.Equal("#FFFFFF", Hex(dark.DangerForeground));
        Assert.Equal("#FFFFFF", Hex(dark.Wash));
        Assert.Equal(dark.Good, dark.GoodText);
        Assert.Equal(dark.Danger, dark.DangerText);
        // The palette the screens read and the numbers the rest of the code
        // has always used are one and the same.
        Assert.Equal(Theme.Background, dark.Background);
        Assert.Equal(Theme.Muted, dark.Muted);
        Assert.Equal(Theme.Danger, dark.Danger);
    }

    [Fact]
    public void TheLightPaletteIsTheOneEveryPlatformShares()
    {
        var light = Theme.Of(Scheme.Light);
        Assert.Equal("#F2F2F7", Hex(light.Background));
        Assert.Equal("#FFFFFF", Hex(light.Surface));
        Assert.Equal("#E5E5EA", Hex(light.SurfaceHigh));
        Assert.Equal("#000000@26", Hex(light.Border));
        Assert.Equal("#1C1C1E", Hex(light.Foreground));
        Assert.Equal("#3C3C43@153", Hex(light.Muted));
        Assert.Equal("#34C759", Hex(light.Good));
        Assert.Equal("#FF3B30", Hex(light.Danger));
        Assert.Equal("#FFFFFF", Hex(light.DangerForeground));
        Assert.Equal("#000000", Hex(light.Wash));
        Assert.Equal("#217E38", Hex(light.GoodText));
        Assert.Equal("#D70015", Hex(light.DangerText));
    }

    /// <summary>The green and the red as words read at 4.5:1 on the light ground and its white surface, which their fills do not.</summary>
    [Fact]
    public void TheGreenAndTheRedAsWordsReadOnTheLightGround()
    {
        var light = Theme.Light;
        foreach (var ground in new[] { light.Background, light.Surface })
        {
            Assert.True(Contrast(light.GoodText, ground) >= 4.5, $"green {Contrast(light.GoodText, ground):F2}");
            Assert.True(Contrast(light.DangerText, ground) >= 4.5, $"red {Contrast(light.DangerText, ground):F2}");
        }
        Assert.True(Contrast(light.Danger, light.Background) < 4.5);
    }

    /// <summary>The border is the wash at a tenth in either scheme: what the chrome's other washes are turned over by.</summary>
    [Theory]
    [InlineData(Scheme.Dark)]
    [InlineData(Scheme.Light)]
    public void TheBorderIsTheWashAtATenth(Scheme scheme)
    {
        var palette = Theme.Of(scheme);
        Assert.Equal(Hex(palette.Wash.WithAlpha(0.10f)), Hex(palette.Border));
    }

    [Theory]
    [InlineData(Accent.Amber, "#FFA23B", "#F2901E")]
    [InlineData(Accent.Orange, "#FF9F0A", "#FF9500")]
    [InlineData(Accent.Pink, "#FF375F", "#FF2D55")]
    [InlineData(Accent.Purple, "#BF5AF2", "#AF52DE")]
    [InlineData(Accent.Indigo, "#5E5CE6", "#5856D6")]
    [InlineData(Accent.Blue, "#0A84FF", "#007AFF")]
    [InlineData(Accent.Teal, "#40C8E0", "#30B0C7")]
    [InlineData(Accent.Green, "#30D158", "#34C759")]
    [InlineData(Accent.Graphite, "#98989D", "#8E8E93")]
    public void EachTallyHasItsDarkAndLightColour(Accent accent, string dark, string light)
    {
        Assert.Equal(dark, Hex(accent.Color()));
        Assert.Equal(dark, Hex(accent.Color(Scheme.Dark)));
        Assert.Equal(light, Hex(accent.Color(Scheme.Light)));
    }

    [Theory]
    [InlineData(Accent.Amber, "#A35C09")]
    [InlineData(Accent.Orange, "#A15E00")]
    [InlineData(Accent.Pink, "#DF002B")]
    [InlineData(Accent.Purple, "#A338D9")]
    [InlineData(Accent.Indigo, "#5856D6")]
    [InlineData(Accent.Blue, "#006BE0")]
    [InlineData(Accent.Teal, "#217989")]
    [InlineData(Accent.Green, "#217E38")]
    [InlineData(Accent.Graphite, "#6E6E73")]
    public void TheTallyAsWordsTakesADeeperInkOnlyInTheLightScheme(Accent accent, string light)
    {
        Assert.Equal(light, Hex(accent.TextColor(Scheme.Light)));
        Assert.Equal(Hex(accent.Color()), Hex(accent.TextColor(Scheme.Dark)));
    }

    /// <summary>The reason for the deeper ink: words in it read at 4.5:1 on the light ground, which the fills do not.</summary>
    [Theory]
    [MemberData(nameof(AllAccents))]
    public void TheTallyAsWordsReadsOnTheLightGround(Accent accent)
    {
        var ground = Theme.Light.Background;
        Assert.True(Contrast(accent.TextColor(Scheme.Light), ground) >= 4.49, $"{accent}: {Contrast(accent.TextColor(Scheme.Light), ground):F2}");
    }

    /// <summary>
    /// What is written on the tally keeps its ink in the light scheme, which
    /// holds only while it still reads there as well as it does on the dark
    /// tally (the palette: unchanged unless the contrast fails).
    /// </summary>
    [Theory]
    [MemberData(nameof(AllAccents))]
    public void TheInkOnTheTallyStillReadsInTheLightScheme(Accent accent)
    {
        var onDark = Contrast(accent.Color(Scheme.Dark), accent.OnColor());
        var onLight = Contrast(accent.Color(Scheme.Light), accent.OnColor());
        Assert.True(onLight >= Math.Min(onDark, 4.5) - 0.01, $"{accent}: {onLight:F2} on the light tally against {onDark:F2} on the dark one");
    }

    public static IEnumerable<object[]> AllAccents() => Accents.All.Select(a => new object[] { a });

    /// <summary>The one switch decides the meeting's scheme, whichever way it is set: dark whatever the system says, or the system's.</summary>
    [Fact]
    public void TheMeetingTakesItsSchemeFromTheOneSwitch()
    {
        Assert.Equal(Daylight.MeetingStaysDark ? Scheme.Dark : Scheme.Light, Daylight.ForMeeting(Scheme.Light));
        Assert.Equal(Scheme.Dark, Daylight.ForMeeting(Scheme.Dark));
    }

    [Fact]
    public void TheTallyFollowsTheSchemeAsked()
    {
        var appearance = new Appearance(null);
        appearance.Choose(Accent.Blue);
        Assert.Equal("#0A84FF", Hex(appearance.Tally));
        Assert.Equal("#0A84FF", Hex(appearance.TallyIn(Scheme.Dark)));
        Assert.Equal("#007AFF", Hex(appearance.TallyIn(Scheme.Light)));
    }

    /// <summary>WCAG's contrast ratio between two opaque colours.</summary>
    private static double Contrast(Shade a, Shade b)
    {
        static double Channel(float c) => c <= 0.04045 ? c / 12.92 : Math.Pow((c + 0.055) / 1.055, 2.4);
        static double Luminance(Shade s)
        {
            var (_, r, g, b) = s.Bytes();
            return 0.2126 * Channel(r / 255f) + 0.7152 * Channel(g / 255f) + 0.0722 * Channel(b / 255f);
        }
        var (high, low) = (Math.Max(Luminance(a), Luminance(b)), Math.Min(Luminance(a), Luminance(b)));
        return (high + 0.05) / (low + 0.05);
    }
}
