using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Windows.UI.ViewManagement;

namespace Tomoshibi.UI;

/// <summary>
/// Whether Windows has apps light or dark (Settings, Personalisation,
/// Colours, "Choose your app mode"), read when the app opens and followed
/// while it runs, so that a person who switches, or whose machine switches
/// at sunset, sees the app go with it without starting it again. The app's
/// own background colour Windows reports is white for light apps and black
/// for dark ones.
/// </summary>
internal sealed class SystemScheme
{
    // Kept: the event is only said to a UISettings that is still alive.
    private readonly UISettings _settings = new();
    private readonly DispatcherQueue _ui;

    public SystemScheme(DispatcherQueue ui)
    {
        _ui = ui;
        Now = Read();
        // Said on a thread of the system's, for any of the colours (the
        // accent's too), so it is asked again on the window's thread and
        // only a change of scheme is passed on.
        _settings.ColorValuesChanged += (_, _) => _ui.TryEnqueue(Check);
    }

    public Scheme Now { get; private set; }

    /// <summary>Said on the window's thread once the system's scheme has changed.</summary>
    public event Action? Changed;

    /// <summary>
    /// Reads the system's choice again and says whether it changed. Also asked
    /// when the window's own theme follows the system there, which the
    /// framework is told of by the same broadcast.
    /// </summary>
    public void Check()
    {
        var now = Read();
        if (now == Now) return;
        Now = now;
        Changed?.Invoke();
    }

    private Scheme Read()
    {
        try
        {
            var ground = _settings.GetColorValue(UIColorType.Background);
            return ground.R + ground.G + ground.B > 3 * 128 ? Scheme.Light : Scheme.Dark;
        }
        catch (Exception)
        {
            // What the app always was, rather than none.
            return Scheme.Dark;
        }
    }
}
