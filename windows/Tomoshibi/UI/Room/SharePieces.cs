using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>The faces a share puts in the room (Android's ui/room/Tile.kt, Watchers.kt; the web's ShareCard.tsx, Watchers.tsx, StageControls.tsx).</summary>
internal static class SharePieces
{
    /// <summary>
    /// Stands in for a screen share that is not on the stage (ShareCard.tsx;
    /// Tile.kt <c>ShareCardFace</c>). A desktop scaled into a grid cell is
    /// unreadable, so drawing it would spend the bandwidth and deliver
    /// nothing: the card says who is sharing, and the whole card takes the
    /// press that makes it big.
    /// </summary>
    public static Grid ShareCard(string name, bool compact)
    {
        // Flat, the raised surface: a grey blend under an accent glow until the
        // gradients went (2026-10-20).
        var grid = new Grid { Background = Ink.SurfaceHigh };
        var column = K.Column(compact ? 4 : 10);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Padding = new Thickness(12, 0, 12, 0);
        var disc = compact ? 30 : 48;
        column.Children.Add(new Border
        {
            Width = disc,
            Height = disc,
            CornerRadius = new CornerRadius(disc / 2.0),
            Background = Ink.Wash(0.08f),
            HorizontalAlignment = HorizontalAlignment.Center,
            Child = K.Symbol("rectangle.inset.filled.and.person.filled", compact ? 15 : 22, Ink.ForegroundAt(0.9f)),
        });
        column.Children.Add(K.Text(T("{name} is sharing", ("name", name)), compact ? 11 : 13.5, K.Semibold, lines: compact ? 1 : 2, align: TextAlignment.Center));
        if (!compact)
        {
            var watch = K.Row(5, K.Symbol("play.fill", 9, Ink.TallyForeground), K.Text(T("Watch"), 12, K.Semibold, Ink.TallyForeground, lines: 1));
            watch.VerticalAlignment = VerticalAlignment.Center;
            column.Children.Add(new Border
            {
                Height = 26,
                CornerRadius = new CornerRadius(13),
                Background = Ink.Tally,
                Padding = new Thickness(12, 0, 12, 0),
                HorizontalAlignment = HorizontalAlignment.Center,
                Child = watch,
            });
        }
        grid.Children.Add(column);
        return grid;
    }

    /// <summary>
    /// This participant's own shared screen (Tile.kt <c>MyShareFace</c>).
    /// There is no preview of the capture to draw, so it says so, with the
    /// way to stop: a picture of your own screen inside your own screen is a
    /// hall of mirrors anyway, and what the sharer needs from this place is
    /// to know it is going out.
    /// </summary>
    public static Grid MyShare(bool compact, Action? stop)
    {
        var grid = new Grid { Background = Ink.SurfaceHigh };
        var column = K.Column(compact ? 4 : 12);
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Padding = new Thickness(12, 0, 12, 0);
        var disc = compact ? 34 : 64;
        column.Children.Add(new Border
        {
            Width = disc,
            Height = disc,
            CornerRadius = new CornerRadius(disc / 2.0),
            Background = Ink.TallyAt(0.18f),
            HorizontalAlignment = HorizontalAlignment.Center,
            Child = K.Symbol("rectangle.on.rectangle", compact ? 14 : 24, Ink.TallyText),
        });
        column.Children.Add(K.Text(T("You are sharing your screen"), compact ? 11 : 14, K.Semibold, lines: compact ? 1 : 2, align: TextAlignment.Center));
        if (stop != null && !compact)
        {
            var button = Tap.Words(T("Stop sharing"), height: 30, size: 12.5);
            button.HorizontalAlignment = HorizontalAlignment.Center;
            button.Click += (_, _) => stop();
            column.Children.Add(button);
        }
        grid.Children.Add(column);
        return grid;
    }

    /// <summary>
    /// What can be done with whatever is on the stage (StageControls.tsx;
    /// Tile.kt <c>StageControls</c>): the same person's other picture, when
    /// they have one, and filling the screen. Its buttons take their own
    /// presses: on the web a press on one carried on to the picture beneath,
    /// and "Their screen" pinned the other picture only for the picture's own
    /// press to pin this one straight back.
    ///
    /// Of the bar's material at the stage's size (Sizing.Stage): 36 high, the
    /// switch a capsule with its words and filling the screen a circle, the
    /// symbols at four tenths of it. They were 30, a clear button in black at
    /// half, beside the bar's glass, two materials over one call. Out only
    /// while the pointer is on the picture (Tile.ShowControls).
    /// </summary>
    public static StackPanel StageControls(Surface.Kinds? other, bool filled, Action switchTo, Action fill)
    {
        var row = K.Row(Sizing.Stage.Gap);
        row.HorizontalAlignment = HorizontalAlignment.Right;
        row.VerticalAlignment = VerticalAlignment.Top;
        if (other is { } kind)
        {
            var screen = kind == Surface.Kinds.Screen;
            var words = Tap.Words(screen ? T("Their screen") : T("Their camera"), screen ? "rectangle.on.rectangle" : "person.fill", Look.Clear, Sizing.Stage.Button - 2, 13, padding: 12);
            words.Click += (_, _) => switchTo();
            row.Children.Add(Shared.OnGlass(words));
        }
        // Inside the glass's hairline, so that the circle is 36 drawn.
        var expand = Tap.Icon(filled ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right", filled ? T("Leave fullscreen") : T("Fill the screen"), Look.Clear, Sizing.Stage.Button - 2, Sizing.Icon(Sizing.Stage.Button));
        expand.Click += (_, _) => fill();
        row.Children.Add(Shared.OnGlass(expand));
        foreach (var child in row.Children) if (child is FrameworkElement element) element.Arrives();
        // Their presses stop here, as the web's stopPropagation does: a tap
        // carried on to the picture beneath would pin or unpin it after the
        // button had done its own work, and in a call of two let the screen
        // go straight back to the stage it had just been taken from.
        row.Tapped += (_, e) => e.Handled = true;
        row.DoubleTapped += (_, e) => e.Handled = true;
        row.RightTapped += (_, e) => e.Handled = true;
        return row;
    }

    /// <summary>
    /// A call being moved to another machine (components/room/Moving.tsx;
    /// CallOverlay.kt <c>MovingOverlay</c>). A move ends the call and
    /// everybody comes straight back; what that looked like was being thrown
    /// out. So this is drawn over the call for the few seconds it takes, says
    /// where the meeting is going, and takes every press, so that the bar
    /// under it cannot be pressed through "you will be back in a moment".
    /// </summary>
    public static Grid Moving(string to)
    {
        var named = Say(to.Trim());
        var grid = new Grid { Background = Ink.BackgroundAt(0.86f), IsHitTestVisible = true };
        grid.Tapped += (_, e) => e.Handled = true;
        grid.PointerPressed += (_, e) => e.Handled = true;
        var column = K.Column(12,
            K.Spinner(18),
            K.Text(named.Length == 0 ? T("Moving this meeting") : T("Moving this meeting to {name}", ("name", named)), 14, K.Medium, align: TextAlignment.Center),
            K.Text(T("You will be back in a moment."), 12, ink: Ink.Muted, align: TextAlignment.Center));
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Padding = new Thickness(24, 0, 24, 0);
        grid.Children.Add(column);
        return grid;
    }
}

/// <summary>
/// Who has this participant's shared screen on their stage, from everybody's
/// <c>watching</c> attribute (components/room/Watchers.tsx,
/// live/watching.ts; Android's ui/room/Watchers.kt). Somebody sharing is
/// doing it blind: they can see that eleven people are in the call and not
/// whether any of them has the picture in front of them. Two sizes: the card,
/// with names, and the eye and the count alone where there is no room for it.
/// </summary>
internal sealed class WatchersCard : Grid
{
    private readonly CallModel _call;
    private readonly bool _compact;
    private readonly TextBlock _count;
    private readonly FrameworkElement _eye;
    private readonly StackPanel? _names;
    private readonly TextBlock? _nobody;
    private string _said = "";

    public WatchersCard(CallModel call, bool compact)
    {
        _call = call;
        _compact = compact;
        _count = K.Text("0", compact ? 12 : 11, K.Semibold, Ink.Muted, lines: 1, tabular: true);
        if (compact)
        {
            _eye = K.Symbol("eye.fill", 10.5, Ink.Muted);
            var row = K.Row(5, _eye, _count);
            row.VerticalAlignment = VerticalAlignment.Center;
            var pill = K.Glass(row, 14, new Thickness(10, 0, 10, 0));
            pill.Height = 28;
            Children.Add(pill);
        }
        else
        {
            _eye = K.Symbol("eye.fill", 10, Ink.Muted);
            var head = K.Line(6, K.Fixed(_eye), K.Fixed(K.Text(T("Watching"), 11, K.Semibold, Ink.Muted, lines: 1)), K.Flex(K.Spacer()), K.Fixed(_count));
            _names = K.Flowing(3);
            _nobody = K.Text(T("Nobody has this open yet."), 11.5, ink: Ink.MutedAt(0.85f));
            var column = K.Column(6, head, _nobody, _names);
            var card = K.Glass(column, 16, new Thickness(12, 10, 12, 10));
            card.Width = 172;
            Children.Add(card);
        }
        this.Named(T("Watching"));
        Update();
    }

    public void Update()
    {
        var watching = Watching(_call);
        var said = string.Join("|", watching.Select(p => p.Id + p.Shown));
        if (said == _said) return;
        _said = said;
        _count.Text = watching.Count.ToString();
        var ink = watching.Count == 0 || !_compact ? Ink.Muted : Ink.Foreground;
        _count.Foreground = ink;
        if (_names == null || _nobody == null) return;
        _nobody.Visibility = watching.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        _names.Children.Clear();
        foreach (var person in watching)
        {
            var row = K.Row(7, K.Dot(5, Ink.Tally), K.Text(person.Shown, 12.5, lines: 1));
            row.Children[0].SetValue(VerticalAlignmentProperty, VerticalAlignment.Center);
            _names.Children.Add(row.Arrives());
        }
    }

    /// <summary>Everybody whose <c>watching</c> names this participant, in roster order: the protocol crate's <c>watchers</c>, which is watching.ts <c>useWatchers</c>.</summary>
    public static IReadOnlyList<Person> Watching(CallModel call)
    {
        var roster = call.People.Select(p => new RosterMember(p.Id, new Dictionary<string, string>(p.Attributes))).ToArray();
        var ids = TomoshibiProtocolMethods.Watchers(call.LocalIdentity, true, roster).ToHashSet();
        return call.People.Where(p => ids.Contains(p.Id)).ToList();
    }
}

/// <summary>
/// The uplink card (components/room/ShareStrain.tsx; the Apple apps'
/// StrainCard): this participant's own share asks more of the line than it
/// carries, so it stutters for everybody, and the card offers the setting
/// that would fit. Applied to the running share and not remembered
/// (strain.ts: the next share starts from what was chosen in the menu).
/// </summary>
internal sealed class StrainCard : Grid
{
    public StrainCard(CallModel call, StrainHint hint)
    {
        var words = K.Column(4,
            K.Text(T("Your connection is sending about {have} Mbps, and this share needs about {need} Mbps, so it stutters.",
                ("have", TomoshibiProtocolMethods.StrainMegabits(hint.Have)), ("need", TomoshibiProtocolMethods.StrainMegabits(hint.Need))), 12.5),
            K.Text(T("Switch to {size} at {rate} frames a second?", ("size", Label(hint.To.Quality)), ("rate", hint.To.FrameRate)), 12.5, ink: Ink.Muted));
        var top = K.Line(10, K.Fixed(new Border { Width = 20, Child = K.Symbol("gauge.with.dots.needle.33percent", 14, Ink.TallyText), VerticalAlignment = VerticalAlignment.Top }), K.Flex(words));
        var later = Tap.Words(T("Not now"), height: 30, size: 12.5);
        later.Click += (_, _) => call.StrainDecline();
        var take = Tap.Words(T("Switch"), look: Look.Prominent, height: 30, size: 12.5);
        take.Click += async (_, _) =>
        {
            if (call.StrainAccept() is not StrainAccept.Apply apply) return;
            take.IsEnabled = later.IsEnabled = false;
            var took = false;
            try
            {
                await call.Sharing.Restart(apply.Setting.Quality, (int)apply.Setting.FrameRate);
                took = true;
            }
            catch (Exception)
            {
                took = false;
            }
            if (!call.StrainAccepted(took)) take.IsEnabled = later.IsEnabled = true;
        };
        var buttons = K.Row(8, later, take);
        buttons.HorizontalAlignment = HorizontalAlignment.Right;
        var card = K.Glass(K.Column(12, top, buttons), 16, new Thickness(14));
        card.MaxWidth = 340;
        Children.Add(card);
    }

    public static string Label(ShareQuality quality) => quality switch
    {
        ShareQuality.P1080 => T("1080p"),
        ShareQuality.P1440 => T("1440p"),
        ShareQuality.P4k => T("4K"),
        _ => T("Automatic"),
    };
}
