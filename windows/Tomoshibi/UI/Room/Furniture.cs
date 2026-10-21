using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Shapes;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// The connection, as bars and, when asked, its figures (Android's
/// kit/Signal.kt): bars for the grade, the round trip beside them in the
/// detailed badge, and the figures under it when it is opened.
/// </summary>
internal sealed class SignalBadge : StackPanel
{
    private readonly CallModel _call;
    private readonly Tap _capsule;
    private readonly StackPanel _bars = new() { Orientation = Orientation.Horizontal, Spacing = 2, VerticalAlignment = VerticalAlignment.Bottom };
    private readonly TextBlock _rtt = K.Text("", 11.5, K.Medium, Ink.Muted, tabular: true);
    private readonly StackPanel _detail = K.Column(5);
    private readonly Border _detailCard;
    private readonly Border _detailHolder;

    public SignalBadge(CallModel call)
    {
        _call = call;
        Spacing = 6;
        var row = K.Row(7, _bars, _rtt);
        row.VerticalAlignment = VerticalAlignment.Center;
        _capsule = new Tap(row, Look.Clear, 28, padding: 9);
        _capsule.Click += (_, _) =>
        {
            if (Appearance.Shared.ConnectionBadge == ConnectionBadge.Detailed) call.ConnectionOpen = !call.ConnectionOpen;
        };
        Children.Add(K.Glass(_capsule, 14, shadow: false).At(HorizontalAlignment.Left, VerticalAlignment.Top));
        _detail.Padding = new Thickness(12, 10, 12, 10);
        _detailCard = K.Glass(_detail, 14);
        _detailCard.Width = 236;
        _detailCard.HorizontalAlignment = HorizontalAlignment.Left;
        _detailHolder = K.Holding(_detailCard).Arrives(0.94f, new Vector2(0, 0));
        Children.Add(_detailHolder);
        Update();
    }

    private static ConnectionGrade Grade(CallModel call) => call.Connection?.Grade ?? call.LocalQuality switch
    {
        LinkQuality.Poor => ConnectionGrade.Poor,
        LinkQuality.Lost => ConnectionGrade.Lost,
        _ => ConnectionGrade.Good,
    };

    public void Update()
    {
        var detailed = Appearance.Shared.ConnectionBadge == ConnectionBadge.Detailed;
        var grade = Grade(_call);
        var lit = grade switch { ConnectionGrade.Good => 3, ConnectionGrade.Fair => 2, ConnectionGrade.Poor => 1, _ => 0 };
        var colour = grade switch { ConnectionGrade.Good => Ink.Good, ConnectionGrade.Fair => Ink.Tally, _ => Ink.Danger };
        _bars.Children.Clear();
        var heights = new[] { 6.0, 10.0, 14.0 };
        for (var i = 0; i < 3; i++)
        {
            _bars.Children.Add(new Rectangle
            {
                Width = 3.5,
                Height = heights[i],
                RadiusX = 1,
                RadiusY = 1,
                VerticalAlignment = VerticalAlignment.Bottom,
                Fill = i < lit ? colour : Ink.MutedAt(0.35f),
            });
        }
        var reading = _call.Connection;
        _rtt.Text = reading?.RttMs is { } rtt ? $"{Math.Round(rtt)} ms" : "—";
        _rtt.Visibility = detailed ? Visibility.Visible : Visibility.Collapsed;
        _capsule.Padding = new Thickness(detailed ? 11 : 9, 0, detailed ? 11 : 9, 0);
        _capsule.Named(grade switch
        {
            ConnectionGrade.Lost => T("Connection lost"),
            ConnectionGrade.Poor => T("Poor connection"),
            ConnectionGrade.Fair => T("Fair connection"),
            _ => T("Good connection"),
        });
        var open = detailed && _call.ConnectionOpen;
        _detailHolder.Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        if (!open) return;
        _detail.Children.Clear();
        void Figure(string label, string value, bool warn = false) =>
            _detail.Children.Add(K.Line(16, K.Flex(K.Text(label, 11.5, ink: Ink.Muted, lines: 1)), K.Fixed(K.Words(value, 11.5, ink: warn ? Ink.Tally : Ink.Foreground, lines: 1, tabular: true))));
        if (_call.Relay is { } relay && _call.Holding is { } holding && holding != relay)
        {
            Figure(T("Relay server"), Say(relay));
            Figure(T("Original server"), Say(holding));
        }
        else if (_call.Relay is { } only)
        {
            Figure(T("Server"), Say(only));
        }
        if (reading == null || !reading.Measured)
        {
            _detail.Children.Add(K.Text(T("Measuring…"), 11.5, ink: Ink.Muted));
            return;
        }
        if (reading.OwnAddress is { } address) Figure(T("Your address"), address);
        Figure(T("Round trip"), reading.RttMs is { } ms ? $"{Math.Round(ms)} ms" : "—");
        Figure(T("Lost"), reading.LossPercent is { } loss ? $"{loss:F1}%" : "—", (reading.LossPercent ?? 0) > 2);
        Figure(T("Jitter"), reading.JitterMs is { } jitter ? $"{Math.Round(jitter)} ms" : "—");
        Figure(T("Sending"), Rate(reading.UpKbps));
        Figure(T("Receiving"), Rate(reading.DownKbps));
    }

    private static string Rate(double? kbps) => kbps switch
    {
        null => "—",
        >= 1000 => $"{kbps / 1000:F1} Mbps",
        _ => $"{Math.Round(kbps.Value)} kbps",
    };
}

/// <summary>
/// The room's name at the top of the call (RoomTitle.kt), and the card for
/// a room nobody else has come to yet (<c>EmptyRoomCard</c>): the way to
/// bring somebody, which is the room's address where the door lets anybody
/// in by the name, an invitation the host makes where it does not, and
/// otherwise who can.
/// </summary>
internal static class RoomTitle
{
    public static Border Title(string room)
    {
        var text = K.Text(room, 13, K.Semibold, lines: 1);
        text.TextTrimming = TextTrimming.CharacterEllipsis;
        var glass = K.Glass(text, 15, new Thickness(12, 0, 12, 0), shadow: false);
        glass.Height = 30;
        text.VerticalAlignment = VerticalAlignment.Center;
        glass.VerticalAlignment = VerticalAlignment.Top;
        return glass;
    }

    public static Border EmptyRoomCard(AppModel app, CallModel call, bool linkWorks, string? link, Action dismiss)
    {
        var host = call.Hosting;
        var row = K.Row(0);
        row.VerticalAlignment = VerticalAlignment.Center;
        if (!linkWorks && !host)
        {
            row.Children.Add(K.Text(T("Only whoever runs this room can let somebody else in."), 12.5, ink: Ink.Muted, lines: 2).Pad(16, 8, 6, 8));
        }
        else
        {
            var symbol = K.Symbol(linkWorks ? "doc.on.doc" : "link", 13);
            var words = K.Text(linkWorks ? T("Copy link") : T("Invite somebody"), 13, K.Medium, lines: 1);
            var button = new Tap(K.Row(7, symbol, words), Look.Clear, 34, padding: 12) { CornerRadius = new CornerRadius(17) };
            button.Named(linkWorks ? T("Copy link") : T("Invite somebody"));
            button.Click += async (_, _) =>
            {
                string? copy = link;
                if (!linkWorks)
                {
                    button.IsEnabled = false;
                    try
                    {
                        var made = await call.Deployment.CreateInvite(call.Room, call.Core.Token());
                        copy = call.Deployment.InviteLink(made.Token, call.Room);
                    }
                    catch (Exception fault)
                    {
                        call.ActionFailed(Faults.Code(fault) == "not_yours" ? T("You no longer run this room.") : Faults.Code(fault) == "rate_limited" ? T("Too many attempts. Try again in a moment.") : T("That could not be done. Try again."));
                    }
                    finally
                    {
                        button.IsEnabled = true;
                    }
                }
                if (copy == null) return;
                Pasteboard.Copy(copy);
                button.Content = K.Row(7, K.Symbol("checkmark", 13, Ink.Tally), K.Text(T("Link copied"), 13, K.Medium, lines: 1));
                await Task.Delay(1600);
                button.Content = K.Row(7, K.Symbol(linkWorks ? "doc.on.doc" : "link", 13), K.Text(linkWorks ? T("Copy link") : T("Invite somebody"), 13, K.Medium, lines: 1));
            };
            row.Children.Add(button);
        }
        row.Children.Add(new Border { Width = 1, Height = 16, Background = Ink.White(0.12f), VerticalAlignment = VerticalAlignment.Center });
        var close = Tap.Icon("xmark", T("Close"), Look.Clear, 34, 10.5);
        close.Click += (_, _) => dismiss();
        row.Children.Add(close);
        var card = K.Glass(row, 20);
        card.MaxWidth = 420;
        return card;
    }

    /// <summary>Something still true, as a line over the call ("Reconnecting…").</summary>
    public static Border Notice(string text, bool danger = false)
    {
        var glass = K.Glass(K.Text(text, 13, K.Medium, danger ? Ink.Brush(Theme.DangerForeground) : Ink.Foreground, lines: 1), 16, new Thickness(14, 8, 14, 8));
        if (danger) glass.Background = Ink.Danger;
        glass.HorizontalAlignment = HorizontalAlignment.Center;
        glass.VerticalAlignment = VerticalAlignment.Top;
        return glass;
    }
}
