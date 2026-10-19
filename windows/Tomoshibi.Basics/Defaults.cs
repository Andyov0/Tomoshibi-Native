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
