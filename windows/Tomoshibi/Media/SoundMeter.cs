namespace Tomoshibi.Media;

/// <summary>
/// How loud a sound has been over the last tenth of a second, as an RMS
/// from nought to full scale (Android's SoundLevel): what the song desk's
/// duck reads to tell whether somebody is talking. Fed 10 ms at a time from
/// whichever thread the sound arrives on, read from the UI thread.
/// </summary>
internal sealed class SoundMeter
{
    private const int Kept = 10;
    private readonly double[] _squares = new double[Kept];
    private readonly int[] _counts = new int[Kept];
    private int _next;
    private readonly object _gate = new();

    /// <summary>Little-endian 16-bit samples, any number of channels.</summary>
    public void Add(byte[] pcm)
    {
        double squares = 0;
        var count = pcm.Length / 2;
        for (var i = 0; i < count; i++)
        {
            double sample = (short)(pcm[2 * i] | pcm[2 * i + 1] << 8);
            squares += sample * sample;
        }
        Add(squares, count);
    }

    /// <summary>A chunk's sum of squares, of samples in the 16-bit range, and how many there were.</summary>
    public void Add(double squares, int count)
    {
        lock (_gate)
        {
            _squares[_next] = squares;
            _counts[_next] = count;
            _next = (_next + 1) % Kept;
        }
    }

    public double Rms
    {
        get
        {
            lock (_gate)
            {
                var count = _counts.Sum();
                return count == 0 ? 0 : Math.Sqrt(_squares.Sum() / count) / 32768.0;
            }
        }
    }

    /// <summary>Forgets what it heard: a microphone turned off is not still talking.</summary>
    public void Clear()
    {
        lock (_gate)
        {
            Array.Clear(_squares);
            Array.Clear(_counts);
        }
    }
}
