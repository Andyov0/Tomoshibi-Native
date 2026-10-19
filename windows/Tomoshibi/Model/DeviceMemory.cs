using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Tomoshibi.Basics;

namespace Tomoshibi.Model;

/// <summary>
/// The choices about one's own devices that a call starts from, kept under
/// the web's own names (live/preferences.ts, live/sound.ts) and read the way
/// the web reads them, as Android's model/DeviceMemory.kt does.
///
/// <c>meet-live.devices</c> is whether the microphone and the camera start
/// on; <c>meet-live.device-choice</c> is which microphone, camera and
/// speakers were last picked, by kind; <c>meet-live.original-sound</c> is
/// whether the voice goes as it is. Each is a string, as a browser's storage
/// holds it, and every reader treats a value it does not recognise as never
/// having been told anything: a value half written, or written by a later
/// build, may not become a call that starts with a silent microphone.
/// </summary>
internal sealed class DeviceMemory
{
    public const string DevicesKey = "meet-live.devices";
    public const string ChoiceKey = "meet-live.device-choice";
    public const string OriginalKey = "meet-live.original-sound";
    public const string BlurKey = "meet-live.blur";

    /// <summary>Longer than any device's identifier, and short enough to refuse what is not one.</summary>
    public const int MostId = 256;

    /// <summary>The kinds of device somebody picks from a list (preferences.ts <c>CHOSEN_KINDS</c>).</summary>
    public enum Kind { Microphone, Camera, Speakers }

    public static string KeyOf(Kind kind) => kind switch
    {
        Kind.Microphone => "audioinput",
        Kind.Camera => "videoinput",
        _ => "audiooutput",
    };

    /// <summary>Whether the camera and the microphone start on.</summary>
    public readonly record struct Joining(bool Camera, bool Microphone)
    {
        /// <summary>The microphone on and the camera off: how most people walk into a call, and the cheaper of the two mistakes.</summary>
        public static readonly Joining First = new(false, true);
    }

    /// <summary>Where it is all kept; null keeps nothing, which is what a test driving the app gets.</summary>
    public IDefaults? Defaults { get; }

    public DeviceMemory(IDefaults? defaults) => Defaults = defaults;

    private static JsonObject? ObjectOf(string? text)
    {
        if (text == null) return null;
        try
        {
            return JsonNode.Parse(text) as JsonObject;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private static bool? Flag(JsonObject parsed, string key) =>
        parsed[key] is JsonValue value && value.GetValueKind() is JsonValueKind.True or JsonValueKind.False ? value.GetValue<bool>() : null;

    public Joining JoiningChoice
    {
        get
        {
            if (ObjectOf(Defaults?.String(DevicesKey)) is not { } parsed) return Joining.First;
            // Each on its own: a record that lost one half keeps the other.
            return new Joining(Flag(parsed, "camera") ?? Joining.First.Camera, Flag(parsed, "microphone") ?? Joining.First.Microphone);
        }
    }

    public void Remember(Joining joining) =>
        Defaults?.Set(DevicesKey, $"{{\"camera\":{(joining.Camera ? "true" : "false")},\"microphone\":{(joining.Microphone ? "true" : "false")}}}");

    /// <summary>The device last picked of this kind, or null.</summary>
    public string? Choice(Kind kind) => Choices().TryGetValue(kind, out var id) ? id : null;

    public IReadOnlyDictionary<Kind, string> Choices()
    {
        var out_ = new Dictionary<Kind, string>();
        if (ObjectOf(Defaults?.String(ChoiceKey)) is not { } parsed) return out_;
        foreach (var kind in Enum.GetValues<Kind>())
        {
            if (parsed[KeyOf(kind)] is JsonValue value && value.TryGetValue<string>(out var id) && id.Length is > 0 and <= MostId) out_[kind] = id;
        }
        return out_;
    }

    /// <summary>
    /// Kept once the device took, and not before (DeviceMenu.tsx
    /// <c>select</c>); null forgets the choice of that kind, which is "the
    /// same as the system".
    /// </summary>
    public void Remember(Kind kind, string? id)
    {
        if (Defaults == null) return;
        if (id != null && (id.Length == 0 || id.Length > MostId)) return;
        var all = Choices().ToDictionary(p => p.Key, p => p.Value);
        if (id == null) all.Remove(kind);
        else all[kind] = id;
        if (all.Count == 0)
        {
            Defaults.Remove(ChoiceKey);
            return;
        }
        var fields = all.OrderBy(p => KeyOf(p.Key), StringComparer.Ordinal).Select(p => $"{Quote(KeyOf(p.Key))}:{Quote(p.Value)}");
        Defaults.Set(ChoiceKey, "{" + string.Join(",", fields) + "}");
    }

    public void ForgetChoices() => Defaults?.Remove(ChoiceKey);

    /// <summary>Which of the remembered devices can be asked for: one that is listed now (preferences.ts <c>restorable</c>).</summary>
    public string? Restorable(Kind kind, IEnumerable<string> present) => Choice(kind) is { } id && present.Contains(id) ? id : null;

    /// <summary>Whether the voice was last sent as it is (sound.ts <c>rememberedOriginal</c>).</summary>
    public bool OriginalSound
    {
        get => Defaults?.String(OriginalKey) == "on";
        set => Defaults?.Set(OriginalKey, value ? "on" : null);
    }

    /// <summary>The background blurred behind somebody's camera (preferences.ts <c>blurWanted</c>).</summary>
    public bool Blur
    {
        get => Defaults?.String(BlurKey) == "on";
        set => Defaults?.Set(BlurKey, value ? "on" : null);
    }

    /// <summary>
    /// A string as JSON, escaping what JSON must and nothing else, as the
    /// web's <c>JSON.stringify</c> writes it, so that a record reads the same
    /// byte for byte as the browser's.
    /// </summary>
    public static string Quote(string text)
    {
        var built = new StringBuilder(text.Length + 2).Append('"');
        foreach (var c in text)
        {
            switch (c)
            {
                case '"': built.Append("\\\""); break;
                case '\\': built.Append("\\\\"); break;
                case '\n': built.Append("\\n"); break;
                case '\r': built.Append("\\r"); break;
                case '\t': built.Append("\\t"); break;
                case '\b': built.Append("\\b"); break;
                case '\f': built.Append("\\f"); break;
                default:
                    if (c < ' ') built.Append($"\\u{(int)c:x4}");
                    else built.Append(c);
                    break;
            }
        }
        return built.Append('"').ToString();
    }
}
