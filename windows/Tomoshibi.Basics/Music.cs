namespace Tomoshibi.Basics;

/// <summary>
/// Music that makes way for speech (web/src/live/duck.ts; Android's
/// feature/music/MusicDuck.kt; TomoshibiBasics/MusicDuck.swift).
///
/// Whenever anybody in the call is talking, this person included, the song
/// desk's music is turned down where it is heard, and brought back up a
/// moment after they stop: what a radio presenter's desk does under a voice.
/// Each participant decides for itself, from the microphones it can hear,
/// and turns down only its own playback, so what goes out is still the file
/// and nobody's choice reaches anybody else. Music also arrives gently: a
/// stream that starts is faded in over a second and a half.
///
/// The arithmetic alone, the times in milliseconds by the caller's clock, so
/// that the tests can hold it to the web's own cases.
/// </summary>
public sealed class MusicDuck
{
    /// <summary>How far the music comes down while somebody talks.</summary>
    public const double Ducked = 0.3;
    /// <summary>Down quickly, so the first word is heard; up slowly, so a pause is not a lurch.</summary>
    public const double AttackMs = 150;
    public const double HoldMs = 700;
    public const double ReleaseMs = 1200;
    public const double FadeInMs = 1500;
    /// <summary>How often the microphones are read and the level moved.</summary>
    public const double TickMs = 40;
    /// <summary>The RMS of a frame that counts as talking, about -38 dB below full scale.</summary>
    public const double Speech = 0.012;
    /// <summary>Readings in a row above <see cref="Speech"/> before it is taken for speech: a click is one.</summary>
    public const int Onset = 2;
    /// <summary>The key a participant's own music output is filed under, beside the identities of whoever else plays music (duck.ts <c>LOCAL</c>).</summary>
    public const string Local = "\u0000local";

    /// <summary>Whether the music is turned down under speech at all; the switch in the panel, kept as <c>meet-live.music-duck</c>.</summary>
    public bool Enabled { get; set; }
    public double Level { get; private set; } = 1;
    private double _lastSpeech = double.NegativeInfinity;
    private int _loud;
    private readonly Dictionary<string, double> _arrivals = new();

    public MusicDuck(bool enabled = true) => Enabled = enabled;

    /// <summary>Whether somebody is talking, as far as the music is concerned.</summary>
    public bool Ducking => Enabled && Level < 1;

    /// <summary>One reading of every microphone that is on, and the level moved by the time since the last; whether any music's factor moved.</summary>
    public bool Tick(IReadOnlyList<double> levels, double now, double elapsed)
    {
        var speaking = levels.Any(l => l > Speech);
        _loud = speaking ? _loud + 1 : 0;
        return Step(_loud >= Onset, now, elapsed);
    }

    /// <summary>Moves the level one tick (duck.ts <c>step</c>): toward <see cref="Ducked"/> at the attack's pace, held there until <see cref="HoldMs"/> after the last word, then back at the release's.</summary>
    public bool Step(bool speaking, double now, double elapsed)
    {
        var before = Level;
        if (speaking) _lastSpeech = now;
        var target = now - _lastSpeech < HoldMs ? Ducked : 1;
        if (Level > target) Level = Math.Max(target, Level - (1 - Ducked) * elapsed / AttackMs);
        else if (Level < target) Level = Math.Min(target, Level + (1 - Ducked) * elapsed / ReleaseMs);
        return Level != before || Fading(now);
    }

    /// <summary>What to multiply a music stream's volume by now: the duck, and the fade-in of a stream that has just started.</summary>
    public double Factor(string key, double now)
    {
        var fade = _arrivals.TryGetValue(key, out var at) ? Math.Min(1, Math.Max(0, (now - at) / FadeInMs)) : 1;
        return (Enabled ? Level : 1) * fade;
    }

    /// <summary>A music stream has started from <paramref name="key"/>: faded in once, and again only after it has gone, so the songs that follow on the same stream are not.</summary>
    public void Arrived(string key, double now) => _arrivals.TryAdd(key, now);

    public void Left(string key) => _arrivals.Remove(key);

    /// <summary>Whether some stream is still fading in, and so still moving.</summary>
    public bool Fading(double now) => _arrivals.Values.Any(at => now - at < FadeInMs + TickMs);

    /// <summary>Whether anything still moves without a microphone saying so.</summary>
    public bool Settled(double now) => Level == 1 && !Fading(now);
}

/// <summary>
/// When the next block of music is due, by a real clock (Android's
/// MusicPacer.kt; TomoshibiBasics/MusicPacer.swift).
///
/// The core's music source sends whatever it is handed at once: it has no
/// clock of its own. A song pushed as fast as it decodes would go out in a
/// few seconds, at the wrong speed, and the listeners' jitter buffers would
/// throw most of it away. So it is handed over one 10 ms block at a time, as
/// many as the time since it started says are due. A thread that was held
/// up owes a burst: up to <see cref="MostBehind"/> blocks are made up at
/// once, and anything beyond is let go and the clock starts again from
/// there, so the song resumes where it stopped rather than racing through
/// seconds of it to catch up.
/// </summary>
public sealed class MusicPacer
{
    public readonly long BlockNanos;
    public readonly int MostBehind;
    private long _base;
    private long _sent;

    public MusicPacer(long start, long blockNanos = 10_000_000, int mostBehind = 20)
    {
        _base = start;
        BlockNanos = blockNanos;
        MostBehind = mostBehind;
    }

    /// <summary>How many blocks are due by <paramref name="now"/>, which are then counted as sent.</summary>
    public int Due(long now)
    {
        if (now <= _base) return 0;
        var owed = (now - _base) / BlockNanos;
        if (owed <= _sent) return 0;
        var due = owed - _sent;
        if (due > MostBehind)
        {
            // Let the rest go: the clock restarts as if only MostBehind
            // blocks had been owed.
            _base += (due - MostBehind) * BlockNanos;
            due = MostBehind;
        }
        _sent += due;
        return (int)due;
    }

    /// <summary>When the block after those sent falls due.</summary>
    public long NextDeadline => _base + (_sent + 1) * BlockNanos;
}

/// <summary>Where the song desk's choices are kept, the web's own names (preferences.ts, duck.ts, sound.ts).</summary>
public static class MusicKeys
{
    public const string Quality = "meet-live.music-quality";
    public const string Duck = "meet-live.music-duck";
    public const string Monitor = "meet-live.music-monitor";
    /// <summary>Shared with the share menu's switch: the web keeps one choice for both (sound.ts <c>rememberedLossless</c>, on unless turned off).</summary>
    public const string Lossless = "meet-live.lossless";

    /// <summary>A monitor volume nothing was kept for is full, and so is one that is not a volume: sound.ts once read "nothing kept" as nought and played the music to everybody but its own speakers.</summary>
    public static float MonitorLevel(string? kept) =>
        kept != null && float.TryParse(kept, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var level) && level is >= 0f and <= 1f ? level : 1f;
}
