using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Tomoshibi.Basics;

/*
 * How loud everybody else is, and who is not to be heard at all: the record
 * the sound panel writes and the call applies (web/src/live/hearing.ts;
 * Android's model/Hearing.kt, the Apple apps' Hearing).
 *
 * The web keeps this record itself rather than on the media library's
 * objects, because those forget: a volume lives on a participant that a
 * reconnect rebuilds, and a block lives on a publication that does not
 * outlive its track, so somebody who stops a share and starts it again comes
 * back audible. The apps forget the same way, more so: every remote sound is
 * given a player of its own when it is subscribed, at full volume. So this is
 * the record, and what the call holds is filled again from it whenever the
 * room changes.
 */

/// <summary>One thing there is to hear from somebody (hearing.ts <c>Sound</c>).</summary>
public enum Sound { Voice, Screen, Music }

public static class Sounds
{
    public static readonly Sound[] All = { Sound.Voice, Sound.Screen, Sound.Music };

    /// <summary>The web's name for the sound, which is half of where its setting is filed.</summary>
    public static string Key(this Sound sound) => sound switch
    {
        Sound.Voice => "voice",
        Sound.Screen => "screen",
        _ => "music",
    };
}

/// <summary>What somebody has decided about one sound.</summary>
/// <param name="Volume">Between silent and as loud as it was sent.</param>
/// <param name="Blocked">
/// Stopped at the media server rather than turned down here: a volume of
/// nought still downloads every packet of a shared film and throws it away.
/// </param>
public readonly record struct HearingSetting(double Volume, bool Blocked)
{
    /// <summary>Untouched, which is what everybody is until somebody says otherwise.</summary>
    public static readonly HearingSetting AsSent = new(1.0, false);

    /// <summary>Whether this is something somebody has decided not to hear.</summary>
    public bool Silenced => Blocked || Volume == 0.0;
}

/// <summary>
/// Where a setting is filed, and whether it outlives the call. A proven
/// signature is the same on every visit and worth writing down; anybody
/// else's identity is fresh each time, and a setting kept against one would
/// be applied to whoever is issued that string next (hearing.ts <c>filing</c>).
/// </summary>
public readonly record struct HearingFiling(string Key, bool Keeps);

/// <summary>Every setting, filed under <c>&lt;who&gt;/&lt;sound&gt;</c>, and what of it is kept.</summary>
public sealed class HearingBook
{
    /// <summary>The web's own storage name, old product name and all.</summary>
    public const string StorageKey = "meet-live.hearing";

    private readonly record struct Held(HearingSetting Setting, bool Keeps);

    private readonly Dictionary<string, Held> _entries;

    public HearingBook() => _entries = new Dictionary<string, Held>();

    /// <summary>
    /// The book as it was stored: the web's JSON, <c>{"&lt;who&gt;/&lt;sound&gt;":
    /// {"volume": 0.5, "blocked": false}}</c>. Anything unreadable reads as
    /// nothing decided.
    /// </summary>
    public HearingBook(string? stored) => _entries = Read(stored);

    /// <summary>What has been decided about one sound from one person, clamped on the way out as well.</summary>
    public HearingSetting Setting(HearingFiling filing, Sound sound)
    {
        if (!_entries.TryGetValue(Place(filing, sound), out var held)) return HearingSetting.AsSent;
        return new HearingSetting(Clamp(held.Setting.Volume), held.Setting.Blocked);
    }

    /// <summary>Writes a decision down. Returns whether what is kept changed, which is the only time storage needs writing.</summary>
    public bool Set(HearingFiling filing, Sound sound, HearingSetting setting)
    {
        var place = Place(filing, sound);
        var clean = new HearingSetting(Clamp(setting.Volume), setting.Blocked);
        var had = _entries.TryGetValue(place, out var before);
        // Nothing decided is not a decision to write down, so turning
        // somebody back up removes them rather than leaving a row saying they
        // are ordinary.
        if (clean == HearingSetting.AsSent) _entries.Remove(place);
        else _entries[place] = new Held(clean, filing.Keeps);
        var has = _entries.TryGetValue(place, out var after);
        return filing.Keeps && (had != has || (had && before != after));
    }

    /// <summary>
    /// What is kept, as the web writes it, or null where nothing is: the web
    /// removes the key rather than storing an empty object. Written by hand,
    /// each entry's fields in the order the web gives them.
    /// </summary>
    public string? Stored
    {
        get
        {
            var kept = _entries.Where(e => e.Value.Keeps).OrderBy(e => e.Key, StringComparer.Ordinal).ToList();
            if (kept.Count == 0) return null;
            var out_ = new StringBuilder("{");
            for (var i = 0; i < kept.Count; i++)
            {
                if (i > 0) out_.Append(',');
                var (place, held) = (kept[i].Key, kept[i].Value);
                out_.Append(JsonWords.Quote(place)).Append(":{\"volume\":").Append(JsonWords.Number(held.Setting.Volume))
                    .Append(",\"blocked\":").Append(held.Setting.Blocked ? "true" : "false").Append('}');
            }
            return out_.Append('}').ToString();
        }
    }

    public override bool Equals(object? other) =>
        other is HearingBook book && book._entries.Count == _entries.Count && _entries.All(e => book._entries.TryGetValue(e.Key, out var held) && held == e.Value);

    public override int GetHashCode() => _entries.Count;

    private static string Place(HearingFiling filing, Sound sound) => $"{filing.Key}/{sound.Key()}";

    /// <summary>
    /// A volume between nought and one. A slider cannot produce anything
    /// else; a hand-edited entry can, and a player handed 3 would be three
    /// times as loud as the person sent it (hearing.ts <c>level</c>).
    /// </summary>
    public static double Clamp(double volume) => double.IsFinite(volume) ? Math.Clamp(volume, 0.0, 1.0) : HearingSetting.AsSent.Volume;

    private static Dictionary<string, Held> Read(string? stored)
    {
        var out_ = new Dictionary<string, Held>();
        if (stored == null) return out_;
        try
        {
            using var parsed = JsonDocument.Parse(stored);
            if (parsed.RootElement.ValueKind != JsonValueKind.Object) return out_;
            foreach (var entry in parsed.RootElement.EnumerateObject())
            {
                if (entry.Value.ValueKind != JsonValueKind.Object) continue;
                var volume = entry.Value.TryGetProperty("volume", out var v) && v.ValueKind == JsonValueKind.Number ? Clamp(v.GetDouble()) : HearingSetting.AsSent.Volume;
                // `=== true`, as the web reads it: a 1 is not a block.
                var blocked = entry.Value.TryGetProperty("blocked", out var b) && b.ValueKind == JsonValueKind.True;
                out_[entry.Name] = new Held(new HearingSetting(volume, blocked), true);
            }
        }
        catch (JsonException)
        {
            return new Dictionary<string, Held>();
        }
        return out_;
    }
}

/// <summary>What JSON is written as where a browser's JSON.stringify must be matched byte for byte.</summary>
public static class JsonWords
{
    /// <summary>A string as JSON, escaping what JSON must and nothing else.</summary>
    public static string Quote(string text)
    {
        var out_ = new StringBuilder(text.Length + 2).Append('"');
        foreach (var c in text)
        {
            switch (c)
            {
                case '"': out_.Append("\\\""); break;
                case '\\': out_.Append("\\\\"); break;
                case '\n': out_.Append("\\n"); break;
                case '\r': out_.Append("\\r"); break;
                case '\t': out_.Append("\\t"); break;
                case '\b': out_.Append("\\b"); break;
                case '\f': out_.Append("\\f"); break;
                default:
                    if (c < ' ') out_.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                    else out_.Append(c);
                    break;
            }
        }
        return out_.Append('"').ToString();
    }

    /// <summary>
    /// A number as JavaScript writes one: 1 rather than 1.0, and 0.0005
    /// rather than 5E-04. .NET's shortest digits are JavaScript's for every
    /// volume a slider can make, so only the notation is changed.
    /// </summary>
    public static string Number(double value)
    {
        if (value == Math.Round(value) && Math.Abs(value) < 9.0e15) return ((long)value).ToString(CultureInfo.InvariantCulture);
        return decimal.TryParse(value.ToString("R", CultureInfo.InvariantCulture), NumberStyles.Float, CultureInfo.InvariantCulture, out var exact)
            ? exact.ToString(CultureInfo.InvariantCulture)
            : value.ToString("R", CultureInfo.InvariantCulture);
    }
}
