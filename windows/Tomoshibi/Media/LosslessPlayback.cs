using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;

namespace Tomoshibi.Media;

/// <summary>
/// The lossless streams somebody's shared sound or music arrives in, played
/// beside the call's other sounds (Android's media/LosslessPlayback.kt, the
/// Apple apps' LosslessPlayback).
///
/// It has to play from the first call: the core asks for a stream whenever a
/// browser offers one, and holds that sound's ordinary Opus track back at the
/// server meanwhile, sink or no sink. A call that never played these heard
/// nothing at all of a song somebody shared.
///
/// Each stream is taken from its own rate to the player's through a
/// windowed-sinc filter (Resampler), at the level the web plays it at
/// (lossless-receiver.ts <c>levelOf</c>): the listener's own volume for that
/// person's sound, and for music the song desk's duck on top. The level is
/// eased rather than stepped, with the web's time constant of thirty
/// milliseconds, since a gain stepped every forty milliseconds under speech
/// is heard as clicks; a stream comes in from silence and goes out to it the
/// same way.
/// </summary>
internal sealed class LosslessPlayback : LosslessSink, IDisposable
{
    /// <summary>The web's time constant for a change of level (lossless-receiver.ts, <c>setTargetAtTime</c>).</summary>
    public const double EaseSeconds = 0.03;
    /// <summary>Long enough for the eased level to fall below a thousandth, with room.</summary>
    private static readonly TimeSpan FadeOut = TimeSpan.FromMilliseconds(300);

    private readonly Playback? _playback;
    private readonly object _gate = new();
    private readonly Dictionary<string, Stream> _streams = new();
    private readonly Dictionary<string, float> _levels = new();
    private readonly Dictionary<string, float> _ducks = new();

    public LosslessPlayback(Playback? playback) => _playback = playback;

    public static string Key(string identity, bool music) => identity + (music ? "/music" : "/screen");

    /// <summary>Samples for one stream, on a core thread.</summary>
    public void OnLossless(string identity, bool music, float[] samples, uint rate, uint channels)
    {
        var key = Key(identity, music);
        // The core drops packets at a rate nobody records at; this is the
        // second wall, as on Apple, because a block saying nought once
        // brought every listener down there.
        if (rate is < 8_000 or > 384_000 || channels is < 1 or > 2) return;
        Stream stream;
        var fresh = false;
        Stream? replaced = null;
        lock (_gate)
        {
            if (_streams.TryGetValue(key, out var existing) && !existing.Ending && existing.Rate == (int)rate && existing.Ring.Channels == (int)channels)
            {
                stream = existing;
            }
            else
            {
                stream = new Stream(key, (int)rate, (int)channels)
                {
                    Volume = _levels.TryGetValue(key, out var level) ? level : 1f,
                    Duck = music && _ducks.TryGetValue(identity, out var duck) ? duck : 1f,
                };
                _streams[key] = stream;
                replaced = existing;
                fresh = true;
            }
        }
        stream.Ring.Write(samples);
        if (replaced != null) EndLater(replaced);
        if (fresh)
        {
            _playback?.Attach(stream);
            Log.Line($"lossless {key} at {rate} Hz in {channels} channels");
        }
    }

    public void OnLosslessEnd(string identity, bool music)
    {
        Stream? stream;
        lock (_gate)
        {
            if (!_streams.Remove(Key(identity, music), out stream)) return;
        }
        EndLater(stream);
    }

    /// <summary>The listener's level for one person's sound, on the stream and on any that starts later.</summary>
    public void SetLevel(string identity, bool music, float level)
    {
        var key = Key(identity, music);
        lock (_gate)
        {
            _levels[key] = Math.Clamp(level, 0f, 1f);
            if (_streams.TryGetValue(key, out var stream)) stream.Volume = _levels[key];
        }
    }

    /// <summary>How far this person's music is turned down now, the duck and the fade-in together; kept for a stream that starts later.</summary>
    public void SetDuck(string identity, float factor)
    {
        lock (_gate)
        {
            _ducks[identity] = Math.Clamp(factor, 0f, 1f);
            if (_streams.TryGetValue(Key(identity, music: true), out var stream)) stream.Duck = _ducks[identity];
        }
    }

    /// <summary>Whether a stream is standing in for this person's sound now.</summary>
    public bool Playing(string identity, bool music)
    {
        lock (_gate) return _streams.ContainsKey(Key(identity, music));
    }

    /// <summary>Frames written and rendered of a stream since the last reading, for the debug log.</summary>
    public IReadOnlyList<string> Reading()
    {
        lock (_gate)
        {
            return _streams.Values.Select(s =>
            {
                var (written, rendered) = s.Ring.Take();
                return $"lossless {s.Key} written={written} rendered={rendered}";
            }).ToList();
        }
    }

    public void CloseAll()
    {
        List<Stream> all;
        lock (_gate)
        {
            all = _streams.Values.ToList();
            _streams.Clear();
        }
        foreach (var stream in all) EndLater(stream);
    }

    /// <summary>Fades the stream out, and takes it out of the player once it is silent.</summary>
    private void EndLater(Stream stream)
    {
        stream.Ending = true;
        _ = Task.Delay(FadeOut).ContinueWith(_ => _playback?.Detach(stream.Key), TaskScheduler.Default);
    }

    public void Dispose() => CloseAll();

    /// <summary>One stream, at its own rate, taken out at the player's through its own filter.</summary>
    private sealed class Stream : PulledSound
    {
        public readonly int Rate;
        // The Apple version's cushion: twenty milliseconds before it plays,
        // two hundred at most, settling back to forty.
        public readonly PcmRing Ring;
        private readonly Resampler _resampler;
        public volatile bool Ending;
        public volatile float Volume = 1f;
        public volatile float Duck = 1f;
        /// <summary>The level as it is now, easing toward the one asked for; from silence.</summary>
        private float _current;
        private readonly float _coefficient = (float)(1 - Math.Exp(-1.0 / (EaseSeconds * Playback.PulledRate)));
        private float[] _gains = Array.Empty<float>();

        public Stream(string key, int rate, int channels) : base(key)
        {
            Rate = rate;
            Ring = new PcmRing(channels, startFrames: rate / 50, ceilingFrames: rate / 5, settleFrames: rate / 25);
            _resampler = new Resampler(rate, Playback.PulledRate, channels, Ring.Read);
        }

        public override void Render(float[] stereo, int frames)
        {
            if (_gains.Length < frames) _gains = new float[frames];
            var target = Ending ? 0f : Volume * Duck;
            var level = _current;
            for (var frame = 0; frame < frames; frame++)
            {
                level += (target - level) * _coefficient;
                _gains[frame] = level;
            }
            _current = Math.Abs(level - target) < 1e-5f ? target : level;
            _resampler.Render(stereo, frames, 0f, _gains);
        }
    }
}
