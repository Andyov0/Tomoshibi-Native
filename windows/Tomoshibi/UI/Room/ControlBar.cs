using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// The call's controls (Android's ui/room/ControlBar.kt): the microphone,
/// the camera, sharing, the hand with its reactions, the messages, the
/// devices, and leaving, on a capsule of the control layer's material at
/// the foot. Where the full bar does not fit the window it goes compact, the
/// panels into a More menu, as the other apps' does at the same lengths.
///
/// TODO(windows 2b): sharing a screen, watching together, music and the host
/// panel are the next phase's; the share button is here, where it is on the
/// other platforms, and says that it is not yet offered.
/// </summary>
internal sealed class ControlBar : Grid
{
    private readonly RoomScreen _room;
    private readonly CallModel _call;
    private readonly StackPanel _row = new() { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
    private readonly Border _capsule;
    private bool? _compact;
    private bool _devicePressing;
    private Tap? _microphone, _camera, _messages, _hand, _devices, _more, _leave, _reactionsHalf;
    private Border? _unread;

    public ControlBar(RoomScreen room, CallModel call)
    {
        _room = room;
        _call = call;
        _capsule = K.Glass(_row, 33, new Thickness(7));
        _capsule.HorizontalAlignment = HorizontalAlignment.Center;
        _capsule.VerticalAlignment = VerticalAlignment.Bottom;
        Children.Add(_capsule);
        IsTabStop = false;
    }

    /// <summary>The full bar's length (BarLength): ten buttons of 44 and the leave of 56, the rule, the gaps and the ends.</summary>
    public static double FullLength => 7 * 2 + 44 * 5 + 60 + 44 + 5 + 56 + 6 * 8;

    /// <summary>Lays the bar out for a window this wide: compact once the full bar would come within 20 of its edges.</summary>
    public void Fit(double width)
    {
        var compact = FullLength > width - 20;
        if (compact == _compact) return;
        _compact = compact;
        Build();
    }

    private double Size => _compact == true ? 40 : 44;

    private void Build()
    {
        _row.Children.Clear();
        var compact = _compact == true;
        _row.Spacing = compact ? 4 : 6;
        _capsule.Padding = new Thickness(compact ? 5 : 7);
        _capsule.CornerRadius = new CornerRadius((Size + (compact ? 10 : 14)) / 2);

        _microphone = Bar("mic.fill", "");
        _microphone.Click += async (_, _) => await Device(() => _call.SetMicrophone(!_call.MicrophoneOn));
        _camera = Bar("video.fill", "");
        _camera.Click += async (_, _) => await Device(() => _call.SetCamera(!_call.CameraOn));
        var share = Bar("rectangle.on.rectangle", T("Share your screen"));
        share.IsEnabled = false;
        _row.Children.Add(_microphone);
        _row.Children.Add(_camera);
        _row.Children.Add(share);
        _row.Children.Add(HandControl());
        _messages = Bar("bubble.left.fill", "");
        _messages.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Messages);
        _unread = new Border
        {
            Width = 9,
            Height = 9,
            CornerRadius = new CornerRadius(4.5),
            Background = Ink.Tally,
            BorderBrush = Ink.SurfaceHigh,
            BorderThickness = new Thickness(2),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(0, 2, 2, 0),
            IsHitTestVisible = false,
            Visibility = Visibility.Collapsed,
        };
        _unread.Arrives();
        _row.Children.Add(new Grid { Children = { _messages, _unread } });
        if (compact)
        {
            _more = Bar("ellipsis", T("More"));
            _more.Click += (_, _) => Overlay.Current?.Toggle("more", _more, MoreMenu, width: 240, edge: Edge.Above, align: Align.End);
            _row.Children.Add(_more);
            _devices = null;
        }
        else
        {
            _devices = Bar("gearshape.fill", T("Devices"));
            _devices.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Devices);
            _row.Children.Add(_devices);
            _row.Children.Add(new Border { Width = 1, Height = 26, Background = Ink.Border, Margin = new Thickness(2, 0, 2, 0), VerticalAlignment = VerticalAlignment.Center });
        }
        _leave = new Tap(K.Symbol("phone.down.fill", 16, Ink.Brush(Basics.Theme.DangerForeground)), Look.Destructive, Size, label: T("Leave"));
        _leave.Width = compact ? 52 : 56;
        _leave.Padding = new Thickness(0);
        _leave.Named(T("Leave"), "bar.leave");
        _leave.Click += (_, _) => OpenLeave();
        _row.Children.Add(_leave);
        Update();
    }

    private Tap Bar(string symbol, string label)
    {
        var tap = new Tap(K.Symbol(symbol, Size * 0.36), Look.Bar, Size, circle: true);
        if (label.Length > 0) tap.Tip(label);
        return tap;
    }

    /// <summary>One device press at a time: a press while one is still starting is not a second device (ControlBar.kt <c>device</c>).</summary>
    private async Task Device(Func<Task> press)
    {
        if (_devicePressing) return;
        _devicePressing = true;
        try
        {
            await press();
        }
        finally
        {
            _devicePressing = false;
        }
    }

    public void Update()
    {
        if (_microphone == null) return;
        var mic = _call.MicrophoneOn;
        Paint(_microphone, mic ? "mic.fill" : "mic.slash.fill", mic ? T("Mute microphone") : T("Unmute microphone"), lit: false, quiet: !mic);
        _microphone.Named(mic ? T("Mute microphone") : T("Unmute microphone"), "bar.microphone");
        var cam = _call.CameraOn;
        Paint(_camera!, cam ? "video.fill" : "video.slash.fill", cam ? T("Turn camera off") : T("Turn camera on"), lit: false, quiet: !cam);
        _camera!.Named(cam ? T("Turn camera off") : T("Turn camera on"), "bar.camera");
        var chat = _room.Showing == RoomScreen.Panel.Messages;
        Paint(_messages!, "bubble.left.fill", chat ? T("Hide messages") : T("Show messages"), lit: chat, quiet: false);
        _messages!.Named(chat ? T("Hide messages") : T("Show messages"), "bar.messages");
        _unread!.Visibility = _call.Unread && !chat ? Visibility.Visible : Visibility.Collapsed;
        if (_devices != null) _devices.Lit = _room.Showing == RoomScreen.Panel.Devices;
        if (_more != null) _more.Lit = _room.Showing == RoomScreen.Panel.Devices;
        if (_hand != null)
        {
            _hand.Lit = _call.HandUp;
            _hand.Content = K.Symbol("hand.raised.fill", 16, Tap.InkFor(Look.Bar, _call.HandUp));
            _hand.Tip(_call.HandUp ? T("Lower your hand") : T("Raise your hand"));
        }
    }

    private void Paint(Tap tap, string symbol, string label, bool lit, bool quiet)
    {
        tap.Lit = lit;
        tap.Content = K.Symbol(symbol, Size * 0.36, lit ? Ink.TallyForeground : quiet ? Ink.Muted : Ink.Foreground);
        tap.Tip(label);
    }

    /// <summary>
    /// The hand and, beside it, the way to the reactions (Reactions.kt
    /// <c>HandControl</c>): a press raises or lowers the hand, the half
    /// beside it opens the six reactions a browser shows.
    /// </summary>
    private UIElement HandControl()
    {
        _hand = new Tap(K.Symbol("hand.raised.fill", 16), Look.Bar, Size);
        _hand.Width = 40;
        _hand.Padding = new Thickness(0);
        _hand.CornerRadius = new CornerRadius(22, 0, 0, 22);
        _hand.Named(T("Raise your hand"), "bar.hand");
        _hand.Click += async (_, _) =>
        {
            try
            {
                await _call.SetHand(!_call.HandUp);
            }
            catch (Exception)
            {
                _call.Announce(new CallToast(T("That could not be done. Try again."), Kind: CallToast.Kinds.Failure, Lasts: 4.0));
            }
        };
        var chevron = K.Symbol("chevron.up", 9, Ink.Muted);
        _reactionsHalf = new Tap(chevron, Look.Bar, Size) { Width = 20, CornerRadius = new CornerRadius(0, 22, 22, 0), Padding = new Thickness(0) };
        _reactionsHalf.Tip(T("Reactions"));
        _reactionsHalf.Click += (_, _) => Overlay.Current?.Toggle("reactions", _reactionsHalf, Reactions, width: 234, edge: Edge.Above, align: Align.Centre);
        return K.Row(0, _hand, _reactionsHalf);
    }

    private UIElement Reactions()
    {
        var row = K.Row(2);
        row.Padding = new Thickness(4);
        foreach (var what in CallModel.ReactionChoices)
        {
            var glyph = new TextBlock { Text = what, FontSize = 20, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            var tap = new Tap(glyph, Look.Clear, 34, padding: 0) { Width = 36, CornerRadius = new CornerRadius(8) };
            tap.Named(what, "reaction." + CallModel.ReactionChoices.ToList().IndexOf(what));
            tap.Click += (_, _) =>
            {
                _call.React(what);
                Overlay.Current?.ClosePopover();
            };
            row.Children.Add(tap);
        }
        return row;
    }

    private UIElement MoreMenu() => Menu.Card(
        Menu.Item(T("Devices"), () => { Overlay.Current?.ClosePopover(); _room.Toggle(RoomScreen.Panel.Devices); }, "gearshape.fill", check: _room.Showing == RoomScreen.Panel.Devices),
        Menu.Rule(),
        Menu.Item(T("Reactions"), () => { Overlay.Current?.ClosePopover(); Overlay.Current?.Toggle("reactions", _reactionsHalf!, Reactions, width: 234, edge: Edge.Above, align: Align.Centre); }, "sparkles"));

    /// <summary>
    /// Leaving (ControlBar.kt <c>LeaveControl</c>): the popover is the
    /// question. Leaving the call; and, to whoever runs the room, ending it
    /// for everybody, which one press does.
    /// </summary>
    public void OpenLeave()
    {
        if (_leave == null) return;
        Overlay.Current?.Toggle("leave", _leave, () =>
        {
            var items = new List<UIElement>
            {
                Menu.Item(T("Leave the call"), () =>
                {
                    Overlay.Current?.ClosePopover();
                    _ = _room.App.LeaveCall(_call);
                }, "rectangle.portrait.and.arrow.right", T("The meeting carries on without you.")),
            };
            if (_call.Hosting)
            {
                Tap? end = null;
                end = Menu.Item(T("End for everybody"), async () =>
                {
                    end!.IsEnabled = false;
                    try
                    {
                        await _call.EndForEverybody();
                        Overlay.Current?.ClosePopover();
                        await _room.App.LeaveCall(_call);
                    }
                    catch (Exception)
                    {
                        end.IsEnabled = true;
                        _call.Announce(new CallToast(T("That could not be done. Try again."), Kind: CallToast.Kinds.Failure, Lasts: 4.0));
                    }
                }, "door.left.hand.closed", T("Everybody is disconnected and links stop working."), role: Menu.Role.Destructive);
                end.Named(T("End for everybody"), "leave.end");
                items.Add(end);
            }
            return Menu.Card(items.ToArray());
        }, width: 272, edge: Edge.Above, align: Align.End);
    }
}
