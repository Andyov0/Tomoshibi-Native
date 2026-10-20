using System.Buffers.Binary;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_protocol;
using Windows.Media.MediaProperties;
using Windows.Media.Transcoding;
using Windows.Storage;

namespace Tomoshibi.Media;

/// <summary>
/// A WAV file read from its sound's start, a block at a time: where the
/// samples begin, what they are, how many frames there are.
/// </summary>
internal sealed class WaveFile
{
    public readonly string Path;
    public readonly long DataOffset;
    public readonly long DataBytes;
    public readonly int Rate;
    public readonly int Channels;
    public readonly int Bits;
    public readonly bool Float;

    public long Frames => DataBytes / (Channels * Bits / 8);

    private WaveFile(string path, long offset, long bytes, int rate, int channels, int bits, bool isFloat)
    {
        Path = path;
        DataOffset = offset;
        DataBytes = bytes;
        Rate = rate;
        Channels = channels;
        Bits = bits;
        Float = isFloat;
    }

    /// <summary>Reads the chunks before the samples: <c>fmt </c> (plain or extensible) and where <c>data</c> starts.</summary>
    public static WaveFile Read(string path)
    {
        using var stream = File.OpenRead(path);
        Span<byte> head = stackalloc byte[12];
        stream.ReadExactly(head);
        if (!head[..4].SequenceEqual("RIFF"u8) || !head[8..12].SequenceEqual("WAVE"u8)) throw new IOException("not a WAV file");
        int rate = 0, channels = 0, bits = 0;
        var isFloat = false;
        Span<byte> chunk = stackalloc byte[8];
        while (stream.Position + 8 <= stream.Length)
        {
            stream.ReadExactly(chunk);
            var size = BinaryPrimitives.ReadUInt32LittleEndian(chunk[4..]);
            if (chunk[..4].SequenceEqual("fmt "u8))
            {
                var format = new byte[size];
                stream.ReadExactly(format);
                var tag = BinaryPrimitives.ReadUInt16LittleEndian(format);
                channels = BinaryPrimitives.ReadUInt16LittleEndian(format.AsSpan(2));
                rate = (int)BinaryPrimitives.ReadUInt32LittleEndian(format.AsSpan(4));
                bits = BinaryPrimitives.ReadUInt16LittleEndian(format.AsSpan(14));
                // WAVE_FORMAT_IEEE_FLOAT, or WAVE_FORMAT_EXTENSIBLE whose
                // subformat GUID begins with the same tag.
                isFloat = tag == 3 || (tag == 0xFFFE && size >= 26 && BinaryPrimitives.ReadUInt16LittleEndian(format.AsSpan(24)) == 3);
            }
            else if (chunk[..4].SequenceEqual("data"u8))
            {
                var bytes = Math.Min(size, stream.Length - stream.Position);
                if (channels == 0 || bits == 0) throw new IOException("a WAV file with its samples before its format");
                return new WaveFile(path, stream.Position, bytes, rate, channels, bits, isFloat);
            }
            else
            {
                stream.Seek(size + (size & 1), SeekOrigin.Current);
            }
        }
        throw new IOException("a WAV file with no samples");
    }
}

/// <summary>
/// A song ready for the call (Android's <c>DecodedSong</c>): two forms of
/// the same sound, each a file read as it plays. <see cref="Pcm"/> is 48 kHz
/// in two channels of 16 bits, what the core's Opus source takes and the
/// holder hears. <see cref="Original"/> is the file as it decodes at its own
/// rate, for the lossless stream beside the Opus track, as the web's lossless
/// sender reads the decoded file at the file's own rate (sound.ts
/// <c>fetchLibraryTrack</c>, <c>playDecoded</c>): only where a lossless
/// stream is offered. Read from disk rather than held, because a five-minute
/// song is fifty-odd megabytes at 48 kHz and over two hundred as floats at
/// 96 kHz, and the next song is prepared beside the one playing.
/// </summary>
internal sealed class DecodedSong : IDisposable
{
    public readonly WaveFile Pcm;
    public readonly WaveFile? Original;
    /// <summary>What the file is, as the panel says it: "FLAC · 24-bit · 44.1 kHz".</summary>
    public readonly string? Quality;
    private readonly string _folder;

    public DecodedSong(WaveFile pcm, WaveFile? original, string? quality, string folder)
    {
        Pcm = pcm;
        Original = original;
        Quality = quality;
        _folder = folder;
    }

    public long Frames => Pcm.Frames;

    /// <summary>The file's own samples a second, which the lossless stream runs at.</summary>
    public int Rate => Original?.Rate ?? 48_000;

    /// <summary>Lets the files go.</summary>
    public void Dispose()
    {
        try
        {
            Directory.Delete(_folder, recursive: true);
        }
        catch (Exception)
        {
            // Still open by the reader on the music's thread, which lets it
            // go a moment later; the folder is cleared at the next start.
        }
    }
}

/// <summary>
/// Fetching and decoding a library track (Android's MusicLoader.kt; sound.ts
/// <c>fetchLibraryTrack</c>, jukebox.ts <c>Desk.load</c>): the track's
/// description, the audio fetched with the app's session as the core would
/// send it, and the file decoded by the system's own decoders (FLAC, ALAC,
/// AAC, MP3, WAV) through Media Foundation's transcoder, which reads a file
/// by what its bytes are as the web's <c>decodeAudioData</c> does. Mono comes
/// out on both sides and more channels folded down by the system's mixer.
/// </summary>
internal static class MusicLoader
{
    public abstract record Outcome
    {
        public sealed record Ready(DecodedSong Song) : Outcome;

        /// <summary>The library's account may not play it.</summary>
        public sealed record Unavailable : Outcome;
    }

    private static readonly HttpClient Fetch = new() { Timeout = Timeout.InfiniteTimeSpan };

    /// <summary>Where songs are fetched and decoded to, cleared at the first song of each run of the app.</summary>
    private static readonly Lazy<string> Folder = new(() =>
    {
        var folder = Path.Combine(Path.GetTempPath(), "Tomoshibi-music");
        try
        {
            if (Directory.Exists(folder)) Directory.Delete(folder, recursive: true);
        }
        catch (Exception)
        {
        }
        Directory.CreateDirectory(folder);
        return folder;
    });

    /// <summary>Describes, fetches and decodes one song. Throws where it could not, and when cancelled, which is how a song taken off the queue is abandoned mid-download.</summary>
    public static async Task<Outcome> Load(Deployment deployment, string source, string id, MusicQuality quality, bool keepOriginal, CancellationToken cancel)
    {
        var audio = await deployment.MusicTrack(source, id, quality);
        if (audio == null) return new Outcome.Unavailable();
        cancel.ThrowIfCancellationRequested();
        var folder = Path.Combine(Folder.Value, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(folder);
        try
        {
            var fetched = Path.Combine(folder, "fetched");
            await Download(deployment.MusicAudioUrl(source, id, quality), deployment.CookieHeader(), fetched, cancel);
            var pcm = await Decode(fetched, Path.Combine(folder, "pcm48.wav"), original: false, cancel);
            WaveFile? original = null;
            if (keepOriginal)
            {
                try
                {
                    original = await Decode(fetched, Path.Combine(folder, "original.wav"), original: true, cancel);
                }
                catch (Exception fault) when (fault is not OperationCanceledException)
                {
                    // The Opus track still plays it; only the lossless
                    // stream goes without.
                    Log.Line($"music: no lossless reading: {fault.Message}");
                }
            }
            // The fetched file is let go with the song's folder: the
            // transcoder's StorageFile holds it open until it is collected,
            // and deleting it here failed the song that had just decoded.
            Log.Line($"music: decoded {pcm.Frames / 48} ms{(original != null ? $", lossless at {original.Rate} Hz" : "")}");
            return new Outcome.Ready(new DecodedSong(pcm, original, TomoshibiProtocolMethods.DescribeMusicAudio(audio), folder));
        }
        catch (Exception)
        {
            try
            {
                Directory.Delete(folder, recursive: true);
            }
            catch (Exception)
            {
            }
            throw;
        }
    }

    /// <summary>The audio, account-gated like everything under /api/music: the session is in the core's jar, so it is handed over as the header the core would send.</summary>
    private static async Task Download(string address, string? cookie, string to, CancellationToken cancel)
    {
        using var request = new HttpRequestMessage(HttpMethod.Get, address);
        if (cookie != null) request.Headers.TryAddWithoutValidation("Cookie", cookie);
        using var answer = await Fetch.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancel);
        if (!answer.IsSuccessStatusCode) throw new IOException($"the library answered {(int)answer.StatusCode}");
        await using var body = await answer.Content.ReadAsStreamAsync(cancel);
        await using var file = File.Create(to);
        await body.CopyToAsync(file, 64 * 1024, cancel);
    }

    /// <summary>
    /// The file decoded to a WAV: at 48 kHz in two channels of 16 bits for
    /// the Opus track, or at the file's own rate in two channels of 32-bit
    /// floats for the lossless stream, which holds every integer of a 16- or
    /// 24-bit file exactly, so the core's <c>to_int24</c> recovers them.
    /// </summary>
    private static async Task<WaveFile> Decode(string from, string to, bool original, CancellationToken cancel)
    {
        var source = await StorageFile.GetFileFromPathAsync(from);
        var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(to)!);
        var target = await folder.CreateFileAsync(Path.GetFileName(to), CreationCollisionOption.ReplaceExisting);
        var profile = MediaEncodingProfile.CreateWav(AudioEncodingQuality.High);
        profile.Video = null;
        if (original)
        {
            var found = await MediaEncodingProfile.CreateFromFileAsync(source);
            var rate = found.Audio?.SampleRate is > 0 and var said ? said : 48_000u;
            var floats = AudioEncodingProperties.CreatePcm(rate, 2, 32);
            floats.Subtype = MediaEncodingSubtypes.Float;
            profile.Audio = floats;
        }
        else
        {
            profile.Audio = AudioEncodingProperties.CreatePcm(48_000, 2, 16);
        }
        var transcoder = new MediaTranscoder { AlwaysReencode = true };
        var prepared = await transcoder.PrepareFileTranscodeAsync(source, target, profile);
        if (!prepared.CanTranscode) throw new IOException($"no decoder for the file ({prepared.FailureReason})");
        await prepared.TranscodeAsync().AsTask(cancel);
        var wave = WaveFile.Read(to);
        if (wave.Frames == 0) throw new IOException("nothing decoded from the file");
        if (!original && (wave.Rate != 48_000 || wave.Channels != 2 || wave.Bits != 16)) throw new IOException($"decoded at {wave.Rate} Hz, {wave.Channels} channels, {wave.Bits} bits");
        return wave;
    }
}
