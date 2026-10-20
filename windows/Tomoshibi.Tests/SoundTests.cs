using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * The ring every stream played here passes through between the core's
 * threads and the player's, and the conversion a lossless stream goes
 * through on its way to the speakers: Android's PcmRingTest and
 * ResamplerTest, case for case. What these guard is a sound that falls
 * further and further behind (the ring never shedding), one that stutters
 * from the first word (playing before it has a cushion), stale sound
 * replayed when it runs dry; and in the conversion, the haze a linear
 * interpolation left in a 44.1 kHz song, a level that wobbles with the
 * phase, and two clocks drifting apart.
 */
public class PcmRingTests
{
    /// <summary>Frames of a constant value, interleaved for the channels, as the core's little-endian bytes.</summary>
    private static byte[] Pcm(int frames, int channels, short value)
    {
        var out_ = new byte[frames * channels * 2];
        for (var i = 0; i < frames * channels; i++)
        {
            out_[i * 2] = (byte)(value & 0xff);
            out_[i * 2 + 1] = (byte)(value >> 8);
        }
        return out_;
    }

    [Fact]
    public void PlaysNothingUntilItHasItsCushionThenPlaysWhatCame()
    {
        var ring = new PcmRing(1, startFrames: 1440, ceilingFrames: 9600, settleFrames: 2880);
        ring.Write(Pcm(960, 1, 16384));
        var mix = new float[480 * 2];
        ring.Mix(mix, 480, 1f);
        Assert.All(mix, sample => Assert.Equal(0f, sample));
        Assert.Equal(960, ring.Buffered);
        ring.Write(Pcm(480, 1, 16384));
        ring.Mix(mix, 480, 1f);
        // Mono goes to both sides, at the level it came.
        Assert.Equal(0.5f, mix[0], 6);
        Assert.Equal(0.5f, mix[1], 6);
        Assert.Equal(960, ring.Buffered);
    }

    [Fact]
    public void RunningDryIsSilenceAndAFreshCushionBeforePlayingAgain()
    {
        var ring = new PcmRing(1, startFrames: 480, ceilingFrames: 9600, settleFrames: 2880);
        ring.Write(Pcm(480, 1, 8192));
        var mix = new float[960 * 2];
        ring.Mix(mix, 960, 1f);
        // The half it had, then nothing rather than anything stale.
        Assert.Equal(0.25f, mix[0], 6);
        Assert.Equal(0f, mix[480 * 2]);
        var dry = new float[480 * 2];
        ring.Mix(dry, 480, 1f);
        Assert.All(dry, sample => Assert.Equal(0f, sample));
        ring.Write(Pcm(100, 1, 8192));
        var again = new float[480 * 2];
        ring.Mix(again, 480, 1f);
        Assert.All(again, sample => Assert.Equal(0f, sample));
    }

    [Fact]
    public void GatheringPastTheCeilingSettlesBackRatherThanFallingBehind()
    {
        var ring = new PcmRing(1, startFrames: 1440, ceilingFrames: 9600, settleFrames: 2880);
        ring.Write(Pcm(9601, 1, 1000));
        Assert.Equal(2880, ring.Buffered);
    }

    [Fact]
    public void AGainAndAStereoTrackAreMixedAsTheyAre()
    {
        var ring = new PcmRing(2, startFrames: 1, ceilingFrames: 9600, settleFrames: 2880);
        // Left 0.5, right -0.25.
        ring.Write(new byte[] { 0, 0x40, 0, 0xE0 });
        var mix = new float[2];
        ring.Mix(mix, 1, 0.5f);
        Assert.Equal(0.25f, mix[0], 6);
        Assert.Equal(-0.125f, mix[1], 6);
    }

    [Fact]
    public void ATrackInOtherChannelsThanAnnouncedIsFoldedNotMisread()
    {
        var stereo = new PcmRing(2, startFrames: 1, ceilingFrames: 9600, settleFrames: 2880);
        stereo.Write(Pcm(4, 1, 16384), sourceChannels: 1);
        Assert.Equal(4, stereo.Buffered);
        var mix = new float[8];
        stereo.Mix(mix, 4, 1f);
        Assert.All(mix, sample => Assert.Equal(0.5f, sample, 6));
    }
}

public class ResamplerTests
{
    /// <summary>A source of so many interleaved frames made by a function, and how many have been taken from it.</summary>
    private sealed class Tone(int channels, int frames, Func<int, int, float> sample)
    {
        public int Taken;
        public readonly Func<int, int, float> Sample = sample;

        public int Read(float[] into, int offset, int count)
        {
            var n = Math.Min(count, frames - Taken);
            for (var f = 0; f < n; f++)
                for (var c = 0; c < channels; c++) into[offset + f * channels + c] = Sample(Taken + f, c);
            Taken += n;
            return n;
        }
    }

    /// <summary>The amplitude of a frequency in one channel of interleaved stereo, by a Goertzel filter.</summary>
    private static double Amplitude(float[] stereo, int channel, int from, int frames, double frequency, int rate)
    {
        var w = 2 * Math.PI * frequency / rate;
        var k = 2 * Math.Cos(w);
        double s1 = 0, s2 = 0;
        for (var n = 0; n < frames; n++)
        {
            var s = stereo[(from + n) * 2 + channel] + k * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        var power = s1 * s1 + s2 * s2 - k * s1 * s2;
        return 2 * Math.Sqrt(power) / frames;
    }

    private static float[] Render(Resampler resampler, int frames)
    {
        var out_ = new float[frames * 2];
        // In the player's own pieces of ten milliseconds.
        var done = 0;
        while (done < frames)
        {
            var piece = Math.Min(480, frames - done);
            var block = new float[piece * 2];
            resampler.Render(block, piece, 1f);
            Array.Copy(block, 0, out_, done * 2, piece * 2);
            done += piece;
        }
        return out_;
    }

    [Fact]
    public void AStreamAtThePlayersRateGoesThroughUntouched()
    {
        var tone = new Tone(2, 48_000, (n, c) => ((n * 7 + c * 3) % 101 - 50) / 64f);
        var out_ = Render(new Resampler(48_000, 48_000, 2, tone.Read), 4_800);
        for (var n = 0; n < 4_800; n++)
            for (var c = 0; c < 2; c++) Assert.Equal(tone.Sample(n, c), out_[n * 2 + c]);
    }

    [Fact]
    public void AConstantStaysConstantAtEveryPhase()
    {
        var tone = new Tone(1, 200_000, (_, _) => 0.25f);
        var out_ = Render(new Resampler(44_100, 48_000, 1, tone.Read), 48_000);
        // Past the filter's first length, where it is still filling from silence.
        for (var n = 200; n < 48_000; n++)
        {
            Assert.True(Math.Abs(out_[n * 2] - 0.25f) <= 2e-6f, $"left at {n} is {out_[n * 2]}");
            Assert.Equal(out_[n * 2], out_[n * 2 + 1]);
        }
    }

    [Fact]
    public void AHighToneKeepsItsLevelAndFoldsNothingBack()
    {
        // 15 kHz at 44.1 kHz: its first image, at 29.1 kHz, lands at 18.9 kHz
        // once taken to 48 kHz, well inside what anybody hears.
        const int rate = 44_100;
        var tone = new Tone(2, rate * 3, (n, c) => (float)(0.5 * Math.Sin(2 * Math.PI * 15_000 * n / rate)) * (c == 0 ? 1f : -1f));
        var out_ = Render(new Resampler(rate, 48_000, 2, tone.Read), 48_000 * 2);
        const int from = 4_800, frames = 48_000;
        var kept = Amplitude(out_, 0, from, frames, 15_000, 48_000);
        var folded = Amplitude(out_, 0, from, frames, 18_900, 48_000);
        Assert.True(Math.Abs(kept - 0.5) <= 0.5 * 0.002, $"15 kHz came out at {kept}");
        Assert.True(folded / 0.5 < 1e-4, $"the image at 18.9 kHz is {20 * Math.Log10(folded / 0.5):F1} dB");
        Assert.True(Math.Abs(-out_[(from + 10) * 2] - out_[(from + 10) * 2 + 1]) <= 1e-6f, "the other channel too, inverted");
    }

    [Fact]
    public void TheTwoClocksNeverDriftApart()
    {
        // 147 input frames to every 160 out, exactly: ten seconds out take
        // ten seconds in, give or take the filter's length in hand.
        var tone = new Tone(2, 44_100 * 20, (_, _) => 0f);
        var resampler = new Resampler(44_100, 48_000, 2, tone.Read);
        Render(resampler, 480_000);
        Assert.InRange(tone.Taken, 441_000, 441_000 + 1_200);
    }

    [Fact]
    public void ADryStreamIsSilenceRatherThanAStall()
    {
        var tone = new Tone(2, 0, (_, _) => 1f);
        var out_ = Render(new Resampler(44_100, 48_000, 2, tone.Read), 960);
        Assert.All(out_, sample => Assert.Equal(0f, sample));
    }

    [Fact]
    public void RatiosAreHeldAsWholeNumbers()
    {
        Assert.Equal((160, 147), Resampler.Fraction(44_100, 48_000));
        Assert.Equal((1, 2), Resampler.Fraction(96_000, 48_000));
        Assert.Equal((3, 2), Resampler.Fraction(32_000, 48_000));
        Assert.Equal((640, 147), Resampler.Fraction(11_025, 48_000));
        // A rate nobody records at comes to the nearest fraction that fits, within a millionth.
        var (phases, step) = Resampler.Fraction(44_101, 48_000);
        Assert.True(phases <= Resampler.MostPhases);
        Assert.True(Math.Abs(44_101.0 / 48_000 - (double)step / phases) <= 1e-6);
    }
}

/*
 * The player's pulled sounds, as a lossless stream replaced under one key
 * puts them: a song at 48 kHz, then one at 44.1 kHz from the same person, is
 * a new stream under the same key while the old one fades out, and the old
 * one is taken out 300 ms later. Held by key, the new one was taken out with
 * it and the rest of the song was never heard.
 */
public class PulledNodesTests
{
    private sealed class Stream(string key) : PulledSound(key)
    {
        public override void Render(float[] stereo, int frames) { }
    }

    [Fact]
    public void AStreamReplacedUnderItsKeyFadesBesideTheNewOneAndGoesAlone()
    {
        var nodes = new PulledNodes<string>();
        var first = new Stream("lee/music");
        nodes.Add(first, "48 kHz");
        var second = new Stream("lee/music");
        nodes.Add(second, "44.1 kHz");
        // Both play for the fade: the old one going out, the new one coming in.
        Assert.Equal(2, nodes.Count);
        Assert.True(nodes.Remove(first, out var stopped));
        Assert.Equal("48 kHz", stopped);
        Assert.Same(second, Assert.Single(nodes.Sounds));
        Assert.Equal("44.1 kHz", Assert.Single(nodes.Nodes));
    }

    [Fact]
    public void ASoundTakenOutTwiceTakesNothingElseTheSecondTime()
    {
        var nodes = new PulledNodes<string>();
        var stopped = new Stream("lee/screen");
        var started = new Stream("lee/screen");
        nodes.Add(stopped, "stopped");
        Assert.True(nodes.Remove(stopped, out _));
        nodes.Add(started, "started again");
        // A share stopped and started again at once: the end of the first
        // arrives after the second has begun.
        Assert.False(nodes.Remove(stopped, out _));
        Assert.Same(started, Assert.Single(nodes.Sounds));
    }
}
