using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;

namespace Tomoshibi.Media;

/// <summary>
/// The song desk's music going out: one published track, kept across songs,
/// fed in real time (Android's MusicOutput.kt; sound.ts <c>Output</c>), and
/// the lossless stream beside it where one is offered.
///
/// Every song is brought to 48 kHz before it goes into Opus, so one track
/// serves every song and is only closed when the music stops. The lossless
/// stream is the file at its own rate, as the web's is; a song at another
/// rate starts a new stream on the same track. Something always goes out,
/// if only zeros: paused, between songs, or past the end of a song while the
/// desk fetches the next, both keep flowing with silence, since the web
/// measured that a track with nothing playing into it stops being delivered
/// and a lossless stream silent for four seconds is given up on by every
/// listener for the rest of the track. The holder hears the same samples
/// through the call's own speakers, behind a volume of their own that
/// changes nothing sent.
/// </summary>
internal sealed class MusicOutput
{
    /// <summary>10 ms at 48 kHz, the frame the core's source cuts sound into.</summary>
    public const int Frame = 480;

    private readonly AudioInput _input;
    /// <summary>Whether the track was published with a lossless stream beside it.</summary>
    public readonly bool Lossless;
    private readonly MonitorSound? _monitor;
    private readonly Action<string> _ended;
    private readonly DispatcherQueue _ui;
    private readonly object _gate = new();
    private DecodedSong? _song;
    private string? _key;
    private FileStream? _pcm;
    private FileStream? _original;
    /// <summary>Where the song is, in 48 kHz frames for Opus; the lossless stream's place is its own reader's, so neither repeats or skips a sample to round.</summary>
    private long _position;
    private bool _paused;
    private bool _told;
    private bool _running = true;
    /// <summary>The rate the lossless stream runs at: the song's, kept through the silence after it until a song at another rate plays.</summary>
    private int _losslessRate;
    /// <summary>Frames of the lossless stream sent at this rate, and the 10 ms blocks of Opus sent meanwhile: 44.1 kHz does not divide into 10 ms, and counting one against the other keeps the stream at its rate without drifting.</summary>
    private long _losslessSent;
    private long _blocksAtRate;

    public MusicOutput(AudioInput input, bool lossless, MonitorSound? monitor, Action<string> ended)
    {
        _input = input;
        Lossless = lossless;
        _monitor = monitor;
        _ended = ended;
        _ui = DispatcherQueue.GetForCurrentThread();
    }

    private static long Now() => (long)(Stopwatch.GetTimestamp() * (1e9 / Stopwatch.Frequency));

    /// <summary>Starts the clock, on a thread of its own at the system's highest priority: late wake-ups are bursts every listener has to absorb.</summary>
    public void Start()
    {
        var thread = new Thread(Run) { IsBackground = true, Priority = ThreadPriority.Highest, Name = "Tomoshibi music" };
        thread.Start();
    }

    private void Run()
    {
        using var waiter = new Waiter();
        var pacer = new MusicPacer(Now());
        var block = new byte[Frame * 4];
        while (Send(pacer, block))
        {
            var wait = pacer.NextDeadline - Now();
            if (wait > 0) waiter.Wait(wait);
        }
    }

    /// <summary>Plays <paramref name="song"/> from its start, in place of whatever was playing, and unpaused: the next song plays unpaused (jukebox.ts <c>Desk.pause</c>).</summary>
    public void Play(DecodedSong song, string key)
    {
        DecodedSong? replaced;
        lock (_gate)
        {
            replaced = _song;
            Close();
            _song = song;
            _key = key;
            _pcm = Open(song.Pcm);
            var original = song.Original is { Channels: 2, Float: true, Bits: 32 } file ? file : null;
            _original = original != null && Lossless ? Open(original) : null;
            _position = 0;
            _paused = false;
            _told = false;
            if (Lossless && _original != null && song.Rate != _losslessRate)
            {
                _losslessRate = song.Rate;
                _losslessSent = 0;
                _blocksAtRate = 0;
            }
        }
        if (!ReferenceEquals(replaced, song)) replaced?.Dispose();
    }

    private static FileStream Open(WaveFile wave)
    {
        var stream = new FileStream(wave.Path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete, 64 * 1024);
        stream.Seek(wave.DataOffset, SeekOrigin.Begin);
        return stream;
    }

    private void Close()
    {
        _pcm?.Dispose();
        _pcm = null;
        _original?.Dispose();
        _original = null;
    }

    /// <summary>Pauses where it is, or goes on from there; both streams keep flowing with silence meanwhile.</summary>
    public void Pause(bool paused)
    {
        lock (_gate) _paused = paused;
    }

    /// <summary>Ends the thread and lets the song's files go; the caller unpublishes the track.</summary>
    public void Stop()
    {
        DecodedSong? last;
        lock (_gate)
        {
            _running = false;
            Close();
            last = _song;
            _song = null;
        }
        last?.Dispose();
    }

    /// <summary>Where the song playing is, in seconds, and how long it is.</summary>
    public (double At, double Of)? Progress
    {
        get
        {
            lock (_gate) return _song is { } song ? (_position / 48_000.0, song.Frames / 48_000.0) : null;
        }
    }

    /// <summary>Hands over every block that is due; false once stopped.</summary>
    private bool Send(MusicPacer pacer, byte[] block)
    {
        var due = pacer.Due(Now());
        for (var i = 0; i < due; i++)
        {
            if (Fill(block) is not { } filled) return false;
            try
            {
                _input.PushPcm(block);
                _monitor?.Write(block);
                if (filled.Original is { } original) _input.PushLossless(original.Samples, 2, (uint)original.Rate, original.At);
            }
            catch (Exception fault)
            {
                // A track the call already took down; the desk stops the
                // output once it hears.
                Log.Line($"music: a block did not go: {fault.Message}");
            }
            if (filled.Ended is { } key) _ui.TryEnqueue(() => _ended(key));
        }
        lock (_gate) return _running;
    }

    private sealed record OriginalBlock(float[] Samples, int Rate, long At);

    private sealed record Filled(OriginalBlock? Original, string? Ended);

    /// <summary>The next 10 ms of both streams: the song where it is playing, silence anywhere else; null once stopped.</summary>
    private Filled? Fill(byte[] block)
    {
        lock (_gate)
        {
            if (!_running) return null;
            var playing = _song != null && !_paused;
            var original = NextOriginal(playing);
            Array.Clear(block);
            if (_song is not { } song || !playing || _position >= song.Frames || _pcm == null) return new Filled(original, null);
            var count = (int)Math.Min(Frame, song.Frames - _position);
            var read = _pcm.ReadAtLeast(block.AsSpan(0, count * 4), count * 4, throwOnEndOfStream: false);
            _position += read / 4;
            if (read < count * 4) _position = song.Frames;
            if (_position >= song.Frames && !_told && _key is { } key)
            {
                _told = true;
                return new Filled(original, key);
            }
            return new Filled(original, null);
        }
    }

    /// <summary>As many frames of the file as are due with this block. Called with the lock held.</summary>
    private OriginalBlock? NextOriginal(bool playing)
    {
        if (!Lossless || _losslessRate <= 0) return null;
        _blocksAtRate += 1;
        var due = _blocksAtRate * _losslessRate / 100;
        var count = (int)(due - _losslessSent);
        if (count <= 0) return null;
        var samples = new float[count * 2];
        if (playing && _original != null && _song?.Rate == _losslessRate)
        {
            var bytes = MemoryMarshal.AsBytes(samples.AsSpan());
            _original.ReadAtLeast(bytes, bytes.Length, throwOnEndOfStream: false);
        }
        var at = _losslessSent * 1_000_000 / _losslessRate;
        _losslessSent = due;
        return new OriginalBlock(samples, _losslessRate, at);
    }
}

/// <summary>
/// A wait to the nearest fraction of a millisecond: a high-resolution
/// waitable timer, where Windows' own sleep rounds every wait up to the next
/// tick of its clock, 15.6 ms by default, and the music would go out in
/// bursts of two blocks. Where the system has none (before Windows 10 1803),
/// an ordinary waitable timer.
/// </summary>
internal sealed partial class Waiter : IDisposable
{
    private readonly IntPtr _timer;

    [LibraryImport("kernel32.dll", EntryPoint = "CreateWaitableTimerExW", StringMarshalling = StringMarshalling.Utf16)]
    private static partial IntPtr CreateWaitableTimerEx(IntPtr attributes, string? name, uint flags, uint access);

    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetWaitableTimer(IntPtr timer, ref long due, int period, IntPtr completion, IntPtr argument, [MarshalAs(UnmanagedType.Bool)] bool resume);

    [LibraryImport("kernel32.dll")]
    private static partial uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CloseHandle(IntPtr handle);

    private const uint HighResolution = 0x2;
    private const uint AllAccess = 0x1F0003;

    public Waiter()
    {
        _timer = CreateWaitableTimerEx(IntPtr.Zero, null, HighResolution, AllAccess);
        if (_timer == IntPtr.Zero) _timer = CreateWaitableTimerEx(IntPtr.Zero, null, 0, AllAccess);
    }

    public void Wait(long nanos)
    {
        if (_timer == IntPtr.Zero)
        {
            Thread.Sleep(TimeSpan.FromTicks(Math.Max(1, nanos / 100)));
            return;
        }
        // Negative: relative, in hundreds of nanoseconds.
        var due = -Math.Max(1, nanos / 100);
        if (SetWaitableTimer(_timer, ref due, 0, IntPtr.Zero, IntPtr.Zero, false)) WaitForSingleObject(_timer, 1000);
    }

    public void Dispose()
    {
        if (_timer != IntPtr.Zero) CloseHandle(_timer);
    }
}

/// <summary>
/// The music playing from here, in this computer's own speakers (the web's
/// <c>monitor</c>; Android's <c>playLocally</c>): written by the music's
/// clock and taken by the speakers' audio graph a quantum at a time, behind
/// a volume of the holder's own and the duck. Kept within a tenth of a
/// second of what goes out, so the holder does not hear the room a beat
/// late.
/// </summary>
internal sealed class MonitorSound : PulledSound
{
    private const int Capacity = 48_000 * 2;
    private const int Cushion = 960;
    private const int Most = 4_800;
    private readonly float[] _ring = new float[Capacity * 2];
    private long _written;
    private long _read;
    private bool _primed;
    private readonly object _gate = new();
    private float _volume = 1;
    private float _duck = 1;

    public MonitorSound() : base("music-monitor") { }

    public float Volume
    {
        set => Volatile.Write(ref _volume, Math.Clamp(value, 0f, 1f));
    }

    public float Duck
    {
        set => Volatile.Write(ref _duck, Math.Clamp(value, 0f, 1f));
    }

    /// <summary>10 ms of the music as it went out, two channels of 16 bits.</summary>
    public void Write(byte[] pcm)
    {
        var frames = pcm.Length / 4;
        lock (_gate)
        {
            for (var i = 0; i < frames * 2; i++)
            {
                _ring[(_written * 2 + i) % (Capacity * 2)] = (short)(pcm[2 * i] | pcm[2 * i + 1] << 8) / 32768f;
            }
            _written += frames;
        }
    }

    public override void Render(float[] stereo, int frames)
    {
        var gain = Volatile.Read(ref _volume) * Volatile.Read(ref _duck);
        lock (_gate)
        {
            var have = _written - _read;
            if (!_primed)
            {
                if (have < Cushion) return;
                _primed = true;
            }
            // Fallen behind (the speakers were changed, the graph was late):
            // the oldest goes, so what is heard stays with what is sent.
            if (have > Most) _read = _written - Cushion;
            var count = (int)Math.Min(frames, _written - _read);
            for (var i = 0; i < count * 2; i++) stereo[i] += _ring[(_read * 2 + i) % (Capacity * 2)] * gain;
            _read += count;
            if (count < frames) _primed = false;
        }
    }
}
