using System.Text.Json;
using System.Text.Json.Nodes;

namespace Tomoshibi.Basics;

/// <summary>
/// Plain choices kept between launches, under the web's own storage names
/// where the web keeps the same thing (<c>meet-live.name</c>,
/// <c>meet-live.relay</c>, <c>meet-live.locale</c>...): the Apple apps'
/// UserDefaults and Android's SharedPreferences (model/Kept.kt).
///
/// Everything is a string or a boolean, as a browser's storage holds it, so
/// that what is kept reads the same as what the web keeps.
/// </summary>
public interface IDefaults
{
    string? String(string key);
    void Set(string key, string? value);
    bool? Bool(string key);
    void Set(string key, bool value);
    void Remove(string key);
}

/// <summary>
/// Kept in memory only: what a rehearsal (a test driving the app) writes to,
/// so that nothing a test does outlives the process, and what the unit tests
/// read back.
/// </summary>
public sealed class MemoryDefaults : IDefaults
{
    private readonly Dictionary<string, object> _values = new();

    public string? String(string key) => _values.TryGetValue(key, out var value) ? value as string : null;

    public void Set(string key, string? value)
    {
        if (value == null) _values.Remove(key);
        else _values[key] = value;
    }

    public bool? Bool(string key) => _values.TryGetValue(key, out var value) && value is bool b ? b : null;

    public void Set(string key, bool value) => _values[key] = value;

    public void Remove(string key) => _values.Remove(key);

    public bool Has(string key) => _values.ContainsKey(key);
}

/// <summary>
/// The app's choices in one JSON file, written whenever one changes
/// (Android's SharedPreferences, the Apple apps' UserDefaults), under the
/// web's own names where the web keeps the same thing. Every value is a
/// string or a boolean, as a browser's storage holds them.
///
/// Written on the UI thread as things change (each letter of a name typed on
/// the join screen, the window's place as it closes), so a write that does
/// not go through is never an exception: a virus scanner or a backup holding
/// the file, or the file written beside it, made File.Move throw into a
/// TextChanged handler and the app closed, what was typed with it. Such a
/// write is tried twice more a moment apart, and then left; the choices stay
/// in memory, where they are read from, and the next change writes them all.
/// </summary>
public sealed class FileDefaults : IDefaults
{
    private const int Attempts = 3;
    private static readonly TimeSpan Pause = TimeSpan.FromMilliseconds(25);
    private static readonly JsonSerializerOptions Indented = new() { WriteIndented = true };

    private readonly string _path;
    private readonly JsonObject _values;
    private readonly object _gate = new();
    private readonly Action<string>? _said;

    /// <summary><paramref name="said"/> hears, for the log, what could not be read or written.</summary>
    public FileDefaults(string path, Action<string>? said = null)
    {
        _path = path;
        _said = said;
        JsonObject? read = null;
        try
        {
            if (File.Exists(path)) read = JsonNode.Parse(File.ReadAllText(path)) as JsonObject;
        }
        catch (Exception fault)
        {
            // A file left half written by a crash is a fresh start rather
            // than an app that will not open.
            said?.Invoke($"preferences unreadable, starting afresh: {fault.GetType().Name}");
        }
        _values = read ?? new JsonObject();
    }

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
        var text = _values.ToJsonString(Indented);
        for (var attempt = 1; ; attempt++)
        {
            try
            {
                File.WriteAllText(temporary, text);
                File.Move(temporary, _path, overwrite: true);
                return;
            }
            // A file held with nothing shared is an IOException to write and
            // an UnauthorizedAccessException to replace.
            catch (Exception fault) when (fault is IOException or UnauthorizedAccessException)
            {
                if (attempt == Attempts)
                {
                    _said?.Invoke($"preferences not written, kept for the next change: {fault.GetType().Name}");
                    return;
                }
                Thread.Sleep(Pause);
            }
        }
    }
}
