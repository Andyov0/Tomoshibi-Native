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
        var file = Path.Combine(_folder, $"Tomoshibi-{build.Version}.msixbundle");
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
/// version offered; Windows then refuses a bundle whose signature is not the
/// publisher its manifest names, or that the machine does not trust.
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

    public async Task<Installed> Install(string file, string version)
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
            if (said != version)
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

    public void Stop() => _operation?.Cancel();

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

/// <summary>The real clock, for the checks' six hours.</summary>
internal sealed class RealUpdateClock : IUpdateClock
{
    public Task Wait(TimeSpan time) => Task.Delay(time);
}
