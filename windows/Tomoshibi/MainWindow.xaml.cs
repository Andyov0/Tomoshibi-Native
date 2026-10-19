using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using Tomoshibi.UI;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi;

/// <summary>
/// Phase one's two screens: who joins which room, and the call. The full
/// screens of the other apps come later; what is here is the plumbing under
/// them, tidily laid out.
/// </summary>
public sealed partial class MainWindow : Window
{
    private readonly Launch _launch;
    private readonly DispatcherQueue _ui;
    private readonly DispatcherQueueTimer _stats;
    private Deployment? _deployment;
    private bool _signedIn;
    private CallSession? _session;
    private readonly List<VideoTile> _tiles = new();

    internal MainWindow(Launch launch)
    {
        _launch = launch;
        InitializeComponent();
        _ui = DispatcherQueue.GetForCurrentThread();
        JoinScreen.Arrives();
        CallScreen.Arrives();
        Alone.Arrives();
        NameBox.Text = launch.Name;
        RoomBox.Text = launch.Autojoin ?? "";
        PassphraseBox.Password = launch.Passphrase ?? "";
        MicBox.IsChecked = launch.Microphone;
        CameraBox.IsChecked = launch.Camera;
        Stage.SizeChanged += (_, _) => Lay();

        _stats = _ui.CreateTimer();
        _stats.Interval = TimeSpan.FromSeconds(2);
        _stats.Tick += (_, _) => Report();

        Closed += async (_, _) => await LeaveCall();
        _ = Start();
    }

    /// <summary>
    /// What a launch asks for, in the order the Apple app does it
    /// (AppModel.start): the deployment, then signing in, then the room.
    /// Only a launch that names its deployment goes anywhere by itself.
    /// </summary>
    private async Task Start()
    {
        if (_launch.Base == null)
        {
            if (_launch.Autojoin != null || _launch.SignInName != null)
                Log.Line("TOMOSHIBI_AUTOJOIN and TOMOSHIBI_SIGNIN need TOMOSHIBI_BASE; waiting for a person instead");
            return;
        }
        try
        {
            _deployment = new Deployment(_launch.Base);
            if (_launch.SignInName != null && _launch.SignInPassphraseFile != null)
            {
                var passphrase = (await File.ReadAllTextAsync(_launch.SignInPassphraseFile)).Trim();
                var account = await _deployment.SignIn(_launch.SignInName, passphrase);
                _signedIn = true;
                Log.Line($"signed in as {account.Name}");
            }
            if (_launch.Autojoin != null) await Join(_launch.Autojoin);
        }
        catch (Exception fault)
        {
            Say(fault);
        }
    }

    private async void OnJoin(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(RoomBox.Text)) return;
        try
        {
            // A person at the window with no deployment named is using the
            // app for real, and the app is the home deployment's.
            _deployment ??= new Deployment(_launch.Base ?? TomoshibiProtocolMethods.HomeDeployment());
            await Join(RoomBox.Text.Trim());
        }
        catch (Exception fault)
        {
            Say(fault);
        }
    }

    private async Task Join(string room)
    {
        JoinButton.IsEnabled = false;
        JoinStatus.Text = "Joining…";
        try
        {
            var name = string.IsNullOrWhiteSpace(NameBox.Text) ? _launch.Name : NameBox.Text.Trim();
            var session = await CallSession.Join(_deployment!, _signedIn, _launch, room, name, PassphraseBox.Password, _ui);
            _session = session;
            session.Event += Heard;
            Log.Line($"in call {session.Room} as {session.Call.LocalIdentity()} with {session.People.Count} others");
            CallTitle.Text = session.Room;
            JoinScreen.Visibility = Visibility.Collapsed;
            CallScreen.Visibility = Visibility.Visible;
            JoinStatus.Text = "";
            if (MicBox.IsChecked == true) await session.SetMicrophone(true);
            if (CameraBox.IsChecked == true) await session.SetCamera(true);
            MicToggle.IsChecked = session.MicrophoneOn;
            CameraToggle.IsChecked = session.CameraOn;
            _stats.Start();
            if (_launch.QuitAfterSeconds is { } seconds) _ = QuitAfter(seconds);
        }
        finally
        {
            JoinButton.IsEnabled = true;
        }
    }

    private async Task QuitAfter(int seconds)
    {
        await Task.Delay(TimeSpan.FromSeconds(seconds));
        Log.Line("leaving, as the launch asked");
        await LeaveCall();
        Application.Current.Exit();
    }

    private void Heard(CallEvent e)
    {
        switch (e)
        {
            case CallEvent.TrackSubscribed { Track.Kind: MediaKind.Video } subscribed:
                AddTile(subscribed.Identity, subscribed.Track);
                break;
            case CallEvent.TrackUnsubscribed unsubscribed:
                RemoveTiles(t => t.Sid == unsubscribed.Sid);
                break;
            case CallEvent.TrackUnpublished unpublished:
                RemoveTiles(t => t.Sid == unpublished.Sid);
                break;
            case CallEvent.ParticipantJoined joined:
                Log.Line($"{joined.Participant.Name} is here");
                break;
            case CallEvent.ParticipantLeft left:
                Log.Line($"{left.Identity} left");
                RemoveTiles(t => t.Identity == left.Identity);
                break;
            case CallEvent.StateChanged changed:
                Log.Line($"call {changed.State}");
                break;
            case CallEvent.Ended ended:
                Log.Line($"call ended: {ended.Reason}");
                _ = LeaveCall();
                break;
        }
        Alone.Visibility = _tiles.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void AddTile(string identity, TrackInfo track)
    {
        if (_session == null || _tiles.Any(t => t.Sid == track.Sid)) return;
        // A camera fills its tile, as on the other apps; a shared screen is
        // read, and is shown whole.
        var fill = track.Source != MediaSource.ScreenShare;
        var tile = new VideoTile(_session.Call, track.Sid, identity, _session.NameOf(identity), fill);
        _tiles.Add(tile);
        Stage.Children.Add(tile);
        Lay();
        tile.Attach();
        Log.Line($"showing {track.Source} {track.Sid} from {identity}");
    }

    private void RemoveTiles(Func<VideoTile, bool> which)
    {
        foreach (var tile in _tiles.Where(which).ToList())
        {
            tile.Detach();
            _tiles.Remove(tile);
            Stage.Children.Remove(tile);
        }
        Lay();
    }

    /// <summary>The tiles in as square a grid as their number allows.</summary>
    private void Lay()
    {
        var count = _tiles.Count;
        if (count == 0) return;
        var wide = Stage.ActualWidth >= Stage.ActualHeight;
        var across = (int)Math.Ceiling(Math.Sqrt(count));
        var down = (int)Math.Ceiling(count / (double)across);
        if (!wide) (across, down) = (down, across);
        Stage.ColumnDefinitions.Clear();
        Stage.RowDefinitions.Clear();
        for (var i = 0; i < across; i++) Stage.ColumnDefinitions.Add(new ColumnDefinition());
        for (var i = 0; i < down; i++) Stage.RowDefinitions.Add(new RowDefinition());
        for (var i = 0; i < count; i++)
        {
            Grid.SetColumn(_tiles[i], i % across);
            Grid.SetRow(_tiles[i], i / across);
        }
    }

    private void Report()
    {
        if (_session == null) return;
        var pictures = _tiles.Select(t => $"video {t.Sid} drawn={t.FramesDrawn}");
        Log.Line($"stats {string.Join("; ", pictures.Append(_session.Reading()))}");
    }

    private async void OnMicrophone(object sender, RoutedEventArgs e)
    {
        if (_session == null) return;
        await _session.SetMicrophone(MicToggle.IsChecked == true);
    }

    private async void OnCamera(object sender, RoutedEventArgs e)
    {
        if (_session == null) return;
        await _session.SetCamera(CameraToggle.IsChecked == true);
    }

    private async void OnLeave(object sender, RoutedEventArgs e) => await LeaveCall();

    private async Task LeaveCall()
    {
        var session = _session;
        if (session == null) return;
        _session = null;
        _stats.Stop();
        Report();
        RemoveTiles(_ => true);
        session.Event -= Heard;
        try
        {
            await session.Leave();
        }
        catch (Exception fault)
        {
            Log.Line($"leaving: {fault.Message}");
        }
        session.Dispose();
        Log.Line("left the call");
        CallScreen.Visibility = Visibility.Collapsed;
        JoinScreen.Visibility = Visibility.Visible;
    }

    /// <summary>A failure, in words for the person and in full for the log.</summary>
    private void Say(Exception fault)
    {
        Log.Line($"failed: {fault.GetType().Name}: {fault.Message}");
        JoinStatus.Text = fault switch
        {
            ProtocolException.Network => "Cannot reach the server.",
            ProtocolException.Refused refused => $"Refused ({refused.code}).",
            CallException.Connect => "Could not connect to the call.",
            _ => "Something went wrong.",
        };
    }
}
