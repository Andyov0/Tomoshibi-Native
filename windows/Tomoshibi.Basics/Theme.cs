namespace Tomoshibi.Basics;

/// <summary>A colour as the other apps write theirs: red, green, blue and alpha from nought to one.</summary>
public readonly record struct Shade(float Red, float Green, float Blue, float Alpha = 1f)
{
    public Shade WithAlpha(float alpha) => this with { Alpha = alpha };

    /// <summary>The colour as four bytes, rounded as Compose rounds a float colour to eight bits.</summary>
    public (byte A, byte R, byte G, byte B) Bytes() => (Byte(Alpha), Byte(Red), Byte(Green), Byte(Blue));

    private static byte Byte(float channel) => (byte)Math.Clamp((int)MathF.Round(channel * 255f), 0, 255);

    public static readonly Shade White = new(1f, 1f, 1f);
    public static readonly Shade Black = new(0f, 0f, 0f);
}

/// <summary>Which of the two palettes something is drawn in.</summary>
public enum Scheme { Dark, Light }

/// <summary>
/// The neutrals and the two signal colours of one scheme, under the names
/// every platform gives them (daylight palette, 2026-10-26), and the wash the
/// chrome's fills, hairlines and edges are drawn in: white over the dark
/// ground, black over the light one, at the same strength, as the border is.
/// The green and the red as words (a latency figure, an error under a form)
/// are their own ink in the light scheme, deeper than the light and the
/// button they colour, which read at about 2:1 and 3:1 as words on the light
/// ground; in the dark scheme they are the colours themselves.
/// </summary>
public sealed record Palette(
    Shade Background,
    Shade Surface,
    Shade SurfaceHigh,
    Shade Border,
    Shade Foreground,
    Shade Muted,
    Shade Good,
    Shade Danger,
    Shade DangerForeground,
    Shade Wash,
    Shade GoodText,
    Shade DangerText);

/// <summary>
/// The apps' palette and motion, the same numbers as Theme.swift and
/// Android's basics/Theme.kt.
///
/// Two palettes, the dark one as it always was and a light one with the same
/// structure (the owner, 2026-10-26: keep this interface, add a day mode, day
/// and night following the system). The call itself stays dark in either
/// (<see cref="Daylight"/>): a call is looked at for an hour at a stretch and
/// the pictures are the only thing in it that should be bright. The neutrals
/// are Apple's own greys, so that the same app looks the same on a machine
/// of any kind. One signal colour, the "tally" a studio camera lights when it
/// is live, chosen by the person (Accent); red appears on leaving and on
/// errors and nowhere else; green only on a connection light.
/// </summary>
public static class Theme
{
    // The dark palette's numbers, unchanged since the apps began: every
    // picture of a dark screen is to be the same after the day mode as
    // before it.
    public static readonly Shade Background = new(0.043f, 0.043f, 0.051f);
    public static readonly Shade Surface = new(0.110f, 0.110f, 0.118f);
    public static readonly Shade SurfaceHigh = new(0.173f, 0.173f, 0.180f);
    public static readonly Shade Border = Shade.White.WithAlpha(0.10f);
    public static readonly Shade Foreground = new(0.961f, 0.961f, 0.969f);
    public static readonly Shade Muted = new(0.922f, 0.922f, 0.961f, 0.6f);
    public static readonly Shade Good = new(0.188f, 0.820f, 0.345f);
    public static readonly Shade Danger = new(1.0f, 0.271f, 0.227f);
    public static readonly Shade DangerForeground = Shade.White;

    public static readonly Palette Dark = new(Background, Surface, SurfaceHigh, Border, Foreground, Muted, Good, Danger, DangerForeground, Shade.White, Good, Danger);

    /// <summary>
    /// The light palette (#F2F2F7, #FFFFFF, #E5E5EA; black at 10 per cent
    /// for the border; #1C1C1E and rgb(60, 60, 67) at 75 per cent for the
    /// words; #34C759 and #FF3B30, and #217E38 and #D70015 as words), the
    /// same numbers on every platform. The muted words were Apple's 60 per
    /// cent, which on the light ground and its white cards read at 3.1 to
    /// 3.4:1, under the 4.5:1 small print needs; at 75 they reach it (the
    /// web's ui-polish, 2026-10-29). The dark palette's muted grey already
    /// reads at over 6:1 and is as it was.
    /// </summary>
    public static readonly Palette Light = new(
        Background: Hex(0xF2F2F7),
        Surface: Hex(0xFFFFFF),
        SurfaceHigh: Hex(0xE5E5EA),
        Border: Shade.Black.WithAlpha(0.10f),
        Foreground: Hex(0x1C1C1E),
        Muted: Hex(0x3C3C43, 0.75f),
        Good: Hex(0x34C759),
        Danger: Hex(0xFF3B30),
        DangerForeground: Shade.White,
        Wash: Shade.Black,
        GoodText: Hex(0x217E38),
        DangerText: Hex(0xD70015));

    public static Palette Of(Scheme scheme) => scheme == Scheme.Light ? Light : Dark;

    /// <summary>A colour written as the palette writes it, #RRGGBB.</summary>
    public static Shade Hex(int rgb, float alpha = 1f) =>
        new(((rgb >> 16) & 0xFF) / 255f, ((rgb >> 8) & 0xFF) / 255f, (rgb & 0xFF) / 255f, alpha);

    /// <summary>The tile's corner, 0.5rem.</summary>
    public const float TileRadius = 8f;

    /// <summary>Arrivals decelerate into place (--ease-settle), over <see cref="SettleMs"/>.</summary>
    public static readonly (float X1, float Y1, float X2, float Y2) SettleCurve = (0.2f, 0.9f, 0.3f, 1f);

    /// <summary>Departures need no such courtesy (--ease-leave), over <see cref="LeaveMs"/>.</summary>
    public static readonly (float X1, float Y1, float X2, float Y2) LeaveCurve = (0.4f, 0f, 1f, 1f);

    public const int SettleMs = 320;
    public const int LeaveMs = 200;

    /// <summary>What an arrival grows from: 97 per cent of its size, fading up.</summary>
    public const float ArriveScale = 0.97f;

    /// <summary>
    /// The screens before a call bring their pieces in from fourteen points
    /// below, over 420 ms on the arriving curve (Android's <c>rise</c>).
    /// </summary>
    public const int RiseMs = 420;
    public const float RiseLift = 14f;

    /// <summary>A picture shown whole sits on a blurred copy of itself, taken again this often and crossfaded over this long, darkened by this much (RemoteVideoView).</summary>
    public const int GlimpseEveryMs = 1000;
    public const int GlimpseFadeMs = 600;
    public const float GlimpseDarken = 0.35f;
}

/// <summary>
/// Which palette each part of the app is drawn in. Everything follows the
/// system's light or dark choice for apps, as it changes, except the meeting
/// itself: the room, its bar and whatever is opened from inside a call stay
/// dark in either, as FaceTime and Meet do, since the pictures are what a
/// call is for and a white frame round them outshines them.
/// </summary>
public static class Daylight
{
    /// <summary>
    /// The one switch for the meeting's appearance: true keeps the call dark
    /// whatever the system says; false lets it follow the system like the
    /// rest of the app. Every piece of the call takes its palette from here.
    /// </summary>
    public const bool MeetingStaysDark = true;

    /// <summary>The scheme the meeting is drawn in, given the system's.</summary>
    public static Scheme ForMeeting(Scheme system) => MeetingStaysDark ? Scheme.Dark : system;
}

/// <summary>
/// The colours on offer for the tally, each with the colour drawn on top of
/// it: Apple's dark-appearance system colours apart from the first, which is
/// the product's, the same on every platform so that somebody's choice looks
/// the same on each of their machines (Appearance.swift, Android's
/// basics/Appearance.kt).
/// </summary>
public enum Accent
{
    Amber, Orange, Pink, Purple, Indigo, Blue, Teal, Green, Graphite,
}

public static class Accents
{
    public static readonly IReadOnlyList<Accent> All = Enum.GetValues<Accent>();

    public static string Key(this Accent accent) => accent.ToString().ToLowerInvariant();

    public static Accent? Of(string? key) => All.Cast<Accent?>().FirstOrDefault(a => a!.Value.Key() == key);

    /// <summary>The colour in the dark scheme, or in the light one when asked (Apple's light-appearance system colours, the first the product's).</summary>
    public static Shade Color(this Accent accent, Scheme scheme) => scheme == Scheme.Light ? accent.LightColor() : accent.Color();

    /// <summary>The light scheme's tally: the same hue a step deeper, so that it still reads on a white ground.</summary>
    public static Shade LightColor(this Accent accent) => accent switch
    {
        Accent.Amber => Theme.Hex(0xF2901E),
        Accent.Orange => Theme.Hex(0xFF9500),
        Accent.Pink => Theme.Hex(0xFF2D55),
        Accent.Purple => Theme.Hex(0xAF52DE),
        Accent.Indigo => Theme.Hex(0x5856D6),
        Accent.Blue => Theme.Hex(0x007AFF),
        Accent.Teal => Theme.Hex(0x30B0C7),
        Accent.Green => Theme.Hex(0x34C759),
        _ => Theme.Hex(0x8E8E93),
    };

    /// <summary>
    /// The tally as words or a thin symbol on the ground (a link, a latency
    /// figure, a tick on a chosen row, the colour's own name): the colour
    /// itself in the dark scheme; in the light one a deeper ink of the same
    /// hue that reads at 4.5:1 on the light ground, where several of the light
    /// fills read under 3:1 there as words, the lamplight about 2.3:1
    /// (daylight palette, addendum). Fills keep <see cref="Color(Accent, Scheme)"/>.
    /// </summary>
    public static Shade TextColor(this Accent accent, Scheme scheme) => scheme == Scheme.Light ? accent switch
    {
        Accent.Amber => Theme.Hex(0xA35C09),
        Accent.Orange => Theme.Hex(0xA15E00),
        Accent.Pink => Theme.Hex(0xDF002B),
        Accent.Purple => Theme.Hex(0xA338D9),
        Accent.Indigo => Theme.Hex(0x5856D6),
        Accent.Blue => Theme.Hex(0x006BE0),
        Accent.Teal => Theme.Hex(0x217989),
        Accent.Green => Theme.Hex(0x217E38),
        _ => Theme.Hex(0x6E6E73),
    } : accent.Color();

    public static Shade Color(this Accent accent) => accent switch
    {
        Accent.Amber => new(1.0f, 0.635f, 0.231f),
        Accent.Orange => new(1.0f, 0.624f, 0.039f),
        Accent.Pink => new(1.0f, 0.216f, 0.373f),
        Accent.Purple => new(0.749f, 0.353f, 0.949f),
        Accent.Indigo => new(0.369f, 0.361f, 0.902f),
        Accent.Blue => new(0.039f, 0.518f, 1.0f),
        Accent.Teal => new(0.251f, 0.784f, 0.878f),
        Accent.Green => new(0.188f, 0.820f, 0.345f),
        _ => new(0.596f, 0.596f, 0.616f),
    };

    /// <summary>What is drawn on the tally, in either scheme: each light colour keeps the ink its dark one has, which reads on it as well or better.</summary>
    public static Shade OnColor(this Accent accent) => accent switch
    {
        Accent.Amber or Accent.Orange => new(0.122f, 0.071f, 0.0f),
        Accent.Teal => new(0.0f, 0.15f, 0.18f),
        Accent.Green => new(0.0f, 0.17f, 0.055f),
        Accent.Graphite => new(0.07f, 0.07f, 0.08f),
        _ => Shade.White,
    };

    /// <summary>The colour's name, in the reader's language.</summary>
    public static string DisplayName(this Accent accent) => accent switch
    {
        Accent.Amber => Words.T("Lamplight"),
        Accent.Orange => Words.T("Orange"),
        Accent.Pink => Words.T("Pink"),
        Accent.Purple => Words.T("Purple"),
        Accent.Indigo => Words.T("Indigo"),
        Accent.Blue => Words.T("Blue"),
        Accent.Teal => Words.T("Teal"),
        Accent.Green => Words.T("Green"),
        _ => Words.T("Graphite"),
    };
}

/// <summary>How the connection is shown in a call: the bars alone unless asked.</summary>
public enum ConnectionBadge { Signal, Detailed, Hidden }

/// <summary>The three places the web lets the controls sit (controls.ts).</summary>
public enum ControlsPlacement { Always, Idle, Side }

public static class Choices
{
    public static string Key(this ConnectionBadge badge) => badge.ToString().ToLowerInvariant();

    public static ConnectionBadge? BadgeOf(string? key) =>
        Enum.GetValues<ConnectionBadge>().Cast<ConnectionBadge?>().FirstOrDefault(b => b!.Value.Key() == key);

    public static string DisplayName(this ConnectionBadge badge) => badge switch
    {
        ConnectionBadge.Signal => Words.T("Signal only"),
        ConnectionBadge.Detailed => Words.T("With details"),
        _ => Words.T("Hidden"),
    };

    public static string Key(this ControlsPlacement placement) => placement.ToString().ToLowerInvariant();

    public static ControlsPlacement? PlacementOf(string? key) =>
        Enum.GetValues<ControlsPlacement>().Cast<ControlsPlacement?>().FirstOrDefault(p => p!.Value.Key() == key);

    public static string DisplayName(this ControlsPlacement placement) => placement switch
    {
        ControlsPlacement.Always => Words.T("Always shown"),
        ControlsPlacement.Idle => Words.T("Hide when nothing is happening"),
        _ => Words.T("At the side"),
    };
}

/// <summary>
/// The colour the interface spends and the two other choices about how a
/// call looks (Appearance.swift; Android's basics/Appearance.kt), under the
/// same keys, and the controls under the web's own name.
/// </summary>
public sealed class Appearance
{
    public const string AccentKey = "tomoshibi.accent";
    public const string BadgeKey = "tomoshibi.connection-badge";
    public const string ControlsKey = "meet-live.controls";

    private readonly IDefaults? _defaults;

    public event Action? Changed;

    public Appearance(IDefaults? defaults)
    {
        _defaults = defaults;
        Accent = Accents.Of(defaults?.String(AccentKey)) ?? Accent.Amber;
        ConnectionBadge = Choices.BadgeOf(defaults?.String(BadgeKey)) ?? ConnectionBadge.Signal;
        ControlsPlacement = Choices.PlacementOf(defaults?.String(ControlsKey)) ?? ControlsPlacement.Always;
    }

    public static Appearance Shared { get; set; } = new(null);

    public Accent Accent { get; private set; }
    public ConnectionBadge ConnectionBadge { get; private set; }
    public ControlsPlacement ControlsPlacement { get; private set; }

    /// <summary>Whether choices are written down; a test's are not, since the defaults it would write are the owner's.</summary>
    public bool Keeps { get; set; } = true;

    public Shade Tally => Accent.Color();
    public Shade TallyIn(Scheme scheme) => Accent.Color(scheme);
    public Shade TallyTextIn(Scheme scheme) => Accent.TextColor(scheme);
    public Shade TallyForeground => Accent.OnColor();

    public void Choose(Accent accent)
    {
        Accent = accent;
        Keep(AccentKey, accent.Key());
    }

    public void Choose(ConnectionBadge badge)
    {
        ConnectionBadge = badge;
        Keep(BadgeKey, badge.Key());
    }

    /// <summary>"always" is kept as nothing, as the web keeps it.</summary>
    public void Choose(ControlsPlacement placement)
    {
        ControlsPlacement = placement;
        Keep(ControlsKey, placement == ControlsPlacement.Always ? null : placement.Key());
    }

    private void Keep(string key, string? value)
    {
        if (Keeps && _defaults != null)
        {
            if (value == null) _defaults.Remove(key);
            else _defaults.Set(key, value);
        }
        Changed?.Invoke();
    }
}
