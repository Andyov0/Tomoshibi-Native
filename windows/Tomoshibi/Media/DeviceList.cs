using Windows.Devices.Enumeration;

namespace Tomoshibi.Media;

/// <summary>A device a person can pick: its system identifier and the name Windows gives it.</summary>
internal sealed record DeviceOption(string Id, string Name);

/// <summary>
/// The microphones, cameras and speakers Windows lists (settings/Platform.kt
/// <c>cameras</c>, <c>microphones</c>, <c>speakers</c>). Listing them opens
/// none of them: no light comes on and nothing is captured, so a test may
/// list the devices of the machine it runs on.
/// </summary>
internal static class DeviceList
{
    public static async Task<IReadOnlyList<DeviceOption>> Microphones() => await Find(DeviceClass.AudioCapture);
    public static async Task<IReadOnlyList<DeviceOption>> Speakers() => await Find(DeviceClass.AudioRender);
    public static async Task<IReadOnlyList<DeviceOption>> Cameras() => await Find(DeviceClass.VideoCapture);

    private static async Task<IReadOnlyList<DeviceOption>> Find(DeviceClass kind)
    {
        try
        {
            var found = await DeviceInformation.FindAllAsync(kind);
            // Two of the same name are told apart by a number, as on Android.
            var seen = new Dictionary<string, int>();
            var options = new List<DeviceOption>();
            foreach (var device in found.Where(d => d.IsEnabled))
            {
                var name = string.IsNullOrWhiteSpace(device.Name) ? Basics.Words.T("Device {number}", ("number", options.Count + 1)) : device.Name.Trim();
                seen[name] = seen.TryGetValue(name, out var count) ? count + 1 : 1;
                options.Add(new DeviceOption(device.Id, seen[name] > 1 ? $"{name} {seen[name]}" : name));
            }
            return options;
        }
        catch (Exception)
        {
            return Array.Empty<DeviceOption>();
        }
    }
}
