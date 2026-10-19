using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using uniffi.tomoshibi_core;

namespace Tomoshibi.UI;

/// <summary>
/// One remote picture: a SwapChainPanel the core draws into (panel.rs), and
/// the name of whose it is. The core is told the panel's size whenever it
/// changes, both to cut the picture to it and to ask the server for the
/// layer that answers it; a tile asking for less was drawn from the layer
/// meant for a thumbnail (Android's VideoHub).
/// </summary>
internal sealed class VideoTile : Grid
{
    public readonly string Sid;
    public readonly string Identity;
    private readonly Call _call;
    private readonly SwapChainPanel _panel = new();
    private readonly bool _fill;
    private ulong _id;

    private sealed class Shape(VideoTile tile) : PictureShape
    {
        // On a core thread, with nothing in the core locked.
        public void Changed(uint width, uint height, uint rotation) =>
            tile.DispatcherQueue.TryEnqueue(() => Log.Line($"picture {tile.Sid} is {width}x{height} turned {rotation}"));
    }

    public VideoTile(Call call, string sid, string identity, string name, bool fill)
    {
        _call = call;
        Sid = sid;
        Identity = identity;
        _fill = fill;
        CornerRadius = (CornerRadius)Application.Current.Resources["TileRadius"];
        Background = (Brush)Application.Current.Resources["Surface"];
        BorderBrush = (Brush)Application.Current.Resources["Border"];
        BorderThickness = new Thickness(1);
        Margin = new Thickness(4);
        Children.Add(_panel);
        Children.Add(new Border
        {
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            Margin = new Thickness(8),
            Padding = new Thickness(8, 3, 8, 3),
            CornerRadius = new CornerRadius(6),
            Background = new SolidColorBrush(Microsoft.UI.ColorHelper.FromArgb(0x99, 0, 0, 0)),
            Child = new TextBlock { Text = name, Foreground = (Brush)Application.Current.Resources["Foreground"] },
        });
        this.Arrives();
        SizeChanged += (_, _) => Place();
    }

    /// <summary>Starts drawing the picture into the panel; UI thread.</summary>
    public void Attach()
    {
        var chain = _call.AttachVideoPanel(Sid, new Shape(this));
        if (chain == 0)
        {
            Log.Line($"no Direct3D device for {Sid}");
            return;
        }
        _id = chain;
        ComCalls.SetSwapChain(_panel, (IntPtr)(long)chain);
        // The core keeps its own reference; this one was handed over for the panel.
        Marshal.Release((IntPtr)(long)chain);
        Place();
    }

    private void Place()
    {
        if (_id == 0 || ActualWidth <= 0 || ActualHeight <= 0) return;
        _call.PlaceVideoPanel(Sid, _id, (float)ActualWidth, (float)ActualHeight, _fill);
        var scale = XamlRoot?.RasterizationScale ?? 1.0;
        try
        {
            _call.RequestVideoSize(Sid, (uint)(ActualWidth * scale), (uint)(ActualHeight * scale));
        }
        catch (CallException fault)
        {
            // A track gone between the last layout and this one.
            Log.Line($"no size asked for {Sid}: {fault.Message}");
        }
    }

    public ulong FramesDrawn => _id == 0 ? 0 : _call.VideoPanelFrames(Sid, _id);

    /// <summary>
    /// Stops the core drawing at once; the panel lets its swap chain go a
    /// little later, after the tile has faded out with its last picture.
    /// </summary>
    public void Detach()
    {
        if (_id == 0) return;
        _call.DetachVideoPanel(Sid, _id);
        _id = 0;
        var timer = DispatcherQueue.CreateTimer();
        timer.Interval = TimeSpan.FromMilliseconds(300);
        timer.IsRepeating = false;
        timer.Tick += (_, _) => ComCalls.SetSwapChain(_panel, IntPtr.Zero);
        timer.Start();
    }
}
