using System.Reflection;
using Microsoft.Win32;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model;

/// <summary>
/// What this app tells the deployment it is, for the deployment's management
/// pages: Windows, the version as the download page writes it (0.1.0 with
/// build 1 is 0.1.0.1), the system and the maker and model. The core puts it
/// in the user agent of every request (core/protocol/src/api/agent.rs), as
/// the Apple apps' and Android's ClientDescription do. Called once, before
/// the first Deployment is made, which reads it when it is made.
/// </summary>
internal static class ClientDescription
{
    /// <summary>Whether the app runs from an installed package, which is what can be updated.</summary>
    public static bool Packaged { get; private set; }

    /// <summary>
    /// The version this app is: the package's four parts where it is packaged,
    /// and otherwise the assembly's first three, 0.1.2, as people are told it
    /// and apps.json names it (owner, 2026-10-22: no build number after it).
    /// </summary>
    public static string Version { get; } = ReadVersion();

    private static string ReadVersion()
    {
        try
        {
            var v = Windows.ApplicationModel.Package.Current.Id.Version;
            Packaged = true;
            return $"{v.Major}.{v.Minor}.{v.Build}.{v.Revision}";
        }
        catch (Exception)
        {
            // Unpackaged: there is no package to ask.
            var version = Assembly.GetExecutingAssembly().GetName().Version;
            return version == null ? "" : $"{version.Major}.{version.Minor}.{version.Build}";
        }
    }

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
