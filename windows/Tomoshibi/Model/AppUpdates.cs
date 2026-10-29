using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Xml.Linq;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_protocol;
using Windows.Management.Deployment;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model;

/// <summary>
/// Builds from the deployment, through the core (api/updates.rs), which asks
/// as the app, compares the versions, and lets nothing through that is not
/// the build it described (Android's CoreUpdates.kt). A deployment of its
/// own rather than the app's: an update is for whoever has the app, signed
/// in or not, and needs no cookie.
/// </summary>
internal sealed class CoreUpdateSource : IUpdateSource
{
    private readonly Lazy<Deployment> _deployment;
    private readonly string _folder;
    private readonly Microsoft.UI.Dispatching.DispatcherQueue _ui;

    /// <param name="ui">Where the model lives, which a download's progress is handed to.</param>
    public CoreUpdateSource(string baseUrl, string folder, Microsoft.UI.Dispatching.DispatcherQueue ui)
    {
        _deployment = new Lazy<Deployment>(() => new Deployment(baseUrl));
        _folder = folder;
        _ui = ui;
    }

    public async Task<Offer?> Latest(string running) =>
        await _deployment.Value.CheckForUpdate(UpdatePlatform.Windows, running) switch
        {
            UpdateCheck.Available available => new Offer(available.Build.Version, (long)available.Build.Size,
                language => TomoshibiProtocolMethods.UpdateNotes(available.Build, language), available.Build),
            _ => null,
        };

    public IUpdateTransfer Download(Offer offer)
    {
        var build = (AppBuild)offer.Handle!;
        Directory.CreateDirectory(_folder);
        // In the app's own local folder, which needs no permission; a part
        // left by a download stopped is carried on from by the next one
        // (the core keeps <file>.<hash>.part).
        // What this install was made from: a bundle for a packaged app, the
        // setup program for the app as it is given out (SetupInstaller).
        var file = Path.Combine(_folder, ClientDescription.Packaged ? $"Tomoshibi-{build.Version}.msixbundle" : $"Tomoshibi-Setup-{build.Version}.exe");
        return new CoreTransfer(_deployment.Value.DownloadUpdate(build, file), file, _ui);
    }

    public void Clear()
    {
        if (!Directory.Exists(_folder)) return;
        foreach (var file in Directory.EnumerateFiles(_folder))
        {
            try
            {
                File.Delete(file);
            }
            catch (Exception)
            {
                // Still open by an install that was handed it; the next start clears it.
            }
        }
    }

    private sealed class CoreTransfer(UpdateDownload download, string file, Microsoft.UI.Dispatching.DispatcherQueue ui) : IUpdateTransfer
    {
        public string File { get; } = file;

        public async Task Run(Action<long, long> progress)
        {
            try
            {
                await download.Run(new Progress((received, total) => ui.TryEnqueue(() => progress(received, total))));
            }
            catch (UpdateException fault)
            {
                throw fault switch
                {
                    UpdateException.Cancelled => new UpdateFailure.Cancelled(),
                    UpdateException.Network network => new UpdateFailure.Broken(network.detail),
                    UpdateException.Mismatch mismatch => new UpdateFailure.Damaged(mismatch.detail),
                    UpdateException.Superseded => new UpdateFailure.Superseded(),
                    UpdateException.Storage storage => new UpdateFailure.Storage(storage.detail),
                    UpdateException.Refused refused => new UpdateFailure.Refused(refused.code),
                    _ => new UpdateFailure.Other(fault.Message),
                };
            }
        }

        public void Cancel() => download.Cancel();
    }

    /// <summary>The core's progress, from a thread of its own; handed on to the UI thread above.</summary>
    private sealed class Progress(Action<long, long> said) : UpdateProgress
    {
        public void Progressed(ulong received, ulong total) => said((long)received, (long)total);
    }
}

/// <summary>
/// Installs a build with the system's package manager (Android's
/// SelfInstall): over the running app, which Windows ends for it
/// (ForceTargetAppShutdown), and opens again afterwards because the app
/// registered to be restarted. Nothing is handed over that is not this app:
/// the bundle's own manifest must name this app and this publisher, and the
/// version offered (its four numbers said as the update check says them,
/// ClientDescription.Joined); Windows then refuses a bundle whose signature
/// is not the publisher its manifest names, or that the machine does not
/// trust.
/// </summary>
internal sealed partial class PackageInstaller : IUpdateInstaller
{
    private Windows.Foundation.IAsyncOperationWithProgress<DeploymentResult, DeploymentProgress>? _operation;

    [LibraryImport("kernel32.dll", StringMarshalling = StringMarshalling.Utf16)]
    private static partial int RegisterApplicationRestart(string? commandLine, int flags);

    [LibraryImport("kernel32.dll")]
    private static partial int UnregisterApplicationRestart();

    /// <summary>Restarted after an update only: not after a crash or a hang (RESTART_NO_CRASH, RESTART_NO_HANG), which would open a call's app again by itself.</summary>
    private const int AfterUpdatesOnly = 1 | 2;

    /// <param name="started">Not called: the package manager's operation can be cancelled until it is done.</param>
    public async Task<Installed> Install(string file, string version, Action started)
    {
        Windows.ApplicationModel.Package running;
        try
        {
            running = Windows.ApplicationModel.Package.Current;
        }
        catch (Exception)
        {
            return new Installed.Failed(T("Tomoshibi cannot update itself here"));
        }
        // What the bundle says it is, read before anything is installed.
        try
        {
            var (name, publisher, said, signers) = Identity(file);
            if (name != running.Id.Name || publisher != running.Id.Publisher)
            {
                Log.Line($"update: the bundle is {name} by {publisher}, not this app");
                return new Installed.Failed(T("The update is not signed by Tomoshibi's developer, so it was not installed."));
            }
            // Signed by a certificate in this app's publisher's name. Windows
            // then checks that the signature is good, that it is that
            // certificate's, and that the machine trusts it.
            if (!signers.Any(signer => SameName(signer, running.Id.Publisher)))
            {
                Log.Line($"update: the bundle is signed by {string.Join("; ", signers)}, not {running.Id.Publisher}");
                return new Installed.Failed(T("The update is not signed by Tomoshibi's developer, so it was not installed."));
            }
            if (!System.Version.TryParse(said, out var numbers) || ClientDescription.Joined(numbers) != version)
            {
                Log.Line($"update: the bundle is version {said}, not {version}");
                return new Installed.Failed(T("The update is not the version that was offered, so it was not installed."));
            }
        }
        catch (Exception fault)
        {
            Log.Line($"update: the bundle could not be read: {fault.Message}");
            return new Installed.Failed(T("The update could not be opened."));
        }
        // Opened again by Windows once the update is in place; no longer
        // asked for when the update did not go in.
        RegisterApplicationRestart(null, AfterUpdatesOnly);
        var done = false;
        try
        {
            var options = new AddPackageOptions { ForceTargetAppShutdown = true };
            Log.Line($"update: installing {version}");
            _operation = new PackageManager().AddPackageByUriAsync(new Uri(file), options);
            var result = await _operation;
            if (result.IsRegistered)
            {
                done = true;
                return new Installed.Done();
            }
            Log.Line($"update: not installed, {result.ExtendedErrorCode?.HResult:X8} {result.ErrorText}");
            return new Installed.Failed(Explain(result.ExtendedErrorCode?.HResult ?? 0));
        }
        catch (TaskCanceledException)
        {
            return new Installed.Stopped();
        }
        catch (OperationCanceledException)
        {
            return new Installed.Stopped();
        }
        catch (Exception fault)
        {
            Log.Line($"update: not installed, {fault.HResult:X8} {fault.Message}");
            return new Installed.Failed(Explain(fault.HResult));
        }
        finally
        {
            _operation = null;
            if (!done) UnregisterApplicationRestart();
        }
    }

    public bool Stop()
    {
        _operation?.Cancel();
        return true;
    }

    /// <summary>
    /// The name, publisher and version a bundle's manifest gives it
    /// (AppxMetadata/AppxBundleManifest.xml), and the subjects of the
    /// certificates its signature carries (AppxSignature.p7x: "PKCX" and a
    /// PKCS #7 blob).
    /// </summary>
    private static (string Name, string Publisher, string Version, string[] Signers) Identity(string file)
    {
        using var zip = ZipFile.OpenRead(file);
        var entry = zip.GetEntry("AppxMetadata/AppxBundleManifest.xml") ?? throw new InvalidDataException("no bundle manifest");
        XDocument manifest;
        using (var stream = entry.Open()) manifest = XDocument.Load(stream);
        var identity = manifest.Root?.Elements().FirstOrDefault(e => e.Name.LocalName == "Identity") ?? throw new InvalidDataException("no identity");
        var signers = Array.Empty<string>();
        if (zip.GetEntry("AppxSignature.p7x") is { } signature)
        {
            using var stream = signature.Open();
            using var bytes = new MemoryStream();
            stream.CopyTo(bytes);
            var blob = bytes.ToArray();
            if (blob.Length > 4 && blob[0] == (byte)'P' && blob[1] == (byte)'K' && blob[2] == (byte)'C' && blob[3] == (byte)'X')
            {
                var certificates = new System.Security.Cryptography.X509Certificates.X509Certificate2Collection();
#pragma warning disable SYSLIB0057 // A PKCS #7 blob, which the certificate loader does not read.
                certificates.Import(blob.AsSpan(4));
#pragma warning restore SYSLIB0057
                signers = certificates.Select(c => c.Subject).ToArray();
            }
        }
        return ((string?)identity.Attribute("Name") ?? "", (string?)identity.Attribute("Publisher") ?? "", (string?)identity.Attribute("Version") ?? "", signers);
    }

    /// <summary>Two distinguished names for the same subject, however each spaces and orders its parts.</summary>
    private static bool SameName(string a, string b)
    {
        static string Plain(string name) => string.Join(",", name.Split(',').Select(part => part.Trim()).OrderBy(part => part, StringComparer.OrdinalIgnoreCase)).ToUpperInvariant();
        return Plain(a) == Plain(b);
    }

    /// <summary>What a deployment error means to the person (the HRESULTs of appxdeployment).</summary>
    private static string Explain(int code) => unchecked((uint)code) switch
    {
        // ERROR_INSTALL_POLICY_FAILURE, ERROR_DEPLOYMENT_BLOCKED_BY_POLICY
        0x80073CFF or 0x80073D01 => T("This device blocked the update."),
        // ERROR_INSTALL_WRONG_PROCESSOR_ARCHITECTURE
        0x80073D10 => T("This update does not run on this device."),
        // ERROR_DISK_FULL, ERROR_INSTALL_OUT_OF_DISK_SPACE
        0x80070070 or 0x80073CF8 => T("There is not enough room on this device for the update."),
        // TRUST_E_NOSIGNATURE, CERT_E_UNTRUSTEDROOT, ERROR_INSTALL_PACKAGE_INVALID with a bad signature
        0x800B0100 or 0x800B0109 or 0x800B0004 => T("The update is not signed by Tomoshibi's developer, so it was not installed."),
        _ => T("The update could not be installed."),
    };
}

/// <summary>
/// Installs a build by running its setup program, for the app as it is given
/// out: not a package but an ordinary program, installed for the person in
/// their own folder by an unsigned setup (windows/installer). The owner chose
/// that over a code-signing certificate (2026-10-20), the first install being
/// let through by hand at the SmartScreen prompt; an MSIX cannot be, since
/// Windows refuses one whose signature it does not trust.
///
/// What is run is what the core checked. The trust comes from the release's
/// offline key: the core verifies its Ed25519 signature over the platform,
/// the version, the file, its size and its SHA-256 before it offers a build
/// at all, and again before the finished download is renamed into place, so
/// a deployment or a shelf that was tampered with cannot hand out a build of
/// its own. The setup itself is still unsigned, by the owner's choice above;
/// nothing here reads an Authenticode signature. The setup's own version
/// stamp is read only to refuse a file that is not the setup offered: its
/// product version is the version as the deployment offers it, 0.1.4 for a
/// first build and 0.1.4.2 for a later one (setup.ps1), which is what every
/// app out there compares it with, this one and those from before builds
/// were said alike.
/// Downloaded by the app rather than a browser, it carries no mark of the
/// web, so SmartScreen does not ask again. The setup runs silently, shows a
/// small window of its own, closes this app if it is still open, and opens
/// the new one when it is done; this app quits once the setup's window is on
/// the screen (SetupSignals).
///
/// Reading the stamp and starting the setup are done off the window's
/// thread. Starting it holds the thread it runs on for as long as the system
/// takes to look a new program over before it runs: eight to eleven seconds
/// for a 145 MB setup the VM had not seen, Defender reading all of it, and
/// longer on the owner's computer; the stamp is read in milliseconds. Done on
/// the window's thread, the window stood frozen on the finished download and
/// then went, with nothing on the screen until the setup's window came
/// (10-23, updating from 0.1.3 to 0.1.4). It had been kept there because a
/// call joined in those seconds would be ended by the install; the lobby now
/// refuses to begin one while a build is being installed (Updates.HoldsCalls).
/// </summary>
internal sealed partial class SetupInstaller : IUpdateInstaller
{
    /// <summary>
    /// The longest this app waits for the setup's window before it quits all
    /// the same: the window came 0.8 to 2 seconds after the setup was started
    /// in the VM, and a setup that never says so (one from before it did)
    /// closes the app itself when it replaces its files.
    /// </summary>
    public static readonly TimeSpan ShownAtMost = TimeSpan.FromSeconds(20);

    private readonly Action _quit;
    private readonly object _gate = new();
    private Attempt? _attempt;

    /// <summary>
    /// One press's install. Its own, so that one given up while its stamp is
    /// still being read stays given up: "Update" pressed again in those
    /// seconds starts another, and the first, once read, starts nothing.
    /// </summary>
    private sealed class Attempt
    {
        public bool Stopped;
        public bool Committed;
        public readonly TaskCompletionSource Stop = new(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    /// <summary>What a process's handle is to a wait: signalled when it has ended.</summary>
    private sealed class Exited : WaitHandle
    {
        public Exited(System.Diagnostics.Process process) => SafeWaitHandle = new Microsoft.Win32.SafeHandles.SafeWaitHandle(process.Handle, ownsHandle: false);
    }

    [LibraryImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool AllowSetForegroundWindow(int process);

    /// <param name="quit">Ends the app, on its own thread, once the setup's window is up.</param>
    public SetupInstaller(Action quit)
    {
        _quit = quit;
    }

    /// <param name="started">
    /// Called at once. The stamp is read in milliseconds (4 to 12 in the VM)
    /// and the setup then started, which nothing calls back; a Cancel on the
    /// install's card was there for a fifth of a second and taken away again,
    /// a button that flickered and did nothing. Being given up is still
    /// possible until the setup is started (Stop), for the lobby left.
    /// </param>
    public async Task<Installed> Install(string file, string version, Action started)
    {
        started();
        var attempt = new Attempt();
        lock (_gate) _attempt = attempt;
        var work = Task.Run(() => Run(attempt, file, version));
        // Given up while the stamp is still being read: said at once, rather
        // than when the system has finished reading; nothing will be started
        // (Run looks under the gate before it starts anything).
        if (await Task.WhenAny(work, attempt.Stop.Task) != work) return new Installed.Stopped();
        var installed = await work;
        if (installed is Installed.Done) _quit();
        return installed;
    }

    /// <summary>Refused once the setup is being started: from then on it runs whatever the app does.</summary>
    public bool Stop()
    {
        Attempt? attempt;
        lock (_gate)
        {
            attempt = _attempt;
            if (attempt == null) return true;
            if (attempt.Committed) return false;
            attempt.Stopped = true;
        }
        attempt.Stop.TrySetResult();
        return true;
    }

    /// <summary>Off the window's thread: reads the setup, starts it, and waits for its window.</summary>
    private Installed Run(Attempt attempt, string file, string version)
    {
        var clock = System.Diagnostics.Stopwatch.StartNew();
        try
        {
            // Inno Setup pads the strings of its version stamp with spaces,
            // and a stamp read untrimmed refused every real setup as not the
            // one offered.
            var stamp = System.Diagnostics.FileVersionInfo.GetVersionInfo(file);
            var name = stamp.ProductName?.Trim(' ', '\0');
            var said = stamp.ProductVersion?.Trim(' ', '\0');
            if (name != "Tomoshibi" || said != version)
            {
                Log.Line($"update: the setup is {name} {said}, not Tomoshibi {version}");
                return new Installed.Failed(T("The update is not the version that was offered, so it was not installed."));
            }
        }
        catch (Exception fault)
        {
            Log.Line($"update: the setup could not be read: {fault.Message}");
            return new Installed.Failed(T("The update could not be opened."));
        }
        Log.Line($"update: read the setup's stamp in {clock.ElapsedMilliseconds} ms");
        // Made before the setup starts, so that its window cannot be said to
        // be up before this app is listening; cleared, in case a setup that
        // went wrong earlier left it said.
        using var shown = new EventWaitHandle(false, EventResetMode.ManualReset, SetupSignals.SetupShown);
        shown.Reset();
        lock (_gate)
        {
            // Given up between the press and here: nothing has been started.
            if (attempt.Stopped) return new Installed.Stopped();
            attempt.Committed = true;
        }
        System.Diagnostics.Process setup;
        clock.Restart();
        try
        {
            var start = new System.Diagnostics.ProcessStartInfo(file)
            {
                Arguments = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /NORESTARTAPPLICATIONS /SP-",
                UseShellExecute = false,
            };
            Log.Line($"update: running the setup for {version}");
            setup = System.Diagnostics.Process.Start(start) ?? throw new InvalidOperationException("no process");
        }
        catch (Exception fault)
        {
            Log.Line($"update: the setup did not start: {fault.HResult:X8} {fault.Message}");
            return new Installed.Failed(T("The update could not be installed."));
        }
        using (setup)
        {
            Log.Line($"update: the setup started in {clock.ElapsedMilliseconds} ms");
            // This app was pressed; the setup it started may then put its
            // window in front of whatever else is open.
            AllowSetForegroundWindow(setup.Id);
            clock.Restart();
            using var ended = new Exited(setup);
            switch (WaitHandle.WaitAny([shown, ended], ShownAtMost))
            {
                case 0:
                    Log.Line($"update: the setup's window is up after {clock.ElapsedMilliseconds} ms; quitting");
                    return new Installed.Done();
                case 1:
                    // Ended before it showed anything (a file that is not a
                    // whole setup, one that would not run here): this app
                    // stays, and says the update did not go in.
                    Log.Line($"update: the setup ended with {setup.ExitCode} before its window showed");
                    return new Installed.Failed(T("The update could not be installed."));
                default:
                    Log.Line($"update: no word of the setup's window in {ShownAtMost.TotalSeconds} s; quitting all the same");
                    return new Installed.Done();
            }
        }
    }
}

/// <summary>The real clock, for the checks' six hours.</summary>
internal sealed class RealUpdateClock : IUpdateClock
{
    public Task Wait(TimeSpan time) => Task.Delay(time);
}
