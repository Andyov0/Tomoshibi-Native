using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Windows.UI;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The palette as Windows brushes (Tomoshibi.Basics.Theme holds the
/// numbers). The tally is read where it is drawn, from the person's choice,
/// and the screens are drawn again when it changes.
/// </summary>
internal static class Ink
{
    public static Color Of(Shade shade)
    {
        var (a, r, g, b) = shade.Bytes();
        return Color.FromArgb(a, r, g, b);
    }

    public static SolidColorBrush Brush(Shade shade) => new(Of(shade));

    public static SolidColorBrush White(float alpha) => Brush(Shade.White.WithAlpha(alpha));
    public static SolidColorBrush Black(float alpha) => Brush(Shade.Black.WithAlpha(alpha));

    public static SolidColorBrush Background => Brush(Theme.Background);
    public static SolidColorBrush Surface => Brush(Theme.Surface);
    public static SolidColorBrush SurfaceHigh => Brush(Theme.SurfaceHigh);
    public static SolidColorBrush Border => Brush(Theme.Border);
    public static SolidColorBrush Foreground => Brush(Theme.Foreground);
    public static SolidColorBrush ForegroundAt(float alpha) => Brush(Theme.Foreground.WithAlpha(alpha));
    public static SolidColorBrush Muted => Brush(Theme.Muted);
    public static SolidColorBrush MutedAt(float times) => Brush(Theme.Muted.WithAlpha(Theme.Muted.Alpha * times));
    public static SolidColorBrush Good => Brush(Theme.Good);
    public static SolidColorBrush Danger => Brush(Theme.Danger);
    public static SolidColorBrush DangerAt(float alpha) => Brush(Theme.Danger.WithAlpha(alpha));
    public static SolidColorBrush Transparent => new(Color.FromArgb(0, 0, 0, 0));

    public static Shade TallyShade => Appearance.Shared.Tally;
    public static SolidColorBrush Tally => Brush(Appearance.Shared.Tally);
    public static SolidColorBrush TallyAt(float alpha) => Brush(Appearance.Shared.Tally.WithAlpha(alpha));
    public static SolidColorBrush TallyForeground => Brush(Appearance.Shared.TallyForeground);

    /// <summary>ServerPicker.tsx <c>Latency</c>: green to 150 ms, the signal colour to 400, red beyond.</summary>
    public static SolidColorBrush Latency(double ms) => ms <= 150 ? Good : ms <= 400 ? Tally : Danger;

    /// <summary>
    /// Brings the Windows controls' accent to the tally: the shared brushes
    /// every ToggleSwitch, text field and progress ring reads are changed in
    /// place, so that controls already on the screen follow as well as new
    /// ones. The accent the system chose for itself means nothing in a call
    /// whose one signal colour the person picked.
    /// </summary>
    public static void ApplyAccent()
    {
        var tally = Of(Appearance.Shared.Tally);
        var on = Of(Appearance.Shared.TallyForeground);
        var resources = Application.Current.Resources;
        void Paint(string key, Color color)
        {
            try
            {
                if (resources.TryGetValue(key, out var found) && found is SolidColorBrush brush) brush.Color = color;
                else resources[key] = new SolidColorBrush(color);
            }
            catch (Exception)
            {
                // A key this version of the controls does not have.
            }
        }
        foreach (var key in new[]
        {
            "AccentFillColorDefaultBrush", "AccentFillColorSecondaryBrush", "AccentFillColorTertiaryBrush",
            "AccentTextFillColorPrimaryBrush", "AccentTextFillColorSecondaryBrush", "AccentTextFillColorTertiaryBrush",
            "ToggleSwitchFillOn", "ToggleSwitchFillOnPointerOver", "ToggleSwitchFillOnPressed",
            "TextControlBorderBrushFocused", "TextControlElevationBorderFocusedBrush",
            "ProgressRingForegroundThemeBrush",
        })
        {
            Paint(key, tally);
        }
        Paint("TextOnAccentFillColorPrimaryBrush", on);
        resources["SystemAccentColor"] = tally;
        resources["SystemAccentColorLight1"] = tally;
        resources["SystemAccentColorLight2"] = tally;
        resources["SystemAccentColorLight3"] = tally;
        resources["SystemAccentColorDark1"] = tally;
    }
}
