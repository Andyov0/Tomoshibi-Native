using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model.Music;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Music;

/// <summary>A song from the library: pressing it adds it to the desk (MusicPanel.kt <c>TrackRow</c>), its length giving way to "Added" once it is.</summary>
internal static class TrackRows
{
    public static FrameworkElement Row(MusicTrack track, bool added, Action add)
    {
        var detail = string.Join(" · ", new[] { string.Join(" / ", track.Artists), track.Album }.Where(s => s.Length > 0));
        var words = K.Column(1, K.Text(track.Title, 13, K.Medium, lines: 1), K.Text(detail, 11, ink: Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var end = new Grid { MinWidth = 40 };
        var length = K.Text(TomoshibiProtocolMethods.MusicClock(track.Duration), 11, K.Medium, Ink.Muted, lines: 1, tabular: true).At(HorizontalAlignment.Right, VerticalAlignment.Center);
        if (added) end.Children.Add(MediaKit.Badge(T("Added"), "checkmark", lit: true).At(HorizontalAlignment.Right, VerticalAlignment.Center).Arrives());
        else end.Children.Add(length);
        var row = MediaKit.Row(K.Line(10, K.Fixed(MediaKit.Artwork(track.Cover, 40, 40, corner: 7)), K.Flex(words), K.Fixed(end)), act: add);
        if (!added)
        {
            // Under the pointer, the way to add it in place of its length.
            row.PointerEntered += (_, _) =>
            {
                end.Children.Clear();
                end.Children.Add(K.Symbol("plus.circle.fill", 17, Ink.TallyText).At(HorizontalAlignment.Right, VerticalAlignment.Center));
            };
            row.PointerExited += (_, _) =>
            {
                end.Children.Clear();
                end.Children.Add(length);
            };
        }
        row.Named(T("Add {title}", ("title", track.Title)), "music.track." + track.Id);
        return row;
    }

    /// <summary>A line of red words in a panel, for what did not work (<c>MediaProblem</c>).</summary>
    public static Border Problem(string text) => new()
    {
        Child = K.Line(6, K.Fixed(K.Symbol("exclamationmark.circle.fill", 11, Ink.DangerText).At(HorizontalAlignment.Left, VerticalAlignment.Top)), K.Flex(K.Text(text, 12, ink: Ink.DangerText))),
        Background = Ink.DangerAt(0.12f),
        CornerRadius = new CornerRadius(10),
        Padding = new Thickness(10, 8, 10, 8),
        Margin = new Thickness(0, 0, 0, 6),
    };
}

/// <summary>Searching the library (MusicPanel.kt <c>SearchTab</c>): which library, the quality, and the songs found.</summary>
internal sealed class SearchTab : MusicTabView
{
    private readonly SongDesk _desk;
    private readonly StackPanel _top = K.Column(10);
    private readonly Grid _go = new() { Width = 34, Height = 34 };
    private readonly Keyed _list = new(2) { Padding = new Thickness(8) };
    private string _topSaid = "";

    public SearchTab(SongDesk desk)
    {
        _desk = desk;
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        _top.Padding = new Thickness(12);
        Children.Add(_top);
        var rule = MediaKit.Rule();
        SetRow(rule, 1);
        Children.Add(rule);
        var scroller = new ScrollViewer { Content = _list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        SetRow(scroller, 2);
        Children.Add(scroller);
    }

    /// <summary>In the desk's keeping: the answer is kept for the tab, which may be closed and opened again before it comes.</summary>
    private void Search() => _ = _desk.SearchLibrary();

    public override void Update()
    {
        var libraries = _desk.Libraries ?? Array.Empty<MusicLibrary>();
        var library = libraries.FirstOrDefault(l => l.Id == _desk.SearchSource);
        var unsigned = library is { SignedIn: false };
        var best = _desk.Quality == MusicQuality.Best;
        var said = $"{string.Join(",", libraries.Select(l => l.Id))}|{_desk.SearchSource}|{best}|{unsigned}|{Localization.Shared.Language}";
        if (said != _topSaid)
        {
            _topSaid = said;
            _top.Children.Clear();
            if (libraries.Count > 1)
            {
                var choice = new Segmented(libraries.Select(l => l.Name).ToList(), Math.Max(0, libraries.ToList().FindIndex(l => l.Id == _desk.SearchSource)), 28, fill: true);
                choice.Chose += index => _desk.ChooseLibrary(libraries[index].Id);
                _top.Children.Add(choice);
            }
            var quality = new Segmented(new List<string> { T("Lossless"), T("Highest") }, best ? 1 : 0, 28, fill: true);
            quality.Named(T("Quality"), "music.quality");
            quality.Chose += index => _desk.Quality = index == 1 ? MusicQuality.Best : MusicQuality.Lossless;
            _top.Children.Add(K.Line(10, K.Fixed(K.Text(T("Quality"), 12, ink: Ink.Muted, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)), K.Flex(quality)));
            if (unsigned) _top.Children.Add(Note(T("Not signed in, so at most 320 kbps.")).Arrives());
            if (best) _top.Children.Add(Note(T("Hi-Res and masters are large, and take longer to start.")).Arrives());
            var field = MediaKit.Field(T("Search songs"), _desk.SearchQuery, text => _desk.SearchQuery = text, Search, "magnifyingglass", 100, out var box);
            box.Named(T("Search songs"), "music.search");
            _top.Children.Add(K.Line(8, K.Flex(field), K.Fixed(_go)));
        }
        _go.Children.Clear();
        if (_desk.Searching) _go.Children.Add(K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center).Arrives());
        else
        {
            var go = MediaKit.IconButton("arrow.right", T("Search"), Search, 34, prominent: _desk.SearchQuery.Trim().Length > 0);
            go.Named(T("Search"), "music.search.go");
            _go.Children.Add(go);
        }
        var language = Localization.Shared.Language.ToString();
        var parts = new List<(string, string, Func<UIElement>)>();
        if (_desk.SearchProblem is { } problem) parts.Add(("problem", problem, () => TrackRows.Problem(problem)));
        if (_desk.SearchResults is not { } results)
        {
            parts.Add(("hint", language, () => MediaKit.Empty("music.magnifyingglass", T("Search for a song to play into the call."))));
        }
        else if (results.Count == 0)
        {
            parts.Add(("none", language, () => MediaKit.Empty("magnifyingglass", T("Nothing found."))));
        }
        else
        {
            foreach (var track in results)
            {
                var one = track;
                var added = _desk.SearchAdded.Contains(one.Id);
                parts.Add(("track " + one.Id, $"{added}|{language}", () => TrackRows.Row(one, added, () =>
                {
                    _desk.Add(new[] { SongDesk.Song(one, _desk.SearchSource) });
                    _desk.SearchAdded.Add(one.Id);
                    Update();
                })));
            }
        }
        _list.Show(parts);
    }

    private static UIElement Note(string text) =>
        K.Line(6, K.Fixed(K.Symbol("info.circle", 11, Ink.Muted).At(HorizontalAlignment.Left, VerticalAlignment.Top)), K.Flex(K.Text(text, 11, ink: Ink.Muted)));
}

/// <summary>A playlist or song link (MusicPanel.kt <c>PlaylistTab</c>): the songs it names, each to add, or all of them.</summary>
internal sealed class PlaylistTab : MusicTabView
{
    private readonly SongDesk _desk;
    private readonly Grid _open = new();
    private readonly Keyed _list = new(2) { Padding = new Thickness(8) };

    public PlaylistTab(SongDesk desk)
    {
        _desk = desk;
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var field = MediaKit.Field(T("Paste a playlist or song link"), desk.Pasted, text => desk.Pasted = text, Open, "link", 2000, out var box);
        box.Named(T("Playlist link"), "music.link");
        var line = K.Line(8, K.Flex(field), K.Fixed(_open));
        line.Padding = new Thickness(12);
        Children.Add(line);
        var rule = MediaKit.Rule();
        SetRow(rule, 1);
        Children.Add(rule);
        var scroller = new ScrollViewer { Content = _list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        SetRow(scroller, 2);
        Children.Add(scroller);
    }

    private void Open() => _ = _desk.OpenLink();

    public override void Update()
    {
        _open.Children.Clear();
        var open = _desk.Reading
            ? new Tap(K.Spinner(13, Ink.TallyForeground), Look.Prominent, 34, padding: 18)
            : MediaKit.Button(T("Open"), null, Open, prominent: true, height: 34);
        open.Named(T("Open"), "music.link.open");
        _open.Children.Add(open);
        var language = Localization.Shared.Language.ToString();
        var parts = new List<(string, string, Func<UIElement>)>();
        if (_desk.LinkProblem is { } problem) parts.Add(("problem", problem, () => TrackRows.Problem(problem)));
        if (_desk.Linked is { } found)
        {
            parts.Add(("link " + found.Title, $"{found.Tracks.Length}|{language}", () => Heading(found)));
            foreach (var track in found.Tracks)
            {
                var one = track;
                var added = _desk.LinkAdded.Contains(one.Id);
                parts.Add(("track " + one.Id, $"{added}|{language}", () => TrackRows.Row(one, added, () =>
                {
                    _desk.Add(new[] { SongDesk.Song(one, found.Source) });
                    _desk.LinkAdded.Add(one.Id);
                    Update();
                })));
            }
        }
        else if (_desk.LinkProblem == null)
        {
            parts.Add(("hint", language, () => MediaKit.Empty("link", T("Paste a link shared from a music app."))));
        }
        _list.Show(parts);
    }

    private FrameworkElement Heading(MusicLink found)
    {
        var words = K.Column(2, K.Text(found.Title, 13, K.Semibold, lines: 2), K.Text(T("{count} songs", ("count", found.Tracks.Length)), 11, ink: Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var all = MediaKit.Button(T("Add all"), null, () =>
        {
            _desk.Add(found.Tracks.Select(t => SongDesk.Song(t, found.Source)).ToList());
            foreach (var track in found.Tracks) _desk.LinkAdded.Add(track.Id);
            Update();
        }, prominent: true);
        all.Named(T("Add all"), "music.link.all");
        all.IsEnabled = found.Tracks.Length > 0;
        all.VerticalAlignment = VerticalAlignment.Center;
        var line = K.Line(10, K.Fixed(MediaKit.Artwork(found.Tracks.FirstOrDefault()?.Cover ?? "", 44, 44, "music.note.list", 8)), K.Flex(words), K.Fixed(all));
        return new Border { Child = line, Background = Ink.Wash(0.05f), CornerRadius = new CornerRadius(14), Padding = new Thickness(10), Margin = new Thickness(0, 0, 0, 6) };
    }
}
