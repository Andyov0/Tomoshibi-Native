using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Windows.Devices.Enumeration;

namespace Tomoshibi.Media;

/// <summary>A device a person can pick: its system identifier and the name Windows gives it.</summary>
internal sealed record DeviceOption(string Id, string Name);

internal enum DeviceKind { Camera, Microphone, Speakers }

/// <summary>
/// Where the lists of cameras, microphones and speakers come from, and word
/// that they changed: Windows' own, or a test's made-up devices, so that
/// picking, switching and unplugging can be driven in a test with nothing
/// real ever opened (a virtual machine's devices are its host's).
/// </summary>
internal interface IDeviceDirectory
{
    Task<IReadOnlyList<DeviceOption>> List(DeviceKind kind);

    /// <summary>A device came or went, or changed; once for a burst of changes, on no particular thread.</summary>
    event Action? Changed;

    /// <summary>Its devices are made up, and what they capture is generated (ScriptedDevices).</summary>
    bool Scripted { get; }
}

/// <summary>
/// The microphones, cameras and speakers Windows lists (settings/Platform.kt
/// <c>cameras</c>, <c>microphones</c>, <c>speakers</c>), through whichever
/// directory the launch uses. Listing them opens none of them: no light
/// comes on and nothing is captured, so a test may list the devices of the
/// machine it runs on.
/// </summary>
internal static class DeviceList
{
    /// <summary>Set once at start (CallMedia); Windows' own until then.</summary>
    public static IDeviceDirectory Directory { get; set; } = SystemDevices.Shared;

    public static Task<IReadOnlyList<DeviceOption>> Microphones() => Directory.List(DeviceKind.Microphone);
    public static Task<IReadOnlyList<DeviceOption>> Speakers() => Directory.List(DeviceKind.Speakers);
    public static Task<IReadOnlyList<DeviceOption>> Cameras() => Directory.List(DeviceKind.Camera);
    public static Task<IReadOnlyList<DeviceOption>> Of(DeviceKind kind) => Directory.List(kind);
}

/// <summary>
/// Windows' own devices, watched: a headset plugged in while the devices
/// panel is open is in it at once, as the web's menu listens for
/// <c>devicechange</c> (DeviceMenu.tsx) and Android's for its device
/// callbacks; and a device unplugged under a call is noticed whether or not
/// its capture says so.
/// </summary>
internal sealed class SystemDevices : IDeviceDirectory
{
    public static readonly SystemDevices Shared = new();

    private readonly object _gate = new();
    private readonly List<DeviceWatcher> _watchers = new();
    private Timer? _soon;
    private Action? _changed;

    public bool Scripted => false;

    /// <summary>
    /// The watchers run while anybody listens. Each says every device it
    /// finds as it starts, and nothing of that is a change; after that, one
    /// word for a burst (a USB headset arrives as a microphone and speakers
    /// within milliseconds), 150 ms after the last of it, as Android does.
    /// </summary>
    public event Action? Changed
    {
        add
        {
            lock (_gate)
            {
                _changed += value;
                if (_watchers.Count == 0) Watch();
            }
        }
        remove
        {
            lock (_gate)
            {
                _changed -= value;
                if (_changed == null) Unwatch();
            }
        }
    }

    private void Watch()
    {
        foreach (var kind in new[] { DeviceClass.VideoCapture, DeviceClass.AudioCapture, DeviceClass.AudioRender })
        {
            try
            {
                var watcher = DeviceInformation.CreateWatcher(kind);
                var settled = false;
                watcher.EnumerationCompleted += (_, _) => settled = true;
                watcher.Added += (_, _) => { if (settled) Soon(); };
                watcher.Removed += (_, _) => { if (settled) Soon(); };
                // A device turned off in Windows' sound settings is updated,
                // not removed, and leaves the lists all the same.
                watcher.Updated += (_, _) => { if (settled) Soon(); };
                watcher.Start();
                _watchers.Add(watcher);
            }
            catch (Exception fault)
            {
                Log.Line($"devices of kind {kind} not watched: {fault.Message}");
            }
        }
    }

    private void Unwatch()
    {
        foreach (var watcher in _watchers)
        {
            try
            {
                if (watcher.Status is DeviceWatcherStatus.Started or DeviceWatcherStatus.EnumerationCompleted) watcher.Stop();
            }
            catch (Exception)
            {
                // Already stopping.
            }
        }
        _watchers.Clear();
        _soon?.Dispose();
        _soon = null;
    }

    private void Soon()
    {
        lock (_gate)
        {
            _soon ??= new Timer(_ => Tell(), null, Timeout.Infinite, Timeout.Infinite);
            _soon.Change(150, Timeout.Infinite);
        }
    }

    private void Tell()
    {
        Action? changed;
        lock (_gate) changed = _changed;
        Log.Line("devices changed");
        changed?.Invoke();
    }

    public async Task<IReadOnlyList<DeviceOption>> List(DeviceKind kind)
    {
        try
        {
            var found = await DeviceInformation.FindAllAsync(kind switch
            {
                DeviceKind.Camera => DeviceClass.VideoCapture,
                DeviceKind.Microphone => DeviceClass.AudioCapture,
                _ => DeviceClass.AudioRender,
            });
            // Two of the same name are told apart by a number, as on Android.
            return DeviceRules.Numbered(found.Where(d => d.IsEnabled).Select(d => new ListedDevice(d.Id, d.Name ?? "")))
                .Select(d => new DeviceOption(d.Id, d.Name))
                .ToList();
        }
        catch (Exception)
        {
            return Array.Empty<DeviceOption>();
        }
    }
}

/// <summary>
/// Made-up devices for a test (TOMOSHIBI_DEVICES=file): each line of the
/// file is a device, <c>kind&lt;tab&gt;name[&lt;tab&gt;number]</c>, kinds
/// <c>camera</c>, <c>microphone</c> and <c>speakers</c>; the number is a
/// camera's hue in degrees or a microphone's tone in hertz, so that a
/// browser in the room can tell which one is being sent. The file is read
/// again whenever it changes, which is how a test plugs a device in or
/// pulls it out under a running call. Nothing here touches a real device:
/// a camera's picture and a microphone's tone are generated (Synthetic).
/// </summary>
internal sealed class ScriptedDevices : IDeviceDirectory
{
    private readonly string _file;
    private readonly Timer _look;
    private DateTime _seen;
    private Dictionary<DeviceKind, List<(DeviceOption Option, double? Number)>> _devices = new();
    private readonly object _gate = new();

    public bool Scripted => true;
    public event Action? Changed;

    public ScriptedDevices(string file)
    {
        _file = file;
        Read();
        // Looked at three times a second: a file written from outside the
        // machine's own session raises no reliable change notice.
        _look = new Timer(_ => Look(), null, 300, 300);
    }

    private void Look()
    {
        try
        {
            if (File.GetLastWriteTimeUtc(_file) == _seen) return;
        }
        catch (Exception)
        {
            return;
        }
        Read();
        Log.Line("devices changed (scripted)");
        Changed?.Invoke();
    }

    private void Read()
    {
        var devices = new Dictionary<DeviceKind, List<(DeviceOption, double?)>>
        {
            [DeviceKind.Camera] = new(),
            [DeviceKind.Microphone] = new(),
            [DeviceKind.Speakers] = new(),
        };
        try
        {
            _seen = File.GetLastWriteTimeUtc(_file);
            foreach (var line in File.ReadAllLines(_file))
            {
                var parts = line.Split('\t');
                if (parts.Length < 2) continue;
                DeviceKind? kind = parts[0].Trim() switch
                {
                    "camera" => DeviceKind.Camera,
                    "microphone" => DeviceKind.Microphone,
                    "speakers" => DeviceKind.Speakers,
                    _ => null,
                };
                if (kind is not { } k) continue;
                var name = parts[1].Trim();
                double? number = parts.Length > 2 && double.TryParse(parts[2], System.Globalization.CultureInfo.InvariantCulture, out var n) ? n : null;
                devices[k].Add((new DeviceOption($"scripted:{parts[0].Trim()}:{name}", name), number));
            }
        }
        catch (Exception fault)
        {
            Log.Line($"scripted devices not read: {fault.Message}");
        }
        lock (_gate) _devices = devices;
    }

    public Task<IReadOnlyList<DeviceOption>> List(DeviceKind kind)
    {
        lock (_gate) return Task.FromResult<IReadOnlyList<DeviceOption>>(_devices[kind].Select(d => d.Option).ToList());
    }

    /// <summary>The number the file gives a device: a camera's hue, a microphone's tone; null for none, or for a device not listed.</summary>
    public double? Number(DeviceKind kind, string? id)
    {
        lock (_gate) return _devices[kind].FirstOrDefault(d => d.Option.Id == id).Number;
    }
}
