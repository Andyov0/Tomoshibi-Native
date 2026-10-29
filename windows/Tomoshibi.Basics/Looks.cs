namespace Tomoshibi.Basics;

/// <summary>
/// What a button wears (Android's GlassButton, prominent and destructive;
/// the bar's BarButton; a quiet one in a list). Here rather than in the
/// app's kit so that what each state draws is a rule the tests can hold,
/// as the palette is.
/// </summary>
public enum Look
{
    /// <summary>The apps' wash over whatever is behind (white in the dark scheme, black in the light), with a hairline edge.</summary>
    Plain,
    /// <summary>The tally, with its own ink: the one thing on the screen to press (start, join, sign in, arrange, let in).</summary>
    Prominent,
    /// <summary>Red, for leaving and ending, and for nothing else.</summary>
    Destructive,
    /// <summary>Nothing until the pointer is over it: a row, a link, a close mark.</summary>
    Clear,
    /// <summary>The call's bar: a stronger wash, lit in the tally while something is happening.</summary>
    Bar,
    /// <summary>
    /// Plain, on a picture (a switch over the camera): the dark scheme's
    /// white wash and ink in either scheme, as everything laid on a picture
    /// is, where the light scheme's black wash and dark ink were lost on a
    /// dark picture.
    /// </summary>
    Picture,
    /// <summary>
    /// A microphone or a camera that is off: a solid plate of the
    /// foreground (white by night, near-black by day) with the symbol in the
    /// page's colour, as FaceTime and the phones' own call screens draw a
    /// muted microphone. It was red before a call and a faint grey in it,
    /// one switch drawn two ways a screen apart; red is for leaving and for
    /// faults, and a muted microphone is a choice (the owner, 2026-10-29).
    /// </summary>
    Off,
    /// <summary>
    /// <see cref="Off"/> laid on a picture: the dark scheme's plate and ink
    /// in either scheme, as <see cref="Picture"/> is to <see cref="Plain"/>.
    /// </summary>
    OffOnPicture,
}

/// <summary>Where the pointer and the press are on a button.</summary>
public enum Touch { Rest, Over, Down }

/// <summary>Everything a button's state decides about how it is drawn.</summary>
public readonly record struct Painted(Shade Fill, Shade? Edge, Shade Ink, float ContentOpacity, float Scale);

/// <summary>
/// How every button moves between its states, one rule for all of them
/// (the web's ui-polish, section 3; the native audit's state table): under
/// the pointer its fill moves a tenth of the way towards the foreground,
/// pressed sixteen hundredths and a little smaller, and where it cannot be
/// pressed it is drawn at four tenths. Each look had numbers of its own
/// before (a hover of six, eight or ten, a press that darkened one and
/// lightened another, three press scales and two dimmings), which is why a
/// hover was hard to see on some and a press on others.
///
/// Two states are not dimmings of the rest. A button whose panel is open
/// (<c>held</c>) is drawn pressed and stays so under the pointer, the
/// neutral grey a press is, not the tally: the tally means something is
/// happening in the call (a share going out, a hand up, a word unread,
/// somebody speaking, somebody at the door), and a panel somebody opened is
/// none of those. A prominent button that cannot be pressed is a grey plate
/// with grey words rather than a faded tally: the tally at four tenths over
/// the dark ground was a brown with its dark arrow at 1.55:1 on it, still
/// reading as the button to press, only weaker.
/// </summary>
public static class Looks
{
    /// <summary>How far towards the foreground a fill moves under the pointer.</summary>
    public const float Hover = 0.10f;

    /// <summary>How far it moves while pressed.</summary>
    public const float Press = 0.16f;

    /// <summary>How large a button is drawn while pressed (the web's <c>pressable</c>).</summary>
    public const float PressedScale = 0.95f;

    /// <summary>How strongly a button that cannot be pressed is drawn, the prominent one apart.</summary>
    public const float Disabled = 0.40f;

    /// <summary>A wash of the foreground's own colour moved towards it: what lies under it is that much more covered.</summary>
    public static float WashToward(float alpha, float by) => alpha + (1 - alpha) * by;

    /// <summary>An opaque colour moved towards another by a fraction of the way, its alpha kept.</summary>
    public static Shade Toward(Shade from, Shade to, float by) => new(
        from.Red + (to.Red - from.Red) * by,
        from.Green + (to.Green - from.Green) * by,
        from.Blue + (to.Blue - from.Blue) * by,
        from.Alpha);

    /// <summary>
    /// A fill moved a step towards the foreground: an opaque one mixed with
    /// it, a translucent wash made that much stronger (a wash is the
    /// foreground's own colour, or a colour of its own like a chosen row's).
    /// </summary>
    public static Shade Step(Shade fill, Shade foreground, float by) =>
        fill.Alpha >= 0.999f ? Toward(fill, foreground, by) : fill.WithAlpha(WashToward(fill.Alpha, by));

    /// <summary>
    /// What a button of this look draws in this state, in <paramref name="palette"/>
    /// with the person's <paramref name="tally"/> and the ink drawn on it.
    /// <paramref name="lit"/> is a bar button whose thing is happening (the
    /// tally); <paramref name="held"/> one whose panel is open (pressed, not
    /// the tally); <paramref name="resting"/> a fill of its own at rest (a
    /// chosen row), which moves as the look's would.
    /// </summary>
    public static Painted Paint(Look look, Palette palette, Shade tally, Shade onTally, Touch touch, bool enabled = true, bool lit = false, bool held = false, Shade? resting = null)
    {
        var wash = palette.Wash;
        var dark = Theme.Dark;
        Shade rest;
        Shade? edge;
        Shade ink;
        // What the fill moves towards: the foreground, except where the fill
        // is the foreground already (a switch that is off), where it moves
        // the other way, towards the page, as the web's off switch does.
        Shade towards = palette.Foreground;
        switch (look)
        {
            case Look.Prominent:
                (rest, edge, ink) = (tally, null, onTally);
                break;
            case Look.Destructive:
                (rest, edge, ink) = (palette.Danger, null, palette.DangerForeground);
                break;
            case Look.Clear:
                (rest, edge, ink) = (wash.WithAlpha(0f), null, palette.Foreground);
                break;
            case Look.Bar when lit:
                (rest, edge, ink) = (tally, Shade.White.WithAlpha(0.3f), onTally);
                break;
            case Look.Bar:
                (rest, edge, ink) = (wash.WithAlpha(0.12f), wash.WithAlpha(0.08f), palette.Foreground);
                break;
            case Look.Picture:
                (rest, edge, ink) = (Shade.White.WithAlpha(0.07f), Shade.White.WithAlpha(0.09f), dark.Foreground);
                towards = dark.Foreground;
                break;
            case Look.Off:
                (rest, edge, ink) = (palette.Foreground, null, palette.Background);
                towards = palette.Background;
                break;
            case Look.OffOnPicture:
                (rest, edge, ink) = (dark.Foreground, null, dark.Background);
                towards = dark.Background;
                break;
            default:
                (rest, edge, ink) = (wash.WithAlpha(0.07f), wash.WithAlpha(0.09f), palette.Foreground);
                break;
        }
        if (resting is { } own) rest = own;
        // Held: drawn pressed, and kept so under the pointer and the press.
        if (held && !lit) rest = Step(rest, towards, Press);

        if (!enabled)
        {
            if (look == Look.Prominent && resting == null)
                return new Painted(palette.SurfaceHigh, null, palette.Muted, 1f, 1f);
            return new Painted(rest.WithAlpha(rest.Alpha * Disabled), edge?.WithAlpha(edge.Value.Alpha * Disabled), ink, Disabled, 1f);
        }
        var fill = held && !lit ? rest : touch switch
        {
            Touch.Over => Step(rest, towards, Hover),
            Touch.Down => Step(rest, towards, Press),
            _ => rest,
        };
        return new Painted(fill, edge, ink, 1f, touch == Touch.Down ? PressedScale : 1f);
    }
}
