namespace Tomoshibi.Basics;

/// <summary>
/// Where every picture sits, as coordinates rather than as a tree: Android's
/// basics/Plan.kt and the Apple apps' TomoshibiBasics/Plan.swift, which follow
/// web/src/live/plan.ts and layout.ts line for line. Lengths are in
/// device-independent pixels.
///
/// The web learned this the hard way: an arrangement expressed by which
/// container a tile is in destroys and rebuilds the tile whenever the
/// arrangement changes, and a picture that is rebuilt resubscribes and waits
/// for a keyframe, the black screen. A picture placed by coordinates is made
/// once for as long as its owner is in the call, and moving it to the stage
/// is an animation rather than an interruption. On Windows that also keeps a
/// tile's swap chain: a SwapChainPanel taken out of the tree and put back
/// elsewhere loses what it was showing until the next frame.
///
/// Single precision throughout, as Plan.kt is, so that the three platforms
/// round the same lengths the same way.
/// </summary>
public static class Plan
{
    /// <summary>Space between pictures, and between a picture and the edge.</summary>
    public const float Gap = 8f;
    /// <summary>Nine keeps a grid readable on a laptop and caps how many streams the call asks for at once.</summary>
    public const int TilesPerPage = 9;
    /// <summary>The controls' reserve at the foot when nothing has been measured.</summary>
    public const float Island = 20f + 58f + Gap;
    /// <summary>Two people are one person being looked at and one being checked on.</summary>
    public const int AloneTogether = 2;
    private const float StripHeight = 112f;
    private const float Headroom = 56f;
    private const float Landscape = 16f / 9f;
    private const float UprightAspect = 3f / 4f;
    private const float Fill = 0.92f;

    /// <summary>
    /// A screen held upright, rather than a window that happens to be tall: a
    /// fifth taller than wide, so a window dragged through square does not
    /// reshape the call under the pointer.
    /// </summary>
    public static bool Upright(Size container) => container.Height > container.Width * 1.2f;

    private static float Aspect(Size container) => Upright(container) ? UprightAspect : Landscape;

    public readonly record struct Arrangement(int Columns, int Rows, float Width, float Height);

    /// <summary>The column count that gives each person the most picture (layout.ts).</summary>
    public static Arrangement? Arrange(Size container, int count, float gap = Gap, float aspect = Landscape)
    {
        var width = container.Width;
        var height = container.Height;
        if (count <= 0 || width <= 0 || height <= 0) return null;
        Arrangement? best = null;
        var bestScore = 0f;
        for (var columns = 1; columns <= count; columns++)
        {
            var rows = (int)Math.Ceiling(count / (double)columns);
            var cellWidth = (width - (columns - 1) * gap) / columns;
            var cellHeight = (height - (rows - 1) * gap) / rows;
            if (cellWidth <= 0 || cellHeight <= 0) continue;
            var tileWidth = MathF.Min(cellWidth, cellHeight * aspect);
            var tileHeight = tileWidth / aspect;
            var ceilingFill = container.Height > container.Width * 1.2f ? 1f : Fill;
            var ceiling = (height * ceilingFill - (rows - 1) * gap) / rows;
            if (tileHeight > ceiling)
            {
                tileHeight = ceiling;
                tileWidth = tileHeight * aspect;
            }
            var score = tileWidth * tileHeight * ((float)count / (columns * rows));
            if (score > bestScore)
            {
                bestScore = score;
                best = new Arrangement(columns, rows, MathF.Floor(tileWidth), MathF.Floor(tileHeight));
            }
        }
        return best;
    }

    /// <summary>
    /// A grid, centred, each row centred on its own.
    ///
    /// Two departures from plan.ts, both about the edges, as Plan.swift and
    /// Plan.kt have them. The grid keeps <paramref name="sides"/> clear at the
    /// left and right, the gap by default, which the stage and the strip
    /// always kept and the grid did not: six people in a wide window were
    /// drawn edge to edge. And it is centred in the window but never lower
    /// than the controls allow: centred on the whole height, a full grid's
    /// last row ran a third of the way under the bar.
    /// </summary>
    public static OrderedDictionary<string, Rect> Grid(Size container, IReadOnlyList<string> ids, float island = Island, float sides = Gap)
    {
        var plan = new OrderedDictionary<string, Rect>();
        var maybe = Arrange(
            new Size(MathF.Max(0f, container.Width - 2 * sides), MathF.Max(0f, container.Height - island)),
            ids.Count, aspect: Aspect(container));
        if (maybe is not { } layout) return plan;
        var gridHeight = layout.Rows * layout.Height + (layout.Rows - 1) * Gap;
        var top = MathF.Max(0f, MathF.Min((container.Height - gridHeight) / 2, container.Height - island - gridHeight));
        for (var index = 0; index < ids.Count; index++)
        {
            var row = index / layout.Columns;
            var column = index % layout.Columns;
            var inRow = row == layout.Rows - 1 ? ids.Count - row * layout.Columns : layout.Columns;
            var rowWidth = inRow * layout.Width + (inRow - 1) * Gap;
            plan[ids[index]] = new Rect(
                (container.Width - rowWidth) / 2 + column * (layout.Width + Gap),
                top + row * (layout.Height + Gap),
                layout.Width, layout.Height);
        }
        return plan;
    }

    /// <summary>One picture on the stage, the rest in a filmstrip beneath.</summary>
    public static OrderedDictionary<string, Rect> Focus(Size container, string stage, IReadOnlyList<string> strip, bool fullscreen = false, float island = Island)
    {
        var plan = new OrderedDictionary<string, Rect>();
        if (container.Width <= 0 || container.Height <= 0) return plan;
        if (fullscreen)
        {
            plan[stage] = Rect.Of(container);
            return plan;
        }
        var bottom = container.Height - island;
        var showStrip = strip.Count > 0;
        var stripTop = bottom - StripHeight;
        plan[stage] = new Rect(Gap, Gap, MathF.Max(0f, container.Width - 2 * Gap), MathF.Max(0f, (showStrip ? stripTop - Gap : bottom) - Gap));
        if (!showStrip) return plan;
        var width = StripHeight * Aspect(container);
        var total = strip.Count * width + (strip.Count - 1) * Gap;
        var left = (container.Width - total) / 2;
        for (var index = 0; index < strip.Count; index++)
        {
            plan[strip[index]] = new Rect(left + index * (width + Gap), stripTop, width, StripHeight);
        }
        return plan;
    }

    /// <summary>How many the filmstrip holds at this width; never fewer than one.</summary>
    public static int StripCapacity(Size container) => StripCapacity(container.Width, Aspect(container));

    /// <summary>The same for a strip <paramref name="width"/> long whose pictures have <paramref name="aspect"/>, which is the window's and not the strip's.</summary>
    private static int StripCapacity(float width, float aspect)
    {
        var picture = StripHeight * aspect;
        var usable = width - 2 * Gap + Gap;
        return Math.Max(1, (int)MathF.Floor(usable / (picture + Gap)));
    }

    /// <summary>The other person filling the screen and the rest as cards down the trailing edge, the way every telephone draws a call of two.</summary>
    public static OrderedDictionary<string, Rect> Close(Size container, string filling, IReadOnlyList<string> cards, float island = Island)
    {
        var plan = new OrderedDictionary<string, Rect>();
        if (container.Width <= 0 || container.Height <= 0) return plan;
        plan[filling] = Rect.Of(container);
        var width = MathF.Min(container.Width * (Upright(container) ? 0.26f : 0.18f), (container.Height - island - 2 * Gap) / 2);
        var height = width / Aspect(container);
        for (var index = 0; index < cards.Count; index++)
        {
            var top = Headroom + index * (height + Gap);
            if (top + height > container.Height - island) continue;
            plan[cards[index]] = new Rect(container.Width - width - Gap, top, width, height);
        }
        return plan;
    }

    /// <summary>How far above the bottom something drawn over the pictures must end to leave these ones uncovered (plan.ts <c>clearOf</c>).</summary>
    public static float? Clear(IReadOnlyDictionary<string, Rect> plan, IReadOnlyList<string> ids, Size container)
    {
        float? highest = null;
        foreach (var id in ids)
        {
            if (!plan.TryGetValue(id, out var spot)) continue;
            if (highest == null || spot.Top < highest) highest = spot.Top;
        }
        if (highest is not { } top) return null;
        return container.Height - top + Gap;
    }

    /// <summary>
    /// How much further up a card stacked on what is below it in its corner
    /// has to go to end <paramref name="clear"/> above the bottom, where it now
    /// ends <paramref name="ends"/> above it: nothing where there is nothing
    /// to clear or it is already clear (Room.tsx <c>aboveStrip</c>).
    /// </summary>
    public static float Lift(float? clear, float ends) => clear is { } c ? MathF.Max(0f, c - ends) : 0f;

    // The whole room

    /// <summary>
    /// How tall a window has to be before the controls fit down its side
    /// (hooks/useRoomFor.ts): the column is a little under four hundred
    /// points, and at a phone's landscape height both of its ends, mute and
    /// leave, were off the screen.
    /// </summary>
    public const float SideNeeds = 480f;
    /// <summary>The lane the controls take down the side when nothing has been measured: the web's <c>mr-[5.75rem]</c>.</summary>
    public const float SideLane = 92f;
    /// <summary>How long a room is left alone before controls that may step aside do, in seconds (hooks/useIdle.ts <c>AFTER</c>).</summary>
    public const double IdleAfter = 3.0;
    /// <summary>
    /// What a room that pages keeps clear either side for its arrows, so that
    /// they sit beside the pictures they turn rather than over the first and
    /// the last of them.
    /// </summary>
    public const float PagerReserve = 52f;

    /// <summary>
    /// Everything the room view needs from one arrangement: where each
    /// picture is, which page of how many is showing (already brought back
    /// inside the pages there are), and the pictures drawn below or around
    /// the stage, in order: the strip, the grid, or the card in a call of two.
    /// </summary>
    public sealed record Room(OrderedDictionary<string, Rect> Spots, int Page, int Pages, IReadOnlyList<string> Shown);

    /// <summary>
    /// Where every picture goes, as Room.tsx's <c>Stage</c> works it out.
    ///
    /// <paramref name="pictures"/> is everybody, in the order they are drawn;
    /// <paramref name="mine"/> is this participant's own camera, which never
    /// pages away: losing sight of yourself because the room got busy is
    /// disorienting, and it costs nothing, since it is drawn from the local
    /// capture. It takes one of the page's places rather than being added
    /// beyond them, so a page is never more than nine. <paramref name="stage"/>
    /// is whatever is pinned.
    /// </summary>
    public static Room ForRoom(
        Size container,
        IReadOnlyList<string> pictures,
        string? mine,
        string? stage,
        bool fullscreen = false,
        int page = 0,
        float island = Island)
    {
        var onStage = stage != null && pictures.Contains(stage) ? stage : null;
        var me = mine != null && pictures.Contains(mine) ? mine : null;
        var kept = me != null && me != onStage ? new List<string> { me } : new List<string>();
        var rest = pictures.Where(p => p != me && p != onStage).ToList();
        // Counted at the window's own shape, which is the shape the strip's
        // pictures are drawn at (Focus). Plan.swift counts them at the shape
        // of the width left between the arrows, and a window a little short
        // of upright (840 by 1000) read as upright there: its strip was
        // counted as narrow cards and drawn as wide ones, off both sides.
        int Places(float width) =>
            Math.Max(1, (onStage != null ? StripCapacity(width, Aspect(container)) : TilesPerPage) - kept.Count);
        // A room that will page gives its arrows room, and the strip holds
        // what fits beside them.
        var paged = rest.Count > Places(container.Width);
        var per = Places(paged ? container.Width - 2 * PagerReserve : container.Width);
        var pages = Math.Max(1, (rest.Count + per - 1) / per);
        var showing = Math.Min(Math.Max(0, page), pages - 1);
        var start = showing * per;
        var from = Math.Min(start, rest.Count);
        var shown = kept.Concat(rest.GetRange(from, Math.Min(rest.Count, start + per) - from)).ToList();
        OrderedDictionary<string, Rect> spots;
        if (onStage != null)
        {
            spots = Focus(container, onStage, fullscreen ? Array.Empty<string>() : shown, fullscreen, island);
        }
        else if (pictures.Count <= AloneTogether && rest.Count > 0)
        {
            // Two people is one filling the screen and the other a card, the
            // way a telephone has always drawn it.
            spots = Close(container, rest[0], me != null ? new[] { me } : Array.Empty<string>(), island);
        }
        else
        {
            spots = Grid(container, shown, island, paged ? PagerReserve : Gap);
        }
        return new Room(spots, showing, pages, shown);
    }

    /// <summary>
    /// The filmstrip on its own, along the top of <paramref name="container"/>,
    /// for a stage drawn somewhere else (Plan.kt <c>strip</c>, which a
    /// foldable on a table uses). Paged as <see cref="ForRoom"/> pages the
    /// strip beneath a stage, this participant's own picture on every page.
    /// </summary>
    public static Room Strip(Size container, IReadOnlyList<string> pictures, string? mine, string stage, int page = 0)
    {
        var me = mine != null && pictures.Contains(mine) && mine != stage ? mine : null;
        var kept = me != null ? new List<string> { me } : new List<string>();
        var rest = pictures.Where(p => p != me && p != stage).ToList();
        var aspect = Aspect(container);
        int Places(float width) => Math.Max(1, StripCapacity(width, aspect) - kept.Count);
        var paged = rest.Count > Places(container.Width);
        var per = Places(paged ? container.Width - 2 * PagerReserve : container.Width);
        var pages = Math.Max(1, (rest.Count + per - 1) / per);
        var showing = Math.Min(Math.Max(0, page), pages - 1);
        var start = showing * per;
        var from = Math.Min(start, rest.Count);
        var shown = kept.Concat(rest.GetRange(from, Math.Min(rest.Count, start + per) - from)).ToList();
        var spots = new OrderedDictionary<string, Rect>();
        var width = StripHeight * aspect;
        var total = shown.Count * width + (shown.Count - 1) * Gap;
        var left = (container.Width - total) / 2;
        for (var index = 0; index < shown.Count; index++)
        {
            spots[shown[index]] = new Rect(left + index * (width + Gap), Gap, width, StripHeight);
        }
        return new Room(spots, showing, pages, shown);
    }

    /// <summary>How tall <see cref="Strip"/> is with the gap above and below it.</summary>
    public const float StripBand = StripHeight + 2 * Gap;
}

/// <summary>
/// Which picture owns the stage (hooks/usePin.ts; Plan.swift and Plan.kt
/// <c>StagePin</c>).
///
/// A share takes the stage when it starts and gives it back when it stops,
/// and the rule the web names as the one easy to miss: only a pin made
/// automatically is taken away automatically. Somebody who put a person on
/// the stage by hand keeps them there when a share starts or stops.
///
/// One rule is the web's intent rather than its code. The web claims the
/// stage for a share whenever nothing is pinned, so pressing the share on the
/// stage, whose label says "Show everybody", put it straight back: the grid
/// could not be reached for as long as anybody shared. Here a share claims
/// the stage when it starts, or when the stage is emptied by something other
/// than the person looking (the picture on it went away); choosing the grid
/// is a choice, and it holds until a new share starts.
/// </summary>
public sealed record StagePin
{
    public string? Pinned { get; init; }
    /// <summary>The pin this made itself, which is the only one it may take back.</summary>
    private string? Auto { get; init; }
    /// <summary>The shares that were already running at the last look, so that only a new one claims a stage somebody emptied on purpose.</summary>
    private IReadOnlySet<string> Seen { get; init; } = new HashSet<string>();

    public StagePin() { }

    private StagePin(string? pinned, string? auto, IReadOnlySet<string> seen)
    {
        Pinned = pinned;
        Auto = auto;
        Seen = seen;
    }

    /// <summary>A choice by hand: a picture, or null for everybody.</summary>
    public StagePin Pin(string? id) => new(id, null, Seen);

    /// <summary>What a press on a picture does: on to the stage, or back off it.</summary>
    public StagePin Toggle(string id) => new(Pinned == id ? null : id, null, Seen);

    /// <summary>
    /// Brings the pin up to date with the room: <paramref name="pictures"/> is
    /// every picture there is, <paramref name="shares"/> those that take the
    /// stage by themselves (a shared screen, the video watched together),
    /// first first.
    /// </summary>
    public StagePin Update(IReadOnlyList<string> pictures, IReadOnlyList<string> shares)
    {
        var pinned = Pinned;
        var auto = Auto;
        var emptied = false;
        if (auto != null && !shares.Contains(auto))
        {
            if (pinned == auto) pinned = null;
            auto = null;
            emptied = true;
        }
        // A pinned participant who leaves must not take the stage with them.
        if (pinned != null && !pictures.Contains(pinned))
        {
            pinned = null;
            auto = null;
            emptied = true;
        }
        if (pinned == null)
        {
            var claim = emptied ? shares.FirstOrDefault() : shares.FirstOrDefault(s => !Seen.Contains(s));
            if (claim != null)
            {
                pinned = claim;
                auto = claim;
            }
        }
        return new StagePin(pinned, auto, shares.ToHashSet());
    }
}

/// <summary>
/// When each person was last heard, for bringing speakers forward in a room
/// that runs to more than one page (hooks/useSpeakingOrder.ts; Plan.swift
/// and Plan.kt <c>SpeakingRecord</c>).
///
/// By when they were last heard rather than by whether they are speaking
/// now: speech is full of gaps, and sorting on the moment would drop
/// somebody down the grid between two words of a sentence.
/// </summary>
public sealed class SpeakingRecord
{
    private readonly IReadOnlyDictionary<string, double> _heard;

    public SpeakingRecord() : this(new Dictionary<string, double>()) { }

    private SpeakingRecord(IReadOnlyDictionary<string, double> heard) => _heard = heard;

    /// <summary>
    /// Notes who is speaking at <paramref name="now"/>, and forgets whoever is
    /// not <paramref name="present"/>: somebody who left should not keep a
    /// place they would take on coming back under the same identity.
    /// </summary>
    public SpeakingRecord Hear(IEnumerable<string> speaking, IEnumerable<string> present, double now)
    {
        var next = new Dictionary<string, double>(_heard);
        foreach (var identity in speaking) next[identity] = now;
        var here = present.ToHashSet();
        return new SpeakingRecord(next.Where(pair => here.Contains(pair.Key)).ToDictionary(pair => pair.Key, pair => pair.Value));
    }

    /// <summary>The ids with the most recently heard first. Ties keep the order they arrived in, so a room where nobody has spoken looks as it did.</summary>
    public IReadOnlyList<string> Order(IReadOnlyList<string> ids) =>
        ids.Select((id, index) => (id, index))
            .OrderByDescending(pair => _heard.TryGetValue(pair.id, out var at) ? at : 0.0)
            .ThenBy(pair => pair.index)
            .Select(pair => pair.id)
            .ToList();
}
