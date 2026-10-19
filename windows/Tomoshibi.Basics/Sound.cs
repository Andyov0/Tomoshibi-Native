namespace Tomoshibi.Basics;

/// <summary>
/// Sound handed over from the core's threads to the player's (Android's
/// media/PcmRing.kt, the Apple apps' PCMRing, with the same numbers).
///
/// The core delivers each stream in 10 ms pieces on a clock of its own; the
/// speaker asks for its buffer on another. The two drift, so the ring keeps a
/// little in hand, starts playing only once it has some, plays silence rather
/// than stale sound when it runs dry, and throws away the oldest sound when
/// it has gathered more than it should hold: a sound that falls further and
/// further behind is worse than one that skips a few milliseconds once.
///
/// The writer is a core thread, which must never wait on anything slow; the
/// only wait is this ring's own short lock, held for one copy at a time.
/// Frames are per channel; at 48 kHz, 480 frames are 10 ms.
/// </summary>
public sealed class PcmRing
{
    public int Channels { get; }
    private readonly int _startFrames;
    private readonly int _ceilingFrames;
    private readonly int _settleFrames;
    private readonly int _capacity;
    private readonly float[] _samples;
    private int _read;
    private int _count;
    private bool _playing;
    private readonly object _gate = new();
    private int _written;
    private int _rendered;

    public PcmRing(int channels, int startFrames = 1440, int ceilingFrames = 9600, int settleFrames = 2880)
    {
        Channels = channels;
        _startFrames = startFrames;
        _ceilingFrames = ceilingFrames;
        _settleFrames = settleFrames;
        _capacity = ceilingFrames * 2;
        _samples = new float[_capacity * channels];
    }

    /// <summary>Interleaved 16-bit little-endian samples, as the core's PcmSink hands them over.</summary>
    public void Write(byte[] pcm, int sourceChannels = 0)
    {
        if (sourceChannels <= 0) sourceChannels = Channels;
        var frames = pcm.Length / 2 / sourceChannels;
        lock (_gate)
        {
            _written += frames;
            for (var frame = 0; frame < frames; frame++)
            {
                var at = Slot();
                for (var c = 0; c < Channels; c++)
                {
                    // A track that arrives in another channel count than it
                    // was announced with is folded rather than misread: one
                    // channel copied to both, or the first of two taken.
                    var from = (frame * sourceChannels + Math.Min(c, sourceChannels - 1)) * 2;
                    _samples[at + c] = (short)(pcm[from] | pcm[from + 1] << 8) / 32768f;
                }
                _count++;
            }
            Settle();
        }
    }

    /// <summary>Interleaved 32-bit float samples, as a lossless stream arrives.</summary>
    public void Write(IReadOnlyList<float> floats)
    {
        var frames = floats.Count / Channels;
        lock (_gate)
        {
            _written += frames;
            for (var frame = 0; frame < frames; frame++)
            {
                var at = Slot();
                for (var c = 0; c < Channels; c++) _samples[at + c] = floats[frame * Channels + c];
                _count++;
            }
            Settle();
        }
    }

    /// <summary>Where the next frame goes, dropping the oldest when the ring is full.</summary>
    private int Slot()
    {
        if (_count == _capacity)
        {
            _read = (_read + 1) % _capacity;
            _count--;
        }
        return (_read + _count) % _capacity * Channels;
    }

    private void Settle()
    {
        if (_count <= _ceilingFrames) return;
        var drop = _count - _settleFrames;
        _read = (_read + drop) % _capacity;
        _count -= drop;
    }

    /// <summary>
    /// Adds <paramref name="frames"/> frames of this ring, times
    /// <paramref name="gain"/>, into <paramref name="into"/>, which is
    /// interleaved stereo; a mono ring goes to both sides. Silence where the
    /// ring has run dry. Returns the sum of squares of what was added.
    /// </summary>
    public double Mix(float[] into, int frames, float gain)
    {
        double energy = 0;
        lock (_gate)
        {
            var available = Available(frames);
            for (var frame = 0; frame < available; frame++)
            {
                var at = (_read + frame) % _capacity * Channels;
                var left = _samples[at];
                var right = Channels > 1 ? _samples[at + 1] : left;
                into[frame * 2] += left * gain;
                into[frame * 2 + 1] += right * gain;
                energy += (left * left + right * right) / 2.0;
            }
            _read = (_read + available) % _capacity;
            _count -= available;
            _rendered += frames;
        }
        return energy;
    }

    /// <summary>
    /// Takes up to <paramref name="frames"/> frames out, interleaved in this
    /// ring's own channel count, written from <paramref name="offset"/> (in
    /// samples), for a reader that resamples; how many it took, nought while
    /// the ring is gathering its cushion or has run dry.
    /// </summary>
    public int Read(float[] into, int offset, int frames)
    {
        lock (_gate)
        {
            var available = Available(frames);
            for (var frame = 0; frame < available; frame++)
            {
                var at = (_read + frame) % _capacity * Channels;
                for (var c = 0; c < Channels; c++) into[offset + frame * Channels + c] = _samples[at + c];
            }
            _read = (_read + available) % _capacity;
            _count -= available;
            _rendered += available;
            return available;
        }
    }

    private int Available(int frames)
    {
        if (!_playing && _count >= _startFrames) _playing = true;
        if (_playing && _count == 0) _playing = false;
        return _playing ? Math.Min(frames, _count) : 0;
    }

    /// <summary>Frames written in and played out since the last call.</summary>
    public (int Written, int Rendered) Take()
    {
        lock (_gate)
        {
            var out_ = (_written, _rendered);
            _written = 0;
            _rendered = 0;
            return out_;
        }
    }

    /// <summary>How many frames are waiting.</summary>
    public int Buffered
    {
        get
        {
            lock (_gate) return _count;
        }
    }

    public void Clear()
    {
        lock (_gate)
        {
            _read = 0;
            _count = 0;
            _playing = false;
        }
    }
}

/// <summary>
/// A stream at its own rate taken out at the player's, through a
/// windowed-sinc filter (a Kaiser window over 48 zero crossings a side, in a
/// polyphase bank): Android's media/Resampler.kt, number for number.
///
/// A lossless stream arrives at whatever rate it was captured at: a song at
/// 44.1 kHz, a shared sound at 48, a file at 96. A linear interpolation,
/// which Android used first, is for a 44.1 kHz song a filter whose stopband
/// barely exists: the images of everything above a few kilohertz folded back
/// into the band as a haze, in the one sound somebody chose to send without
/// loss. This passes everything below 94.5 per cent of the lower of the two
/// Nyquist frequencies and is down about ninety decibels by the Nyquist
/// itself; each phase is normalised to a gain of exactly one; and the ratio
/// is held as a fraction of whole numbers, so the two clocks never drift
/// apart by rounding.
///
/// Not thread-safe: one per stream, used by the player's thread alone.
/// </summary>
public sealed class Resampler
{
    /// <summary>Where input frames come from: interleaved into the array at the offset, up to the count; how many came.</summary>
    public delegate int Source(float[] into, int offset, int frames);

    public const double ZeroCrossings = 48.0;
    public const double Rolloff = 0.945;
    public const double Beta = 9.0;
    public const int MostPhases = 1024;
    private const int Block = 512;

    public int From { get; }
    public int To { get; }
    public int Channels { get; }
    private readonly Source _source;
    private readonly int _phases;
    private readonly int _step;
    private readonly int _half;
    private readonly int _taps;
    private readonly float[] _bank;
    private readonly bool _passthrough;
    private readonly float[] _buffer;
    private int _start;
    private int _end;
    private int _phase;

    public Resampler(int from, int to, int channels, Source source)
    {
        if (from <= 0 || to <= 0 || channels <= 0) throw new ArgumentOutOfRangeException(nameof(from));
        From = from;
        To = to;
        Channels = channels;
        _source = source;
        _passthrough = from == to;
        if (_passthrough)
        {
            _phases = 1;
            _step = 1;
            _half = 0;
            _taps = 1;
            _bank = Array.Empty<float>();
        }
        else
        {
            (_phases, _step) = Fraction(from, to);
            // The filter's scale: its cutoff in input frames, below the lower
            // of the two Nyquists.
            var scale = Rolloff * Math.Min(1.0, (double)to / from);
            var window = ZeroCrossings / scale;
            _half = (int)Math.Ceiling(window) + 1;
            _taps = 2 * _half;
            _bank = new float[_phases * _taps];
            for (var p = 0; p < _phases; p++)
            {
                double sum = 0;
                var row = p * _taps;
                var values = new double[_taps];
                for (var k = 0; k < _taps; k++)
                {
                    // How far this tap's input frame is from the output's
                    // instant, in input frames.
                    var distance = (_half - 1 - k) + (double)p / _phases;
                    var value = Math.Abs(distance) >= window ? 0.0 : Sinc(distance * scale) * Kaiser(distance / window);
                    values[k] = value;
                    sum += value;
                }
                for (var k = 0; k < _taps; k++) _bank[row + k] = (float)(values[k] / sum);
            }
        }
        _buffer = new float[(_taps + Block) * channels];
        // The first output stands at the newest of `half` frames of silence,
        // so the stream starts at once rather than after a filter's length.
        _end = Math.Max(_half - 1, 0);
    }

    /// <summary>
    /// Adds <paramref name="frames"/> frames into <paramref name="into"/>
    /// (interleaved stereo; a mono stream goes to both sides), each times its
    /// own gain from <paramref name="gains"/> when given or
    /// <paramref name="gain"/> otherwise. A source that has run dry gives
    /// none, and silence is filtered in its place. Returns the sum of squares
    /// of what was added, before the gain.
    /// </summary>
    public double Render(float[] into, int frames, float gain, float[]? gains = null)
    {
        double energy = 0;
        var channels = Channels;
        var buffer = _buffer;
        for (var frame = 0; frame < frames; frame++)
        {
            if (_end - _start < _taps) Fill();
            var at = _start * channels;
            float left, right;
            if (_passthrough)
            {
                left = buffer[at];
                right = channels > 1 ? buffer[at + 1] : left;
                _start++;
            }
            else
            {
                var row = _phase * _taps;
                float l = 0, r = 0;
                var i = at;
                if (channels > 1)
                {
                    for (var k = row; k < row + _taps; k++)
                    {
                        var c = _bank[k];
                        l += c * buffer[i];
                        r += c * buffer[i + 1];
                        i += channels;
                    }
                }
                else
                {
                    for (var k = row; k < row + _taps; k++)
                    {
                        l += _bank[k] * buffer[i];
                        i++;
                    }
                    r = l;
                }
                left = l;
                right = r;
                _phase += _step;
                while (_phase >= _phases)
                {
                    _phase -= _phases;
                    _start++;
                }
            }
            var g = gains != null ? gains[frame] : gain;
            into[frame * 2] += left * g;
            into[frame * 2 + 1] += right * g;
            energy += (left * left + right * right) / 2.0;
        }
        return energy;
    }

    /// <summary>Input frames from the source until a whole filter's worth is in hand, silence where it has none.</summary>
    private void Fill()
    {
        var capacity = _buffer.Length / Channels;
        if (_start > 0 && _end + Block / 2 > capacity)
        {
            Array.Copy(_buffer, _start * Channels, _buffer, 0, (_end - _start) * Channels);
            _end -= _start;
            _start = 0;
        }
        while (_end - _start < _taps)
        {
            var got = _source(_buffer, _end * Channels, capacity - _end);
            if (got <= 0)
            {
                var upTo = _start + _taps;
                Array.Clear(_buffer, _end * Channels, (upTo - _end) * Channels);
                _end = upTo;
                return;
            }
            _end += got;
        }
    }

    private static double Sinc(double x) => x == 0.0 ? 1.0 : Math.Sin(Math.PI * x) / (Math.PI * x);

    private static double Kaiser(double x)
    {
        var inside = 1.0 - x * x;
        if (inside <= 0.0) return 0.0;
        return Bessel(Beta * Math.Sqrt(inside)) / Bessel(Beta);
    }

    /// <summary>The modified Bessel function of the first kind, order zero, by its series.</summary>
    private static double Bessel(double x)
    {
        double sum = 1, term = 1, half = x / 2;
        for (var k = 1; k < 64; k++)
        {
            term *= half / k * (half / k);
            sum += term;
            if (term < sum * 1e-12) break;
        }
        return sum;
    }

    /// <summary>
    /// Output frames over input frames as whole numbers (phases, step):
    /// <paramref name="to"/> over <paramref name="from"/> in lowest terms, or
    /// the nearest fraction with no more than <see cref="MostPhases"/> phases,
    /// by its continued fraction.
    /// </summary>
    public static (int Phases, int Step) Fraction(int from, int to)
    {
        var common = Gcd(from, to);
        if (to / common <= MostPhases) return (to / common, from / common);
        var x = (double)from / to;
        long h0 = 0, h1 = 1, k0 = 1, k1 = 0;
        var best = (1, Math.Max(1, (int)Math.Round(x)));
        for (var i = 0; i < 32; i++)
        {
            var a = (long)Math.Floor(x);
            var h2 = a * h1 + h0;
            var k2 = a * k1 + k0;
            if (k2 > MostPhases) break;
            best = ((int)k2, (int)h2);
            (h0, h1, k0, k1) = (h1, h2, k1, k2);
            var rest = x - a;
            if (rest < 1e-12) break;
            x = 1 / rest;
        }
        return best;
    }

    private static int Gcd(int a, int b) => b == 0 ? a : Gcd(b, a % b);
}
