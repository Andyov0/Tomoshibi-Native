using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Room;

/// <summary>
/// The small console for whoever is running the meeting
/// (components/room/HostPanel.tsx; Android's ui/call/HostPanel.kt).
///
/// A link somebody can be sent, who is at the door, where the meeting is
/// held, and the people in it, each of whom can be quietened, made host or
/// removed; then ending it for everybody, asked twice, in the panel itself
/// rather than in a dialog over the call. Deliberately less than the
/// management pages can do: this reaches nobody outside this room. The room
/// can change hands while the panel is open, and then the panel says that
/// somebody else runs it rather than keeping controls that would all answer
/// "not yours".
/// </summary>
internal sealed class HostPanel : Grid
{
    private readonly HostFeature _feature;
    private readonly Keyed _body = new(12);
    private readonly Arrivals _people = new(), _door = new(), _link = new(), _relays = new();
    private (string Id, FocusState State, DateTime At)? _lost;

    public HostPanel(HostFeature feature, Action close)
    {
        _feature = feature;
        _body.Padding = new Thickness(12);
        var column = K.Column(0, Heading(close), Panels.Rule(), _body);
        Children.Add(K.Glass(new ScrollViewer { Content = column, VerticalScrollBarVisibility = ScrollBarVisibility.Auto }, 22));
        Loaded += (_, _) =>
        {
            feature.PanelOpen = true;
            _ = feature.LoadRelays();
        };
        Unloaded += (_, _) => feature.PanelOpen = false;
        feature.Changed += Update;
        Unloaded += (_, _) => feature.Changed -= Update;
        Update();
    }

    private static Grid Heading(Action close)
    {
        var line = Panels.Heading(T("Running this room"), "crown.fill", T("Close"), close);
        // The crown in the tally, as the bar's button is.
        if (line.Children[0] is Border tile) tile.Child = K.Symbol("crown.fill", 12, Ink.TallyText);
        return line;
    }

    public void Update()
    {
        var kept = Focused();
        if (kept == null && _lost is { } lost && DateTime.UtcNow - lost.At < TimeSpan.FromSeconds(5)) kept = (lost.Id, lost.State);
        _lost = null;
        _body.Show(Parts());
        if (kept is { } focus) DispatcherQueue.TryEnqueue(() => Refocus(focus.Id, focus.State));
    }

    private static string Said(params object?[] parts) => string.Join("|", parts);

    /// <summary>
    /// The panel's sections, each with what it says: a section is drawn
    /// again only when that changes, and a row in it arrives only the first
    /// time it is there. Drawn whole on every change, somebody unmuting
    /// faded every row of the panel in again.
    /// </summary>
    private List<(string, string, Func<UIElement>)> Parts()
    {
        var f = _feature;
        var language = Localization.Shared.Language.ToString();
        if (!f.Standing.Yours) return new List<(string, string, Func<UIElement>)> { ("not-yours", language, NotYours) };
        var parts = new List<(string, string, Func<UIElement>)>
        {
            ("invite", Said(language, f.Copied, f.Busy == "invite", f.Busy == "revoke", f.Link, f.Scoped), Inviting),
        };
        // The list at the door keeps itself up to date.
        if (f.Waiting.Count > 0) parts.Add(("door", language, Door));
        if (f.UsableRelays.Count >= 2)
        {
            parts.Add(("move", Said(language, f.Busy == "move", f.ChoosingRelay, string.Join(",", f.UsableRelays.Select(r => r.Name + "=" + r.Label))), Moving));
        }
        parts.Add(("people", Said(language, f.Busy, f.Scoped,
            string.Join(",", f.Call.People.Select(p => $"{p.Id}/{p.Shown}/{p.Track(MediaSource.Microphone) is { Muted: false }}/{HostFeature.IsGuest(p)}"))), People));
        parts.Add((f.Ending ? "ending" : "end", Said(language, f.Busy == "close", f.Scoped), Ending));
        parts.Add(("note", Said(language, f.Standing.Admin), Note));
        return parts;
    }

    private static UIElement NotYours()
    {
        var row = K.Row(10, K.Symbol("crown", 13, Ink.Muted), K.Text(T("Somebody else runs this room."), 12.5, ink: Ink.Muted));
        row.HorizontalAlignment = HorizontalAlignment.Center;
        row.Padding = new Thickness(0, 18, 0, 18);
        return row;
    }

    private UIElement Note() => K.Text(_feature.Standing.Admin
        ? T("You can do this because you administer this server. Removing somebody ends their call; it does not stop them rejoining.")
        : T("Removing somebody ends their call. It does not stop them rejoining with the same name."), 11, ink: Ink.MutedAt(0.85f)).Pad(6, 0, 6, 0);

    // The keyboard's place

    /// <summary>The control in this panel the keyboard is on, by its automation id, and how it got there.</summary>
    private (string Id, FocusState State)? Focused()
    {
        if (XamlRoot == null || FocusManager.GetFocusedElement(XamlRoot) is not FrameworkElement focused) return null;
        var id = AutomationProperties.GetAutomationId(focused);
        if (string.IsNullOrEmpty(id)) return null;
        for (DependencyObject? at = focused; at != null; at = VisualTreeHelper.GetParent(at))
        {
            if (ReferenceEquals(at, this)) return (id, focused.FocusState);
        }
        return null;
    }

    /// <summary>
    /// Puts the keyboard back on the control it was on, drawn again. Every
    /// change draws the panel's sections again, and a focused button taken
    /// out of the tree sent the keyboard to the first thing in the window:
    /// muting somebody from the keyboard left the next press on the room's
    /// name. A control that has just been turned off (the mute, once they
    /// are muted) hands it to the next one along its row; a row that has
    /// gone (somebody let in) to the invitation at the top.
    /// </summary>
    private void Refocus(string id, FocusState state)
    {
        var tried = new List<string> { id };
        var person = id.LastIndexOf('.') is var dot and > 0 ? id[(dot + 1)..] : "";
        if (id.StartsWith("host.mute.")) tried.AddRange(new[] { "host.crown." + person, "host.remove." + person });
        if (id.StartsWith("host.crown.")) tried.Add("host.remove." + person);
        // The question in place of the press that asked it, and back.
        if (id == "host.end") tried.Add("host.cancel");
        if (id == "host.cancel") tried.Add("host.end");
        foreach (var each in tried)
        {
            if (Find(this, each) is not Control control) continue;
            // Turned off while the press it made is under way: kept for the
            // next drawing, which comes when it is over, rather than handed
            // to something else in the meantime.
            if (!control.IsEnabled)
            {
                _lost = (id, state, DateTime.UtcNow);
                return;
            }
            if (control.Focus(state == FocusState.Unfocused ? FocusState.Programmatic : state)) return;
        }
        if (Find(this, "host.invite") is Control first && first.IsEnabled) first.Focus(state == FocusState.Unfocused ? FocusState.Programmatic : state);
    }

    private static FrameworkElement? Find(DependencyObject under, string id)
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(under); i++)
        {
            var child = VisualTreeHelper.GetChild(under, i);
            if (child is FrameworkElement element && AutomationProperties.GetAutomationId(element) == id) return element;
            if (Find(child, id) is { } found) return found;
        }
        return null;
    }

    private static Border Card(UIElement child, Shade? tint = null, double padding = 4) => new()
    {
        Child = child,
        CornerRadius = new CornerRadius(16),
        Background = tint is { } t ? Ink.Brush(t.WithAlpha(0.10f)) : Ink.Wash(0.05f),
        BorderBrush = tint is { } e ? Ink.Brush(e.WithAlpha(0.28f)) : Ink.Wash(0.07f),
        BorderThickness = new Thickness(1),
        Padding = new Thickness(padding),
    };

    // A link, and the door

    private UIElement Inviting()
    {
        var f = _feature;
        var invite = Menu.Item(f.Copied ? T("Link copied") : T("Invite somebody"), () => _ = f.Invite(link => Pasteboard.Copy(link)),
            f.Copied ? "checkmark" : "link", role: Menu.Role.Prominent, busy: f.Busy == "invite", enabled: f.Busy != "invite");
        invite.Named(f.Copied ? T("Link copied") : T("Invite somebody"), "host.invite");
        var column = K.Column(0, invite);
        if (f.Link is { } link)
        {
            var address = new TextBlock
            {
                Text = link,
                FontFamily = K.Mono,
                FontSize = 10.5,
                Foreground = Ink.ForegroundAt(0.8f),
                TextWrapping = TextWrapping.Wrap,
                IsTextSelectionEnabled = true,
            };
            var box = new Border
            {
                Child = address,
                Background = Ink.Well(0.25f),
                BorderBrush = Ink.Wash(0.07f),
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(10),
                Padding = new Thickness(10, 8, 10, 8),
            };
            var note = K.Text(T("Anybody with this link can join, until you end the meeting. No passphrase needed.") + " " + T("It stops working after a day either way."), 11, ink: Ink.Muted);
            // A link goes out and cannot be taken back; this stops it without
            // ending the meeting. Under a scope it also stops the standing
            // links members made, so it says every link.
            var stop = Quiet(f.Scoped ? T("Stop every link to this room working") : T("Stop this link working"), f.Busy == "revoke", () => _ = f.Revoke());
            stop.IsEnabled = f.Busy != "revoke";
            stop.Named(f.Scoped ? T("Stop every link to this room working") : T("Stop this link working"), "host.revoke");
            var detail = K.Column(8, box, note, stop);
            detail.Padding = new Thickness(8, 4, 8, 8);
            _link.Begin();
            column.Children.Add(_link.Row("link:" + link, detail));
            _link.End();
        }
        else
        {
            _link.Begin();
            _link.End();
        }
        return Card(column);
    }

    /// <summary>Who is outside, under the control for letting people in: the two are the same thought (HostPanel.tsx).</summary>
    private UIElement Door()
    {
        var list = new DoorList(_feature, _door) { Margin = new Thickness(10, 4, 10, 10) };
        return Card(K.Column(4, Menu.Heading(T("Waiting to be let in")), list), Appearance.Shared.Tally, 0);
    }

    // Where it is held

    /// <summary>The move under way takes the chevron's place with the app's spinner (HostPanel.tsx <c>Elsewhere</c>).</summary>
    private UIElement Moving()
    {
        var f = _feature;
        var busy = f.Busy == "move";
        var item = Menu.Item(T("Hold this call somewhere else"), () =>
        {
            f.ChoosingRelay = !f.ChoosingRelay;
            Update();
        }, "server.rack", enabled: !busy);
        var mark = busy ? (FrameworkElement)K.Spinner(14) : K.Symbol("chevron.down", 10, Ink.Muted);
        if (!busy && f.ChoosingRelay) mark.RenderTransform = new RotateTransform { Angle = 180, CenterX = 5, CenterY = 5 };
        mark.HorizontalAlignment = HorizontalAlignment.Right;
        mark.VerticalAlignment = VerticalAlignment.Center;
        mark.Margin = new Thickness(0, 0, 12, 0);
        mark.IsHitTestVisible = false;
        var column = K.Column(2, K.Stack(item, mark));
        if (f.ChoosingRelay)
        {
            var choices = K.Column(2, K.Text(T("Everybody is reconnected. Nobody has to do anything."), 11, ink: Ink.Muted).Pad(10, 2, 10, 4));
            foreach (var relay in f.UsableRelays)
            {
                var label = Say(string.IsNullOrEmpty(relay.Label) ? relay.Name : relay.Label);
                choices.Children.Add(Menu.Item(label, () =>
                {
                    f.ChoosingRelay = false;
                    _ = f.Move(relay);
                }, "arrow.right"));
            }
            _relays.Begin();
            column.Children.Add(_relays.Row("choices", choices));
            _relays.End();
        }
        else
        {
            _relays.Begin();
            _relays.End();
        }
        return Card(column);
    }

    // The people

    private UIElement People()
    {
        var f = _feature;
        var column = K.Flowing(0);
        if (f.Call.People.Count == 0)
        {
            column.Children.Add(K.Text(T("Nobody else is here."), 12, ink: Ink.Muted, align: TextAlignment.Center).Pad(0, 14, 0, 14));
        }
        _people.Begin();
        foreach (var person in f.Call.People) column.Children.Add(_people.Row(person.Id, PersonRow(person)));
        _people.End();
        return Card(column);
    }

    /// <summary>One person, and what can be done about them (HostPanel.tsx's list).</summary>
    private FrameworkElement PersonRow(Person person)
    {
        var f = _feature;
        var busy = f.Busy != null;
        var name = person.Shown;
        // `isMicrophoneEnabled`: a microphone there and not already muted.
        var on = person.Track(MediaSource.Microphone) is { Muted: false };
        var parts = new List<(UIElement, double)>
        {
            K.Fixed(Faces.Monogram(name, person.Id, 26)),
            K.Flex(K.Text(name, 13, K.Medium, lines: 1).At(HorizontalAlignment.Left, VerticalAlignment.Center)),
        };
        if (f.Busy == person.Id) parts.Add(K.Fixed(K.Spinner(14).At(HorizontalAlignment.Center, VerticalAlignment.Center)));
        var mute = Round("mic.slash.fill", T("Mute"), () => _ = f.Silence(person), enabled: !busy && on);
        mute.Named(T("Mute"), "host.mute." + person.Id);
        parts.Add(K.Fixed(mute));
        // A room held under a scope has no host to hand over, and the server
        // refuses it outright; the button goes rather than answering with a
        // refusal. Faint beside a guest rather than taken away, and
        // pressable, so the reason can be said.
        if (!f.Scoped)
        {
            var guest = HostFeature.IsGuest(person);
            var crown = Round("crown", T("Make host"), () =>
            {
                if (guest) f.RefuseHandOver();
                else _ = f.HandOver(person);
            }, faint: guest, enabled: !busy);
            crown.Named(T("Make host"), "host.crown." + person.Id);
            parts.Add(K.Fixed(crown));
        }
        var remove = Round("person.badge.minus", T("Remove from room"), () => _ = f.TurnOut(person), destructive: true, enabled: !busy);
        remove.Named(T("Remove from room"), "host.remove." + person.Id);
        parts.Add(K.Fixed(remove));
        var line = K.Line(8, parts.ToArray());
        line.Padding = new Thickness(8, 6, 6, 6);
        return line;
    }

    // Ending it

    private UIElement Ending()
    {
        var f = _feature;
        if (!f.Ending)
        {
            var end = Menu.Item(T("End this meeting"), () =>
            {
                f.Ending = true;
                Update();
            }, "door.left.hand.closed");
            end.Named(T("End this meeting"), "host.end");
            return Card(end);
        }
        // The question names what will happen rather than asking whether
        // they are sure, which nobody has ever answered no to by reading it.
        var words = K.Text(f.Scoped
            ? T("Everybody will be disconnected and told the room has closed. Links made in the call stop working; links made on an account page do not.")
            : T("Everybody will be disconnected and told the room has closed. Links to it stop working."), 12);
        var top = K.Line(10, K.Fixed(new Border { Width = 18, Child = K.Symbol("door.left.hand.closed", 13, Ink.DangerText), VerticalAlignment = VerticalAlignment.Top }), K.Flex(words));
        var cancel = Tap.Words(T("Cancel"), height: 30, size: 12.5);
        cancel.Named(T("Cancel"), "host.cancel");
        cancel.Click += (_, _) =>
        {
            f.Ending = false;
            Update();
        };
        // Red: ending it for everybody is leaving, for everybody.
        var close = Tap.Words(T("Close the room"), look: Look.Destructive, height: 30, size: 12.5);
        close.HorizontalAlignment = HorizontalAlignment.Stretch;
        close.IsEnabled = f.Busy != "close";
        close.Named(T("Close the room"), "host.close");
        close.Click += (_, _) => _ = f.Dissolve();
        var buttons = K.Line(8, K.Fixed(cancel), K.Flex(close));
        var card = Card(K.Column(12, top, buttons), Ink.Palette.Danger, 12);
        return card;
    }

    /// <summary>
    /// A person's control (Notices.kt <c>RoundButton</c>): nothing behind it
    /// and the symbol muted until the pointer is on it, then lit, in red for
    /// removing somebody. Three of them on every row would otherwise be a
    /// column of buttons louder than the names they are about.
    /// </summary>
    private static Tap Round(string symbol, string label, Action act, bool destructive = false, bool faint = false, bool enabled = true)
    {
        var tap = new Tap(K.Symbol(symbol, 11, Ink.Muted), Look.Clear, 28, circle: true, label: label) { IsEnabled = enabled, Faint = faint ? 0.45 : 1 };
        tap.PointerEntered += (_, _) =>
        {
            if (!tap.IsEnabled) return;
            tap.Content = K.Symbol(symbol, 11, destructive ? Ink.DangerText : Ink.Foreground);
            if (destructive) tap.Resting = Ink.Palette.Danger.WithAlpha(0.16f);
        };
        tap.PointerExited += (_, _) =>
        {
            tap.Content = K.Symbol(symbol, 11, Ink.Muted);
            tap.Resting = null;
        };
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>
    /// A small, quiet press for something that undoes rather than does:
    /// stopping the link (HostPanel.kt <c>QuietButton</c>). It warms to red
    /// under the pointer, where it is about to happen.
    /// </summary>
    private static Tap Quiet(string title, bool busy, Action act)
    {
        var mark = busy ? (FrameworkElement)K.Spinner(10) : StruckLink();
        var row = K.Row(6, mark, K.Text(title, 11.5, K.Medium, Ink.Muted, lines: 1));
        row.VerticalAlignment = VerticalAlignment.Center;
        var tap = new Tap(row, Look.Plain, 26, padding: 10) { HorizontalAlignment = HorizontalAlignment.Left };
        tap.PointerEntered += (_, _) => tap.Resting = Ink.Palette.Danger.WithAlpha(0.14f);
        tap.PointerExited += (_, _) => tap.Resting = null;
        tap.Named(title);
        tap.Click += (_, _) => act();
        return tap;
    }

    /// <summary>A link struck through; there is no such symbol.</summary>
    private static Grid StruckLink()
    {
        var grid = new Grid { Width = 12, Height = 12 };
        grid.Children.Add(K.Symbol("link", 10, Ink.Muted));
        grid.Children.Add(new Microsoft.UI.Xaml.Shapes.Line { X1 = 1, Y1 = 11, X2 = 11, Y2 = 1, Stroke = Ink.Muted, StrokeThickness = 1.4 });
        return grid;
    }
}

/// <summary>
/// Who is at the door, each with the two answers (AtTheDoor.tsx;
/// CallOverlay.kt <c>DoorList</c>): what they typed to be called, which is a
/// claim, and where they knocked from, which is not. Drawn in the door's
/// notice and inside the host panel alike, and keeping itself up to date,
/// so that the notice need not be drawn again for somebody else knocking.
/// </summary>
internal sealed class DoorList : StackPanel
{
    private readonly HostFeature _feature;
    private readonly Arrivals _rows;
    private string _said = "\0";

    public DoorList(HostFeature feature, Arrivals? rows = null)
    {
        _feature = feature;
        _rows = rows ?? new Arrivals();
        Spacing = 8;
        Loaded += (_, _) =>
        {
            feature.Changed -= Update;
            feature.Changed += Update;
            Update();
        };
        Unloaded += (_, _) => feature.Changed -= Update;
        Update();
    }

    private void Update()
    {
        var feature = _feature;
        var said = string.Join("|", feature.Answering, Localization.Shared.Language, string.Join(",", feature.Waiting.Select(k => $"{k.Id}/{k.Name}/{k.Address}")));
        if (said == _said) return;
        _said = said;
        Children.Clear();
        _rows.Begin();
        foreach (var one in feature.Waiting)
        {
            var name = one.Name.Length == 0 ? T("Somebody") : one.Name;
            var words = K.Column(1, K.Text(name, 12.5, K.Medium, lines: 1), K.Text(one.Address, 10.5, ink: Ink.Muted, mono: true, lines: 1));
            words.VerticalAlignment = VerticalAlignment.Center;
            var later = Tap.Words(T("Not now"), height: 28, size: 12);
            later.Named(T("Not now"), "door.later." + one.Id);
            later.Click += (_, _) => _ = feature.Answer(one, admit: false);
            var admit = Tap.Words(T("Let in"), look: Look.Prominent, height: 28, size: 12);
            admit.Named(T("Let in"), "door.admit." + one.Id);
            admit.Click += (_, _) => _ = feature.Answer(one, admit: true);
            var busy = feature.Answering == one.Id;
            later.IsEnabled = admit.IsEnabled = !busy;
            var row = K.Line(9, K.Fixed(Faces.Monogram(one.Name.Length == 0 ? "?" : one.Name, one.Id, 26)), K.Flex(words), K.Fixed(later), K.Fixed(admit));
            Children.Add(_rows.Row(one.Id, row));
        }
        _rows.End();
    }
}
