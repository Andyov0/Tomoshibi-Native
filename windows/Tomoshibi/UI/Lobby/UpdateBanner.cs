using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Updates;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// A newer build, said at the top of the lobby without stopping anything
/// (Android's UpdateBanner.kt, the Mac's UpdateBanner.swift): what it is and
/// what changed, "Update" and "Later"; then how far the download has got,
/// with a way to stop it; then the install, which ends the app and opens it
/// again. Windows asks for no permission and shows no confirmation of its
/// own, so Android's two steps between are not here.
///
/// Shown only while the lobby is the screen (Updates.Shown), so that no call
/// is ever interrupted by it; under the sheets and the settings, which cover
/// it as they cover the lobby. Inline, in the sidebar's foot, in the narrow
/// lobby's column under its large title and at the head of the sign-in
/// screen's column, under the deployment's notice where there is one
/// (LobbyNoticeCard), it opens a space for itself and closes it again, so
/// that the two never lie across each other and either moves the other as it
/// comes and goes; floating, where a screen asks for it, it slides down from
/// above and fades, and leaves the same way. What it says changes with a
/// cross-fade and a change of height, never a cut.
/// </summary>
internal sealed class UpdateBanner : Grid
{
    private readonly Updates _updates;
    private readonly bool _inline;
    private readonly bool _sidebar;
    private readonly double _top;
    private readonly double _below;
    private Border? _card;
    private Border? _holder;
    private Border? _lit;
    private Unfolding? _body;
    private Unfolding? _space;
    private string? _said;
    private Paced? _paced;
    private long _total;

    /// <param name="inline">In a column, opening a space, rather than floating over the content.</param>
    /// <param name="top">What it clears below the top of where it is put.</param>
    /// <param name="below">Inline, the space it keeps below itself while it is open.</param>
    /// <param name="sidebar">
    /// In the sidebar's foot, above the account, where it covers nothing (over
    /// the stage it lay across the greeting): as wide as the sidebar, on its
    /// raised grey, its notes to three lines. Always in the flow there.
    /// </param>
    public UpdateBanner(Updates updates, bool inline = false, double top = 12, double below = 0, bool sidebar = false)
    {
        _updates = updates;
        _sidebar = sidebar;
        inline |= sidebar;
        _inline = inline;
        _top = top;
        _below = below;
        if (inline)
        {
            _space = new Unfolding();
            Children.Add(_space);
        }
        else
        {
            VerticalAlignment = VerticalAlignment.Top;
        }
        updates.Changed += Update;
        Unloaded += (_, _) => updates.Changed -= Update;
        Loaded += (_, _) =>
        {
            updates.Changed -= Update;
            updates.Changed += Update;
            Update();
        };
        Update();
    }

    private void Update()
    {
        if (!_updates.Shown)
        {
            Leave();
            return;
        }
        var step = _updates.Now;
        _total = step switch
        {
            Step.Downloading downloading => downloading.Total,
            Step.Held held => held.Total,
            _ => _total,
        };
        if (_card == null) Arrive();
        // What is said, apart from how far a download has got, which moves
        // in place rather than drawing the card again.
        var said = step switch
        {
            Step.Offered offered => $"offered {offered.Offer.Version} {offered.Problem}",
            Step.Downloading downloading => $"downloading {downloading.Offer.Version}",
            Step.Held held => $"downloading {held.Offer.Version}",
            Step.Installing installing => $"installing {installing.Offer.Version}",
            _ => "",
        } + Localization.Shared.Language;
        if (said != _said)
        {
            _said = said;
            _body!.Show(Body(step));
        }
        if (step is Step.Downloading or Step.Held) _paced?.Wake();
    }

    /// <summary>The card, flat as the screens are drawn: the surface colour with the theme's edge, and a shadow only where it floats.</summary>
    private void Arrive()
    {
        _body = new Unfolding();
        var radius = _sidebar ? 16 : 20;
        _card = new Border
        {
            Background = _sidebar ? Ink.SurfaceHigh : Ink.Surface,
            BorderBrush = Ink.Border,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(radius),
            Padding = new Thickness(_sidebar ? 12 : 14),
            Child = _body,
        }.Named("update.banner");
        var lit = Light(radius);
        if (_inline)
        {
            // In a holder that carries the margins, so that they open and
            // close with the card rather than standing empty when it is gone.
            _space!.Show(new Border { Child = new Grid { Children = { _card, lit } }, Padding = new Thickness(0, _top, 0, _below) });
            Flash(lit);
            return;
        }
        _card.MaxWidth = 520;
        _card.Margin = new Thickness(16, _top, 16, 0);
        _card.Shadow = new ThemeShadow();
        _card.Translation = new Vector3(0, 0, 24);
        // From above the window's edge: its own height and what it clears.
        _holder = K.Holding(_card).Arrives(1f, fromY: -(float)(_top + 160));
        Children.Add(_holder);
        lit.Margin = _card.Margin;
        lit.MaxWidth = _card.MaxWidth;
        _lit = lit;
        Children.Add(lit);
        Flash(lit);
    }

    private void Leave()
    {
        if (_card == null) return;
        if (_inline) _space!.Show(null);
        else
        {
            Children.Remove(_holder);
            Children.Remove(_lit);
        }
        _card = null;
        _holder = null;
        _lit = null;
        _body = null;
        _said = null;
        _paced = null;
    }

    private UIElement Body(Step step) => step switch
    {
        Step.Offered offered => Said(
            offered.Problem != null ? "exclamationmark.circle.fill" : "sparkles",
            T("Tomoshibi {version} is available", ("version", TomoshibiProtocolMethods.VersionLabel(offered.Offer.Version))),
            Notes(offered.Offer),
            warning: offered.Problem != null,
            problem: offered.Problem,
            buttons:
            [
                Button(T("Later"), _updates.Later, "update.later"),
                Button(offered.Problem != null ? T("Try again") : T("Update"), _updates.Accept, "update.accept", prominent: true),
            ]),
        Step.Downloading or Step.Held => Downloading(TomoshibiProtocolMethods.VersionLabel(_updates.Offer!.Version)),
        Step.Installing installing => Said(
            null,
            T("Installing Tomoshibi {version}", ("version", TomoshibiProtocolMethods.VersionLabel(installing.Offer.Version))),
            [T("Tomoshibi will open again when it is done.")],
            buttons: [Button(T("Cancel"), _updates.StopInstalling, "update.cancel")]),
        _ => new Grid(),
    };

    /// <summary>What changed, in the language on screen, or in English where the deployment wrote nothing in it.</summary>
    private static string[] Notes(Offer offer)
    {
        var notes = offer.Notes(Localization.Shared.Language.Code()) ?? offer.Notes("en");
        return notes == null ? [] : [notes];
    }

    /// <param name="version">As people are told it (version_label).</param>
    private UIElement Downloading(string version)
    {
        var line = K.Text("", 12.5, ink: Ink.Muted);
        var track = new Border { Height = 5, CornerRadius = new CornerRadius(2.5), Background = Ink.SurfaceHigh, Margin = new Thickness(0, 4, 0, 0) };
        var fill = new Border { Height = 5, CornerRadius = new CornerRadius(2.5), Background = Ink.Tally, HorizontalAlignment = HorizontalAlignment.Stretch };
        track.Child = fill;
        track.AutomationName(T("Downloading Tomoshibi {version}", ("version", version)));
        // Filled from its leading edge by its visual's scale, set at each
        // frame (Paced); the fill takes no presses and is not raised, so
        // its visual is its own to set (K.Holding).
        var visual = ElementCompositionPreview.GetElementVisual(fill);
        visual.CenterPoint = Vector3.Zero;
        var percent = -1;
        _paced = new Paced(track, _updates.Bar, shown =>
        {
            visual.Scale = new Vector3(Math.Max(0.0001f, (float)shown), 1, 1);
            // What the bar says it has, which is never more than has arrived.
            line.Text = T("{received} of {total}", ("received", Megabytes((long)(shown * _total))), ("total", Megabytes(_total)));
            var now = (int)(shown * 100);
            if (now == percent) return;
            percent = now;
            track.SetValue(Microsoft.UI.Xaml.Automation.AutomationProperties.ItemStatusProperty, $"{now}%");
        });
        var said = Said(
            "arrow.clockwise",
            T("Downloading Tomoshibi {version}", ("version", version)),
            [],
            extra: [line, track],
            buttons: [Button(T("Cancel"), _updates.Cancel, "update.cancel")]);
        return said;
    }

    /// <summary>Bytes as megabytes to a tenth, in the reader's own way of writing a number.</summary>
    internal static string Megabytes(long bytes) => (bytes / 1_000_000.0).ToString("0.0", Reading) + " MB";

    private static Tap Button(string words, Action act, string id, bool prominent = false)
    {
        var tap = Tap.Words(words, look: prominent ? Look.Prominent : Look.Plain, height: 30, size: 13);
        tap.Named(words, id);
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>The symbol in a lit circle, the title, what there is to say, and the buttons at the trailing end.</summary>
    /// <summary>
    /// The edge's light: the tally's border over the card, which comes up once
    /// and goes as the card arrives (Flash), so that something new at the edge
    /// of the eye is seen. Through XAML's own opacity, never the visual's: the
    /// card it lies over takes presses (K.Holding).
    /// </summary>
    private static Border Light(double radius) => new()
    {
        BorderBrush = Ink.TallyAt(0.85f),
        BorderThickness = new Thickness(1.5),
        CornerRadius = new CornerRadius(radius),
        Opacity = 0,
        IsHitTestVisible = false,
    };

    /// <summary>Once, a moment after the card has settled: the light up over 450 ms and away over 900.</summary>
    private void Flash(Border lit)
    {
        void After(int ms, Action then)
        {
            var timer = DispatcherQueue.CreateTimer();
            timer.Interval = TimeSpan.FromMilliseconds(ms);
            timer.IsRepeating = false;
            timer.Tick += (_, _) => then();
            timer.Start();
        }
        After(360, () =>
        {
            lit.OpacityTransition = new ScalarTransition { Duration = TimeSpan.FromMilliseconds(450) };
            lit.Opacity = 1;
            After(520, () =>
            {
                lit.OpacityTransition = new ScalarTransition { Duration = TimeSpan.FromMilliseconds(900) };
                lit.Opacity = 0;
            });
        });
    }

    private Grid Said(string? badge, string title, string[] lines, Tap[] buttons, bool warning = false, string? problem = null, UIElement[]? extra = null)
    {
        var tint = warning ? Ink.Danger : Ink.Tally;
        var circle = new Border
        {
            Width = 30,
            Height = 30,
            CornerRadius = new CornerRadius(15),
            Background = warning ? Ink.DangerAt(0.18f) : Ink.TallyAt(0.18f),
            VerticalAlignment = VerticalAlignment.Top,
            Child = badge != null ? K.Symbol(badge, 13, tint) : K.Spinner(14, tint),
        };
        if (circle.Child is FrameworkElement inside)
        {
            inside.HorizontalAlignment = HorizontalAlignment.Center;
            inside.VerticalAlignment = VerticalAlignment.Center;
        }
        var words = K.Column(4, K.Text(title, 14, K.Semibold));
        foreach (var line in lines) words.Children.Add(K.Text(line, 12.5, ink: Ink.Muted, lines: _sidebar ? 3 : 6));
        if (problem != null) words.Children.Add(K.Text(problem, 12.5, ink: Ink.Danger));
        foreach (var part in extra ?? []) words.Children.Add(part);
        var row = K.Row(8, buttons);
        row.HorizontalAlignment = HorizontalAlignment.Right;
        row.Margin = new Thickness(0, 6, 0, 0);
        words.Children.Add(row);
        return K.Line(12, K.Fixed(circle), K.Flex(words));
    }
}

internal static class AutomationNames
{
    public static T AutomationName<T>(this T element, string name) where T : UIElement
    {
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(element, name);
        return element;
    }
}
