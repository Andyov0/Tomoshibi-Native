using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Shapes;
using Windows.UI;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// Words over a picture with a soft shadow under them (Lobby.kt
/// <c>ShadowedText</c>: black at 0.45, four down, a 32-pixel blur), so the
/// greeting reads over whatever the camera shows. XAML text casts none, so
/// the shadow is a composition drop shadow cut from the text's own shape.
/// </summary>
internal static class Shadowed
{
    public static Grid Text(TextBlock text)
    {
        var host = new Rectangle { IsHitTestVisible = false };
        var grid = new Grid();
        grid.Children.Add(host);
        grid.Children.Add(text);
        text.Loaded += (_, _) =>
        {
            var compositor = ElementCompositionPreview.GetElementVisual(text).Compositor;
            var shadow = compositor.CreateDropShadow();
            shadow.Color = Color.FromArgb(115, 0, 0, 0);
            shadow.BlurRadius = 16;
            shadow.Offset = new Vector3(0, 2, 0);
            shadow.Mask = text.GetAlphaMask();
            var sprite = compositor.CreateSpriteVisual();
            sprite.Shadow = shadow;
            sprite.Size = new Vector2((float)text.ActualWidth, (float)text.ActualHeight);
            ElementCompositionPreview.SetElementChildVisual(host, sprite);
            text.SizeChanged += (_, e) => sprite.Size = new Vector2((float)e.NewSize.Width, (float)e.NewSize.Height);
        };
        return grid;
    }
}
