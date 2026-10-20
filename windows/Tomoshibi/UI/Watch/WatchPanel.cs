using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model.Watch;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Watch;

/// <summary>
/// Watching a video together: a link to paste, what is playing and coming
/// up, and the media servers where this person may browse them (Android's
/// ui/watch/WatchPanel.kt; WatchPanelView.swift; WatchPanel.tsx,
/// WatchTab.tsx). The web's tab strip is a segmented choice, its field a
/// capsule that lights while it is typed in, each video a row with its
/// poster.
/// </summary>
internal sealed class WatchPanel : Grid
{
    private readonly WatchEngine _engine;
    private readonly Grid _tabs = new();
    private readonly Grid _body = new();
    private bool _browsing;
    private string _tabsSaid = "";
    private QueueView? _queue;
    private MediaBrowser? _browser;

    public WatchPanel(WatchEngine engine, Action close)
    {
        _engine = engine;
        var column = new Grid();
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var header = MediaKit.Header(T("Watch together"), T("Close watch together"), close, "play.rectangle.fill");
        column.Children.Add(header);
        SetRow(_tabs, 1);
        column.Children.Add(_tabs);
        var rule = MediaKit.Rule();
        SetRow(rule, 2);
        column.Children.Add(rule);
        SetRow(_body, 3);
        column.Children.Add(_body);
        Children.Add(MediaKit.Glass(column));
        if (engine.Sealed)
        {
            // The bar offers no button in a sealed call; should the panel be
            // reached anyway, it says why there is nothing.
            _body.Children.Add(MediaKit.Empty("lock.fill", T("Not available in an encrypted call")).At(HorizontalAlignment.Center, VerticalAlignment.Center));
            return;
        }
        Loaded += (_, _) =>
        {
            engine.Changed -= Update;
            engine.Changed += Update;
            // When it opens, and again once the server has said whether this
            // person may start a show: browsing is for choosing, and choosing
            // is theirs.
            _ = engine.LoadServers();
            Update();
        };
        Unloaded += (_, _) => engine.Changed -= Update;
        Update();
    }

    private bool _askedServers;

    public void Update()
    {
        if (_engine.Sealed) return;
        if (_engine.Ready && !_askedServers)
        {
            _askedServers = true;
            _ = _engine.LoadServers();
        }
        // The tab is there only for somebody the server lists media servers
        // for, which is somebody given them.
        var hasServers = _engine.Servers.Count > 0;
        var said = $"{hasServers}|{_browsing}|{Localization.Shared.Language}";
        if (said != _tabsSaid)
        {
            _tabsSaid = said;
            _tabs.Children.Clear();
            if (hasServers)
            {
                var choice = new Segmented(new List<string> { T("Queue"), T("Media servers") }, _browsing ? 1 : 0, fill: true) { Margin = new Thickness(12, 0, 12, 10) };
                choice.Named(T("Media servers"), "watch.tabs");
                choice.Chose += index => Browse(index == 1);
                _tabs.Children.Add(choice.Arrives());
            }
            if (!hasServers && _browsing) Browse(false);
        }
        var browsing = _browsing && hasServers;
        if (browsing)
        {
            if (_browser == null)
            {
                _body.Children.Clear();
                _queue = null;
                _browser = new MediaBrowser(_engine, () => Browse(false));
                _body.Children.Add(_browser.Arrives());
            }
        }
        else
        {
            if (_queue == null)
            {
                _body.Children.Clear();
                _browser = null;
                _queue = new QueueView(_engine);
                _body.Children.Add(_queue.Arrives());
            }
            _queue.Update();
        }
    }

    private void Browse(bool browsing)
    {
        if (_browsing == browsing) return;
        _browsing = browsing;
        _tabsSaid = "";
        Update();
    }
}

/// <summary>The box a link is pasted into, and the show's videos.</summary>
internal sealed class QueueView : Grid
{
    private readonly WatchEngine _engine;
    private readonly Grid _adder = new();
    private readonly Grid _addButton = new();
    private readonly Keyed _list = new(2) { Padding = new Thickness(8) };
    private readonly Grid _footer = new();
    private TextBox? _box;
    private string _pasted = "";
    private bool _busy;
    private string _footerSaid = "";

    public QueueView(WatchEngine engine)
    {
        _engine = engine;
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var field = MediaKit.Field(T("A video link, or the text it was shared in"), "", text =>
        {
            _pasted = text;
            PaintAdd();
        }, Add, "link", WatchEngine.LinkLength, out var box);
        _box = box;
        box.Named(T("A video link, or the text it was shared in"), "watch.link");
        var line = K.Line(8, K.Flex(field), K.Fixed(_addButton));
        line.Padding = new Thickness(12);
        _adder.Children.Add(line);
        Children.Add(_adder);
        var rule = MediaKit.Rule();
        SetRow(rule, 1);
        Children.Add(rule);
        var scroller = new ScrollViewer { Content = _list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        SetRow(scroller, 2);
        Children.Add(scroller);
        SetRow(_footer, 3);
        Children.Add(_footer);
        PaintAdd();
    }

    /// <summary>Pasting starts the show here when nobody runs one and this person may read links; anybody may add to a show that is running.</summary>
    private bool CanAdd => _engine.Show != null || _engine.Ready;

    private void PaintAdd()
    {
        _addButton.Children.Clear();
        _addButton.Width = 34;
        _addButton.Height = 34;
        if (_busy)
        {
            _addButton.Children.Add(K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center).Arrives());
            return;
        }
        var add = MediaKit.IconButton("plus", T("Add a video"), Add, 34, prominent: _pasted.Trim().Length > 0);
        add.Named(T("Add a video"), "watch.add");
        add.IsEnabled = CanAdd;
        _addButton.Children.Add(add);
    }

    private void Add()
    {
        var text = _pasted.Trim();
        if (text.Length == 0 || _busy || !CanAdd) return;
        _busy = true;
        PaintAdd();
        _ = Adding(text);
    }

    /// <summary>In the engine's own keeping rather than the panel's: a panel closed while the link is read must not take the adding with it.</summary>
    private async Task Adding(string text)
    {
        try
        {
            if (await _engine.Add(text))
            {
                _pasted = "";
                if (_box != null) _box.Text = "";
            }
        }
        finally
        {
            _busy = false;
            PaintAdd();
        }
    }

    public void Update()
    {
        _adder.Opacity = CanAdd ? 1 : 0.5;
        _adder.IsHitTestVisible = CanAdd;
        PaintAdd();
        var show = _engine.Show;
        var parts = new List<(string, string, Func<UIElement>)>();
        var language = Localization.Shared.Language.ToString();
        // Each video is a part of its own, which arrives the first time it is
        // there and is drawn again only when what it says changes.
        if (show?.Now is { } now)
        {
            parts.Add(("now " + now.Key, Said(now, language), () => VideoRow(now, playing: true)));
            if (show.Queue.Length > 0) parts.Add(("heading", language, () => Menu.Heading(T("Queue")).Pad(0, 6, 0, 0)));
            foreach (var video in show.Queue)
            {
                var one = video;
                parts.Add((one.Key, Said(one, language), () => VideoRow(one, playing: false)));
            }
        }
        else
        {
            var words = CanAdd ? T("Paste a link and everybody watches it together, in step.") : T("When somebody signed in starts a video, it shows up here.");
            parts.Add(("empty", words, () => MediaKit.Empty("film", words)));
        }
        _list.Show(parts);
        var footer = $"{_engine.Mine}|{language}";
        if (footer != _footerSaid)
        {
            _footerSaid = footer;
            _footer.Children.Clear();
            if (_engine.Mine)
            {
                var stop = MediaKit.Button(T("Stop watching together"), "stop.circle", _engine.Stop, quiet: true);
                stop.Named(T("Stop watching together"), "watch.stop");
                stop.HorizontalAlignment = HorizontalAlignment.Right;
                var column = K.Column(0, MediaKit.Rule(), stop.Pad(10));
                _footer.Children.Add(column.Arrives(1f, fromY: 20));
            }
        }
    }

    private string Said(WatchVideo video, string language)
    {
        var info = _engine.Infos.TryGetValue(video.Ticket, out var known) ? known : null;
        return $"{video.By}|{video.Duration}|{info?.Video?.Title}|{info?.Video?.Cover}|{info?.GetType().Name}|{language}";
    }

    /// <summary>One video in the show. Its title and picture come from the server by the ticket, never from the call.</summary>
    private FrameworkElement VideoRow(WatchVideo video, bool playing)
    {
        var info = _engine.Info(video.Ticket);
        var known = info?.Video;
        var art = MediaKit.Artwork(known?.Cover ?? "", 72, 40, "film", 6);
        if (playing)
        {
            var mark = new Border
            {
                Width = 16,
                Height = 16,
                CornerRadius = new CornerRadius(8),
                Background = Ink.Tally,
                Child = K.Symbol("play.fill", 8, Ink.TallyForeground),
                HorizontalAlignment = HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Bottom,
                Margin = new Thickness(4),
            };
            art.Children.Add(mark);
        }
        var title = known?.Title ?? info switch
        {
            WatchInfo.Unavailable => T("That video cannot be played here."),
            WatchInfo.Failed => T("The video could not be read. Try again."),
            _ => "…",
        };
        var details = new List<string>();
        if (playing) details.Add(T("Playing"));
        details.Add(T("Added by {name}", ("name", video.By)));
        if (video.Duration > 0) details.Add(WatchWords.Clock(video.Duration));
        var words = K.Column(2, K.Text(title, 12.5, K.Medium, lines: 1), K.Text(string.Join(" · ", details), 10.5, ink: playing ? Ink.Tally : Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var parts = new List<(UIElement, double)> { K.Fixed(art), K.Flex(words) };
        if (!playing)
        {
            var take = MediaKit.IconButton("xmark", T("Take {title} off", ("title", known?.Title ?? "")), () => _engine.Remove(video.Key), 24);
            take.Named(T("Take {title} off", ("title", known?.Title ?? "")), "watch.remove." + video.Key);
            parts.Add(K.Fixed(take));
        }
        return MediaKit.Row(K.Line(10, parts.ToArray()), lit: playing);
    }
}

/// <summary>
/// The deployment's media servers, browsed: their libraries, what is in
/// them, and a search, for somebody given them choosing what everybody will
/// watch (Android's ui/watch/MediaBrowser.kt; MediaBrowser.tsx). Choosing
/// something hands its link to the show as a pasted one is, so it is read
/// under this person's grant even in somebody else's show.
/// </summary>
internal sealed class MediaBrowser : Grid
{
    private readonly WatchEngine _engine;
    private readonly Action _chose;
    private readonly Grid _servers = new();
    private readonly Grid _back = new();
    private readonly Grid _list = new();
    private string _server;
    /// <summary>Where in the server: a trail of folders, the first being the server's libraries.</summary>
    private readonly List<(string Id, string Name)> _trail = new();
    private string _words = "";
    private string? _searched;
    private string? _adding;
    private int _asking;

    public MediaBrowser(WatchEngine engine, Action chose)
    {
        _engine = engine;
        _chose = chose;
        _server = engine.Servers.FirstOrDefault()?.Key ?? "";
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var top = K.Column(10);
        top.Padding = new Thickness(12);
        if (engine.Servers.Count > 1)
        {
            var choice = new Segmented(engine.Servers.Select(s => s.Name).ToList(), 0, fill: true);
            choice.Chose += index =>
            {
                _server = engine.Servers[index].Key;
                _trail.Clear();
                _searched = null;
                _ = Load();
            };
            _servers.Children.Add(choice);
            top.Children.Add(_servers);
        }
        var field = MediaKit.Field(T("Search films and series"), "", text => _words = text, Search, "magnifyingglass", 100, out var box);
        box.Named(T("Search films and series"), "watch.search");
        var go = MediaKit.IconButton("arrow.right", T("Search"), Search, 34);
        top.Children.Add(K.Line(8, K.Flex(field), K.Fixed(go)));
        top.Children.Add(_back);
        Children.Add(top);
        var rule = MediaKit.Rule();
        SetRow(rule, 1);
        Children.Add(rule);
        SetRow(_list, 2);
        Children.Add(_list);
        _ = Load();
    }

    private async Task Load()
    {
        if (_server.Length == 0) return;
        var asked = ++_asking;
        Face(K.Spinner(20).At(HorizontalAlignment.Center, VerticalAlignment.Top).Pad(0, 40, 0, 40));
        MediaItem[]? found;
        try
        {
            found = _searched is { } words
                ? await _engine.Deployment.SearchMedia(_server, words)
                : await _engine.Deployment.BrowseMedia(_server, _trail.Count > 0 ? _trail[^1].Id : null);
        }
        catch (Exception)
        {
            found = null;
        }
        if (asked != _asking) return;
        PaintBack();
        if (found == null) Face(MediaKit.Empty("externaldrive.badge.exclamationmark", T("The media server did not answer. Try again.")));
        else if (found.Length == 0) Face(MediaKit.Empty("magnifyingglass", T("Nothing found.")));
        else Face(Items(found));
    }

    private void Face(FrameworkElement face)
    {
        _list.Children.Clear();
        _list.Children.Add(face.Arrives());
    }

    private void Search()
    {
        var asked = _words.Trim();
        _searched = asked.Length == 0 ? null : asked;
        _ = Load();
    }

    private void PaintBack()
    {
        _back.Children.Clear();
        if (_trail.Count == 0 && _searched == null) return;
        var label = _searched is { } words ? T("Results for {words}", ("words", words)) : _trail[^1].Name;
        var back = MediaKit.Button(label, "chevron.left", () =>
        {
            if (_searched != null) _searched = null;
            else if (_trail.Count > 0) _trail.RemoveAt(_trail.Count - 1);
            _ = Load();
        }, quiet: true, height: 26);
        back.HorizontalAlignment = HorizontalAlignment.Left;
        _back.Children.Add(back.Arrives());
    }

    private ScrollViewer Items(MediaItem[] items)
    {
        var column = K.Column(2);
        column.Padding = new Thickness(8);
        foreach (var item in items) column.Children.Add(ItemRow(item));
        return new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
    }

    private FrameworkElement ItemRow(MediaItem item)
    {
        var opens = item.Playable || item.Folder;
        // A media server's poster, which the deployment fetches with its
        // account and hands only to somebody whose session may browse: so it
        // is asked for with this app's session.
        var art = MediaKit.Artwork(item.Image, 34, 50, item.Folder ? "folder.fill" : "play.fill", 6, Poster);
        var detail = new List<string>();
        if (item.Year > 0) detail.Add(item.Year.ToString());
        if (item.Duration > 0) detail.Add(WatchWords.Clock(item.Duration));
        var words = K.Column(2, K.Text(item.Name, 13, K.Medium, lines: 1));
        // Nothing at all for a folder, which has no year or length: an empty
        // line under its name held the name off centre.
        if (detail.Count > 0) words.Children.Add(K.Text(string.Join(" · ", detail), 11, ink: Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var end = new Grid { Width = 20 };
        if (_adding == item.Id) end.Children.Add(K.Spinner(14));
        else if (opens) end.Children.Add(K.Symbol(item.Folder ? "chevron.right" : "play.circle.fill", item.Folder ? 12 : 17, item.Folder ? Ink.Muted : Ink.Tally).At(HorizontalAlignment.Right, VerticalAlignment.Center));
        var row = MediaKit.Row(K.Line(10, K.Fixed(art), K.Flex(words), K.Fixed(end)), act: opens ? () => Open(item) : null);
        row.Opacity = opens ? 1 : 0.5;
        row.Named(item.Playable ? T("Watch {title}", ("title", item.Name)) : item.Name, "watch.item." + item.Id);
        return row;
    }

    private void Open(MediaItem item)
    {
        if (item.Playable)
        {
            _adding = item.Id;
            _ = Adding(item);
        }
        else if (item.Folder)
        {
            _searched = null;
            _trail.Add((item.Id, item.Name));
            _ = Load();
        }
    }

    private async Task Adding(MediaItem item)
    {
        bool added;
        try
        {
            added = await _engine.Add(TomoshibiProtocolMethods.WatchMediaLink(_server, item.Id));
        }
        finally
        {
            _adding = null;
        }
        if (added) _chose();
    }

    private static readonly HttpClient Fetch = new() { Timeout = TimeSpan.FromSeconds(15) };

    /// <summary>A poster asked for with the app's session cookie, which the platform's own fetching would not send.</summary>
    private async Task<byte[]?> Poster(string address)
    {
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, address);
            if (_engine.Deployment.CookieHeader() is { } cookie) request.Headers.TryAddWithoutValidation("Cookie", cookie);
            using var answer = await Fetch.SendAsync(request);
            return answer.IsSuccessStatusCode ? await answer.Content.ReadAsByteArrayAsync() : null;
        }
        catch (Exception)
        {
            return null;
        }
    }
}
