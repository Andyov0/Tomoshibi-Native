using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model.Music;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Music;

/// <summary>One of the music panel's tabs, brought up to date when the desk changes.</summary>
internal abstract class MusicTabView : Grid
{
    public abstract void Update();
}

/// <summary>
/// The music panel: the song desk, and the library to choose from
/// (Android's ui/music/MusicPanel.kt; MusicPanelView.swift; MusicPanel.tsx).
///
/// Three tabs: the desk, which everybody has; and search and playlists, for
/// whoever can reach the library. Choosing a song adds it to the desk, and
/// starts the desk here if nobody runs one; somebody who cannot reach the
/// library asks the desk instead, by a song's name or a link as it was
/// shared. The volume beside what is playing is the listener's own: for
/// somebody listening it is the setting the sound panel keeps for that
/// person's music, so muting here stops it at the media server; for whoever
/// plays it, it is their own speakers alone.
/// </summary>
internal sealed class MusicPanel : Grid
{
    private readonly SongDesk _desk;
    private readonly Grid _tabs = new();
    private readonly Grid _body = new();
    private string _tabsSaid = "";
    private MusicTab? _shown;
    private MusicTabView? _content;

    public MusicPanel(SongDesk desk, Action close)
    {
        _desk = desk;
        var column = new Grid();
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        column.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        column.Children.Add(MediaKit.Header(T("Music"), T("Close music"), close, "music.note"));
        SetRow(_tabs, 1);
        column.Children.Add(_tabs);
        var rule = MediaKit.Rule();
        SetRow(rule, 2);
        column.Children.Add(rule);
        SetRow(_body, 3);
        column.Children.Add(_body);
        Children.Add(MediaKit.Glass(column));
        Loaded += (_, _) =>
        {
            desk.Changed -= Update;
            desk.Changed += Update;
            // While it is open the bar keeps its button, even after the music
            // has stopped, so the button that opened it is there to close it.
            desk.PanelOpen = true;
            Update();
        };
        Unloaded += (_, _) =>
        {
            desk.Changed -= Update;
            desk.PanelOpen = false;
        };
        Update();
    }

    public void Update()
    {
        // The library's tabs only for somebody who can reach the library.
        var library = _desk.Libraries != null;
        var said = $"{library}|{_desk.Tab}|{Localization.Shared.Language}";
        if (said != _tabsSaid)
        {
            _tabsSaid = said;
            _tabs.Children.Clear();
            if (library)
            {
                var tabs = new[] { MusicTab.Desk, MusicTab.Search, MusicTab.Playlist };
                var choice = new Segmented(new List<string> { T("Song desk"), T("Search"), T("Playlist") }, Array.IndexOf(tabs, _desk.Tab), fill: true)
                {
                    Margin = new Thickness(12, 0, 12, 10),
                };
                choice.Named(T("Music"), "music.tabs");
                choice.Chose += index =>
                {
                    _desk.Tab = tabs[index];
                    Update();
                };
                _tabs.Children.Add(choice.Arrives());
            }
        }
        var tab = library ? _desk.Tab : MusicTab.Desk;
        if (tab != _shown)
        {
            _shown = tab;
            _body.Children.Clear();
            _content = tab switch
            {
                MusicTab.Search => new SearchTab(_desk),
                MusicTab.Playlist => new PlaylistTab(_desk),
                _ => new DeskTab(_desk),
            };
            _body.Children.Add(_content.Arrives());
        }
        _content?.Update();
    }
}
