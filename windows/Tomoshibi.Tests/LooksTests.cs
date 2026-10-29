using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// How a button moves between its states (Looks.Paint), held to the rules
/// the four apps and the web now share (2026-10-29): a tenth of the way to
/// the foreground under the pointer, sixteen hundredths and 0.95 pressed,
/// four tenths where it cannot be pressed; a device that is off a plate of
/// the foreground; a panel's button held in the neutral grey, never the
/// tally; a prominent button that cannot be pressed a grey plate, never a
/// faded tally.
/// </summary>
public class LooksTests
{
    private static readonly Shade Amber = Accent.Amber.Color();
    private static readonly Shade OnAmber = Accent.Amber.OnColor();

    private static Painted Paint(Look look, Scheme scheme = Scheme.Dark, Touch touch = Touch.Rest, bool enabled = true, bool lit = false, bool held = false) =>
        Looks.Paint(look, Theme.Of(scheme), Accent.Amber.Color(scheme), OnAmber, touch, enabled, lit, held);

    private static void Near(float expected, float actual, string what) =>
        Assert.True(Math.Abs(expected - actual) < 0.001f, $"{what}: {actual} where {expected} was expected");

    [Fact]
    public void AWashMovesATenthTowardsTheForegroundUnderThePointerAndSixteenHundredthsPressed()
    {
        // The bar's buttons: white at 12 per cent over the dark glass.
        Near(0.12f, Paint(Look.Bar).Fill.Alpha, "rest");
        Near(0.12f + 0.88f * 0.10f, Paint(Look.Bar, touch: Touch.Over).Fill.Alpha, "over");
        Near(0.12f + 0.88f * 0.16f, Paint(Look.Bar, touch: Touch.Down).Fill.Alpha, "down");
        // A clear one from nothing: exactly the two steps.
        Near(0.10f, Paint(Look.Clear, touch: Touch.Over).Fill.Alpha, "clear over");
        Near(0.16f, Paint(Look.Clear, touch: Touch.Down).Fill.Alpha, "clear down");
        // The wash is the foreground's own colour: white by night, black by day.
        Assert.Equal(Shade.White.WithAlpha(0.16f), Paint(Look.Clear, touch: Touch.Down).Fill);
        Assert.Equal(Shade.Black.WithAlpha(0.16f), Paint(Look.Clear, Scheme.Light, Touch.Down).Fill);
    }

    [Theory]
    [InlineData(Scheme.Dark)]
    [InlineData(Scheme.Light)]
    public void AFilledButtonMovesTowardsTheForegroundOfItsScheme(Scheme scheme)
    {
        var palette = Theme.Of(scheme);
        var rest = Paint(Look.Prominent, scheme).Fill;
        Assert.Equal(Looks.Toward(rest, palette.Foreground, 0.10f), Paint(Look.Prominent, scheme, Touch.Over).Fill);
        Assert.Equal(Looks.Toward(rest, palette.Foreground, 0.16f), Paint(Look.Prominent, scheme, Touch.Down).Fill);
        // Lighter by night, darker by day: the same step, towards the words.
        var lighter = Luminance(Paint(Look.Prominent, scheme, Touch.Down).Fill) > Luminance(rest);
        Assert.Equal(scheme == Scheme.Dark, lighter);
    }

    [Theory]
    [MemberData(nameof(EveryLook))]
    public void EveryLookIsSmallerOnlyWhilePressedAndAlwaysByTheSameAmount(Look look)
    {
        Assert.Equal(1f, Paint(look).Scale);
        Assert.Equal(1f, Paint(look, touch: Touch.Over).Scale);
        Assert.Equal(Looks.PressedScale, Paint(look, touch: Touch.Down).Scale);
        Assert.Equal(0.95f, Looks.PressedScale);
        // Not pressable, not pressed.
        Assert.Equal(1f, Paint(look, touch: Touch.Down, enabled: false).Scale);
    }

    [Theory]
    [MemberData(nameof(EveryLook))]
    public void EveryLookMovesFurtherPressedThanUnderThePointer(Look look)
    {
        foreach (var scheme in new[] { Scheme.Dark, Scheme.Light })
        {
            var rest = Paint(look, scheme).Fill;
            var over = Paint(look, scheme, Touch.Over).Fill;
            var down = Paint(look, scheme, Touch.Down).Fill;
            Assert.True(Distance(rest, over) > 0.01f, $"{look} {scheme}: no hover");
            Assert.True(Distance(rest, down) > Distance(rest, over), $"{look} {scheme}: the press is not further than the hover");
        }
    }

    [Theory]
    [InlineData(Look.Plain)]
    [InlineData(Look.Clear)]
    [InlineData(Look.Bar)]
    [InlineData(Look.Destructive)]
    [InlineData(Look.Picture)]
    [InlineData(Look.Off)]
    public void ADisabledButtonIsDrawnAtFourTenths(Look look)
    {
        var on = Paint(look);
        var off = Paint(look, enabled: false);
        Assert.Equal(0.40f, off.ContentOpacity);
        Near(on.Fill.Alpha * 0.40f, off.Fill.Alpha, $"{look} fill");
        Assert.Equal(on.Ink, off.Ink);
    }

    [Theory]
    [InlineData(Scheme.Dark)]
    [InlineData(Scheme.Light)]
    public void ADisabledProminentButtonIsAGreyPlateNotAFadedTally(Scheme scheme)
    {
        var palette = Theme.Of(scheme);
        var off = Paint(Look.Prominent, scheme, enabled: false);
        Assert.Equal(palette.SurfaceHigh, off.Fill);
        Assert.Equal(palette.Muted, off.Ink);
        // Drawn whole, not faded: grey reads as "not yet", a pale tally as
        // the button to press, only weaker.
        Assert.Equal(1f, off.ContentOpacity);
        Assert.Equal(1f, off.Fill.Alpha);
        // The faded tally it replaces: the tally and its dark arrow both at
        // 45 per cent over the dark ground, a brown with the arrow on it at
        // about 1.6:1 (the native audit measured 1.55).
        var faded = Over(Amber.WithAlpha(0.45f), Theme.Dark.Background);
        var arrow = Over(OnAmber.WithAlpha(0.45f), faded);
        Assert.True(Contrast(arrow, faded) < 2, $"{Contrast(arrow, faded):F2}");
        Assert.True(Contrast(off.Ink, off.Fill) > Contrast(arrow, faded));
    }

    [Theory]
    [InlineData(Scheme.Dark, "#F5F5F7", "#0B0B0D")]
    [InlineData(Scheme.Light, "#1C1C1E", "#F2F2F7")]
    public void ADeviceThatIsOffIsAPlateOfTheForegroundWithThePagesInk(Scheme scheme, string plate, string ink)
    {
        var off = Paint(Look.Off, scheme);
        Assert.Equal(plate, Hex(off.Fill));
        Assert.Equal(ink, Hex(off.Ink));
        Assert.True(Contrast(off.Fill, off.Ink) >= 7, $"{Contrast(off.Fill, off.Ink):F2}");
        // Not red: red is for leaving and for faults.
        Assert.NotEqual(Theme.Of(scheme).Danger, off.Fill);
        // Its hover and press move it towards the page, the foreground being
        // as far as it can go the other way.
        var over = Paint(Look.Off, scheme, Touch.Over).Fill;
        Assert.Equal(Looks.Toward(off.Fill, Theme.Of(scheme).Background, 0.10f), over);
    }

    [Fact]
    public void ADeviceThatIsOffOnAPictureIsTheNightsPlateInEitherScheme()
    {
        var off = Paint(Look.OffOnPicture, Scheme.Light);
        Assert.Equal(Theme.Dark.Foreground, off.Fill);
        Assert.Equal(Theme.Dark.Background, off.Ink);
        Assert.Equal(off, Paint(Look.OffOnPicture, Scheme.Dark));
    }

    [Fact]
    public void AnOpenPanelsButtonIsHeldInTheNeutralGreyAndNeverTheTally()
    {
        var held = Paint(Look.Bar, held: true);
        Assert.Equal(Paint(Look.Bar, touch: Touch.Down).Fill, held.Fill);
        Assert.NotEqual(Amber, held.Fill);
        Assert.Equal(Theme.Dark.Foreground, held.Ink);
        // Held under the pointer as well: it does not seem to let go.
        Assert.Equal(held.Fill, Paint(Look.Bar, touch: Touch.Over, held: true).Fill);
        Assert.Equal(held.Fill, Paint(Look.Bar, touch: Touch.Down, held: true).Fill);
        Assert.Equal(Looks.PressedScale, Paint(Look.Bar, touch: Touch.Down, held: true).Scale);
    }

    [Fact]
    public void TheTallyIsForWhatIsHappening()
    {
        // A share going out or a hand up: the tally, with its ink.
        var lit = Paint(Look.Bar, lit: true);
        Assert.Equal(Amber, lit.Fill);
        Assert.Equal(OnAmber, lit.Ink);
        // Lit wins over held: a share's own menu open does not grey it.
        Assert.Equal(Amber, Paint(Look.Bar, lit: true, held: true).Fill);
        // Nothing else on the bar wears it.
        Assert.NotEqual(Amber, Paint(Look.Bar).Fill);
        Assert.NotEqual(Amber, Paint(Look.Off).Fill);
    }

    [Fact]
    public void LeavingIsRed()
    {
        var leave = Paint(Look.Destructive);
        Assert.Equal(Theme.Dark.Danger, leave.Fill);
        Assert.Equal(Theme.Dark.DangerForeground, leave.Ink);
    }

    [Fact]
    public void AFillOfItsOwnMovesAsTheLooksWould()
    {
        // A chosen row: the tally at a fifth, stronger under the pointer.
        var chosen = Amber.WithAlpha(0.20f);
        var over = Looks.Paint(Look.Clear, Theme.Dark, Amber, OnAmber, Touch.Over, resting: chosen).Fill;
        Near(0.20f + 0.80f * 0.10f, over.Alpha, "chosen row over");
        // An opaque card (the narrow lobby's server card) moves towards the
        // foreground: as a wash at full strength it did not move at all.
        var card = Theme.Light.Surface;
        var pressed = Looks.Paint(Look.Clear, Theme.Light, Amber, OnAmber, Touch.Down, resting: card).Fill;
        Assert.Equal(Looks.Toward(card, Theme.Light.Foreground, 0.16f), pressed);
    }

    public static IEnumerable<object[]> EveryLook() => Enum.GetValues<Look>().Select(l => new object[] { l });

    private static float Distance(Shade a, Shade b) =>
        Math.Abs(a.Red * a.Alpha - b.Red * b.Alpha) + Math.Abs(a.Green * a.Alpha - b.Green * b.Alpha) + Math.Abs(a.Blue * a.Alpha - b.Blue * b.Alpha) + Math.Abs(a.Alpha - b.Alpha);

    internal static string Hex(Shade shade)
    {
        var (a, r, g, b) = shade.Bytes();
        return a == 255 ? $"#{r:X2}{g:X2}{b:X2}" : $"#{r:X2}{g:X2}{b:X2}@{a}";
    }

    /// <summary>A colour with alpha laid over an opaque ground.</summary>
    internal static Shade Over(Shade top, Shade ground) => new(
        top.Red * top.Alpha + ground.Red * (1 - top.Alpha),
        top.Green * top.Alpha + ground.Green * (1 - top.Alpha),
        top.Blue * top.Alpha + ground.Blue * (1 - top.Alpha));

    internal static double Luminance(Shade s)
    {
        static double Channel(float c) => c <= 0.04045 ? c / 12.92 : Math.Pow((c + 0.055) / 1.055, 2.4);
        return 0.2126 * Channel(s.Red) + 0.7152 * Channel(s.Green) + 0.0722 * Channel(s.Blue);
    }

    /// <summary>WCAG's contrast ratio between two opaque colours.</summary>
    internal static double Contrast(Shade a, Shade b)
    {
        var (high, low) = (Math.Max(Luminance(a), Luminance(b)), Math.Min(Luminance(a), Luminance(b)));
        return (high + 0.05) / (low + 0.05);
    }
}
