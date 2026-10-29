namespace Tomoshibi.Basics;

/// <summary>
/// The sizes the four apps and the web draw to (the web's ui-polish,
/// section 3; the native audit's section 5): one number for each kind of
/// thing, so that two controls of a kind side by side are the same size and
/// a corner inside a corner follows it. The values the Windows app had were
/// copied one by one from the Apple apps and had drifted into eighteen
/// radii and sixteen button heights, each right where it stood and together
/// a little untidy.
/// </summary>
public static class Sizing
{
    /// <summary>The steps of space between things.</summary>
    public static readonly IReadOnlyList<float> Spacing = new[] { 2f, 4f, 6f, 8f, 10f, 12f, 14f, 16f, 20f, 24f, 32f };

    /// <summary>The corners: a picture, a row, a menu, a card, a panel, the lobby's camera card; capsules are half their height.</summary>
    public static readonly IReadOnlyList<float> Radii = new[] { Theme.TileRadius, 10f, 14f, 18f, 22f, 30f };

    /// <summary>The heights a control has under a pointer: small, middle, normal, large.</summary>
    public static readonly IReadOnlyList<float> PointerHeights = new[] { 28f, 32f, 36f, 44f };

    /// <summary>A symbol on a button is four tenths of it (the web's 18 in 44); at 0.36 the bar's buttons read as having little on them.</summary>
    public const float IconRatio = 0.40f;

    public static float Icon(float button) => button * IconRatio;

    /// <summary>The close mark of a panel or a sheet: 32 drawn, where 26 and 24 were hard to hit with a pointer.</summary>
    public const float Close = 32f;

    /// <summary>
    /// A menu and the popovers that open off a control: the menu's corner,
    /// its rows' and the inset between them, which is the difference of the
    /// two, so that a row's corner follows the menu's (fourteen less four is
    /// ten). The rows were ten inside eighteen with six between, which
    /// drew four corners that did not quite sit inside each other.
    /// </summary>
    public static class Menu
    {
        public const float Radius = 14f;
        public const float RowRadius = 10f;
        public const float Inset = Radius - RowRadius;

        /// <summary>A row under a pointer.</summary>
        public const float Row = 32f;

        /// <summary>A question's answers (leaving, ending for everybody), which are pressed in a hurry and must not be missed.</summary>
        public const float AnswerRow = 44f;
    }

    /// <summary>
    /// The call's bar: round buttons of 44 (40 where the window is narrow),
    /// the hand with the way to the reactions beside it, and leaving, the one
    /// capsule on the bar, red and 56 by 44.
    /// </summary>
    public static class Bar
    {
        public const float Button = 44f;
        public const float CompactButton = 40f;
        public const float Spacing = 6f;
        public const float CompactSpacing = 4f;
        public const float Inset = 7f;
        public const float CompactInset = 5f;
        public const float LeaveWidth = 56f;
        public const float CompactLeaveWidth = 52f;

        /// <summary>
        /// The half beside the hand that opens the reactions: 32 across, as
        /// the web's became. At 20 it was the hardest thing on the bar to hit,
        /// a sliver beside the hand.
        /// </summary>
        public const float ReactionsArrow = 32f;

        /// <summary>The hand itself: a little narrower than a button, as the hand and its arrow together are one control.</summary>
        public static float Hand(float button) => button - 4f;

        /// <summary>The rule before leaving, with its margins.</summary>
        public const float Rule = 1f + 4f;

        /// <summary>A reaction in the tray, 40 square, its corner the menu's row corner inside the tray's inset.</summary>
        public const float Reaction = 40f;
        public const float ReactionGap = 2f;

        /// <summary>The tray: six reactions, the gaps between them and the inset each side.</summary>
        public static float ReactionsWidth(int count) => count * Reaction + (count - 1) * ReactionGap + 2 * Menu.Inset;

        /// <summary>
        /// How long the bar is (ControlBar.kt <c>BarLength</c>): the round
        /// buttons, the hand with its arrow, the rule before leaving on a full
        /// bar, and leaving, with the gaps between them and the ends.
        /// </summary>
        public static float Length(bool compact, int roundButtons)
        {
            var size = compact ? CompactButton : Button;
            var parts = roundButtons + 1 + (compact ? 0 : 1) + 1;
            var along = roundButtons * size + Hand(size) + ReactionsArrow + (compact ? 0 : Rule) + (compact ? CompactLeaveWidth : LeaveWidth);
            return 2 * (compact ? CompactInset : Inset) + along + (compact ? CompactSpacing : Spacing) * (parts - 1);
        }
    }

    /// <summary>
    /// The name on a picture: a capsule 24 high, 8 in from the corner, black
    /// at seven tenths, the name white whether or not its owner is speaking.
    /// It was a rounded square on Windows (a capsule elsewhere) at half
    /// black, and the speaker's name turned the tally, which on a bright
    /// picture read at about 2:1: a name that disappeared exactly while its
    /// owner talked, when the ring round the picture already says who that is.
    /// </summary>
    public static class NamePlate
    {
        public const float Height = 24f;
        public const float Inset = 8f;
        /// <summary>On a picture under 140 high (the strip's), where the full plate covered too much of it.</summary>
        public const float CompactHeight = 20f;
        public const float CompactInset = 4f;
        public const float Ground = 0.70f;

        public static Shade Fill => Shade.Black.WithAlpha(Ground);

        /// <summary>The name, and the marks beside it (the microphone off), in the dark scheme's white, speaking or not.</summary>
        public static Shade Ink => Theme.Dark.Foreground;
    }

    /// <summary>
    /// The buttons over a picture on the stage (their screen or their camera,
    /// and filling the screen): 36 round, of the bar's material, ten in from
    /// the corner. They were 30 on Windows, in black at half over a clear
    /// button, beside the web's 40 square and 32 tab.
    /// </summary>
    public static class Stage
    {
        public const float Button = 36f;
        public const float Inset = 10f;
        public const float Gap = 6f;

        /// <summary>
        /// Whether they are out: while the pointer is on the picture and the
        /// bar is out (they step aside with it when nobody has moved), or
        /// while the keyboard is on one of them, as on the Mac and the web.
        /// They were out whenever the bar was on Windows, which uses a pointer
        /// as the Mac does, over a shared screen somebody was reading.
        /// </summary>
        public static bool Shown(bool pointerOver, bool barOut, bool focusInside) => (pointerOver && barOut) || focusInside;

        /// <summary>
        /// How far above the picture's foot they sit in a call of two, where
        /// they are at the foot of the other person's picture (the corner at
        /// its top holds one's own picture, and a button there reads as the
        /// card's): nothing where the bar is clear of them, and over the bar's
        /// band where it reaches under them, as a name there is lifted.
        /// <paramref name="width"/> is how wide the buttons are together.
        /// </summary>
        public static float FootLift(Rect picture, float width, float barLeft, float barRight, bool barOut)
        {
            if (!barOut) return 0f;
            var right = picture.Right - Inset;
            var left = right - width;
            return left < barRight + Plan.Gap && right > barLeft - Plan.Gap ? Plan.Island - Plan.Gap : 0f;
        }

        /// <summary>
        /// The picture the pointer is on: of those under it, the one drawn on
        /// top, which is the smallest (one's own card over the other person
        /// in a call of two), so that the pointer on the card is not also on
        /// the picture beneath it.
        /// </summary>
        public static string? Under(IEnumerable<(string Id, Rect Spot)> spots, float x, float y)
        {
            string? under = null;
            var area = float.MaxValue;
            foreach (var (id, spot) in spots)
            {
                if (x < spot.Left || x >= spot.Right || y < spot.Top || y >= spot.Bottom) continue;
                var size = spot.Width * spot.Height;
                if (size < area)
                {
                    area = size;
                    under = id;
                }
            }
            return under;
        }
    }

    /// <summary>The screens before a call (the web's lobby and join screen after its polish).</summary>
    public static class Lobby
    {
        /// <summary>The cards the lists and the forms sit on.</summary>
        public const float CardRadius = 18f;

        /// <summary>The buttons in the top corner (the language, the server, the account): one height, 32 drawn, where they were 36, 38 and 42 with the glass round them.</summary>
        public const float CornerButton = 32f;

        /// <summary>The microphone and the camera before a call, on every screen: 44, where they were 38 on the join screen and 40 on the lobby's card.</summary>
        public const float DeviceSwitch = 44f;

        /// <summary>A day in the calendar a meeting is arranged on: 44 high, where the system's 40 were the smallest thing on the sheet.</summary>
        public const float Day = 44f;
    }
}
