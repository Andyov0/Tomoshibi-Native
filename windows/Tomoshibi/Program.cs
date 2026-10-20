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
