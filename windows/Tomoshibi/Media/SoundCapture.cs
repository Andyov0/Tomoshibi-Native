using System.Diagnostics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using uniffi.tomoshibi_core;

namespace Tomoshibi.Media;

/// <summary>
/// The sound a share sends, captured once and handed to whichever track
/// carries it now (Android's PlaybackCapture): the screen's own sound, or
/// sound shared on its own, which takes it over while it runs. Sent as
/// 48 kHz stereo to the track, and the same samples as floats to the lossless
/// stream beside sound shared on its own, where one is offered.
/// </summary>
internal abstract class SoundCapture : IDisposable
{
    /// <summary>The track the sound goes to now, or nowhere.</summary>
    public volatile AudioInput? Sink;
    /// <summary>Whether the lossless stream beside the track is fed too (sound shared on its own, offered without loss).</summary>
    public volatile bool Lossless;
    public long ChunksPushed;
    private byte[] _pcm = Array.Empty<byte>();
    private readonly Stopwatch _clock = Stopwatch.StartNew();

    /// <summary>Interleaved stereo floats at 48 kHz, from the capture's own thread.</summary>
    protected void Heard(float[] floats, int frames)
    {
        if (Sink is not { } sink) return;
        var count = frames * Loopback.Channels;
        if (_pcm.Length < count * 2) _pcm = new byte[count * 2];
        for (var i = 0; i < count; i++)
        {
            var sample = (short)Math.Clamp((int)(floats[i] * 32767f), short.MinValue, short.MaxValue);
            _pcm[2 * i] = (byte)sample;
            _pcm[2 * i + 1] = (byte)(sample >> 8);
        }
        sink.PushPcm(count * 2 == _pcm.Length ? _pcm : _pcm.AsSpan(0, count * 2).ToArray());
        if (Lossless) sink.PushLossless(floats.AsSpan(0, count).ToArray(), (uint)Loopback.Channels, (uint)Loopback.Rate, (long)_clock.Elapsed.TotalMicroseconds);
        Interlocked.Increment(ref ChunksPushed);
    }

    public abstract void Dispose();
}

/// <summary>
/// What the computer plays, every app's sound but this one's: the screen
/// shared with its sound, or the sound shared on its own, without the call
/// coming back to everybody in it a beat late (Loopback).
/// </summary>
internal sealed class SystemSound : SoundCapture
{
    private readonly Loopback _loopback;

    private SystemSound(Func<SoundCapture, Loopback> start) => _loopback = start(this);

    /// <summary>Every process's sound but this one's and its children's.</summary>
    public static SystemSound Start() =>
        new(capture => Loopback.Start((uint)Environment.ProcessId, include: false, ((SystemSound)capture).Heard));

    public override void Dispose() => _loopback.Dispose();
}

/// <summary>
/// A shared sound that is not there: a chord whose two sides differ, 523 Hz
/// on the left and 659 Hz on the right at -20 dBFS, so that a browser in the
/// test can tell that the sound arrived in stereo as well as that it
/// arrived. Sent, never played: the browser that receives it runs muted.
/// </summary>
internal sealed class SyntheticSound : SoundCapture
{
    public const double Left = 523.25;
    public const double Right = 659.25;
    private readonly Thread _thread;
    private volatile bool _stopped;

    public SyntheticSound()
    {
        _thread = new Thread(Run) { IsBackground = true, Name = "synthetic shared sound", Priority = ThreadPriority.AboveNormal };
        _thread.Start();
    }

    private void Run()
    {
        var clock = Stopwatch.StartNew();
        long chunks = 0;
        const int chunk = Loopback.Rate / 100;
        var floats = new float[chunk * Loopback.Channels];
        double left = 0, right = 0;
        while (!_stopped)
        {
            for (; chunks * 10 <= clock.Elapsed.TotalMilliseconds; chunks++)
            {
                for (var i = 0; i < chunk; i++)
                {
                    floats[2 * i] = (float)(Math.Sin(left) * 0.1);
                    floats[2 * i + 1] = (float)(Math.Sin(right) * 0.1);
                    left += 2 * Math.PI * Left / Loopback.Rate;
                    right += 2 * Math.PI * Right / Loopback.Rate;
                }
                left %= 2 * Math.PI;
                right %= 2 * Math.PI;
                try
                {
                    Heard(floats, chunk);
                }
                catch (Exception fault)
                {
                    Log.Line($"synthetic shared sound: {fault.Message}");
                }
            }
            Thread.Sleep(2);
        }
    }

    public override void Dispose()
    {
        _stopped = true;
        _thread.Join(500);
    }
}
