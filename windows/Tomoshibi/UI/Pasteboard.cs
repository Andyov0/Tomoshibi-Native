using Windows.ApplicationModel.DataTransfer;

namespace Tomoshibi.UI;

/// <summary>The system's clipboard, for the links the app hands out (Android's Stage.kt <c>Pasteboard</c>).</summary>
internal static class Pasteboard
{
    /// <summary>Puts the text on the clipboard; whether it took (another app holding the clipboard open refuses it).</summary>
    public static bool Copy(string text)
    {
        try
        {
            var package = new DataPackage();
            package.SetText(text);
            Clipboard.SetContent(package);
            // Kept after the app quits, as a copied link is expected to be.
            Clipboard.Flush();
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>The text on the clipboard, or nothing.</summary>
    public static async Task<string?> Text()
    {
        try
        {
            var content = Clipboard.GetContent();
            return content.Contains(StandardDataFormats.Text) ? await content.GetTextAsync() : null;
        }
        catch (Exception)
        {
            return null;
        }
    }
}
