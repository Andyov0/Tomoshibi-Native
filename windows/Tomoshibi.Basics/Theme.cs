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

/// <summary>
/// The apps' palette and motion, the same numbers as Theme.swift and
/// Android's basics/Theme.kt.
///
/// Dark in every system appearance, on purpose: a call is looked at for an
/// hour at a stretch and the pictures are the only thing in it that should
/// be bright. The neutrals are Apple's own dark greys, so that the same app
/// looks the same on a machine of any kind. One signal colour, the "tally" a
/// studio camera lights when it is live, chosen by the person (Accent); red
/// appears on leaving and on errors and nowhere else; green only on a
/// connection light.
/// </summary>
public static class Theme
{
    public static readonly Shade Background = new(0.043f, 0.043f, 0.051f);
    public static readonly Shade Surface = new(0.110f, 0.110f, 0.118f);
    public static readonly Shade SurfaceHigh = new(0.173f, 0.173f, 0.180f);
    public static readonly Shade Border = Shade.White.WithAlpha(0.10f);
    public static readonly Shade Foreground = new(0.961f, 0.961f, 0.969f);
    public static readonly Shade Muted = new(0.922f, 0.922f, 0.961f, 0.6f);
    public static readonly Shade Good = new(0.188f, 0.820f, 0.345f);
    public static readonly Shade Danger = new(1.0f, 0.271f, 0.227f);
    public static readonly Shade DangerForeground = Shade.White;

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
