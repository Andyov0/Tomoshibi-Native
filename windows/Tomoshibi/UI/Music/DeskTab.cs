using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.Model.Music;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Room;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Music;

/// <summary>
/// The desk (MusicPanel.kt <c>DeskTab</c>): what is playing, what is coming,
/// and the box a song is asked for in. Each part is drawn again only when
/// what it says changes, so a vote or a song added does not fade the card
/// of what is playing in again.
/// </summary>
internal sealed class DeskTab : MusicTabView
{
    private readonly SongDesk _desk;
    private readonly Keyed _list = new(2) { Padding = new Thickness(12) };
    private readonly Grid _addButton = new() { Width = 34, Height = 34 };
    private readonly Grid _asker;
    private readonly DispatcherQueueTimer _clock;
    private SoundPanel.SoundRow? _hearing;
    private string? _hearingOf;
    private Grid? _progress;
    private Border? _progressFill;
    private TextBlock? _progressAt, _progressOf;
    private TextBox? _box;
    private string _typed = "";

    public DeskTab(SongDesk desk)
    {
        _desk = desk;
        RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Children.Add(new ScrollViewer { Content = _list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        var rule = MediaKit.Rule();
        SetRow(rule, 1);
        Children.Add(rule);
        var field = MediaKit.Field(T("A song's name, or a playlist link"), "", text =>
        {
            _typed = text;
            PaintAdd();
        }, Ask, "music.note.list", 2000, out var box);
        _box = box;
        box.Named(T("Ask for a song"), "music.ask");
        var line = K.Line(8, K.Flex(field), K.Fixed(_addButton));
        line.Padding = new Thickness(12);
        _asker = new Grid { Children = { line } };
        SetRow(_asker, 2);
        Children.Add(_asker);
        // Where the song playing from here is, read twice a second: only the
        // holder knows, and a holder with a queue wants to see how long is left.
        _clock = DispatcherQueue.GetForCurrentThread().CreateTimer();
        _clock.Interval = TimeSpan.FromMilliseconds(500);
        _clock.Tick += (_, _) => Tick();
        Loaded += (_, _) => _clock.Start();
        Unloaded += (_, _) => _clock.Stop();
        PaintAdd();
    }

    private bool CanAsk => _desk.State != null || _desk.Libraries != null;

    private void Ask()
    {
        // Kept while the last ask is still being looked up, as the web's box
        // refuses a second one then.
        if (_typed.Trim().Length == 0 || _desk.Asking || !CanAsk) return;
        _desk.Ask(_typed);
        _typed = "";
        if (_box != null) _box.Text = "";
    }

    private void PaintAdd()
    {
        _addButton.Children.Clear();
        if (_desk.Asking)
        {
            _addButton.Children.Add(K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center).Arrives());
            return;
        }
        var add = MediaKit.IconButton("plus", T("Ask for a song"), Ask, 34, prominent: _typed.Trim().Length > 0);
        add.Named(T("Ask for a song"), "music.ask.go");
        _addButton.Children.Add(add);
    }

    public override void Update()
    {
        _asker.Opacity = CanAsk ? 1 : 0.5;
        _asker.IsHitTestVisible = CanAsk;
        PaintAdd();
        var state = _desk.State;
        var language = Localization.Shared.Language.ToString();
        var parts = new List<(string, string, Func<UIElement>)>();
        if (state is { Now: { } now })
        {
            var voted = state.Votes.Contains(_desk.Call.LocalIdentity);
            parts.Add(("now " + now.Key, $"{state.Paused}|{state.Quality}|{_desk.Mine}|{state.Votes.Length}/{state.Needed}|{voted}|{state.Loading}|{language}", () => NowPlaying(state, now)));
        }
        else if (_desk.Player is { } player)
        {
            // Music arriving from somebody whose desk is not heard here (in a
            // sealed call desks say nothing) still has a volume.
            parts.Add(("player " + player.Id, $"{player.Shown}|{language}", () => PlayerCard(player)));
        }
        else
        {
            parts.Add(("idle", $"{state != null}|{_desk.Libraries != null}|{_desk.Mine}|{language}", Idle));
        }
        if (state?.Next is { } next)
        {
            parts.Add(("next " + next.Key, $"{_desk.Mine}|{language}", () => Loading(next)));
        }
        if (state is { Queue.Length: > 0 })
        {
            parts.Add(("queue", language, () => Menu.Heading(T("Queue")).Pad(0, 12, 0, 0)));
            for (var i = 0; i < state.Queue.Length; i++)
            {
                var entry = state.Queue[i];
                var index = i;
                parts.Add(("entry " + entry.Key, $"{index}|{_desk.Mine}|{language}", () => QueueRow(entry, index)));
            }
        }
        _list.Show(parts);
        UpdateHearing();
        Tick();
    }

    // What is playing

    /// <summary>What is playing, on a card of its own (MusicPanel.kt <c>NowPlaying</c>).</summary>
    private FrameworkElement NowPlaying(DeskState state, DeskEntry now)
    {
        _hearing = null;
        _hearingOf = null;
        _progress = null;
        var words = K.Column(3, K.Text(now.Title, 14.5, K.Semibold, lines: 2));
        if (now.Artists.Length > 0) words.Children.Add(K.Text(string.Join(" / ", now.Artists), 12, ink: Ink.ForegroundAt(0.72f), lines: 1));
        words.Children.Add(K.Text(T("Asked for by {name}", ("name", now.By)), 11, ink: Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var column = K.Column(12, K.Line(12, K.Fixed(MediaKit.Artwork(now.Cover, 66, 66, corner: 10)), K.Flex(words)));
        if (state.Paused || state.Quality != null)
        {
            var badges = K.Row(6);
            if (state.Paused) badges.Children.Add(MediaKit.Badge(T("Paused"), "pause.fill"));
            // What the file is, once it is decoded: "FLAC · 24-bit · 44.1 kHz".
            if (state.Quality is { } quality) badges.Children.Add(MediaKit.Badge(quality, "waveform", lit: true));
            column.Children.Add(badges.Arrives());
        }
        if (_desk.Mine)
        {
            column.Children.Add(Progress());
            column.Children.Add(Monitor());
        }
        else if (_desk.Call.PersonOf(state.Holder) is { } holder)
        {
            _hearing = new SoundPanel.SoundRow(HostFeature.Of(_desk.Call), Sound.Music);
            _hearingOf = holder.Id;
            column.Children.Add(_hearing);
        }
        // This participant's own, like the volume above it: see duck.ts.
        var duck = Fields.Switch(_desk.Ducks, on => _desk.Ducks = on, T("Lower the music while anybody talks"));
        column.Children.Add(K.Line(8,
            K.Fixed(new Border { Width = 18, Child = K.Symbol("person.wave.2.fill", 11, Ink.Muted) }),
            K.Flex(K.Text(T("Lower the music while anybody talks"), 11.5, ink: Ink.Muted).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
            K.Fixed(duck)));
        column.Children.Add(Presses(state, now));
        var card = new Grid { CornerRadius = new CornerRadius(18), Background = Ink.White(0.05f), BorderBrush = Ink.White(0.09f), BorderThickness = new Thickness(1) };
        if (now.Cover.StartsWith("http", StringComparison.Ordinal))
        {
            // Lit from behind by its own cover, blurred and dimmed evenly
            // (MusicPanel.kt): the cover decoded a dozen pixels wide and
            // stretched over the card is the blur, and black at 45 per cent
            // over it, flat, is the dimming, where a gradient was until the
            // gradients went.
            var blurred = new Microsoft.UI.Xaml.Media.Imaging.BitmapImage { DecodePixelWidth = 12, UriSource = new Uri(now.Cover) };
            var behind = new Image { Source = blurred, Stretch = Microsoft.UI.Xaml.Media.Stretch.UniformToFill, Opacity = 0.55, RenderTransformOrigin = new Windows.Foundation.Point(0.5, 0.5), RenderTransform = new Microsoft.UI.Xaml.Media.ScaleTransform { ScaleX = 1.6, ScaleY = 1.6 } };
            card.Children.Add(behind);
            card.Children.Add(new Border { Background = Ink.Black(0.45f) });
        }
        column.Padding = new Thickness(14);
        card.Children.Add(column);
        return card;
    }

    private UIElement Presses(DeskState state, DeskEntry now)
    {
        if (_desk.Mine)
        {
            var play = MediaKit.IconButton(state.Paused ? "play.fill" : "pause.fill", state.Paused ? T("Play") : T("Pause"), () => _desk.Pause(!state.Paused), 34, prominent: true);
            play.Named(state.Paused ? T("Play") : T("Pause"), "music.pause");
            var next = MediaKit.Button(T("Next song"), "forward.fill", _desk.Skip, height: 34);
            next.Named(T("Next song"), "music.skip");
            next.HorizontalAlignment = HorizontalAlignment.Stretch;
            // A round button with its words for the screen reader: written
            // out, they took the room the next song's button needs.
            var stop = MediaKit.IconButton("stop.circle", T("Stop the desk"), _desk.Close, 34);
            stop.Named(T("Stop the desk"), "music.close");
            return K.Line(8, K.Fixed(play), K.Flex(next), K.Fixed(stop));
        }
        // Not while the next is on its way: the desk does not count those
        // votes (jukebox.ts `Desk.vote`).
        var voted = state.Votes.Contains(_desk.Call.LocalIdentity);
        var vote = MediaKit.Button(T("Vote to skip ({votes}/{needed})", ("votes", state.Votes.Length), ("needed", (int)state.Needed)), voted ? "checkmark" : "forward.fill", _desk.Vote, height: 34);
        vote.Named(T("Vote to skip ({votes}/{needed})", ("votes", state.Votes.Length), ("needed", (int)state.Needed)), "music.vote");
        vote.HorizontalAlignment = HorizontalAlignment.Stretch;
        vote.IsEnabled = !voted && !state.Loading;
        return vote;
    }

    /// <summary>How far through the song playing from here is, as a thin bar in the tally, with the two times under it.</summary>
    private FrameworkElement Progress()
    {
        _progressFill = new Border { Height = 3, CornerRadius = new CornerRadius(1.5), Background = Ink.Tally, HorizontalAlignment = HorizontalAlignment.Left, Width = 0 };
        var track = new Grid { Height = 3, CornerRadius = new CornerRadius(1.5), Background = Ink.White(0.12f), Children = { _progressFill } };
        _progressAt = K.Text("", 10, K.Medium, Ink.Muted, lines: 1, tabular: true);
        _progressOf = K.Text("", 10, K.Medium, Ink.Muted, lines: 1, tabular: true);
        var times = K.Line(0, K.Fixed(_progressAt), K.Flex(K.Spacer()), K.Fixed(_progressOf));
        _progress = new Grid { Children = { K.Column(4, track, times) }, Visibility = Visibility.Collapsed };
        _progress.Arrives();
        return _progress;
    }

    private void Tick()
    {
        if (_progress == null || _progressFill == null) return;
        var shown = _desk.Progress is { Of: > 0 } progress ? progress : ((double At, double Of)?)null;
        _progress.Visibility = shown != null ? Visibility.Visible : Visibility.Collapsed;
        if (shown is not { } p) return;
        var width = (_progressFill.Parent as FrameworkElement)?.ActualWidth ?? 0;
        _progressFill.Width = Math.Max(0, width * Math.Clamp(p.At / p.Of, 0, 1));
        _progressAt!.Text = TomoshibiProtocolMethods.MusicClock(p.At);
        _progressOf!.Text = TomoshibiProtocolMethods.MusicClock(p.Of);
    }

    /// <summary>How loud the music playing from here is in this computer's own speakers; what everybody else hears is untouched.</summary>
    private UIElement Monitor()
    {
        var amount = K.Text($"{Math.Round(_desk.MonitorVolume * 100)}%", 11, K.Medium, Ink.Muted, lines: 1, align: TextAlignment.Right, tabular: true);
        amount.Width = 38;
        amount.VerticalAlignment = VerticalAlignment.Center;
        var slider = MediaKit.Slider(_desk.MonitorVolume, level =>
        {
            _desk.MonitorVolume = (float)level;
            amount.Text = $"{Math.Round(level * 100)}%";
        }, 0, 1, 0.05, label: T("Music volume in your speakers"));
        return K.Line(8,
            K.Fixed(new Border { Width = 18, Child = K.Symbol("speaker.wave.2.fill", 11, Ink.Muted) }),
            K.Fixed(K.Text(T("In your speakers"), 11.5, ink: Ink.Muted, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
            K.Flex(slider),
            K.Fixed(amount));
    }

    private FrameworkElement PlayerCard(Person player)
    {
        _hearing = new SoundPanel.SoundRow(HostFeature.Of(_desk.Call), Sound.Music);
        _hearingOf = player.Id;
        _progress = null;
        return new Border
        {
            Child = _hearing,
            CornerRadius = new CornerRadius(16),
            Background = Ink.White(0.05f),
            Padding = new Thickness(12),
        };
    }

    /// <summary>The listener's own setting for whoever's music is heard, kept in step with the sound panel's.</summary>
    private void UpdateHearing()
    {
        if (_hearing == null || _hearingOf == null) return;
        if (_desk.Call.PersonOf(_hearingOf) is not { } person) return;
        var name = _desk.State != null ? T("Music") : T("{name} (music)", ("name", person.Shown));
        _hearing.Show(person, name, "music.note");
    }

    /// <summary>Nothing playing: what to do about it, and for whoever runs an idle desk the way to put it away (it still tells the room it is there every fifteen seconds).</summary>
    private FrameworkElement Idle()
    {
        _hearing = null;
        _hearingOf = null;
        _progress = null;
        var words = _desk.State != null
            ? T("Nothing is playing. Ask for a song below.")
            : _desk.Libraries != null
                ? T("Search a song or paste a playlist link to start the song desk.")
                : T("When somebody plays music in this call, it shows up here.");
        var column = K.Column(4, MediaKit.Empty("music.note.list", words));
        if (_desk.Mine)
        {
            var stop = MediaKit.Button(T("Stop the desk"), "stop.circle", _desk.Close, quiet: true);
            stop.HorizontalAlignment = HorizontalAlignment.Center;
            column.Children.Add(stop.Arrives());
        }
        return column;
    }

    // What is coming

    private FrameworkElement Loading(DeskEntry next)
    {
        var parts = new List<(UIElement, double)>
        {
            K.Fixed(K.Spinner(13)),
            K.Flex(K.Text(T("Loading {title}", ("title", next.Title)), 12, ink: Ink.Muted, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
        };
        // Out of the queue already, and still the holder's to take out: it
        // was on the list a moment ago.
        if (_desk.Mine) parts.Add(K.Fixed(MediaKit.IconButton("xmark", T("Take {title} off", ("title", next.Title)), () => _desk.Remove(next.Key), 24)));
        var line = K.Line(10, parts.ToArray());
        line.Padding = new Thickness(10, 12, 10, 0);
        return line;
    }

    private FrameworkElement QueueRow(DeskEntry entry, int index)
    {
        var number = K.Text($"{index + 1}", 11, K.Medium, Ink.Muted, lines: 1, align: TextAlignment.Right, tabular: true);
        number.Width = 16;
        number.VerticalAlignment = VerticalAlignment.Center;
        var detail = string.Join(" · ", new[] { string.Join(" / ", entry.Artists), T("Asked for by {name}", ("name", entry.By)) }.Where(s => s.Length > 0));
        var words = K.Column(1, K.Text(entry.Title, 12.5, K.Medium, lines: 1), K.Text(detail, 10.5, ink: Ink.Muted, lines: 1));
        words.VerticalAlignment = VerticalAlignment.Center;
        var parts = new List<(UIElement, double)> { K.Fixed(number), K.Fixed(MediaKit.Artwork(entry.Cover, 34, 34, corner: 6)), K.Flex(words) };
        if (_desk.Mine)
        {
            if (index > 0)
            {
                var top = MediaKit.IconButton("arrow.up.to.line", T("Play {title} next", ("title", entry.Title)), () => _desk.ToTop(entry.Key), 24);
                top.Named(T("Play {title} next", ("title", entry.Title)), "music.top." + entry.Key);
                parts.Add(K.Fixed(top));
            }
            var off = MediaKit.IconButton("xmark", T("Take {title} off", ("title", entry.Title)), () => _desk.Remove(entry.Key), 24);
            off.Named(T("Take {title} off", ("title", entry.Title)), "music.remove." + entry.Key);
            parts.Add(K.Fixed(off));
        }
        var row = MediaKit.Row(K.Line(10, parts.ToArray()));
        row.Margin = new Thickness(0, 2, 0, 0);
        return row;
    }
}
