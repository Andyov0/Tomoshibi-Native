using System.Runtime.InteropServices;

namespace Tomoshibi.Model;

/// <summary>
/// What the app and its setup program (windows/installer/Tomoshibi.iss) say
/// to each other while an update is put in place, as named events in the
/// person's session; the setup's [Code] spells the same names.
///
/// An update used to leave the screen empty twice: the app quit the moment
/// it had started the setup, and the setup's window came about half a second
/// later; the setup's window went as it opened the new app, whose window came
/// about a second and a half after that. Somebody looking at an empty screen
/// opens the app again, and that app fought the setup for its files. So each
/// window now waits for the next to be there before it goes, and an app
/// opened while the files are being replaced hands the person to the setup's
/// window and does not open.
///
/// Nothing here is needed by a setup an older app starts: such an app quits
/// as soon as it has started it, and the setup shows itself as early as it
/// can for it all the same.
/// </summary>
internal static partial class SetupSignals
{
    /// <summary>Set by the setup once its window is on the screen; the app that started it quits then (SetupInstaller).</summary>
    public const string SetupShown = @"Local\Tomoshibi.Setup.Shown";

    /// <summary>There, unset, for as long as the setup is putting the app's files in place.</summary>
    public const string Installing = @"Local\Tomoshibi.Setup.Installing";

    /// <summary>Set by an app opened meanwhile, for the setup to bring its window to the front.</summary>
    public const string Forward = @"Local\Tomoshibi.Setup.Forward";

    /// <summary>Made by the setup once the files are in place, and set by the app it opens once that app's first frame is on the screen; the setup's window goes then.</summary>
    public const string AppShown = @"Local\Tomoshibi.App.Shown";

    private const int AnyProcess = -1;

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool AllowSetForegroundWindow(int process);

    /// <summary>Whether a setup is putting the app's files in place now.</summary>
    public static bool InstallUnderWay()
    {
        try
        {
            if (!EventWaitHandle.TryOpenExisting(Installing, out var flag)) return false;
            flag.Dispose();
            return true;
        }
        catch (Exception)
        {
            // Something of that name that is not an event, or one this user
            // may not open: not our setup.
            return false;
        }
    }

    /// <summary>
    /// Asks the setup to bring its window to the front. This launch was
    /// started by the person, so it may give the setup the right to come
    /// forward, which a process that has not been clicked does not have.
    /// </summary>
    public static void BringSetupForward()
    {
        AllowSetForegroundWindow(AnyProcess);
        Set(Forward);
    }

    /// <summary>Tells a setup waiting to close its window that this app's window is on the screen; nothing where no setup waits.</summary>
    public static void SayAppShown() => Set(AppShown);

    /// <summary>Whether a setup is waiting to hear that this app's window is on the screen.</summary>
    public static bool AppShownAwaited()
    {
        try
        {
            if (!EventWaitHandle.TryOpenExisting(AppShown, out var signal)) return false;
            signal.Dispose();
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    [LibraryImport("dwmapi.dll")]
    private static partial int DwmFlush();

    /// <summary>
    /// Returns once the desktop compositor has presented the surface updates
    /// outstanding when it was called (DwmFlush), twice over, the first
    /// return being possibly for a present already under way.
    /// </summary>
    public static void WaitForThePresent()
    {
        DwmFlush();
        DwmFlush();
    }

    private static void Set(string name)
    {
        try
        {
            if (!EventWaitHandle.TryOpenExisting(name, out var signal)) return;
            using (signal) signal.Set();
        }
        catch (Exception)
        {
        }
    }
}
