using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using Tomoshibi.UI.Lobby;
using Tomoshibi.UI.Room;

namespace Tomoshibi.UI.Screens;

/// <summary>
/// The whole app (Screens.swift <c>AppRoot</c>, Android's ui/App.kt): one
/// screen at a time, each arriving and leaving on the same two curves, over
/// the window's dark ground. Only the key decides when one screen gives way
/// to another, so a new picture on the account does not cross-fade the
/// lobby; the account and the call ride along with the key so that a screen
/// leaving after its account or call has gone is still drawn as it was.
/// </summary>
internal sealed class ScreenHost : Grid
{
    private readonly AppModel _app;
    private Screen? _current;
    private string? _key;

    public ScreenHost(AppModel app)
    {
        _app = app;
        app.Changed += Follow;
        Follow();
    }

    /// <summary>Which screen is up, and what it is drawn for.</summary>
    private string KeyOf() => _app.Showing switch
    {
        AppModel.Screen.Starting => "starting",
        AppModel.Screen.Unreachable => "unreachable",
        // Signed in or not is the whole difference (Lobby.swift `LobbyView`).
        AppModel.Screen.Lobby => _app.Account is { } account ? "lobby:" + account.Trip : "signin",
        AppModel.Screen.Prejoin => "prejoin",
        AppModel.Screen.Knocking => "knocking",
        AppModel.Screen.Waiting => "waiting",
        AppModel.Screen.Call => _app.Call is { } call ? "call:" + call.GetHashCode()
            : _app.MovingTo is { } to ? "moving:" + to
            // Never nothing: a call screen with no call and no move in
            // progress was a black window after a rejoin that failed; the
            // join screen, with the reason on it, is where that person can
            // do something.
            : "prejoin",
        AppModel.Screen.Ended => "ended",
        _ => "starting",
    };

    private Screen Make(string key) => key switch
    {
        "starting" => new StartingScreen(_app),
        "unreachable" => new UnreachableScreen(_app),
        "signin" => new SignInScreen(_app),
        "prejoin" => new PreJoinScreen(_app),
        "knocking" => new KnockingScreen(_app),
        "waiting" => new WaitingScreen(_app),
        "ended" => new EndedScreen(_app),
        _ when key.StartsWith("lobby:") => new LobbyScreen(_app, _app.Account!),
        _ when key.StartsWith("call:") => new RoomScreen(_app, _app.Call!),
        _ when key.StartsWith("moving:") => new MovingScreen(_app, _app.MovingTo ?? ""),
        _ => new StartingScreen(_app),
    };

    private void Follow()
    {
        var key = KeyOf();
        if (key == _key)
        {
            _current?.Update();
            return;
        }
        Log.Line($"showing {key.Split(':')[0]}");
        _key = key;
        Swap(Make(key), animated: true);
    }

    private void Swap(Screen next, bool animated)
    {
        var old = _current;
        _current = next;
        // The leaving one fades by its own hide animation as it is taken out.
        if (animated) next.Arrives();
        void Settled(object sender, RoutedEventArgs e)
        {
            next.Loaded -= Settled;
            next.SettleFocus();
        }
        next.Loaded += Settled;
        Children.Add(next);
        if (old != null)
        {
            old.Gone();
            // Made again in place, the old one goes at once rather than
            // fading over its own replacement.
            if (!animated) Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.SetImplicitHideAnimation(old, null);
            Children.Remove(old);
        }
    }

    /// <summary>The language or the colour changed: the screen says itself again, or is made again in place.</summary>
    public void Refresh()
    {
        if (_current == null || _key == null) return;
        if (_current.Refresh()) return;
        var again = Make(_key);
        Swap(again, animated: false);
    }

    /// <summary>Escape with nothing open over the screen.</summary>
    public bool Back() => _current?.Back() ?? false;

    public Screen? Current => _current;
}
