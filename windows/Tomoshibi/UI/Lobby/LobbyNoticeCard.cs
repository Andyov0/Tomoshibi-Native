using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// The deployment's notice (Remote.Notice) at the top of the lobby
/// (Android's LobbyNotice.kt, LobbyNoticeCard.swift on Apple): what it says,
/// in the reader's language, and a close that puts this one away for good,
/// until the deployment puts up one with another id. Shown only while the
/// lobby is the screen, so never over a call. It arrives opening a room for
/// itself, fading in as it does, and leaves the same way, closing the room
/// up behind it (Unfolding); new words for the same notice cross-fade.
///
/// Drawn as the update's card is in a column (UpdateBanner), being content
/// the person reads rather than a control over a picture: the surface colour
/// with the theme's edge, and where the deployment calls it a warning the red
/// edge and mark the app draws a problem with. In the flow, with no shadow,
/// so nothing about it is raised (K.Holding): the space it opens is what
/// moves, and the card inside it only fades.
/// </summary>
internal sealed class LobbyNoticeCard : Grid
{
    private readonly Remote _remote;
    private readonly Thickness _padding;
    private readonly Unfolding _space = new();
    private string? _said;

    /// <param name="padding">Kept around the card, only while it is there: with no notice it takes no room at all.</param>
    public LobbyNoticeCard(Remote remote, Thickness padding)
    {
        _remote = remote;
        _padding = padding;
        Children.Add(_space);
        Loaded += (_, _) =>
        {
            remote.Changed -= Update;
            remote.Changed += Update;
            Update();
        };
        Unloaded += (_, _) => remote.Changed -= Update;
        Update();
    }

    private void Update()
    {
        var notice = _remote.Notice;
        // Its words are the deployment's, in the language on screen, and do
        // not go through T(); a language it has nothing in shows no notice.
        var words = notice?.In(Localization.Shared.Language);
        if (notice == null || words == null)
        {
            _said = null;
            _space.Show(null);
            return;
        }
        var said = $"{notice.Id}|{notice.Warning}|{words}";
        if (said == _said) return;
        _said = said;
        _space.Show(Card(notice, words));
    }

    private UIElement Card(LobbyNotice notice, string words)
    {
        var tint = notice.Warning ? Ink.Danger : Ink.Tally;
        var mark = K.Symbol(notice.Warning ? "exclamationmark.circle.fill" : "megaphone.fill", 13, tint);
        mark.HorizontalAlignment = HorizontalAlignment.Center;
        mark.VerticalAlignment = VerticalAlignment.Center;
        var circle = new Border
        {
            Width = 30,
            Height = 30,
            CornerRadius = new CornerRadius(15),
            Background = notice.Warning ? Ink.DangerAt(0.18f) : Ink.TallyAt(0.18f),
            VerticalAlignment = VerticalAlignment.Top,
            Child = mark,
        };
        var text = K.Text(words, 13.5, lines: 8);
        text.VerticalAlignment = VerticalAlignment.Center;
        text.Margin = new Thickness(0, 5, 0, 5);
        var close = Tap.Icon("xmark", T("Dismiss"), Look.Plain, 26, 10.5);
        close.Named(T("Dismiss"), "lobby.notice.close");
        close.VerticalAlignment = VerticalAlignment.Top;
        close.Margin = new Thickness(0, 2, 0, 0);
        close.Click += (_, _) => _remote.Close(notice);
        var card = new Border
        {
            Background = Ink.Surface,
            BorderBrush = notice.Warning ? Ink.DangerAt(0.45f) : Ink.Border,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(20),
            Padding = new Thickness(14, 12, 10, 12),
            Child = K.Line(12, K.Fixed(circle), K.Flex(text), K.Fixed(close)),
        }.Named(words, "lobby.notice");
        // As wide as there is room for, up to 560, from the leading edge.
        var holder = new Grid { Padding = _padding };
        holder.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star), MaxWidth = 560 });
        holder.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        holder.Children.Add(card);
        return holder;
    }
}
