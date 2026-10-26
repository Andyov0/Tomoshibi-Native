using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Screens;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>
/// Starting a meeting or joining one from the lobby (Android's
/// ui/lobby/MeetingSheet.kt): Start or Join at its head, the picture of
/// yourself and its switches, the room's name, the relay and sealing, and
/// the button. A room from the recent list is asked about first and opens
/// as Join where a meeting is going on in it and as Start where none is.
/// </summary>
internal sealed class MeetingSheet : Grid
{
    private readonly AppModel _app;
    private readonly Account _account;
    private readonly Action _close;
    private readonly bool _wide;
    private bool _starting;
    private bool _again;
    private string _room;
    private bool _sealing;
    private bool _working;
    private CancellationTokenSource? _going;

    private Segmented? _choice;
    private RoomField? _field;
    private TextBlock? _hint;
    private TextBlock? _problem;
    private Tap? _knock;
    private Shared.Pressing? _go;
    private Shared.CameraStage? _stage;

    public bool Wide => _wide;
    public bool Starting => _starting;
    public string Room => _room;
    public bool Sealing => _sealing;
    public bool Working => _working;

    public MeetingSheet(AppModel app, Account account, bool start, string room, Action close, bool wide, bool? sealing = null)
    {
        _app = app;
        _account = account;
        _close = close;
        _wide = wide;
        _starting = start;
        _room = room;
        _sealing = sealing ?? app.SealingWord.Length > 0;
        // A room from the recent list: is a meeting going on in it?
        _again = !start && room.Length > 0;
        Build();
        Loaded += (_, _) => _field?.Focus();
        if (_again) _ = Resolve(room);
    }

    private async Task Resolve(string room)
    {
        bool? live = null;
        try
        {
            if (_app.Deployment is { } deployment) live = await deployment.RoomLive(room);
        }
        catch (Exception)
        {
        }
        if (!_again || live == null) return;
        _again = false;
        _starting = live == false;
        _choice?.Choose(_starting ? 0 : 1, tell: false);
        Update();
    }

    public void Refresh() => Build();

    private void Build()
    {
        Children.Clear();
        var column = K.Flowing(16);

        // The head: close, and Start or Join; on a phone's width the choice
        // takes the whole width beside the close button (centred between two
        // buttons' worth of space, it cut "Start new meeting" short there).
        var cancel = Tap.Icon("xmark", T("Cancel"), Look.Plain, 32, 13);
        cancel.Click += (_, _) => Cancel();
        _choice = new Segmented(new[] { T("Start new meeting"), T("Join a meeting") }, _starting ? 0 : 1, 32, fill: !_wide);
        _choice.Chose += index =>
        {
            _starting = index == 0;
            _again = false;
            Rebuild();
        };
        if (_wide)
        {
            _choice.HorizontalAlignment = HorizontalAlignment.Center;
            _choice.MaxWidth = 300;
            var head = new Grid();
            head.Children.Add(cancel.At(HorizontalAlignment.Left, VerticalAlignment.Center));
            head.Children.Add(_choice);
            column.Children.Add(head);
        }
        else
        {
            column.Children.Add(K.Line(8, K.Fixed(cancel), K.Flex(_choice)));
        }

        if (_wide)
        {
            // The picture of yourself and its switches, in the sheet.
            _stage = new Shared.CameraStage(_app, 20);
            var picture = new Grid { Children = { _stage } };
            var switches = new Border
            {
                Child = Shared.DeviceSwitches(_app, 38),
                Background = K.Acrylic(),
                CornerRadius = new CornerRadius(25),
                Padding = new Thickness(6),
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Bottom,
                Margin = new Thickness(0, 0, 0, 12),
            };
            picture.Children.Add(switches);
            picture.SizeChanged += (_, e) => picture.Height = e.NewSize.Width * 9 / 16;
            column.Children.Add(picture);
        }
        else
        {
            // On a phone's width the picture is the whole window behind the
            // pane, and the switches are in the pane, large.
            var switches = Shared.DeviceSwitches(_app, 52);
            switches.HorizontalAlignment = HorizontalAlignment.Center;
            column.Children.Add(switches);
        }

        // The form.
        var form = K.Flowing(10);
        _field = new RoomField(_starting ? T("Room name") : T("Room name or invite link"), _room, _account.Scopes, _account.Admin, capsule: false);
        _field.Changed += text =>
        {
            _room = text;
            Update();
        };
        _field.Submitted += Go;
        form.Children.Add(_field);
        _hint = K.Text(T("Name it yourself, or leave it blank for one nobody has used."), 12, ink: Ink.Muted).Pad(6, 0, 6, 0);
        _hint.Arrives();
        form.Children.Add(_hint);
        form.Children.Add(Shared.OptionsCard(_app, _sealing, on => _sealing = on, "sheet.server"));
        column.Children.Add(form);

        // The foot: why the last press did not take, the knock it may offer,
        // and the button: beside Cancel in a dialog, the whole width on a
        // phone's, where the cross is the way out.
        _problem = K.Text("", 12.5, ink: Ink.DangerText, align: TextAlignment.Center);
        _problem.Arrives();
        column.Children.Add(_problem);
        _knock = Tap.Words(T("Ask to be let in"), height: 40, size: 13.5);
        _knock.Arrives();
        _knock.Click += async (_, _) =>
        {
            await _app.Knock();
            _close();
        };
        _go = _wide ? new Shared.Pressing(_starting ? T("Start") : T("Join"), 36, 13.5) : new Shared.Pressing(_starting ? T("Start") : T("Join"), 56, 16);
        _go.Named(_starting ? T("Start") : T("Join"), "sheet.go");
        _go.Click += (_, _) => Go();
        if (_wide)
        {
            var cancelWords = Tap.Words(T("Cancel"), height: 36);
            cancelWords.Click += (_, _) => Cancel();
            _go.HorizontalAlignment = HorizontalAlignment.Right;
            column.Children.Add(K.Line(10, K.Fixed(_knock), K.Flex(K.Spacer()), K.Fixed(cancelWords), K.Fixed(_go)));
        }
        else
        {
            column.Children.Add(K.Line(10, K.Fixed(_knock), K.Flex(_go)));
        }

        if (_wide)
        {
            column.Padding = new Thickness(20);
            // The evening room faintly behind the dialog's surface.
            Children.Add(new Backdrop { Opacity = 0.55 });
            Children.Add(column);
        }
        else
        {
            // The whole window: the picture of yourself behind a pane of
            // glass at the foot (MeetingSheet.kt's phone layout). The pane is
            // the control layer over the picture, so the one material is its.
            _stage = new Shared.CameraStage(_app);
            Children.Add(_stage);
            column.Padding = new Thickness(18);
            var pane = new Border
            {
                Child = column,
                Background = K.Acrylic(),
                CornerRadius = new CornerRadius(34),
                BorderBrush = Ink.Wash(0.10f),
                BorderThickness = new Thickness(1),
                Margin = new Thickness(10),
                VerticalAlignment = VerticalAlignment.Bottom,
            };
            // A window too short for the pane scrolls it, still at the foot
            // when it fits.
            var holder = new Grid { Children = { pane } };
            var scroller = new ScrollViewer { Content = holder, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollMode = ScrollMode.Disabled };
            scroller.SizeChanged += (_, e) => holder.MinHeight = e.NewSize.Height;
            Children.Add(scroller);
        }
        Update();
    }

    private void Rebuild()
    {
        // Start and Join differ in the field's prompt, the hint and the button.
        Build();
    }

    public void Update()
    {
        if (_go == null) return;
        var ready = _starting || RoomNames.ValidTyped(_room) || SignInScreen.PastedLink(_room) != null;
        _go.Busy = _working || _app.Busy;
        _go.IsEnabled = !_working && !_app.Busy && ready;
        _hint!.Visibility = _starting ? Visibility.Visible : Visibility.Collapsed;
        if (_app.Problem is { } problem) _problem!.Text = problem;
        _problem!.Visibility = _app.Problem != null ? Visibility.Visible : Visibility.Collapsed;
        _knock!.Visibility = _app.Problem != null && _app.CanKnock ? Visibility.Visible : Visibility.Collapsed;
        _knock.IsEnabled = !_app.Busy;
        _stage?.Update();
    }

    /// <summary>Closes it, calling off a join that is out (AppModel <c>CancelJoin</c>).</summary>
    public void Cancel()
    {
        _going?.Cancel();
        _going = null;
        _app.CancelJoin();
        _close();
    }

    private async void Go()
    {
        if (_working || _app.Busy || _app.Deployment is not { } deployment) return;
        if (!_starting && SignInScreen.PastedLink(_room) is { } link)
        {
            if (!_sealing) _app.SealingWord = "";
            _close();
            await _app.Open(link);
            return;
        }
        var typed = string.IsNullOrWhiteSpace(_room) ? "" : RoomNames.Normalise(_room);
        var target = _starting ? (RoomNames.Valid(typed) ? typed : RoomNames.Generate()) : typed;
        if (target.Length == 0 || !RoomNames.Valid(target)) return;
        if (!_sealing) _app.SealingWord = "";
        _working = true;
        _app.Problem = null;
        Update();
        var going = _going = new CancellationTokenSource();
        try
        {
            bool? live;
            try
            {
                live = await deployment.RoomLive(target);
            }
            catch (Exception)
            {
                live = null;
            }
            if (going.IsCancellationRequested) return;
            if (_starting && live == true) _app.Problem = T("A meeting is already happening under that name.");
            else if (!_starting && !_again && live == false) _app.Problem = T("No meeting is happening under that name.");
            else await _app.Enter(target);
        }
        finally
        {
            _working = false;
            Update();
        }
    }
}
