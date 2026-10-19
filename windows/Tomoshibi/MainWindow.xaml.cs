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
using Windows.Graphics;
using Windows.System;

namespace Tomoshibi;

/// <summary>
/// The app's one window: the screens, what lies over them, and a title bar
/// the content runs under, as Windows 11's own apps draw theirs. Dark like
/// the rest, its caption buttons in the apps' greys.
/// </summary>
public sealed partial class MainWindow : Window
{
    private readonly AppModel _app;
    private readonly ScreenHost _screens;
    private readonly Overlay _overlay;

    /// <summary>The app's one window, for what changes the window itself: the full screen a filled stage takes.</summary>
    internal static MainWindow? Current { get; private set; }

    internal MainWindow(AppModel app)
    {
        _app = app;
        Current = this;
        InitializeComponent();
        Title = "Tomoshibi";
        Root.Background = Ink.Background;
        Root.RequestedTheme = ElementTheme.Dark;

        _screens = new ScreenHost(app);
        _overlay = new Overlay();
        var titleBar = TitleStrip();
        Root.Children.Add(_screens);
        Root.Children.Add(titleBar);
        Root.Children.Add(_overlay);

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(titleBar);
        var bar = AppWindow.TitleBar;
        bar.ButtonBackgroundColor = Colors.Transparent;
        bar.ButtonInactiveBackgroundColor = Colors.Transparent;
        bar.ButtonForegroundColor = Ink.Of(Theme.Foreground);
        bar.ButtonInactiveForegroundColor = Ink.Of(Theme.Muted);
        bar.ButtonHoverBackgroundColor = Ink.Of(Shade.White.WithAlpha(0.08f));
        bar.ButtonHoverForegroundColor = Ink.Of(Theme.Foreground);
        bar.ButtonPressedBackgroundColor = Ink.Of(Shade.White.WithAlpha(0.14f));

        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            // The narrowest a phone is held, and tall enough for a join
            // screen's pane: below this the screens have nowhere to put
            // their forms.
            presenter.PreferredMinimumWidth = Scaled(360);
            presenter.PreferredMinimumHeight = Scaled(560);
        }
        var (width, height) = app.Launch.Window ?? (1280, 800);
        AppWindow.Resize(new SizeInt32(Scaled(width), Scaled(height)));

        // Escape closes what lies on top, then does what the screen's Back
        // does, as the system's Back does on the other platforms.
        Root.KeyDown += (_, e) =>
        {
            if (e.Key != VirtualKey.Escape) return;
            e.Handled = _overlay.Back() || _screens.Back();
        };

        Localization.Shared.Changed += Refresh;
        Appearance.Shared.Changed += () =>
        {
            Ink.ApplyAccent();
            Refresh();
        };
        Closed += async (_, _) =>
        {
            if (app.Call is { } call) await call.Leave();
        };
    }

    private void Refresh()
    {
        _overlay.ClosePopover();
        _screens.Refresh();
    }

    /// <summary>
    /// The strip the window is dragged by, the height of the caption buttons,
    /// with the mark and the name at its start as Windows' own apps have
    /// theirs. Only its empty part drags; the screens keep their controls
    /// below it.
    /// </summary>
    private static Grid TitleStrip()
    {
        var row = K.Row(10, new Mark(16), K.Text("Tomoshibi", 12, ink: Ink.Muted, lines: 1));
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

    private int Scaled(int points)
    {
        var dpi = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
        return (int)Math.Round(points * (dpi == 0 ? 1 : dpi / 96.0));
    }
}
