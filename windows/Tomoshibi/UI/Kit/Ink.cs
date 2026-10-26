using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Windows.UI;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// The palette as Windows brushes (Tomoshibi.Basics.Theme holds the
/// numbers), in the scheme being drawn: the system's for the screens before
/// and after a call, the meeting's for the call (Daylight), set by the
/// screen host as each screen is made and by the window when the system's
/// choice changes; what is drawn reads it then, and is drawn again when it
/// changes, as it is for the tally and the language. The tally is read
/// where it is drawn, from the person's choice.
/// </summary>
internal static class Ink
{
    /// <summary>The scheme the interface is being drawn in.</summary>
    public static Scheme Scheme { get; private set; } = Scheme.Dark;

    /// <summary>Said once the scheme has changed (the title bar's buttons are painted again then).</summary>
    public static event Action? SchemeChanged;

    public static void Use(Scheme scheme)
    {
        if (scheme == Scheme) return;
        Scheme = scheme;
        SchemeChanged?.Invoke();
    }

    public static bool Light => Scheme == Scheme.Light;

    /// <summary>The XAML theme that goes with the scheme, for the Windows controls' own colours.</summary>
    public static ElementTheme Element => Light ? ElementTheme.Light : ElementTheme.Dark;

    /// <summary>The numbers of the scheme being drawn.</summary>
    public static Palette Palette => Theme.Of(Scheme);

    public static Color Of(Shade shade)
    {
        var (a, r, g, b) = shade.Bytes();
        return Color.FromArgb(a, r, g, b);
    }

    public static SolidColorBrush Brush(Shade shade) => new(Of(shade));

    /// <summary>
    /// White and black as they are, for what sits on a picture (a name pill
    /// over a camera, the scrim under a video's controls): a picture is the
    /// same in either scheme, and so is what is laid over it.
    /// </summary>
    public static SolidColorBrush White(float alpha) => Brush(Shade.White.WithAlpha(alpha));
    public static SolidColorBrush Black(float alpha) => Brush(Shade.Black.WithAlpha(alpha));

    /// <summary>Words on a picture or on the dark pill over one: the dark scheme's ink, in either scheme.</summary>
    public static SolidColorBrush OnPicture => Brush(Theme.Dark.Foreground);
    public static SolidColorBrush MutedOnPicture => Brush(Theme.Dark.Muted);

    /// <summary>
    /// The chrome's wash: the fills, hairlines and edges the dark scheme
    /// draws in white over its ground, and the light one in black, at the
    /// same strength, as the palette's border is.
    /// </summary>
    public static Shade WashShade(float alpha) => Palette.Wash.WithAlpha(alpha);
    public static SolidColorBrush Wash(float alpha) => Brush(WashShade(alpha));

    /// <summary>
    /// A well sunk into the chrome (a field's box, a segmented track, a
    /// switch's track): black over the dark ground; in the light scheme the
    /// raised surface's grey, #E5E5EA, which the palette keeps for fills
    /// inside a surface, where black at the dark scheme's strength was a hole
    /// in the page.
    /// </summary>
    public static Shade WellShade(float alpha) => Light ? Palette.SurfaceHigh : Shade.Black.WithAlpha(alpha);
    public static SolidColorBrush Well(float alpha) => Brush(WellShade(alpha));

    /// <summary>
    /// The dim behind a dialog: black in either scheme, two thirds as dark
    /// over the light one, which a dark veil turned grey and dirty.
    /// </summary>
    public static SolidColorBrush Dim(float alpha) => Black(Light ? alpha * 2f / 3f : alpha);

    public static SolidColorBrush Background => Brush(Palette.Background);
    public static SolidColorBrush BackgroundAt(float alpha) => Brush(Palette.Background.WithAlpha(alpha));
    public static SolidColorBrush Surface => Brush(Palette.Surface);
    public static SolidColorBrush SurfaceHigh => Brush(Palette.SurfaceHigh);
    public static SolidColorBrush Border => Brush(Palette.Border);
    public static SolidColorBrush Foreground => Brush(Palette.Foreground);
    public static SolidColorBrush ForegroundAt(float alpha) => Brush(Palette.Foreground.WithAlpha(alpha));
    public static SolidColorBrush Muted => Brush(Palette.Muted);
    /// <summary>
    /// The muted grey fainter still (a chevron, a symbol withheld): so in the
    /// dark scheme; in the light one the muted grey itself, since anything
    /// fainter falls under 3:1 on the light ground (daylight palette,
    /// addendum 2).
    /// </summary>
    public static SolidColorBrush MutedAt(float times) => Light && times < 1 ? Muted : Brush(Palette.Muted.WithAlpha(Palette.Muted.Alpha * times));
    public static SolidColorBrush Good => Brush(Palette.Good);
    public static SolidColorBrush Danger => Brush(Palette.Danger);
    public static SolidColorBrush DangerAt(float alpha) => Brush(Palette.Danger.WithAlpha(alpha));
    public static SolidColorBrush DangerForeground => Brush(Palette.DangerForeground);

    /// <summary>The green and the red as words or a thin symbol (a latency figure, an error under a form); <see cref="Good"/> and <see cref="Danger"/> are for dots, bars and fills.</summary>
    public static SolidColorBrush GoodText => Brush(Palette.GoodText);
    public static SolidColorBrush DangerText => Brush(Palette.DangerText);
    public static SolidColorBrush Transparent => new(Color.FromArgb(0, 0, 0, 0));

    public static Shade TallyShade => Appearance.Shared.TallyIn(Scheme);
    public static SolidColorBrush Tally => Brush(TallyShade);
    public static SolidColorBrush TallyAt(float alpha) => Brush(TallyShade.WithAlpha(alpha));
    public static SolidColorBrush TallyForeground => Brush(Appearance.Shared.TallyForeground);

    /// <summary>
    /// The tally as words or a thin symbol on the ground: the tally itself in
    /// the dark scheme, a deeper ink of it in the light one (Accents.TextColor).
    /// Fills, rings, bars and dots take <see cref="Tally"/>.
    /// </summary>
    public static Shade TallyTextShade => Appearance.Shared.TallyTextIn(Scheme);
    public static SolidColorBrush TallyText => Brush(TallyTextShade);

    /// <summary>ServerPicker.tsx <c>Latency</c>: green to 150 ms, the signal colour to 400, red beyond; drawn as a figure, so the tally's ink.</summary>
    public static SolidColorBrush Latency(double ms) => ms <= 150 ? GoodText : ms <= 400 ? TallyText : DangerText;

    /// <summary>
    /// Brings the Windows controls' accent to the tally: the shared brushes
    /// every ToggleSwitch, text field and progress ring reads are changed in
    /// place, so that controls already on the screen follow as well as new
    /// ones. The accent the system chose for itself means nothing in a call
    /// whose one signal colour the person picked. Each theme's brushes are
    /// painted in that scheme's tally, so that a switch in the light settings
    /// and one in the dark call are each lit in their own: App.xaml's, which
    /// the controls' templates find first, and the controls' own, which the
    /// controls' other resources are made of (the calendar's ring round the
    /// chosen day is drawn in the ink on the accent, and drawn in the
    /// system's black when only App.xaml's was painted). A brush the controls
    /// share between their themes (the calendar's chosen day is one) takes the
    /// tally of the scheme on the screen, and is painted again when that
    /// changes: painted for each theme in turn it kept the last, and the dark
    /// lobby's calendar wore the light tally.
    /// </summary>
    public static void ApplyAccent()
    {
        var on = Of(Appearance.Shared.TallyForeground);
        var found = new Dictionary<SolidColorBrush, (bool Dark, bool Light, bool Ink)>(ReferenceEqualityComparer.Instance);
        // The light theme's first, so that a shared brush found twice over
        // (as two wrappers of one brush) is painted for the dark one last,
        // as the dark scheme always had it.
        Collect(Application.Current.Resources, new[] { "Light" }, Scheme.Light, found, 0);
        Collect(Application.Current.Resources, new[] { "Dark", "Default" }, Scheme.Dark, found, 0);
        var dark = Of(Appearance.Shared.TallyIn(Scheme.Dark));
        var light = Of(Appearance.Shared.TallyIn(Scheme.Light));
        var now = Of(TallyShade);
        foreach (var (brush, (inDark, inLight, onAccent)) in found)
        {
            brush.Color = onAccent ? on : inDark && inLight ? now : inLight ? light : dark;
        }
        // The system accent's colours, which the controls' other brushes are
        // made from as they are made, are one set for the app: the tally of
        // the scheme on the screen, set again when that changes.
        var app = Application.Current.Resources;
        foreach (var key in new[] { "SystemAccentColor", "SystemAccentColorLight1", "SystemAccentColorLight2", "SystemAccentColorLight3", "SystemAccentColorDark1" })
        {
            app[key] = now;
        }
    }

    private static readonly string[] AccentKeys =
    {
        "AccentFillColorDefaultBrush", "AccentFillColorSecondaryBrush", "AccentFillColorTertiaryBrush",
        "AccentTextFillColorPrimaryBrush", "AccentTextFillColorSecondaryBrush", "AccentTextFillColorTertiaryBrush",
        "ToggleSwitchFillOn", "ToggleSwitchFillOnPointerOver", "ToggleSwitchFillOnPressed",
        "TextControlBorderBrushFocused", "TextControlElevationBorderFocusedBrush",
        "ProgressRingForegroundThemeBrush",
    };

    /// <summary>The accent's brushes under one theme's names in a dictionary and in every dictionary it merges.</summary>
    private static void Collect(ResourceDictionary dictionary, string[] themes, Scheme scheme, Dictionary<SolidColorBrush, (bool Dark, bool Light, bool Ink)> found, int depth)
    {
        if (depth > 6) return;
        try
        {
            foreach (var theme in themes)
            {
                if (!dictionary.ThemeDictionaries.TryGetValue(theme, out var any) || any is not ResourceDictionary themed) continue;
                void Add(string key, bool ink)
                {
                    if (!themed.TryGetValue(key, out var brush) || brush is not SolidColorBrush solid) return;
                    (bool Dark, bool Light, bool Ink) was = found.TryGetValue(solid, out var w) ? w : (false, false, ink);
                    found[solid] = (was.Dark || scheme == Scheme.Dark, was.Light || scheme == Scheme.Light, was.Ink);
                }
                foreach (var key in AccentKeys) Add(key, false);
                Add("TextOnAccentFillColorPrimaryBrush", true);
            }
            foreach (var merged in dictionary.MergedDictionaries) Collect(merged, themes, scheme, found, depth + 1);
        }
        catch (Exception)
        {
            // A dictionary this version of the controls lays out otherwise.
        }
    }
}
