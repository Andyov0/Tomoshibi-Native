using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;

namespace Tomoshibi.Media;

/// <summary>
/// One remote sound, arriving from the core as 10 ms of 48 kHz PCM
/// (`attach_pcm`): measured always, played unless the launch is muted.
/// </summary>
internal sealed class RemoteSound : PcmSink, IDisposable
{
    public readonly string Sid;
    private readonly Playback? _playback;
    private readonly object _gate = new();
    private long _chunks;
    private double _squares;
    private long _samples;
    private long _chunksSeen;

    public RemoteSound(string sid, Playback? playback)
    {
        Sid = sid;
        _playback = playback;
    }

    public void OnPcm(byte[] pcm, uint sampleRate, uint channels)
    {
        double squares = 0;
        var count = pcm.Length / 2;
        for (var i = 0; i < count; i++)
        {
            double sample = (short)(pcm[2 * i] | pcm[2 * i + 1] << 8);
            squares += sample * sample;
        }
        lock (_gate)
        {
            _chunks++;
            _squares += squares;
            _samples += count;
        }
        _playback?.Play(Sid, pcm, sampleRate, channels);
    }

    /// <summary>Chunks since the last reading, and their level in dBFS (the energy a browser's stats would show as loudness).</summary>
    public (long Total, long Since, double Dbfs) Reading()
    {
        lock (_gate)
        {
            var since = _chunks - _chunksSeen;
            _chunksSeen = _chunks;
            var rms = _samples > 0 ? Math.Sqrt(_squares / _samples) / 32768.0 : 0;
            _squares = 0;
            _samples = 0;
            return (_chunks, since, rms > 0 ? 20 * Math.Log10(rms) : double.NegativeInfinity);
        }
    }

    public void Dispose() => _playback?.Forget(Sid);
}
