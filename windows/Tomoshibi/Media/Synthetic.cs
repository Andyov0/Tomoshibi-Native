using System.Diagnostics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;

namespace Tomoshibi.Media;

/// <summary>
/// A camera and a microphone that are not there: a moving picture and a
/// steady tone, pushed into the core exactly as captured ones would be
/// (`push_planes` and `push_pcm`). What the tests publish, so that nothing
/// in a test ever captures a real room, and what a machine without devices
/// can still send.
///
/// The tone is sent, never played: the browser that receives it runs muted.
/// </summary>
internal sealed class Synthetic : IDisposable
{
    public const uint Width = 1280;
    public const uint Height = 720;
    private const int FramesPerSecond = 30;
    private const int Rate = 48_000;
    /// <summary>Ten milliseconds, the core's audio frame.</summary>
    private const int Chunk = Rate / 100;
    /// <summary>A frequency a spectrum finds at once, and nobody's voice.</summary>
    private const double Tone = 440.0;

    private readonly Thread _thread;
    private volatile bool _stopped;

    /// <summary>Where the picture goes, or nowhere; set and cleared as the camera is published and unpublished.</summary>
    public volatile VideoInput? Video;
    /// <summary>Where the tone goes, or nowhere.</summary>
    public volatile AudioInput? Audio;

    public long FramesPushed;
    public long ChunksPushed;

    public Synthetic()
    {
        _thread = new Thread(Run) { IsBackground = true, Name = "synthetic media", Priority = ThreadPriority.AboveNormal };
        _thread.Start();
    }

    private void Run()
    {
        var clock = Stopwatch.StartNew();
        long frames = 0, chunks = 0;
        var pcm = new byte[Chunk * 2];
        var (y, u, v) = (new byte[Width * Height], new byte[Width * Height / 4], new byte[Width * Height / 4]);
        double phase = 0;
        while (!_stopped)
        {
            try
            {
                var now = clock.Elapsed.TotalMilliseconds;
                // Everything that is due, so a late wake-up catches up rather
                // than sending less than real time.
                for (; chunks * 10 <= now; chunks++)
                {
                    if (Audio is not { } audio) continue;
                    for (var i = 0; i < Chunk; i++)
                    {
                        // A tenth of full scale: -20 dBFS.
                        var sample = (short)(Math.Sin(phase) * 3277);
                        phase += 2 * Math.PI * Tone / Rate;
                        pcm[2 * i] = (byte)sample;
                        pcm[2 * i + 1] = (byte)(sample >> 8);
                    }
                    phase %= 2 * Math.PI;
                    audio.PushPcm(pcm);
                    Interlocked.Increment(ref ChunksPushed);
                }
                if (frames * 1000.0 / FramesPerSecond <= now)
                {
                    if (Video is { } video)
                    {
                        Paint(y, u, v, frames);
                        video.PushPlanes(y, u, v, Width, Width / 2, 1, Width, Height, (long)(now * 1000), 0);
                        Interlocked.Increment(ref FramesPushed);
                    }
                    frames++;
                }
            }
            catch (Exception fault)
            {
                // An input unpublished between the check and the push.
                Log.Line($"synthetic media: {fault.GetType().Name}: {fault.Message}");
            }
            Thread.Sleep(2);
        }
    }

    /// <summary>
    /// Bands of light that slide down the picture and a bright bar that
    /// crosses it every four seconds, on a ground whose hue turns: anything
    /// that freezes or tears shows at once.
    /// </summary>
    private static void Paint(byte[] y, byte[] u, byte[] v, long frame)
    {
        var bar = (int)(frame * Width / (FramesPerSecond * 4) % Width);
        for (var row = 0; row < Height; row++)
        {
            var line = y.AsSpan(row * (int)Width, (int)Width);
            line.Fill((byte)(48 + (row + frame * 2) % 160));
            line.Slice(bar, Math.Min(96, (int)Width - bar)).Fill(235);
        }
        var turn = frame * 2 * Math.PI / (FramesPerSecond * 10);
        u.AsSpan().Fill((byte)(128 + 60 * Math.Cos(turn)));
        v.AsSpan().Fill((byte)(128 + 60 * Math.Sin(turn)));
    }

    public void Dispose()
    {
        _stopped = true;
        _thread.Join(500);
    }
}
