namespace Tomoshibi.Basics;

/// <summary>A device as a list shows it: the system's identifier and the name it is listed under.</summary>
public sealed record ListedDevice(string Id, string Name);

/// <summary>
/// Which camera, microphone or speakers a call uses, and what becomes of it
/// when the devices change under it. The rules of the web's SDK
/// (livekit-client 2.x, <c>LocalParticipant.handleTrackEnded</c> and
/// <c>Room.selectDefaultDevices</c>), which a browser in the same room
/// follows, written once here so that the camera, the microphone and the
/// speakers follow them alike and the tests can hold them to it without a
/// device.
/// </summary>
public static class DeviceRules
{
    /// <summary>What a capture or a player does when the list of devices changes.</summary>
    public enum Move
    {
        /// <summary>Its device is still there: nothing changes.</summary>
        Stay,
        /// <summary>Its device went: it carries on, on the same track, on the device named.</summary>
        Switch,
        /// <summary>Nothing of its kind is left: it stops, and the call says so.</summary>
        Stop,
    }

    /// <summary>
    /// The device a capture opens: the one asked for where it is listed,
    /// otherwise the system's own (null). A camera has no system's own on
    /// Windows, which names a default microphone and default speakers but no
    /// default camera, so for a camera that is the first listed, as the
    /// browser's <c>getUserMedia</c> without a device takes the first.
    /// Null for a camera means there is none.
    /// </summary>
    public static string? Opened(string? asked, IReadOnlyList<string> present, bool systemHasOne)
    {
        if (asked != null && present.Contains(asked)) return asked;
        return systemHasOne ? null : present.FirstOrDefault();
    }

    /// <summary>
    /// What becomes of a capture or a player on <paramref name="using_"/>
    /// (null: the system's own, which follows the system's choice by itself)
    /// when the devices listed are now <paramref name="present"/>.
    ///
    /// The SDK moves a microphone whose track ended to the default device
    /// and a camera to whichever the browser gives (<c>restartTrack</c>),
    /// keeping the track, and mutes it when that fails; it moves speakers
    /// that went away to the first listed. The choice somebody remembered is
    /// not touched by any of this: the web remembers only what was picked
    /// (DeviceMenu.tsx <c>select</c>), so the headset unplugged for a moment
    /// is still the one the next call starts on.
    /// </summary>
    public static (Move Move, string? Next) Follow(string? using_, IReadOnlyList<string> present, bool systemHasOne)
    {
        if (using_ != null && present.Contains(using_)) return (Move.Stay, using_);
        if (present.Count == 0) return (Move.Stop, null);
        // The system's own follows the system: Windows moves a stream opened
        // on the default device to the new default by itself.
        if (using_ == null) return (Move.Stay, null);
        return (Move.Switch, systemHasOne ? null : present[0]);
    }

    /// <summary>
    /// Two of the same name told apart by a number (Platform.kt
    /// <c>numbered</c>): two identical webcams are otherwise two identical
    /// rows, and the person cannot tell which one they picked.
    /// </summary>
    public static IReadOnlyList<ListedDevice> Numbered(IEnumerable<ListedDevice> devices)
    {
        var seen = new Dictionary<string, int>();
        var out_ = new List<ListedDevice>();
        foreach (var device in devices)
        {
            var name = device.Name.Trim();
            if (name.Length == 0) name = Words.T("Device {number}", ("number", out_.Count + 1));
            seen[name] = seen.TryGetValue(name, out var count) ? count + 1 : 1;
            out_.Add(device with { Name = seen[name] > 1 ? $"{name} {seen[name]}" : name });
        }
        return out_;
    }

    /// <summary>
    /// The audio endpoint's own identifier, <c>{0.0.0.00000000}.{guid}</c>,
    /// from the device interface path Windows' device lists give
    /// (<c>\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{guid}#{e6327cad-…}</c>): the
    /// echo canceller is told which speakers the call plays to by the
    /// former. Null for anything that is not an audio endpoint's path.
    /// </summary>
    public static string? EndpointId(string? path)
    {
        if (path == null) return null;
        const string marker = "MMDEVAPI#";
        var start = path.IndexOf(marker, StringComparison.OrdinalIgnoreCase);
        if (start < 0) return path.StartsWith("{0.0.", StringComparison.Ordinal) ? path : null;
        start += marker.Length;
        var end = path.IndexOf('#', start);
        var id = end < 0 ? path[start..] : path[start..end];
        return id.StartsWith("{0.0.", StringComparison.Ordinal) ? id : null;
    }
}

/// <summary>
/// The size and rate a camera is opened at, and the size it is sent at.
///
/// What is sent is the web's (share.rs <c>camera_capture</c>): a computer
/// sends 1280x720 at thirty. The web asks <c>getUserMedia</c> for that size
/// as an ideal and the browser scales what the camera gives to it; Android
/// asks CameraX for the size with a 16:9 fallback and takes the closest
/// above, then below (CameraCapture.kt <c>makeAnalysis</c>). Windows lists
/// every format a camera offers, so the choice is made here, the same way.
/// </summary>
public static class CameraFormats
{
    /// <summary>One way a camera offers its frames: a size, a rate, and its encoding (NV12, YUY2, MJPG, …).</summary>
    public readonly record struct Offer(uint Width, uint Height, double Rate, string Subtype);

    /// <summary>
    /// The offer to open, or -1 when there is none with a picture.
    ///
    /// In order: a 16:9 picture where there is one (a 4:3 picture sent as a
    /// call's camera shows bars or a crop to everybody); the rate asked for
    /// where any offer reaches it, since a call at ten frames looks broken
    /// and a camera's larger size at full rate is cheaply scaled down; then
    /// the smallest size at least the one asked for, or the largest below
    /// it; then the lowest rate that reaches the one asked for, or the
    /// highest below; then an uncompressed encoding, which needs no decoder.
    /// </summary>
    public static int Choose(IReadOnlyList<Offer> offers, uint width, uint height, uint rate)
    {
        var usable = offers.Select((offer, index) => (offer, index)).Where(o => o.offer.Width > 0 && o.offer.Height > 0).ToList();
        if (usable.Count == 0) return -1;
        var wide = usable.Where(o => IsWide(o.offer)).ToList();
        if (wide.Count > 0) usable = wide;
        // A little under the rate counts as reaching it: cameras report
        // 29.97 for thirty.
        var reaching = usable.Where(o => o.offer.Rate >= rate - 0.5).ToList();
        if (reaching.Count > 0) usable = reaching;
        var large = usable.Where(o => o.offer.Width >= width && o.offer.Height >= height).ToList();
        var size = large.Count > 0
            ? large.MinBy(o => (long)o.offer.Width * o.offer.Height).offer
            : usable.MaxBy(o => (long)o.offer.Width * o.offer.Height).offer;
        var sameSize = usable.Where(o => o.offer.Width == size.Width && o.offer.Height == size.Height).ToList();
        var fast = sameSize.Where(o => o.offer.Rate >= rate - 0.5).ToList();
        var paced = fast.Count > 0
            ? fast.Where(o => Math.Abs(o.offer.Rate - fast.Min(f => f.offer.Rate)) < 0.01)
            : sameSize.Where(o => Math.Abs(o.offer.Rate - sameSize.Max(f => f.offer.Rate)) < 0.01);
        return paced.OrderBy(o => Cost(o.offer.Subtype)).First().index;
    }

    private static bool IsWide(Offer offer) => Math.Abs((double)offer.Width / offer.Height - 16.0 / 9.0) < 0.02;

    /// <summary>What decoding an encoding costs, cheapest first: NV12 goes to the core as it is.</summary>
    private static int Cost(string subtype) => subtype.ToUpperInvariant() switch
    {
        "NV12" => 0,
        "YUY2" => 1,
        "MJPG" => 3,
        _ => 2,
    };

    /// <summary>
    /// The size a picture of <paramref name="width"/> by
    /// <paramref name="height"/> is sent at to be no larger than the one
    /// asked for: scaled down, its shape kept, to even sides (the encoder's
    /// chroma is halved both ways); never scaled up, which would send more
    /// bits for nothing.
    /// </summary>
    public static (uint Width, uint Height) Sent(uint width, uint height, uint wantWidth, uint wantHeight)
    {
        if (width == 0 || height == 0) return (0, 0);
        var scale = Math.Min(1.0, Math.Min((double)wantWidth / width, (double)wantHeight / height));
        var w = (uint)Math.Round(width * scale) & ~1u;
        var h = (uint)Math.Round(height * scale) & ~1u;
        return (Math.Max(2, w), Math.Max(2, h));
    }
}

/// <summary>
/// Which of a camera's frames are sent, so that a camera running faster than
/// the web's rate does not send faster (CameraCapture.kt's Analyzer, its
/// rule ported as it is): a deadline that moves on by the interval rather
/// than by the gap since the last frame taken. Android's first version paced
/// by the gap, and a camera at thirty taking every frame fifty milliseconds
/// after the last took every other one, and sent fifteen rather than twenty.
/// A little early is taken, for the camera's own jitter.
/// </summary>
public sealed class FramePacer
{
    private readonly long _interval;
    private long _due;

    public FramePacer(uint rate) => _interval = 1_000_000L / Math.Max(1u, rate);

    /// <summary>Whether the frame taken at <paramref name="micros"/> (any steady clock, in microseconds) is sent.</summary>
    public bool Take(long micros)
    {
        if (micros < _due - _interval / 10) return false;
        _due = micros - _due > _interval ? micros + _interval : _due + _interval;
        return true;
    }
}

/// <summary>
/// How the voice is captured (Android's CallAudio <c>openRecord</c>): for a
/// call, the system's voice processing, which is where Windows keeps its
/// echo canceller, noise suppression and automatic gain (a communications
/// stream); the web's "original sound" asks for none of it, and is captured
/// raw where the microphone offers raw capture and plain where it does not.
/// </summary>
public static class VoiceCapture
{
    public enum Mode
    {
        /// <summary>Echo cancelled, noise suppressed, gain controlled: Android's VOICE_COMMUNICATION source with its effects.</summary>
        Communications,
        /// <summary>No processing at all: Android's UNPROCESSED.</summary>
        Raw,
        /// <summary>The microphone's ordinary capture: Android's MIC, where UNPROCESSED is not offered.</summary>
        Plain,
    }

    public static Mode For(bool original, bool rawOffered) => !original ? Mode.Communications : rawOffered ? Mode.Raw : Mode.Plain;

    /// <summary>
    /// Floats in -1..1 as the core's little-endian 16-bit samples, clipped
    /// rather than wrapped: a shout past full scale wrapped round to its
    /// opposite is a crack in everybody's ears.
    /// </summary>
    public static void ToPcm16(ReadOnlySpan<float> floats, Span<byte> pcm)
    {
        for (var i = 0; i < floats.Length; i++)
        {
            var value = floats[i] * 32767f;
            var sample = float.IsNaN(value) ? (short)0 : (short)Math.Clamp(value, short.MinValue, short.MaxValue);
            pcm[2 * i] = (byte)sample;
            pcm[2 * i + 1] = (byte)(sample >> 8);
        }
    }

    /// <summary>
    /// How full a level meter is, 0 to 1, for a root-mean-square level 0 to
    /// 1 (Android's MicrophoneLevel <c>shown</c>, MicLevel.tsx): in decibels
    /// from -60 to -10, which puts a voice in the upper half of the bar and
    /// the hiss of a quiet room under the "heard" line. Drawn linearly,
    /// speech at -26 dBFS barely moved the meter.
    /// </summary>
    public static double Shown(double rms)
    {
        if (rms <= 0) return 0;
        var decibels = 20 * Math.Log10(rms);
        return Math.Clamp((decibels + 60) / 50, 0, 1);
    }

    /// <summary>Anything shown above this has been heard (MicLevel.tsx's 0.06).</summary>
    public const double Heard = 0.06;
}
