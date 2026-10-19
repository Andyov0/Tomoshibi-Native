using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;

namespace Tomoshibi.Model;

/// <summary>
/// Where the app keeps its files: %LOCALAPPDATA%\Tomoshibi. An unpackaged
/// app has no ApplicationData of its own (that needs a package identity), so
/// this is the folder a packaged build's LocalFolder would be the
/// counterpart of. Local rather than roaming: what is kept here (a device's
/// identity in each deployment, its choices of camera) belongs to this
/// machine and would be wrong on another.
/// </summary>
internal static class Folders
{
    public static string Local
    {
        get
        {
            var path = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Tomoshibi");
            Directory.CreateDirectory(path);
            return path;
        }
    }
}

/// <summary>
/// The app's choices in one JSON file, written whenever one changes
/// (Android's SharedPreferences, the Apple apps' UserDefaults), under the
/// web's own names where the web keeps the same thing. Every value is a
/// string or a boolean, as a browser's storage holds them.
/// </summary>
internal sealed class FileDefaults : IDefaults
{
    private readonly string _path;
    private readonly JsonObject _values;
    private readonly object _gate = new();

    public FileDefaults(string path)
    {
        _path = path;
        JsonObject? read = null;
        try
        {
            if (File.Exists(path)) read = JsonNode.Parse(File.ReadAllText(path)) as JsonObject;
        }
        catch (Exception fault)
        {
            // A file left half written by a crash is a fresh start rather
            // than an app that will not open.
            Log.Line($"preferences unreadable, starting afresh: {fault.GetType().Name}");
        }
        _values = read ?? new JsonObject();
    }

    public static FileDefaults App() => new(Path.Combine(Folders.Local, "preferences.json"));

    public string? String(string key)
    {
        lock (_gate)
        {
            return _values[key] is JsonValue value && value.TryGetValue<string>(out var text) ? text : null;
        }
    }

    public void Set(string key, string? value)
    {
        lock (_gate)
        {
            if (value == null) _values.Remove(key);
            else _values[key] = value;
            Save();
        }
    }

    public bool? Bool(string key)
    {
        lock (_gate)
        {
            return _values[key] is JsonValue value && value.TryGetValue<bool>(out var flag) ? flag : null;
        }
    }

    public void Set(string key, bool value)
    {
        lock (_gate)
        {
            _values[key] = value;
            Save();
        }
    }

    public void Remove(string key)
    {
        lock (_gate)
        {
            if (_values.Remove(key)) Save();
        }
    }

    private void Save()
    {
        // Written beside and moved over, so that the file is always either
        // the old choices or the new ones.
        var temporary = _path + ".new";
        File.WriteAllText(temporary, _values.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        File.Move(temporary, _path, overwrite: true);
    }
}

/// <summary>
/// Secrets kept between launches: the account's session cookie and the
/// passphrase typed at the join form, under the names the other apps use
/// (<c>account:&lt;base&gt;</c>, <c>passphrase:&lt;base&gt;</c>). The sealing
/// word for an encrypted call is never kept anywhere, here as on the web.
/// </summary>
internal interface ISecrets
{
    string? Read(string name);
    /// <summary>Nothing, or an empty value, forgets it.</summary>
    void Write(string name, string? value);
}

/// <summary>
/// Each secret sealed with DPAPI for the signed-in Windows user
/// (CryptProtectData), one file each in the app's local folder: the
/// counterpart of the Keychain on Apple and the Keystore on Android.
///
/// Not the Credential Locker (PasswordVault), which looks like the obvious
/// counterpart and is not: what it holds roams to every PC the person signs
/// into with the same Microsoft account, and a deployment's session cookie
/// belongs to the device it was signed in on, as the identity beside it
/// does. DPAPI's key is the user's own on this machine; another user of the
/// same PC, or a copy of the file elsewhere, cannot open it.
///
/// A secret that will not open is forgotten rather than reported, as on
/// Android: the remedy is to sign in again, which is what an absent cookie
/// already asks for.
/// </summary>
internal sealed class DpapiSecrets : ISecrets
{
    private readonly string _folder;

    /// <summary>Tells these secrets apart from anything else this user's DPAPI seals.</summary>
    private static readonly byte[] Entropy = Encoding.UTF8.GetBytes("app.tomoshibi.secrets");

    public DpapiSecrets(string folder)
    {
        _folder = folder;
        Directory.CreateDirectory(folder);
    }

    public static DpapiSecrets App() => new(Path.Combine(Folders.Local, "secrets"));

    /// <summary>A file name for a secret's name, which holds a URL and may hold anything.</summary>
    private string PathOf(string name) =>
        Path.Combine(_folder, Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(name)))[..32] + ".bin");

    public string? Read(string name)
    {
        var path = PathOf(name);
        if (!File.Exists(path)) return null;
        try
        {
            return Encoding.UTF8.GetString(Unprotect(File.ReadAllBytes(path)));
        }
        catch (Exception fault)
        {
            Log.Line($"a kept secret could not be read and is forgotten: {fault.GetType().Name}");
            File.Delete(path);
            return null;
        }
    }

    public void Write(string name, string? value)
    {
        var path = PathOf(name);
        if (string.IsNullOrEmpty(value))
        {
            if (File.Exists(path)) File.Delete(path);
            return;
        }
        try
        {
            File.WriteAllBytes(path, Protect(Encoding.UTF8.GetBytes(value)));
        }
        catch (Exception fault)
        {
            // Not kept is the same as never typed. Kept in the clear instead
            // would be the one thing worse.
            Log.Line($"a secret could not be kept: {fault.GetType().Name}");
            if (File.Exists(path)) File.Delete(path);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Blob
    {
        public int Size;
        public IntPtr Data;
    }

    [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool CryptProtectData(ref Blob input, string? description, ref Blob entropy, IntPtr reserved, IntPtr prompt, int flags, out Blob output);

    [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool CryptUnprotectData(ref Blob input, IntPtr description, ref Blob entropy, IntPtr reserved, IntPtr prompt, int flags, out Blob output);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    /// <summary>No prompt is ever shown: there is nobody to answer one in the middle of a launch.</summary>
    private const int UiForbidden = 0x1;

    private static byte[] Protect(byte[] plain) => Call(plain, sealing: true);

    private static byte[] Unprotect(byte[] sealedBytes) => Call(sealedBytes, sealing: false);

    private static byte[] Call(byte[] bytes, bool sealing)
    {
        var input = GCHandle.Alloc(bytes, GCHandleType.Pinned);
        var salt = GCHandle.Alloc(Entropy, GCHandleType.Pinned);
        try
        {
            var inBlob = new Blob { Size = bytes.Length, Data = input.AddrOfPinnedObject() };
            var saltBlob = new Blob { Size = Entropy.Length, Data = salt.AddrOfPinnedObject() };
            var ok = sealing
                ? CryptProtectData(ref inBlob, "Tomoshibi", ref saltBlob, IntPtr.Zero, IntPtr.Zero, UiForbidden, out var output)
                : CryptUnprotectData(ref inBlob, IntPtr.Zero, ref saltBlob, IntPtr.Zero, IntPtr.Zero, UiForbidden, out output);
            if (!ok) throw new CryptographicException(Marshal.GetLastWin32Error());
            try
            {
                var result = new byte[output.Size];
                Marshal.Copy(output.Data, result, 0, output.Size);
                return result;
            }
            finally
            {
                LocalFree(output.Data);
            }
        }
        finally
        {
            input.Free();
            salt.Free();
        }
    }
}

/// <summary>Secrets held for the life of the process: a rehearsal's.</summary>
internal sealed class MemorySecrets : ISecrets
{
    private readonly Dictionary<string, string> _values = new();

    public string? Read(string name) => _values.TryGetValue(name, out var value) ? value : null;

    public void Write(string name, string? value)
    {
        if (string.IsNullOrEmpty(value)) _values.Remove(name);
        else _values[name] = value;
    }
}
