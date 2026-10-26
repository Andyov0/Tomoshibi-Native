using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using Tomoshibi.UI;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Screens;
using Windows.System;

namespace Tomoshibi;

/// <summary>
/// The app's one window: the screens, what lies over them, and a title bar
/// the content runs under, as Windows 11's own apps draw theirs, its caption
/// buttons in the apps' greys of the scheme on the screen: the system's,
/// followed as it changes, and dark over a call.
/// </summary>
public sealed partial class MainWindow : Window
{
    private readonly AppModel _app;
    private readonly ScreenHost _screens;
    private readonly Overlay _overlay;
    private readonly SystemScheme _system;
    private readonly TextBlock _titleName;
    private int _restyling;
    private readonly Border _veil = new() { IsHitTestVisible = false, Visibility = Visibility.Collapsed };

    /// <summary>The app's one window, for what changes the window itself: the full screen a filled stage takes.</summary>
    internal static MainWindow? Current { get; private set; }

    /// <summary>
    /// Whether the window can be seen at all: shown, and not minimised. What
    /// moves only to be looked at (the lobby's camera card) stops while it
    /// cannot be.
    /// </summary>
    internal bool InSight { get; private set; } = true;

    /// <summary>Said when <see cref="InSight"/> has changed.</summary>
    internal event Action? SightChanged;

    internal MainWindow(AppModel app)
    {
        _app = app;
        Current = this;
        InitializeComponent();
        Title = "Tomoshibi";
        // The window's own theme is left to follow the system's (App.xaml
        // asks for none), so that the framework's news of a change reaches
        // it; every screen and every dialog asks for its own scheme's.
        _system = new SystemScheme(DispatcherQueue);
        Root.ActualThemeChanged += (_, _) => _system.Check();

        _screens = new ScreenHost(app, () => _system.Now);
        Root.Background = Ink.Background;
        _overlay = new Overlay();
        var titleBar = TitleStrip(out _titleName);
        Root.Children.Add(_screens);
        Root.Children.Add(titleBar);
        Root.Children.Add(_overlay);
        Root.Children.Add(_veil);

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(titleBar);
        PaintTitleBar();
        // The first screen's scheme was chosen before anything here listened.
        Ink.ApplyAccent();
        // A screen of the other scheme came up (a call over a light lobby),
        // or the system's changed: the window's ground and its caption go
        // with it.
        Ink.SchemeChanged += () =>
        {
            Root.Background = Ink.Background;
            Ink.ApplyAccent();
            PaintTitleBar();
        };
        _system.Changed += () => _ = Restyle();

        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            // The narrowest a phone is held, and tall enough for a join
            // screen's pane: below this the screens have nowhere to put
            // their forms.
            presenter.PreferredMinimumWidth = Scaled(Placement.MinimumWidth);
            presenter.PreferredMinimumHeight = Scaled(Placement.MinimumHeight);
        }
        // A desktop window where it was left, or the Mac app's first size in
        // the middle of the screen (Placement).
        Placement.Restore(AppWindow, app.Defaults, Scaled(1000) / 1000.0);
        AppWindow.Closing += (_, _) => Placement.Keep(AppWindow, app.Defaults);

        // Escape closes what lies on top, then does what the screen's Back
        // does, as the system's Back does on the other platforms.
        Root.KeyDown += (_, e) =>
        {
            if (e.Key != VirtualKey.Escape) return;
            e.Handled = _overlay.Back() || _screens.Back();
        };

        // Minimised is a state of the presenter, which the window's own
        // visibility does not always follow, so both are listened to.
        VisibilityChanged += (_, e) =>
        {
            _shown = e.Visible;
            Look();
        };
        AppWindow.Changed += (_, _) => Look();
        Closed += (_, _) => _closed = true;

        Localization.Shared.Changed += Refresh;
        // Back at the front, the deployment is asked what it says now (Remote
        // `Refresh`, which holds back a question asked a moment before).
        Activated += (_, e) =>
        {
            if (e.WindowActivationState != WindowActivationState.Deactivated) app.Remote.Refresh();
        };
        Appearance.Shared.Changed += () =>
        {
            Ink.ApplyAccent();
            Refresh();
        };
        // Closed in a call: the call is left first, and then the window
        // goes, so that the others see the person go at once, as a closed
        // browser tab is seen to; closed without that, the app was gone
        // before its leave was sent, and the room kept a picture of
        // nobody until the server gave up on it. Two seconds at most.
        var leaving = false;
        AppWindow.Closing += async (_, e) =>
        {
            if (leaving || app.Call is not { } call) return;
            e.Cancel = true;
            leaving = true;
            try
            {
                await Task.WhenAny(call.Leave(), Task.Delay(2000));
            }
            catch (Exception fault)
            {
                Log.Line($"leaving as the window closed: {fault.Message}");
            }
            Close();
        };
    }

    private bool _shown = true;
    private bool _closed;

    private void Look()
    {
        if (_closed) return;
        bool now;
        try
        {
            var minimised = AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized };
            now = _shown && AppWindow.IsVisible && !minimised;
        }
        catch (Exception)
        {
            // Asked while the window is being taken down: the window's
            // properties throw then (Window.Visible did, closing the app).
            return;
        }
        if (now == InSight) return;
        InSight = now;
        Log.Line(now ? "window in sight" : "window out of sight");
        SightChanged?.Invoke();
    }

    private void Refresh()
    {
        _overlay.ClosePopover();
        _screens.Refresh();
    }

    /// <summary>
    /// The system went light or dark. The screen up, unless it is the call
    /// (which Daylight keeps dark), goes with it, and so does what lies over
    /// it: a veil of the new scheme's ground fades in over the window on the
    /// leaving curve, everything is drawn again under it in the new scheme as
    /// it is for a new language, and the veil fades away on the settle curve,
    /// rather than the colours jumping under the person. Fading the screens
    /// themselves instead showed the sidebar's acrylic as a dark slab for a
    /// tenth of a second as it came back. A second change while one is going
    /// on is the one that is drawn.
    /// </summary>
    private async Task Restyle()
    {
        var mine = ++_restyling;
        var wanted = _screens.Wanted;
        if (wanted == Ink.Scheme)
        {
            // Back where it was before a change under way was drawn, or in a
            // call, which stays as it is: a veil that had come up goes.
            if (_veil.Visibility == Visibility.Visible) Lift(mine);
            return;
        }
        Log.Line($"system scheme now {_system.Now}");
        _veil.Background = Ink.Brush(Theme.Of(wanted).Background);
        if (_veil.Visibility != Visibility.Visible)
        {
            Motion.Hide(_veil);
            _veil.Visibility = Visibility.Visible;
        }
        Motion.FadeTo(_veil, 1);
        await Task.Delay(Motion.Leave);
        if (mine != _restyling) return;
        Ink.Use(_screens.Wanted);
        _overlay.Repaint();
        Refresh();
        // What was made again rises into place under the veil (the lobby's
        // sidebar over 420 ms, its switches after it), and the veil goes only
        // once it has: lifted sooner, the sidebar's acrylic showed through it
        // as a dark slab while it was still rising.
        await Task.Delay(Theme.RiseMs + 120);
        if (mine != _restyling) return;
        Lift(mine);
    }

    private void Lift(int mine)
    {
        Motion.FadeTo(_veil, 0);
        _ = Task.Delay(Motion.Settle).ContinueWith(_ => DispatcherQueue.TryEnqueue(() =>
        {
            if (mine == _restyling) _veil.Visibility = Visibility.Collapsed;
        }));
    }

    /// <summary>
    /// The caption buttons and the title in the scheme on the screen: the
    /// ink and the muted grey, and the wash under the pointer and the press,
    /// white over the dark ground and black over the light one.
    /// </summary>
    private void PaintTitleBar()
    {
        var bar = AppWindow.TitleBar;
        bar.ButtonBackgroundColor = Colors.Transparent;
        bar.ButtonInactiveBackgroundColor = Colors.Transparent;
        bar.ButtonForegroundColor = Ink.Of(Ink.Palette.Foreground);
        bar.ButtonInactiveForegroundColor = Ink.Of(Ink.Palette.Muted);
        bar.ButtonHoverBackgroundColor = Ink.Of(Ink.WashShade(0.08f));
        bar.ButtonHoverForegroundColor = Ink.Of(Ink.Palette.Foreground);
        bar.ButtonPressedBackgroundColor = Ink.Of(Ink.WashShade(0.14f));
        // The system's own for the dark scheme, as it always was; the ink
        // for the light one, where the system's was drawn for a dark bar.
        bar.ButtonPressedForegroundColor = Ink.Light ? Ink.Of(Ink.Palette.Foreground) : null;
        _titleName.Foreground = Ink.Muted;
    }

    /// <summary>
    /// The strip the window is dragged by, the height of the caption buttons,
    /// with the mark and the name at its start as Windows' own apps have
    /// theirs. Only its empty part drags; the screens keep their controls
    /// below it.
    /// </summary>
    private static Grid TitleStrip(out TextBlock name)
    {
        name = K.Text("Tomoshibi", 12, ink: Ink.Muted, lines: 1);
        var row = K.Row(10, new Mark(16), name);
        row.VerticalAlignment = VerticalAlignment.Center;
        row.Margin = new Thickness(14, 0, 0, 0);
        foreach (var child in row.Children) ((FrameworkElement)child).VerticalAlignment = VerticalAlignment.Center;
        var strip = new Grid
        {
            Height = Shared.TitleBar,
            VerticalAlignment = VerticalAlignment.Top,
            Background = K.Clear,
        };
        strip.Children.Add(row);
        return strip;
    }

    [DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(IntPtr window);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr window);

    /// <summary>
    /// In front of whatever the person is using, restored if it was
    /// minimised: for a link handed over by a later launch (a tomoshibi://
    /// link clicked in a browser, the app started again from Start), which
    /// is opened here. Activate alone restored the window behind the
    /// browser and only flashed it in the taskbar, so the link seemed to do
    /// nothing and was clicked again. Windows lets it come forward because
    /// the launch that handed the link, which the person started, allowed it
    /// (Program, AllowSetForegroundWindow).
    /// </summary>
    internal void ComeForward()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter) presenter.Restore();
        Activate();
        // Refused where nothing allowed it: the window is then restored
        // behind and its taskbar button flashes, which is all Windows lets a
        // process do uninvited.
        var forward = SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
        Log.Line(forward ? "came forward" : "restored, but Windows kept another window in front");
    }

    private int Scaled(int points)
    {
        var dpi = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
        return (int)Math.Round(points * (dpi == 0 ? 1 : dpi / 96.0));
    }
}
