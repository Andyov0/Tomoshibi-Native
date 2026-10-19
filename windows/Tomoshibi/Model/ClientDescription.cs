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
    public static void Tell()
    {
        var version = Assembly.GetExecutingAssembly().GetName().Version;
        var dotted = version == null ? "" : $"{version.Major}.{version.Minor}.{version.Build}.{version.Revision}";
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
