using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.UI.Kit;
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
/// it as they cover the lobby. Floating, in the wide lobby above the content
/// and on the sign-in screen under its language menu, it slides down from
/// above and fades, and leaves the same way; inline, in the narrow lobby's
/// column under its large title, it opens a space for itself and closes it
/// again. What it says changes with a cross-fade and a change of height,
/// never a cut.
/// </summary>
internal sealed class UpdateBanner : Grid
{
    private readonly Updates _updates;
    private readonly bool _inline;
    private readonly double _top;
    private readonly double _below;
    private Border? _card;
    private Unfolding? _body;
    private Unfolding? _space;
    private string? _said;
    private TextBlock? _progressLine;
    private Border? _progressFill;
    private Border? _progressTrack;
    private float _fraction;

    /// <param name="inline">In a column, opening a space, rather than floating over the content.</param>
    /// <param name="top">What it clears below the top of where it is put.</param>
    /// <param name="below">Inline, the space it keeps below itself while it is open.</param>
    public UpdateBanner(Updates updates, bool inline = false, double top = 12, double below = 0)
    {
        _updates = updates;
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
        switch (step)
        {
            case Step.Downloading downloading:
                Progress(downloading.Received, downloading.Total);
                break;
            case Step.Held held:
                Progress(held.Received, held.Total);
                break;
        }
    }

    /// <summary>The card, flat as the screens are drawn: the surface colour with the theme's edge, and a shadow only where it floats.</summary>
    private void Arrive()
    {
        _body = new Unfolding();
        _card = new Border
        {
            Background = Ink.Surface,
            BorderBrush = Ink.Border,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(20),
            Padding = new Thickness(14),
            Child = _body,
        }.Named("update.banner");
        if (_inline)
        {
            // In a holder that carries the margins, so that they open and
            // close with the card rather than standing empty when it is gone.
            _space!.Show(new Border { Child = _card, Padding = new Thickness(0, _top, 0, _below) });
            return;
        }
        _card.MaxWidth = 520;
        _card.Margin = new Thickness(16, _top, 16, 0);
        _card.Shadow = new ThemeShadow();
        _card.Translation = new Vector3(0, 0, 24);
        // From above the window's edge: its own height and what it clears.
        _card.Arrives(1f, fromY: -(float)(_top + 160));
        Children.Add(_card);
    }

    private void Leave()
    {
        if (_card == null) return;
        if (_inline) _space!.Show(null);
        else Children.Remove(_card);
        _card = null;
        _body = null;
        _said = null;
        _progressLine = null;
        _progressFill = null;
        _progressTrack = null;
        _fraction = 0;
    }

    private UIElement Body(Step step) => step switch
    {
        Step.Offered offered => Said(
            offered.Problem != null ? "exclamationmark.circle.fill" : "sparkles",
            T("Tomoshibi {version} is available", ("version", offered.Offer.Version)),
            Notes(offered.Offer),
            warning: offered.Problem != null,
            problem: offered.Problem,
            buttons:
            [
                Button(T("Later"), _updates.Later, "update.later"),
                Button(offered.Problem != null ? T("Try again") : T("Update"), _updates.Accept, "update.accept", prominent: true),
            ]),
        Step.Downloading or Step.Held => Downloading(_updates.Offer!.Version),
        Step.Installing installing => Said(
            null,
            T("Installing Tomoshibi {version}", ("version", installing.Offer.Version)),
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

    private UIElement Downloading(string version)
    {
        _progressLine = K.Text("", 12.5, ink: Ink.Muted);
        _progressTrack = new Border { Height = 5, CornerRadius = new CornerRadius(2.5), Background = Ink.SurfaceHigh, Margin = new Thickness(0, 4, 0, 0) };
        _progressFill = new Border { Height = 5, CornerRadius = new CornerRadius(2.5), Background = Ink.Tally, HorizontalAlignment = HorizontalAlignment.Stretch };
        ElementCompositionPreview.SetIsTranslationEnabled(_progressFill, true);
        var visual = ElementCompositionPreview.GetElementVisual(_progressFill);
        visual.Scale = new Vector3(Math.Max(0.0001f, _fraction), 1, 1);
        _progressTrack.Child = _progressFill;
        _progressTrack.AutomationName(T("Downloading Tomoshibi {version}", ("version", version)));
        var said = Said(
            "arrow.clockwise",
            T("Downloading Tomoshibi {version}", ("version", version)),
            [],
            extra: [_progressLine, _progressTrack],
            buttons: [Button(T("Cancel"), _updates.Cancel, "update.cancel")]);
        return said;
    }

    /// <summary>How much has arrived, as a filling capsule that moves smoothly between reports, and the megabytes under the title.</summary>
    private void Progress(long received, long total)
    {
        if (_progressLine == null || _progressFill == null) return;
        _progressLine.Text = T("{received} of {total}", ("received", Megabytes(received)), ("total", Megabytes(total)));
        var fraction = total > 0 ? Math.Clamp((float)received / total, 0f, 1f) : 0f;
        if (Math.Abs(fraction - _fraction) < 0.001f) return;
        _fraction = fraction;
        var visual = ElementCompositionPreview.GetElementVisual(_progressFill);
        var compositor = visual.Compositor;
        visual.CenterPoint = Vector3.Zero;
        var grow = compositor.CreateVector3KeyFrameAnimation();
        grow.InsertKeyFrame(1f, new Vector3(Math.Max(0.0001f, fraction), 1, 1), Motion.SettleCurve(compositor));
        grow.Duration = Motion.Settle;
        visual.StartAnimation("Scale", grow);
        _progressTrack?.SetValue(Microsoft.UI.Xaml.Automation.AutomationProperties.ItemStatusProperty, $"{(int)(fraction * 100)}%");
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
    private static Grid Said(string? badge, string title, string[] lines, Tap[] buttons, bool warning = false, string? problem = null, UIElement[]? extra = null)
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
        foreach (var line in lines) words.Children.Add(K.Text(line, 12.5, ink: Ink.Muted, lines: 6));
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
