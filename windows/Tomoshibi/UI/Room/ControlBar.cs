using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// What the features layered over the call say about their buttons
/// (ControlBar.kt <c>Offers</c>), read in the room so the bar does not hold
/// the features themselves: watching together to whoever can start a show
/// and to everybody once there is one, the music to whoever has a library
/// or while somebody sends music, the host panel only to whoever runs the
/// room, with the people at its door counted on it. The screen in the share's
/// menu unless the deployment has switched screen sharing off (Remote), and
/// then still while a share is running, which is not stopped by it and needs
/// its Stop.
/// </summary>
internal sealed record Offers(bool Watch = false, bool WatchPlaying = false, bool Music = false, bool MusicPlaying = false, bool Host = false, int Knocking = 0, bool ScreenShare = true);

/// <summary>What a feature says about its button, kept in the call's parts under its name (ControlBar.kt <c>BarOffer</c>).</summary>
internal interface IBarOffer
{
    const string Watch = "offer.watch";
    const string Music = "offer.music";
    bool Offered { get; }
    bool Playing { get; }
}

/// <summary>
/// The call's controls (Android's ui/room/ControlBar.kt): the microphone,
/// the camera, the share, the hand with its reactions, the messages, the
/// panels, and leaving, on a capsule of the control layer's material at the
/// foot. Where the full bar does not fit the window it goes compact, the
/// panels into a More menu, as the other apps' does at the same lengths,
/// worked out from the buttons' own sizes before either is drawn.
/// </summary>
internal sealed class ControlBar : Grid
{
    private readonly RoomScreen _room;
    private readonly CallModel _call;
    private readonly StackPanel _row = new() { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
    private readonly Border _capsule;
    private bool? _compact;
    private bool _devicePressing;
    private Tap? _microphone, _camera, _share, _messages, _hand, _sound, _watch, _music, _host, _devices, _more, _leave, _reactionsHalf;
    private Border? _unread, _watchDot, _musicDot, _moreDot;
    private Border? _knocking;
    private TextBlock? _knockingCount;
    private Offers _offers = new();
    private string _built = "";

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

    /// <summary>
    /// How long the bar is (ControlBar.kt <c>BarLength</c>): the buttons, the
    /// hand with its reactions' half beside it, the rule and leaving, the
    /// gaps between them and the ends; the buttons the features offer only
    /// while they are offered.
    /// </summary>
    public static double Length(bool compact, Offers offers, RoomScreen.Panel? panel, bool sealedCall)
    {
        var size = compact ? 40.0 : 44.0;
        var along = new List<double> { size, size, size, 60, size };
        if (compact)
        {
            along.Add(size);
        }
        else
        {
            along.Add(size); // sound
            if (!sealedCall && (offers.Watch || panel == RoomScreen.Panel.Watch)) along.Add(size);
            if (offers.Music || panel == RoomScreen.Panel.Music) along.Add(size);
            if (offers.Host || panel == RoomScreen.Panel.Host) along.Add(size);
            along.Add(size); // devices
            along.Add(1 + 4); // the rule before leaving
        }
        along.Add(compact ? 52 : 56);
        var inset = compact ? 5 : 7;
        var spacing = compact ? 4 : 6;
        return 2 * inset + along.Sum() + spacing * (along.Count - 1);
    }

    /// <summary>The full bar's length as it stands, for the names lifted clear of it.</summary>
    public double FullLength => Length(false, _offers, _room.Showing, _call.Sealed);

    /// <summary>Lays the bar out for a window this wide: compact once the full bar would come within 20 of its edges.</summary>
    public void Fit(double width, Offers offers)
    {
        var screenChanged = offers.ScreenShare != _offers.ScreenShare;
        _offers = offers;
        var compact = Length(false, offers, _room.Showing, _call.Sealed) > width - 20;
        // Built again only when a button comes or goes: opening a panel whose
        // button is there already must not take the button from under the
        // keyboard's focus, which then fell to the first thing on the screen.
        var built = compact ? "compact" : $"{WatchShown}|{MusicShown}|{HostShown}";
        if (compact == _compact && built == _built)
        {
            // The share's button says what its menu has.
            if (screenChanged) Update();
            return;
        }
        _compact = compact;
        _built = built;
        Build();
    }

    private double Size => _compact == true ? 40 : 44;

    // A feature's button is on the bar while it is offered, and while its
    // panel is open, so that the panel can be closed where it was opened.
    private bool WatchShown => !_call.Sealed && (_offers.Watch || _room.Showing == RoomScreen.Panel.Watch);
    private bool MusicShown => _offers.Music || _room.Showing == RoomScreen.Panel.Music;
    private bool HostShown => _offers.Host || _room.Showing == RoomScreen.Panel.Host;

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
        _share = Bar("rectangle.on.rectangle", T("Share your screen"));
        _share.Click += (_, _) => Overlay.Current?.Toggle("share", _share, () => new SharePanel(_call, _room.App.Remote), width: 376, edge: Edge.Above, align: Align.Centre);
        _row.Children.Add(_microphone);
        _row.Children.Add(_camera);
        _row.Children.Add(_share);
        _row.Children.Add(HandControl());
        _messages = Bar("bubble.left.fill", "");
        _messages.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Messages);
        _unread = Dot(9);
        _row.Children.Add(new Grid { Children = { _messages, _unread } });
        _sound = _watch = _music = _host = _devices = _more = null;
        _watchDot = _musicDot = _moreDot = null;
        _knocking = null;
        if (compact)
        {
            _more = Bar("ellipsis", T("More"));
            _more.Click += (_, _) => Overlay.Current?.Toggle("more", _more, MoreMenu, width: 240, edge: Edge.Above, align: Align.End);
            _moreDot = Dot(9);
            _row.Children.Add(new Grid { Children = { _more, _moreDot } });
        }
        else
        {
            _sound = Bar("speaker.wave.2.fill", "");
            _sound.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Sound);
            _row.Children.Add(_sound);
            if (WatchShown)
            {
                _watch = Bar("play.rectangle.fill", "");
                _watch.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Watch);
                _watchDot = Dot(9);
                _row.Children.Add(Offered(new Grid { Children = { _watch, _watchDot } }));
            }
            if (MusicShown)
            {
                _music = Bar("music.note", "");
                _music.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Music);
                _musicDot = Dot(9);
                _row.Children.Add(Offered(new Grid { Children = { _music, _musicDot } }));
            }
            if (HostShown)
            {
                _host = Bar("crown.fill", T("Running this room"));
                _host.Named(T("Running this room"), "bar.host");
                _host.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Host);
                // Counted as well as coloured (HostPanel.tsx ~400): somebody
                // waiting outside is the one thing in the room that is lost
                // if nobody notices.
                _knockingCount = K.Text("", 10, K.Bold, Ink.TallyForeground, lines: 1, align: TextAlignment.Center, tabular: true);
                _knocking = new Border
                {
                    MinWidth = 16,
                    Height = 16,
                    CornerRadius = new CornerRadius(8),
                    Background = Ink.Tally,
                    BorderBrush = Ink.SurfaceHigh,
                    BorderThickness = new Thickness(2),
                    Padding = new Thickness(3, 0, 3, 0),
                    HorizontalAlignment = HorizontalAlignment.Right,
                    VerticalAlignment = VerticalAlignment.Top,
                    Margin = new Thickness(0, -2, -2, 0),
                    IsHitTestVisible = false,
                    Child = _knockingCount,
                    Visibility = Visibility.Collapsed,
                };
                _knocking.Arrives();
                _row.Children.Add(Offered(new Grid { Children = { _host, _knocking } }));
            }
            _devices = Bar("gearshape.fill", T("Devices"));
            _devices.Click += (_, _) => _room.Toggle(RoomScreen.Panel.Devices);
            _row.Children.Add(_devices);
            _row.Children.Add(new Border { Width = 1, Height = 26, Background = Ink.Border, Margin = new Thickness(2, 0, 2, 0), VerticalAlignment = VerticalAlignment.Center });
        }
        _leave = new Tap(K.Symbol("phone.down.fill", 16, Ink.DangerForeground), Look.Destructive, Size, label: T("Leave"));
        _leave.Width = compact ? 52 : 56;
        _leave.Padding = new Thickness(0);
        _leave.Named(T("Leave"), "bar.leave");
        _leave.Click += (_, _) => OpenLeave();
        _row.Children.Add(_leave);
        Update();
    }

    /// <summary>A button a feature offers, arriving along the bar as the bar grows round it.</summary>
    private static UIElement Offered(FrameworkElement button) => button.Arrives(0.8f);

    /// <summary>The one dot that says something happened behind a button, in the signal colour (ControlBar.tsx ~383).</summary>
    private static Border Dot(double size)
    {
        var dot = new Border
        {
            Width = size,
            Height = size,
            CornerRadius = new CornerRadius(size / 2),
            Background = Ink.Tally,
            BorderBrush = Ink.SurfaceHigh,
            BorderThickness = new Thickness(2),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(0, 2, 2, 0),
            IsHitTestVisible = false,
            Visibility = Visibility.Collapsed,
        };
        dot.Arrives();
        return dot;
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
        var sharing = _call.Sharing;
        // With the screen switched off by the deployment the button is the
        // sound's alone, and says so, rather than offering a screen its menu
        // no longer has (Android's ControlBar.kt the same).
        var screen = _offers.ScreenShare;
        var shareLabel = sharing.Active ? T("Screen sharing settings") : sharing.Listening ? T("Sharing sound") : screen ? T("Share your screen") : T("Share only sound");
        Paint(_share!, sharing.Active ? "rectangle.inset.filled.on.rectangle" : sharing.Listening ? "waveform.circle.fill" : screen ? "rectangle.on.rectangle" : "waveform",
            shareLabel, lit: sharing.Active || sharing.Listening, quiet: false);
        _share!.Named(shareLabel, "bar.share");
        var chat = _room.Showing == RoomScreen.Panel.Messages;
        Paint(_messages!, "bubble.left.fill", chat ? T("Hide messages") : T("Show messages"), lit: chat, quiet: false);
        _messages!.Named(chat ? T("Hide messages") : T("Show messages"), "bar.messages");
        _unread!.Visibility = _call.Unread && !chat ? Visibility.Visible : Visibility.Collapsed;
        if (_sound != null)
        {
            var open = _room.Showing == RoomScreen.Panel.Sound;
            Paint(_sound, "speaker.wave.2.fill", open ? T("Hide sound") : T("Show sound"), lit: open, quiet: false);
            _sound.Named(open ? T("Hide sound") : T("Show sound"), "bar.sound");
        }
        if (_watch != null)
        {
            var open = _room.Showing == RoomScreen.Panel.Watch;
            Paint(_watch, "play.rectangle.fill", open ? T("Hide watch together") : T("Show watch together"), lit: open, quiet: false);
            _watch.Named(open ? T("Hide watch together") : T("Show watch together"), "bar.watch");
            // The show is playing while its panel is closed (ControlBar.tsx ~430).
            _watchDot!.Visibility = _offers.WatchPlaying && !open ? Visibility.Visible : Visibility.Collapsed;
        }
        if (_music != null)
        {
            var open = _room.Showing == RoomScreen.Panel.Music;
            Paint(_music, "music.note", open ? T("Hide music") : T("Show music"), lit: open, quiet: false);
            _music.Named(open ? T("Hide music") : T("Show music"), "bar.music");
            _musicDot!.Visibility = _offers.MusicPlaying && !open ? Visibility.Visible : Visibility.Collapsed;
        }
        if (_host != null)
        {
            var open = _room.Showing == RoomScreen.Panel.Host;
            _host.Lit = open;
            _host.Content = K.Symbol("crown.fill", Size * 0.36, open ? Ink.TallyForeground : Ink.Foreground);
            _knockingCount!.Text = _offers.Knocking.ToString();
            _knocking!.Visibility = _offers.Knocking > 0 && !open ? Visibility.Visible : Visibility.Collapsed;
        }
        if (_devices != null) _devices.Lit = _room.Showing == RoomScreen.Panel.Devices;
        if (_more != null)
        {
            var behind = _room.Showing is RoomScreen.Panel.Sound or RoomScreen.Panel.Watch or RoomScreen.Panel.Music or RoomScreen.Panel.Host or RoomScreen.Panel.Devices;
            _more.Lit = behind;
            // It carries the dot whenever one of the buttons it stands for would.
            var busy = (_offers.WatchPlaying && _room.Showing != RoomScreen.Panel.Watch) || (_offers.MusicPlaying && _room.Showing != RoomScreen.Panel.Music)
                || (_offers.Knocking > 0 && _room.Showing != RoomScreen.Panel.Host);
            _moreDot!.Visibility = busy ? Visibility.Visible : Visibility.Collapsed;
        }
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
                _call.PressFailed();
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

    /// <summary>
    /// The panels, behind one button on a narrow bar (ControlBar.kt
    /// <c>MoreControl</c>), each with the dot its own button would carry.
    /// </summary>
    private UIElement MoreMenu()
    {
        Tap Item(string title, string symbol, RoomScreen.Panel target, bool dot = false)
        {
            var item = Menu.Item(title, () =>
            {
                Overlay.Current?.ClosePopover();
                _room.Toggle(target);
            }, symbol, check: _room.Showing == target);
            if (dot && _room.Showing != target)
            {
                var mark = K.Dot(6, Ink.Tally);
                mark.HorizontalAlignment = HorizontalAlignment.Left;
                mark.VerticalAlignment = VerticalAlignment.Top;
                mark.Margin = new Thickness(24, 6, 0, 0);
                item.Content = K.Stack((UIElement)item.Content, mark);
            }
            return item;
        }
        var items = new List<UIElement> { Item(T("Sound"), "speaker.wave.2.fill", RoomScreen.Panel.Sound) };
        if (!_call.Sealed && (_offers.Watch || _room.Showing == RoomScreen.Panel.Watch)) items.Add(Item(T("Watch together"), "play.rectangle.fill", RoomScreen.Panel.Watch, _offers.WatchPlaying));
        if (_offers.Music || _room.Showing == RoomScreen.Panel.Music) items.Add(Item(T("Music"), "music.note", RoomScreen.Panel.Music, _offers.MusicPlaying));
        if (_offers.Host || _room.Showing == RoomScreen.Panel.Host)
        {
            items.Add(Item(_offers.Knocking > 0 ? $"{T("Running this room")} ({_offers.Knocking})" : T("Running this room"), "crown.fill", RoomScreen.Panel.Host, _offers.Knocking > 0));
        }
        items.Add(Menu.Rule());
        items.Add(Item(T("Devices"), "gearshape.fill", RoomScreen.Panel.Devices));
        items.Add(Menu.Rule());
        items.Add(Menu.Item(T("Reactions"), () => { Overlay.Current?.ClosePopover(); Overlay.Current?.Toggle("reactions", _reactionsHalf!, Reactions, width: 234, edge: Edge.Above, align: Align.Centre); }, "sparkles"));
        return Menu.Card(items.ToArray());
    }

    /// <summary>The share's popover, as a launch that asks for it opens it (TOMOSHIBI_SHOW=share).</summary>
    public void OpenShare()
    {
        if (_share != null) Overlay.Current?.Show("share", _share, new SharePanel(_call, _room.App.Remote), width: 376, edge: Edge.Above, align: Align.Centre);
    }

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
                        _call.PressFailed();
                    }
                }, "door.left.hand.closed", T("Everybody is disconnected and links stop working."), role: Menu.Role.Destructive);
                end.Named(T("End for everybody"), "leave.end");
                items.Add(end);
            }
            return Menu.Card(items.ToArray());
        }, width: 272, edge: Edge.Above, align: Align.End);
    }
}

/// <summary>
/// The share's popover (ShareButton.tsx; ControlBar.kt <c>SharePanel</c>):
/// the size and the frame rate first, because they are what somebody came
/// to change, then the press that starts the share; while sharing, the same
/// settings, applied to the running share at once, and stopping. Below it,
/// sound shared on its own, which the web keeps in the same menu, with the
/// lossless switch beneath what it qualifies. One button, as Android's and
/// the iPhone's bar have it, rather than the Mac's two.
/// </summary>
internal sealed class SharePanel : Grid
{
    private readonly CallModel _call;
    private readonly Remote _remote;
    private readonly StackPanel _column = K.Flowing(2);
    /// <summary>
    /// The screen's half, in a space of its own that opens and closes: gone
    /// while the deployment has screen sharing switched off (Remote), and
    /// back when it is on again, never while a share runs.
    /// </summary>
    private readonly Unfolding _screenSpace = new();
    private readonly StackPanel _screen = K.Flowing(2);
    private readonly StackPanel _sound = K.Flowing(2);
    private string _screenSaid = "";
    private string _soundSaid = "";

    public SharePanel(CallModel call, Remote remote)
    {
        _call = call;
        _remote = remote;
        _column.Padding = new Thickness(6);
        _column.Children.Add(_screenSpace);
        _column.Children.Add(_sound);
        Children.Add(_column);
        call.Sharing.Changed += Update;
        remote.Changed += Update;
        Unloaded += (_, _) =>
        {
            call.Sharing.Changed -= Update;
            remote.Changed -= Update;
        };
        Update();
    }

    /// <summary>
    /// What somebody is choosing between rather than pixels (ShareButton.tsx
    /// <c>QUALITY_LABELS</c>): automatic is the only setting that will
    /// quietly send less.
    /// </summary>
    private static string Describes(ShareQuality quality) => quality switch
    {
        ShareQuality.P1080 => T("Up to 240 frames a second"),
        ShareQuality.P1440 => T("Up to 120 frames a second"),
        ShareQuality.P4k => T("Up to 60 frames a second"),
        _ => T("Adjusts to the connection"),
    };

    private static readonly ShareQuality[] Qualities = { ShareQuality.Auto, ShareQuality.P1080, ShareQuality.P1440, ShareQuality.P4k };

    private void Update()
    {
        var sharing = _call.Sharing;
        var offered = _remote.Allows(RemoteFeature.ScreenShare) || sharing.Active;
        var shown = sharing.Shown;
        var screenSaid = offered ? string.Join("|", shown, sharing.Active, sharing.Listening, sharing.ScreenBusy) : "";
        if (screenSaid != _screenSaid)
        {
            _screenSaid = screenSaid;
            _screen.Children.Clear();
            if (offered) Screen(sharing, shown);
            _screenSpace.Show(offered ? _screen : null);
        }
        var soundSaid = string.Join("|", sharing.Active, sharing.Listening, sharing.SoundBusy, sharing.SendingLossless, sharing.LosslessChosen, sharing.WithSound, _call.Sealed);
        if (soundSaid == _soundSaid) return;
        _soundSaid = soundSaid;
        _sound.Children.Clear();
        Sound(sharing);
    }

    private void Screen(Sharing sharing, Sharing.Setting shown)
    {
        _screen.Children.Add(Menu.Heading(T("Picture")));
        var sizes = new Segmented(Qualities.Select(StrainCard.Label).ToList(), Array.IndexOf(Qualities, shown.Quality), fill: true) { Margin = new Thickness(4, 0, 4, 0) };
        sizes.Named(T("Picture"), "share.quality");
        sizes.Chose += index => Change(() => sharing.Choose(Qualities[index]));
        _screen.Children.Add(sizes);
        _screen.Children.Add(K.Text(Describes(shown.Quality), 11.5, ink: Ink.Muted).Pad(10, 4, 10, 0));
        // Absent for automatic, which chooses the rate as well: a control
        // that did nothing would be a control somebody set and believed.
        if (shown.Quality != ShareQuality.Auto)
        {
            var allowed = ShareRates.Allowed(Sharing.ShorterSide(shown.Quality));
            _screen.Children.Add(Menu.Heading(T("Frames a second")).Arrives());
            var rates = new Segmented(allowed.Select(r => r.ToString()).ToList(), Array.IndexOf(allowed, shown.Fps), fill: true) { Margin = new Thickness(4, 0, 4, 0) };
            rates.Named(T("Frames a second"), "share.rate");
            rates.Chose += index => Change(() => sharing.Choose(shown.Quality, allowed[index]));
            _screen.Children.Add(rates.Arrives());
        }
        _screen.Children.Add(Menu.Rule());
        // Not turned off while busy: a change waits its turn and a stop
        // always goes through (Sharing).
        if (sharing.Active)
        {
            var stop = Menu.Item(T("Stop sharing"), () =>
            {
                Overlay.Current?.ClosePopover();
                Change(sharing.StopScreen);
            }, "stop.circle", busy: sharing.ScreenBusy);
            stop.Named(T("Stop sharing"), "share.stop");
            _screen.Children.Add(stop);
        }
        else
        {
            var start = Menu.Item(T("Share your screen"), () =>
            {
                Overlay.Current?.ClosePopover();
                _ = _call.ShareScreen();
            }, "rectangle.on.rectangle", sharing.Listening ? T("Without its sound, while sound is shared on its own") : null, role: Menu.Role.Prominent, busy: sharing.ScreenBusy);
            start.Named(T("Share your screen"), "share.start");
            _screen.Children.Add(start);
        }
        _screen.Children.Add(Menu.Rule());
    }

    private void Sound(Sharing sharing)
    {
        if (sharing.Listening)
        {
            // Which of the two is going out, because the switch that chose it
            // is not shown while it is (ShareButton.tsx).
            var stop = Menu.Item(T("Stop sharing sound"), () =>
            {
                Overlay.Current?.ClosePopover();
                Change(sharing.StopSound);
            }, "stop.circle", sharing.SendingLossless ? T("Lossless") : T("Real time"), busy: sharing.SoundBusy);
            stop.Named(T("Stop sharing sound"), "share.sound.stop");
            _sound.Children.Add(stop);
        }
        else
        {
            // Everything the computer plays but the call, since Windows names
            // no app's sound by the window it plays in; Android's line is
            // "Pick the app that is playing", for its own picker.
            var start = Menu.Item(T("Share only sound"), () =>
            {
                Overlay.Current?.ClosePopover();
                _ = _call.ShareSound();
            }, "waveform", sharing.Active && sharing.WithSound ? T("Your shared screen goes quiet while this plays") : T("Everything this computer plays, except the call"),
                role: Menu.Role.Prominent, busy: sharing.SoundBusy);
            start.Named(T("Share only sound"), "share.sound.start");
            _sound.Children.Add(start);
            // Beneath what it qualifies and decided before the share starts.
            // Off where it cannot work, with the reason in place of the
            // description: lossless is data, which the SDK does not seal and
            // which would cross the relay as plain samples
            // (sound.ts `losslessUnavailable`).
            var toggle = Fields.Switch(!_call.Sealed && sharing.LosslessChosen, on => sharing.LosslessChosen = on, T("Lossless"));
            toggle.IsEnabled = !_call.Sealed;
            var words = K.Column(1, K.Text(T("Lossless"), 13.5), K.Text(_call.Sealed ? T("Not available in an encrypted call") : T("Every sample as it is played, about a second behind"), 11.5, ink: Ink.Muted));
            words.VerticalAlignment = VerticalAlignment.Center;
            var row = K.Line(10, K.Fixed(new Border { Width = 18, Child = K.Symbol("hifispeaker", 13, Ink.ForegroundAt(0.85f)) }), K.Flex(words), K.Fixed(toggle));
            row.Padding = new Thickness(10, 6, 4, 6);
            row.Opacity = _call.Sealed ? 0.7 : 1;
            _sound.Children.Add(row);
        }
    }

    /// <summary>
    /// A change that did not take says so; the share goes on as it was. A
    /// cancelled picker is an answer, not a failure, and says nothing
    /// (ControlBar.tsx: it was once announced as a camera nobody could use).
    /// </summary>
    private void Change(Func<Task> work) => _ = Run(work);

    private async Task Run(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (Exception)
        {
            _call.PressFailed();
        }
    }
}
