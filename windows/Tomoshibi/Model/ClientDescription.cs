using System.Reflection;
using Microsoft.Win32;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model;

/// <summary>
/// What this app tells the deployment it is, for the deployment's management
/// pages: Windows, the version with its build (0.1.4 for build 1, 0.1.4.2 for
/// build 2), the system and the maker and model. The core puts it
/// in the user agent of every request (core/protocol/src/api/agent.rs), as
/// the Apple apps' and Android's ClientDescription do. Called once, before
/// the first Deployment is made, which reads it when it is made.
/// </summary>
internal static class ClientDescription
{
    /// <summary>Whether the app runs from an installed package, which is what can be updated.</summary>
    public static bool Packaged { get; private set; }

    /// <summary>
    /// The version this app is, as the update check compares it and the
    /// deployment offers it: the core's version_with_build of the version and
    /// the build, which are the first three parts of the assembly's version
    /// (the package's, where it is packaged) and the fourth. The owner,
    /// 2026-10-22: a small fix keeps the version and takes the next build, so
    /// build 1 of 0.1.4 is 0.1.4 and build 2 is 0.1.4.2. Said to the update
    /// check as the version alone, every build after the first of a version
    /// was that version to it and offered to nobody; said with a fourth part
    /// for the first build, 0.1.4.1 would be newer than the 0.1.4 the
    /// deployment offers, and the app on 0.1.4 offered itself. People are
    /// told it as the core's version_label writes it, 0.1.4 (2).
    /// </summary>
    public static string Version { get; } = ReadVersion();

    private static string ReadVersion()
    {
        System.Version? version;
        try
        {
            var v = Windows.ApplicationModel.Package.Current.Id.Version;
            Packaged = true;
            version = new System.Version(v.Major, v.Minor, v.Build, v.Revision);
        }
        catch (Exception)
        {
            // Unpackaged: there is no package to ask.
            version = Assembly.GetExecutingAssembly().GetName().Version;
        }
        return version == null ? "" : Joined(version);
    }

    /// <summary>Four numbers, the fourth the build, as the update check says them (version_with_build): 0.1.4.1 as 0.1.4, 0.1.4.2 as itself.</summary>
    public static string Joined(System.Version version) =>
        TomoshibiProtocolMethods.VersionWithBuild($"{version.Major}.{version.Minor}.{Math.Max(0, version.Build)}", $"{Math.Max(0, version.Revision)}");

    public static void Tell()
    {
        var dotted = Version;
        // .NET reports the version Windows really is (10.0.26200 for Windows
        // 11 25H2), not the compatibility shim's 6.2 that an unmanifested
        // Win32 caller of GetVersionEx is told.
        var os = Environment.OSVersion.Version;
        TomoshibiProtocolMethods.DescribeClient("Windows", dotted, $"Windows {os.Major}.{os.Minor}.{os.Build}", Device());
    }

    /// <summary>
    /// The maker and model as the firmware tells Windows them, read from the
    /// registry where Windows keeps its copy of the SMBIOS (no WMI query,
    /// which takes a second on a cold start); empty where it is not there.
    /// </summary>
    private static string Device()
    {
        try
        {
            using var bios = Registry.LocalMachine.OpenSubKey(@"HARDWARE\DESCRIPTION\System\BIOS");
            var maker = (bios?.GetValue("SystemManufacturer") as string)?.Trim();
            var model = (bios?.GetValue("SystemProductName") as string)?.Trim();
            return string.Join(" ", new[] { maker, model }.Where(part => !string.IsNullOrEmpty(part)));
        }
        catch (Exception)
        {
            return "";
        }
    }
}
