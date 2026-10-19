using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Screens;

/// <summary>
/// While the deployment is first asked about: the ground and the mark,
/// breathing, rather than a form asking for an address the app already has
/// (Screens.kt <c>StartingScreen</c>).
/// </summary>
internal sealed class StartingScreen : Screen
{
    public StartingScreen(AppModel app) : base(app)
    {
        var mark = new Mark(56);
        var column = K.Column(18, mark, K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        Children.Add(new Backdrop());
        Children.Add(column);
        mark.Loaded += (_, _) =>
        {
            // From 55 per cent to whole and back, over 1.1 s each way.
            var visual = ElementCompositionPreview.GetElementVisual(mark);
            var compositor = visual.Compositor;
            var breathe = compositor.CreateScalarKeyFrameAnimation();
            var easing = compositor.CreateCubicBezierEasingFunction(new Vector2(0.42f, 0f), new Vector2(0.58f, 1f));
            breathe.InsertKeyFrame(0f, 0.55f);
            breathe.InsertKeyFrame(0.5f, 1f, easing);
            breathe.InsertKeyFrame(1f, 0.55f, easing);
            breathe.Duration = TimeSpan.FromMilliseconds(2200);
            breathe.IterationBehavior = AnimationIterationBehavior.Forever;
            visual.StartAnimation("Opacity", breathe);
        };
    }
}

/// <summary>
/// Opening on a deployment that will not answer: the one way on, which is
/// to ask again (Screens.kt <c>UnreachableScreen</c>). Without the address:
/// which server this is is not something the app says anywhere.
/// </summary>
internal sealed class UnreachableScreen : Screen
{
    private readonly Shared.Pressing _again;
    private readonly Shared.ProblemToast _toast;

    public UnreachableScreen(AppModel app) : base(app)
    {
        _again = new Shared.Pressing(T("Try again"));
        _again.Click += async (_, _) => await app.Connect();
        _toast = new Shared.ProblemToast(app);
        var content = K.Column(18, new Mark(56).At(HorizontalAlignment.Center, VerticalAlignment.Top), Shared.Heading(T("Could not reach the server.")), _again);
        Children.Add(Frames.CardScreen(content, foot: _toast));
        Update();
    }

    public override void Update()
    {
        _again.Busy = App.Busy;
        _again.IsEnabled = !App.Busy;
        _toast.Update();
    }
}

/// <summary>Waiting for somebody inside to let you in (Screens.kt <c>KnockingScreen</c>).</summary>
internal sealed class KnockingScreen : Screen
{
    public KnockingScreen(AppModel app) : base(app)
    {
        var waiting = K.Row(12, K.Spinner(16), K.Text(T("Waiting for somebody to let you in."), 14));
        waiting.HorizontalAlignment = HorizontalAlignment.Center;
        waiting.Margin = new Thickness(0, 8, 0, 8);
        var content = K.Column(18, Shared.Heading(app.Room, mono: true), waiting, Shared.BackButton(app.StopKnocking));
        Children.Add(Frames.CardScreen(content));
    }

    public override bool Back()
    {
        App.StopKnocking();
        return true;
    }
}

/// <summary>App.tsx's three endings, and the one somebody chose (Screens.kt <c>EndedScreen</c>).</summary>
internal sealed class EndedScreen : Screen
{
    private readonly Shared.Pressing? _again;
    private readonly Shared.ProblemSlot _problem;

    public EndedScreen(AppModel app) : base(app)
    {
        var ending = app.Ending;
        var symbol = ending switch
        {
            Ending.Left => "phone.down.fill",
            Ending.Closed => "door.left.hand.closed",
            Ending.Removed => "person.crop.circle.badge.xmark",
            _ => "wifi.exclamationmark",
        };
        var badge = new Border
        {
            Width = 64,
            Height = 64,
            CornerRadius = new CornerRadius(32),
            Background = Ink.White(0.07f),
            BorderBrush = Ink.White(0.09f),
            BorderThickness = new Thickness(1),
            Child = K.Symbol(symbol, 26, ending == Ending.Left ? Ink.Muted : Ink.Foreground),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var heading = Shared.Heading(ending switch
        {
            Ending.Left => T("You left the call."),
            Ending.Closed => T("The host closed this room."),
            Ending.Removed => T("You were removed from this room."),
            _ => T("The connection to this room was lost."),
        }, app.Room.Length > 0 ? app.Room : null);
        var content = K.Flowing(18);
        content.Children.Add(badge);
        content.Children.Add(heading);
        // Back in where that is something to do: after leaving, and after a
        // connection that dropped, which is one press back with the devices
        // already chosen (App.tsx). Not into a room the host closed, or one
        // somebody was put out of.
        if (ending is Ending.Left or Ending.Lost)
        {
            _again = new Shared.Pressing(T("Join again"));
            _again.Click += async (_, _) => await app.Join();
            content.Children.Add(_again);
        }
        _problem = new Shared.ProblemSlot(app);
        content.Children.Add(_problem);
        content.Children.Add(Shared.BackButton(app.BackToLobby));
        Children.Add(Frames.CardScreen(content));
        Update();
    }

    public override void Update()
    {
        if (_again != null)
        {
            _again.Busy = App.Busy;
            _again.IsEnabled = !App.Busy;
        }
        _problem.Update();
    }

    public override bool Back()
    {
        App.BackToLobby();
        return true;
    }
}

/// <summary>
/// Where a move is taking the meeting while there is no call to show it
/// over (CallOverlay.swift <c>MovingOverlay</c>): between leaving the old
/// room and entering the new one the room is gone, and the screen was blank.
/// </summary>
internal sealed class MovingScreen : Screen
{
    public MovingScreen(AppModel app, string to) : base(app)
    {
        var named = Say(to.Trim());
        var title = named.Length == 0 ? T("Moving this meeting") : T("Moving this meeting to {name}", ("name", named));
        var column = K.Column(12,
            K.Spinner(18).At(HorizontalAlignment.Center, VerticalAlignment.Center),
            K.Text(title, 14, K.Medium, align: TextAlignment.Center),
            // The one thing worth saying: nobody has to do anything.
            K.Text(T("You will be back in a moment."), 12, ink: Ink.Muted, align: TextAlignment.Center));
        column.HorizontalAlignment = HorizontalAlignment.Center;
        column.VerticalAlignment = VerticalAlignment.Center;
        column.Margin = new Thickness(24, 0, 24, 0);
        column.Named(title);
        Background = Ink.Brush(Theme.Background.WithAlpha(0.86f));
        Children.Add(column);
    }
}

/// <summary>
/// The join screen for somebody who came through a link or chose a room
/// (PreJoin.tsx; Screens.kt <c>PreJoinScreen</c>): the picture of yourself,
/// the room and what its door asks, your name, and in. The line saying why
/// the last press did not take, and the knock it may offer, come and go.
/// </summary>
internal sealed class PreJoinScreen : Screen
{
    private bool _sealing;
    private bool? _wide;
    private Shared.Pressing? _join;
    private Tap? _knock;
    private Shared.ProblemSlot? _problem;
    private Shared.CameraStage? _stage;
    private TextBlock? _identityLine;

    public PreJoinScreen(AppModel app) : base(app)
    {
        _sealing = app.SealingWord.Length > 0;
        SizeChanged += (_, e) => Build(Wide(e.NewSize.Width));
    }

    private void JoinNow()
    {
        if (App.Busy || App.JoiningName.Length == 0) return;
        if (!_sealing) App.SealingWord = "";
        _ = App.Join();
    }

    private void Build(bool wide)
    {
        if (_wide == wide) return;
        _wide = wide;
        Relaying();
        Children.Clear();
        var content = K.Flowing(18);
        content.Children.Add(RoomHeading(App, App.Room, Door(App)));
        if (App.Account == null)
        {
            // Somebody on an invitation has a name to choose and nothing
            // else to answer: the link is what lets them in, and a passphrase
            // field beside it reads as something they forgot. A name the link
            // was made for is shown and cannot be changed, because the server
            // signs that name in whatever is typed.
            var identity = K.Column(8,
                Shared.IdentityBox(App.InvitedName ?? App.Name, text => App.Name = text, App.Passphrase, text => App.Passphrase = text,
                    nameOnly: App.ArrivedInvited, fixedName: App.InvitedName != null, submit: JoinNow));
            _identityLine = Shared.Note(IdentityLine(App));
            identity.Children.Add(_identityLine);
            if (LinkWindow(App) is { } window) identity.Children.Add(Shared.Note(window));
            content.Children.Add(identity);
        }
        // Where the call is held, only for somebody who has not already
        // chosen it in the lobby, and only where there is a choice.
        content.Children.Add(Shared.OptionsCard(App, _sealing, on => _sealing = on, "prejoin.server", offerServer: App.Account == null));
        _problem = new Shared.ProblemSlot(App);
        content.Children.Add(_problem);
        _join = new Shared.Pressing(T("Join"), 52);
        _join.Named(T("Join"), "prejoin.join");
        _join.Click += (_, _) => JoinNow();
        content.Children.Add(_join);
        _knock = Tap.Words(T("Ask to be let in"), height: 44, size: 14);
        _knock.HorizontalAlignment = HorizontalAlignment.Stretch;
        _knock.Click += async (_, _) => await App.Knock();
        _knock.Arrives();
        content.Children.Add(_knock);
        // Calls off a join that is out (AppModel `CancelJoin`).
        content.Children.Add(Shared.BackButton(App.BackToLobby));
        Children.Add(Frames.PaneScreen(App, wide, content, out _stage));
        Update();
    }

    public override void Update()
    {
        if (_join == null) return;
        _join.Busy = App.Busy;
        _join.IsEnabled = !App.Busy && App.JoiningName.Length > 0;
        _knock!.Visibility = App.Problem != null && App.CanKnock ? Visibility.Visible : Visibility.Collapsed;
        _knock.IsEnabled = !App.Busy && App.JoiningName.Length > 0;
        _problem!.Update();
        _stage?.Update();
        if (_identityLine != null) _identityLine.Text = IdentityLine(App);
    }

    public override bool Back()
    {
        App.BackToLobby();
        return true;
    }

    /// <summary>
    /// PreJoin.tsx's one line under the identity, which changes rather than
    /// doubling: what a passphrase is for until there is one, and what it
    /// does once there is.
    /// </summary>
    private static string IdentityLine(AppModel app)
    {
        if (app.InvitedName is { } invited) return T("You were invited to this room as {name}.", ("name", invited));
        if (app.ArrivedInvited) return T("You were invited to this room. Choose a name and go in.");
        if (app.Passphrase.Length > 0) return T("Only you can join as {name}", ("name", app.JoiningName.Length > 0 ? app.JoiningName : "?"));
        return T("Add a passphrase to keep your name, and to run the room you open.");
    }

    /// <summary>When the link stops working, in the reader's own clock: a guest early for a call at four should know the link is still good at four.</summary>
    private static string? LinkWindow(AppModel app)
    {
        if (!app.ArrivedInvited || Refusals.Instant(app.Invitation?.Until) is not { } until) return null;
        if (Refusals.Instant(app.Invitation?.From) is { } from) return T("This link works from {from} until {until}.", ("from", LobbyModel.Said(from)), ("until", LobbyModel.Said(until)));
        return T("This link works until {until}.", ("until", LobbyModel.Said(until)));
    }

    /// <summary>
    /// RoomTitle.tsx: what this name is worth at this deployment's door,
    /// said before anybody presses Join. Not to somebody holding an
    /// invitation, nor to somebody signed in, whose door the session answers.
    /// </summary>
    private static string? Door(AppModel app)
    {
        if (app.Account != null || app.ArrivedInvited || app.Info is not { } info) return null;
        var scope = AppModel.SafeScope(app.Room);
        if (scope.Length > 0) return T("Only members of {scope} can join this room.", ("scope", scope));
        switch (info.JoinedBy)
        {
            case Joining.Accounts: return T("Sign in to join a room here.");
            case Joining.Invited: return T("Rooms here are by invitation. Open the link you were sent, or sign in.");
        }
        return info.OpenedBy switch
        {
            Opening.Admins => T("Only administrators can start new rooms. Enter the name you were given."),
            Opening.Signed => T("Set a passphrase to start a new room. Anybody can join one that already exists."),
            _ => LooksGenerated(app.Room) ? null : T("Anyone who guesses this name can join."),
        };
    }

    private static bool LooksGenerated(string room)
    {
        try
        {
            return TomoshibiProtocolMethods.LooksGenerated(room);
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>
    /// The room, as the heading of the screen, in the machine's monospace,
    /// with the way to copy its address where a plain address is a way in
    /// (Screens.kt <c>RoomHeading</c>): only where anybody may come in by the
    /// name, and never for a room held under a scope.
    /// </summary>
    public static StackPanel RoomHeading(AppModel app, string room, string? door)
    {
        string? link = null;
        if (app.Info?.JoinedBy == Joining.Anyone && room.Length > 0 && AppModel.SafeScope(room).Length == 0 && app.Deployment is { } deployment)
        {
            var baseUrl = deployment.BaseUrl();
            link = (baseUrl.EndsWith('/') ? baseUrl : baseUrl + "/") + "#/" + room;
        }
        var title = K.Text(room, 20, mono: true, lines: 2, align: TextAlignment.Center);
        var row = K.Row(8, title);
        row.HorizontalAlignment = HorizontalAlignment.Center;
        if (link != null) row.Children.Add(CopyLink(link, 30));
        var column = K.Column(6, row);
        if (door != null) column.Children.Add(K.Text(door, 12.5, ink: Ink.Muted, align: TextAlignment.Center).Arrives());
        return column;
    }

    /// <summary>A circle that copies a link, and says so for a moment (kit/Fields.kt <c>CopyLinkButton</c>).</summary>
    public static Tap CopyLink(string link, double size = 32)
    {
        var tap = Tap.Icon("doc.on.doc", T("Copy link"), size: size, glyph: size * 0.38);
        tap.Click += async (_, _) =>
        {
            Pasteboard.Copy(link);
            tap.Content = K.Symbol("checkmark", size * 0.38, Ink.Tally);
            tap.Tip(T("Link copied"));
            await Task.Delay(1600);
            tap.Content = K.Symbol("doc.on.doc", size * 0.38, Ink.Foreground);
            tap.Tip(T("Copy link"));
        };
        return tap;
    }
}

/// <summary>
/// An arranged meeting (Waiting.tsx inside PreJoin.tsx; Screens.kt
/// <c>WaitingScreen</c>): the ordinary join screen, because the point of
/// arriving early is to have the camera, the microphone and the name settled
/// before the meeting, with a button that waits. Nothing is asked of the
/// door until it is pressed.
/// </summary>
internal sealed class WaitingScreen : Screen
{
    private bool _sealing;
    private bool? _wide;
    private StackPanel? _state;
    private string? _said;
    private Shared.ProblemSlot? _problem;
    private Shared.CameraStage? _stage;
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _clock;

    public WaitingScreen(AppModel app) : base(app)
    {
        _sealing = app.SealingWord.Length > 0;
        SizeChanged += (_, e) => Build(Wide(e.NewSize.Width));
        // The host's button comes alive when the window opens, without
        // anybody reloading anything: a clock, every thirty seconds.
        _clock = DispatcherQueue.CreateTimer();
        _clock.Interval = TimeSpan.FromSeconds(30);
        _clock.Tick += (_, _) => Update();
        _clock.Start();
    }

    public override void Gone() => _clock.Stop();

    private void Build(bool wide)
    {
        if (_wide == wide) return;
        _wide = wide;
        Relaying();
        Children.Clear();
        var content = K.Flowing(18);
        content.Children.Add(Shared.Heading(App.Room, mono: true));
        if (App.Account == null)
        {
            // A guest with no saved name gives one here; the button waits for
            // it (Waiting.tsx: "Nothing is asked until there is one").
            content.Children.Add(Shared.IdentityBox(App.Name, text => App.Name = text, App.Passphrase, text => App.Passphrase = text, nameOnly: true));
        }
        content.Children.Add(Shared.OptionsCard(App, _sealing, on => _sealing = on, "waiting.server", offerServer: false));
        _state = K.Flowing(12);
        content.Children.Add(_state);
        _problem = new Shared.ProblemSlot(App);
        content.Children.Add(_problem);
        content.Children.Add(Shared.BackButton(App.StopWaiting));
        Children.Add(Frames.PaneScreen(App, wide, content, out _stage));
        Update();
    }

    public override void Update()
    {
        if (_state == null) return;
        _problem!.Update();
        _stage?.Update();
        var waiting = App.Waiting;
        // Drawn again only when what it says changes, so that the button is
        // not made anew under the pointer at every change of the model.
        var said = $"{waiting}|{App.Arrangement}|{App.ArrangedReady}|{App.Busy}|{App.JoiningName.Length > 0}|{DateTimeOffset.UtcNow.ToUnixTimeSeconds() / 30}|{Localization.Shared.Language}";
        if (said == _said) return;
        _said = said;
        _state.Children.Clear();
        switch (waiting.Gone)
        {
            case AppModel.WaitingState.GoneHow.Cancelled:
                _state.Children.Add(Shared.Note(T("That invitation is no longer good. Ask for another.")));
                return;
            case AppModel.WaitingState.GoneHow.Ended:
                _state.Children.Add(Shared.Note(T("That meeting has ended.")));
                return;
            case AppModel.WaitingState.GoneHow.NotComing:
                var again = Tap.Words(T("Ask again"), height: 44, size: 14);
                again.HorizontalAlignment = HorizontalAlignment.Stretch;
                again.Click += (_, _) => App.AskAgain();
                _state.Children.Add(Shared.Note(T("The host has not started this meeting.")));
                _state.Children.Add(again);
                return;
        }
        var arrangement = App.Arrangement;
        var opens = App.ArrangedOpens;
        var now = DateTimeOffset.UtcNow;
        var tooEarly = arrangement?.Mine == true && !App.ArrangedReady && opens != null && now < opens;
        if (arrangement != null && !App.ArrangedReady)
        {
            var whenSaid = Refusals.Instant(arrangement.At) is { } at ? LobbyModel.Said(at) : arrangement.At;
            _state.Children.Add(Shared.Note(arrangement.Mine
                ? tooEarly ? T("Your meeting can start from {when}.", ("when", LobbyModel.Said(opens!.Value))) : T("Your meeting is arranged for {when}. Joining starts it.", ("when", whenSaid))
                : T("This meeting is arranged for {when}.", ("when", whenSaid))));
        }
        else if (arrangement == null)
        {
            if (waiting.Unreachable) _state.Children.Add(Shared.Note(T("Could not reach the meeting. Trying again.")));
            else _state.Children.Add(K.Spinner(16).At(HorizontalAlignment.Center, VerticalAlignment.Center));
        }
        var words = waiting.Asking ? T("Waiting for the host to start the meeting.")
            : App.ArrangedReady ? T("Join")
            : arrangement?.Mine == true ? T("Start the meeting")
            : T("Wait for the host");
        var go = new Shared.Pressing(words, 52) { Busy = waiting.Asking || App.Busy };
        go.IsEnabled = App.JoiningName.Length > 0 && !waiting.Asking && !App.Busy && arrangement != null && !tooEarly;
        go.Click += (_, _) =>
        {
            if (!_sealing) App.SealingWord = "";
            _ = App.GoArranged();
        };
        _state.Children.Add(go);
    }

    public override bool Back()
    {
        App.StopWaiting();
        return true;
    }
}
