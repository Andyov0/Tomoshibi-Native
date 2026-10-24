using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Windows.ApplicationModel.Activation;

namespace Tomoshibi;

/// <summary>
/// Where the app starts, before XAML: it reads how it was launched and, for
/// a link (<c>tomoshibi://</c>, which the package registers, or a link given
/// on the command line), hands it to the app that is already running rather
/// than opening a second window on a second deployment session. The running
/// app brings its window forward and opens the link, as the other apps open
/// one handed to them while they run.
///
/// One app at a time for somebody using it. A test's launches are their own
/// apps, several at once in one room, unless the test gave them a store of
/// their own (TOMOSHIBI_KEEP), which is one app per store: a test of a link
/// handed to a running app needs the second launch to find the first.
/// </summary>
internal static class Program
{
    /// <summary>How the app was launched, read once.</summary>
    public static Launch Launch { get; private set; } = new();

    /// <summary>A link the app was opened with, or nothing.</summary>
    public static string? OpenedWith { get; private set; }

    /// <summary>
    /// Links handed to this app by later launches (null for a launch with
    /// none, which only brings the window forward), on a thread of the
    /// system's; kept from the moment the app registers until the app
    /// listens, which is once its window is up (Handover).
    /// </summary>
    public static readonly Handover<string?> Handed = new();

    [STAThread]
    private static int Main(string[] args)
    {
        // Opened while an update's setup is putting the files in place (the
        // person saw the window go and opened the app again, as the owner did
        // on 10-23, more than once): a window here would be an app running
        // from files that are being replaced, and holding them against the
        // setup. The setup's own window is brought to the front instead, and
        // this launch ends. Asked first, of the system alone, and the rest of
        // the start is a method of its own, so that nothing of the app's other
        // files (the Windows App SDK, the core) is loaded to ask it.
        if (SetupSignals.InstallUnderWay())
        {
            SetupSignals.BringSetupForward();
            return 0;
        }
        return Start(args);
    }

    [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
    private static int Start(string[] args)
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Launch = Launch.FromEnvironment();
        AppActivationArguments? activated = null;
        try
        {
            activated = AppInstance.GetCurrent().GetActivatedEventArgs();
        }
        catch (Exception)
        {
            // An unpackaged launch on a system without the lifecycle API's
            // registration still runs; it reads its link from the command line.
        }
        OpenedWith = LinkOf(activated) ?? LinkIn(args);
        if (InstanceKey() is { } key)
        {
            var first = AppInstance.FindOrRegisterForKey(key);
            if (!first.IsCurrent && activated != null)
            {
                // This launch was started by the person (a click on a link,
                // Start), so it may put a window in front; the app it hands
                // the link to may not, and its window stayed behind. The
                // right is passed on before the link, as Windows asks of a
                // process handing over to another.
                AllowSetForegroundWindow(first.ProcessId);
                // Redirected off this thread, which must not wait on itself
                // (the documented pattern); then this process ends.
                var done = new ManualResetEventSlim();
                Task.Run(async () =>
                {
                    try
                    {
                        await first.RedirectActivationToAsync(activated);
                    }
                    finally
                    {
                        done.Set();
                    }
                });
                done.Wait(TimeSpan.FromSeconds(10));
                // The app that was already open has come forward with its
                // window: what a setup that opened this launch waits for.
                SetupSignals.SayAppShown();
                return 0;
            }
            first.Activated += (_, e) => Handed.Hand(LinkOf(e));
        }
        Application.Start(started =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new App();
        });
        return 0;
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern bool AllowSetForegroundWindow(uint process);

    /// <summary>One app per person, one per test store, and none for a test's throwaway launches.</summary>
    private static string? InstanceKey()
    {
        if (!Launch.Rehearsal) return "tomoshibi";
        return Launch.KeepIn is { } keep ? "tomoshibi-test-" + keep : null;
    }

    private static string? LinkOf(AppActivationArguments? activated)
    {
        switch (activated?.Data)
        {
            case IProtocolActivatedEventArgs protocol:
                return protocol.Uri?.OriginalString;
            case ILaunchActivatedEventArgs launch when !string.IsNullOrWhiteSpace(launch.Arguments):
                return LinkIn(launch.Arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries));
            default:
                return null;
        }
    }

    /// <summary>The first argument that is a link the app opens: its own scheme, or a deployment's address.</summary>
    private static string? LinkIn(IEnumerable<string> args) =>
        args.Select(a => a.Trim('"')).FirstOrDefault(a =>
            a.StartsWith("tomoshibi:", StringComparison.OrdinalIgnoreCase)
            || a.StartsWith("https://", StringComparison.OrdinalIgnoreCase)
            || a.StartsWith("http://", StringComparison.OrdinalIgnoreCase));
}
