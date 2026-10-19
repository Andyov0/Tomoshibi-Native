using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;

namespace Tomoshibi.UI.Screens;

/// <summary>
/// One of the app's screens: built for the model as it is, and brought up
/// to date when the model says it changed. The host swaps one for another
/// with the apps' arriving and leaving motion.
/// </summary>
internal abstract class Screen : Grid
{
    protected readonly AppModel App;

    protected Screen(AppModel app)
    {
        App = app;
        Background = Ink.Background;
    }

    /// <summary>The model changed; what this screen shows of it is brought up to date.</summary>
    public virtual void Update() { }

    /// <summary>Escape with nothing open over the screen: whether this screen did something with it (Screens.swift <c>BackButton</c>).</summary>
    public virtual bool Back() => false;

    /// <summary>This screen is being replaced.</summary>
    public virtual void Gone() { }

    /// <summary>The language or the colour changed: the screen is made again unless it can say it again itself.</summary>
    public virtual bool Refresh() => false;

    /// <summary>Whether a window this wide is laid out as the other apps lay out a tablet or a computer (their 600-point line).</summary>
    public static bool Wide(double width) => width >= 600;
}

/// <summary>The frames the plainer screens are drawn in (Android's ui/screens/Screens.kt).</summary>
internal static class Frames
{
    /// <summary>
    /// The pane a form sits on over the evening room: the surface grey let
    /// through by the room behind it, a rim of light along its top. No
    /// material: it is content, and the room behind it is a gradient a blur
    /// would not change.
    /// </summary>
    public static Border Pane(UIElement child, double radius, Thickness padding)
    {
        var rim = new LinearGradientBrush { StartPoint = new Windows.Foundation.Point(0.5, 0), EndPoint = new Windows.Foundation.Point(0.5, 1) };
        rim.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.White.WithAlpha(0.20f)), Offset = 0 });
        rim.GradientStops.Add(new GradientStop { Color = Ink.Of(Shade.White.WithAlpha(0.06f)), Offset = 1 });
        return new Border
        {
            Child = child,
            CornerRadius = new CornerRadius(radius),
            Background = Ink.Brush(new Shade(0.13f, 0.14f, 0.16f, 0.62f)),
            BorderBrush = rim,
            BorderThickness = new Thickness(1),
            Padding = padding,
        };
    }

    /// <summary>
    /// A screen that is one card over the evening room, with the language in
    /// its corner (Screens.kt <c>CardScreen</c>).
    /// </summary>
    public static Grid CardScreen(StackPanel content, double width = 420, UIElement? foot = null)
    {
        content.Spacing = 18;
        content.HorizontalAlignment = HorizontalAlignment.Stretch;
        var card = Pane(content, 30, new Thickness(22));
        card.MaxWidth = width;
        card.HorizontalAlignment = HorizontalAlignment.Stretch;
        card.VerticalAlignment = VerticalAlignment.Center;
        card.Rises();
        var scroller = new ScrollViewer
        {
            Content = new Grid { Children = { card }, Padding = new Thickness(16, Shared.TitleBar + 40, 16, 60), VerticalAlignment = VerticalAlignment.Center },
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        var grid = K.Stack(new Backdrop(), scroller, Shared.LanguageMenu());
        if (foot != null) grid.Children.Add(foot);
        return grid;
    }

    /// <summary>
    /// The join screens (Screens.kt <c>PaneScreen</c>): on a wide window the
    /// card of 560 with the picture of yourself in it; on a narrow one the
    /// picture filling the window and the form on a pane at its foot.
    /// </summary>
    public static Grid PaneScreen(AppModel app, bool wide, StackPanel content, out Shared.CameraStage stage, UIElement? foot = null)
    {
        content.HorizontalAlignment = HorizontalAlignment.Stretch;
        if (wide)
        {
            content.Spacing = 18;
            var card = Pane(content, 30, new Thickness(22));
            card.MaxWidth = 560;
            card.Rises();
            stage = new Shared.CameraStage(app, 20);
            var switches = Shared.DeviceSwitches(app, 38);
            var picture = K.Stack(stage, new Border
            {
                Child = switches,
                Background = K.Acrylic(),
                CornerRadius = new CornerRadius(25),
                Padding = new Thickness(6),
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Bottom,
                Margin = new Thickness(0, 0, 0, 12),
            });
            var frame = new Grid { Children = { picture } };
            frame.SizeChanged += (_, e) => frame.Height = e.NewSize.Width * 9 / 16;
            content.Children.Insert(Math.Min(1, content.Children.Count), frame);
            var scroller = new ScrollViewer
            {
                Content = new Grid { Children = { card }, Padding = new Thickness(16, Shared.TitleBar + 28, 16, 60) },
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            };
            var grid = K.Stack(new Backdrop(), scroller, Shared.LanguageMenu());
            if (foot != null) grid.Children.Add(foot);
            return grid;
        }
        else
        {
            content.Spacing = 16;
            stage = new Shared.CameraStage(app);
            content.Children.Insert(Math.Min(1, content.Children.Count), Shared.DeviceSwitches(app, 52));
            var pane = K.Glass(content, 34, new Thickness(18));
            pane.Margin = new Thickness(10);
            pane.VerticalAlignment = VerticalAlignment.Bottom;
            pane.Rises();
            var scroller = new ScrollViewer
            {
                Content = new Grid { Children = { pane }, Padding = new Thickness(0, Shared.TitleBar, 0, 0), VerticalAlignment = VerticalAlignment.Bottom },
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            };
            var grid = K.Stack(stage, scroller, Shared.LanguageMenu());
            if (foot != null) grid.Children.Add(foot);
            return grid;
        }
    }
}
