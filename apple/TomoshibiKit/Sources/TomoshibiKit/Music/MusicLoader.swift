import AVFoundation
import Foundation
import TomoshibiCore

/// A song ready for the call.
///
/// Two forms of the same sound. `samples` is 48 kHz, 16-bit, two channels
/// interleaved: what the core's Opus source takes, and what the holder's own
/// speakers play. `original` is the file as it decodes, at its own rate, for
/// the lossless stream beside the Opus track: the web plays the decoded file
/// through a context at the file's own rate and its lossless sender reads it
/// there (sound.ts `fetchLibraryTrack`, `playDecoded`), so a listener hears
/// the file's samples and not a resampling of them. Only where a lossless
/// stream is offered.
struct DecodedSong: Sendable {
    let samples: [Int16]
    let original: OriginalStream?
    /// The file's own samples a second, which the lossless stream runs at.
    let rate: UInt32
    /// What the file is, as the panel says it: "FLAC · 24-bit · 44.1 kHz".
    let quality: String?

    var frames: Int { samples.count / 2 }
}

/// The file's own samples for the lossless stream, two channels of floats
/// interleaved, read from the file a few seconds ahead of where the stream
/// is rather than held whole.
///
/// They were held whole: the length of the song in floats at the file's own
/// rate, a hundred megabytes for five minutes of CD audio and over two
/// hundred at 96 kHz, for the song playing and again for the next one
/// prepared beside it. On a phone that is the memory the system ends an app
/// for. Now the fetched file is read from for as long as the song is
/// wanted, the reading happens on a queue of its own, and the music's clock
/// takes each 10 ms of it from what was read, never waiting on the disk or
/// the decoder. The samples are the decoder's floats exactly as they were.
///
/// The file's name is removed as soon as it is open, and what is read is
/// read through the open file: kept by name until the song was let go, every
/// song of a call that ended without that -- the app quit, or killed --
/// stayed behind in the temporary folder, tens of megabytes each.
final class OriginalStream: @unchecked Sendable {
    let rate: UInt32
    /// How much is read ahead, and how much at a time, in seconds.
    static let ahead = 4.0
    static let piece: AVAudioFrameCount = 16_384

    private let file: AVAudioFile
    private let input: AVAudioPCMBuffer
    /// More than two channels folded to two, as the web's two-channel
    /// destination folds them.
    private let folding: (AVAudioConverter, AVAudioPCMBuffer)?
    private let queue = DispatchQueue(label: "app.tomoshibi.music.original", qos: .userInitiated)
    private let lock = NSLock()
    /// What has been read, as a ring: frames `taken..<read` of the file.
    private var ring: [Float]
    private let capacity: Int
    private var taken = 0
    private var read = 0
    private var reading = false
    private var finished = false

    /// Takes over `url`, whose name is removed once it is open, and reads
    /// the first seconds at once, so the stream begins with the song.
    init(url: URL) throws {
        let file = try AVAudioFile(forReading: url)
        try? FileManager.default.removeItem(at: url)
        let source = file.processingFormat
        guard source.sampleRate > 0, source.channelCount > 0,
              let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: Self.piece)
        else { throw CocoaError(.fileReadCorruptFile) }
        if source.channelCount > 2,
           let stereo = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: source.sampleRate, channels: 2, interleaved: false),
           let fold = AVAudioConverter(from: source, to: stereo),
           let into = AVAudioPCMBuffer(pcmFormat: stereo, frameCapacity: Self.piece) {
            fold.downmix = true
            folding = (fold, into)
        } else {
            folding = nil
        }
        self.file = file
        self.input = input
        rate = UInt32(source.sampleRate.rounded())
        capacity = max(Int(Self.piece) * 2, Int(source.sampleRate * Self.ahead))
        ring = [Float](repeating: 0, count: capacity * 2)
        fill()
    }

    /// Up to `count` frames from where the stream is, into `into`, and how
    /// many there were: fewer only at the end of the file, or if the reading
    /// fell behind, which a decoder many times faster than playing does not.
    /// Asks for more to be read once less than half the ring is left.
    func take(_ count: Int, into: UnsafeMutableBufferPointer<Float>) -> Int {
        lock.withLock {
            let have = min(count, read - taken, into.count / 2)
            ring.withUnsafeBufferPointer { all in
                for frame in 0..<max(0, have) {
                    let at = ((taken + frame) % capacity) * 2
                    into[frame * 2] = all[at]
                    into[frame * 2 + 1] = all[at + 1]
                }
            }
            taken += max(0, have)
            if !reading, !finished, read - taken < capacity / 2 {
                reading = true
                queue.async { [weak self] in self?.fill() }
            }
            return max(0, have)
        }
    }

    /// Reads until the ring is full or the file ends. On the queue, or from
    /// the initialiser before anything else can touch the file.
    private func fill() {
        while true {
            let room = lock.withLock { capacity - (read - taken) }
            guard room >= Int(Self.piece) else { break }
            do {
                try file.read(into: input, frameCount: Self.piece)
            } catch {
                input.frameLength = 0
            }
            var from = input
            if let (fold, into) = folding, input.frameLength > 0 {
                if (try? fold.convert(to: into, from: input)) != nil { from = into } else { input.frameLength = 0 }
            }
            let frames = Int(from.frameLength)
            guard frames > 0, let planes = from.floatChannelData else {
                lock.withLock { finished = true }
                break
            }
            let left = planes[0]
            let right = from.format.channelCount > 1 ? planes[1] : planes[0]
            lock.withLock {
                ring.withUnsafeMutableBufferPointer { all in
                    for frame in 0..<frames {
                        let at = ((read + frame) % capacity) * 2
                        all[at] = left[frame]
                        all[at + 1] = right[frame]
                    }
                }
                read += frames
            }
        }
        lock.withLock { reading = false }
    }
}

/// Fetching and decoding a library track (sound.ts `fetchLibraryTrack`,
/// jukebox.ts `Desk.load`).
enum MusicLoader {
    enum Outcome: Sendable {
        case ready(DecodedSong)
        /// The library's account may not play it.
        case unavailable
    }

    /// Describes, fetches and decodes one song. Throws where it could not be
    /// fetched or decoded, and when the task is cancelled, which is how a
    /// song taken off the queue is abandoned mid-download.
    ///
    /// Not isolated to anything, so the decoding runs off the main thread.
    static func load(source: String, id: String, quality: MusicQuality, keepOriginal: Bool,
                     from deployment: Deployment) async throws -> Outcome {
        guard let audio = try await deployment.musicTrack(source: source, id: id, quality: quality) else {
            return .unavailable
        }
        try Task.checkCancellation()

        var request = URLRequest(url: try url(deployment.musicAudioUrl(source: source, id: id, quality: quality)))
        // The audio is account-gated like everything under /api/music, and the
        // session is in the core's jar, not in URLSession's: handed over as
        // the header the core would send, and URLSession kept from adding or
        // keeping cookies of its own.
        request.httpShouldHandleCookies = false
        if let cookie = deployment.cookieHeader() {
            request.setValue(cookie, forHTTPHeaderField: "Cookie")
        }
        let (downloaded, response) = try await URLSession.shared.download(for: request)
        defer { try? FileManager.default.removeItem(at: downloaded) }
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw CocoaError(.fileReadUnknown)
        }

        let decoded = try decodeFile(at: downloaded, said: audio.format, mime: http.mimeType, keepOriginal: keepOriginal)
        return .ready(DecodedSong(samples: decoded.samples, original: decoded.original, rate: decoded.rate,
                                  quality: describeMusicAudio(audio: audio)))
    }

    private static func url(_ text: String) throws -> URL {
        guard let url = URL(string: text) else { throw URLError(.badURL) }
        return url
    }

    struct Decoded {
        let samples: [Int16]
        let original: OriginalStream?
        let rate: UInt32
        /// What the file turned out to be, by its first bytes.
        let kind: String
        let file: AVAudioFormat
    }

    /// Names a fetched file for what it is and decodes it.
    ///
    /// Core Audio opens a file by its extension on the systems the apps still
    /// support: a FLAC in a file called .tmp was not opened at all, and every
    /// track used to be named either .flac or .mp3, so an AAC, an ALAC, a WAV
    /// or an AIFF from the library failed to decode and was skipped. The web
    /// hands the bytes to `decodeAudioData`, which reads them by what they
    /// are; so are they here, by their first bytes, with what the library
    /// said and the answer's type as the guess where those say nothing.
    static func decodeFile(at downloaded: URL, said format: String?, mime: String?, keepOriginal: Bool) throws -> Decoded {
        let head = (try? FileHandle(forReadingFrom: downloaded)).flatMap { handle -> Data? in
            defer { try? handle.close() }
            return try? handle.read(upToCount: 64)
        } ?? Data()
        let kind = sniff(head) ?? guess(format: format, mime: mime) ?? "mp3"
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("tomoshibi-music-\(UUID().uuidString)")
            .appendingPathExtension(kind)
        // Moved, not copied: whoever handed it over removes what is left at
        // the old name, which is then nothing. The lossless stream opens its
        // own reading of it before the name goes.
        try FileManager.default.moveItem(at: downloaded, to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let opened = try AVAudioFile(forReading: file)
        let original = try keepOriginal ? OriginalStream(url: file) : nil
        let samples = try decode(opened)
        return Decoded(samples: samples, original: original, rate: UInt32(opened.processingFormat.sampleRate.rounded()),
                       kind: kind, file: opened.fileFormat)
    }

    /// What a file is by its first bytes, as an extension Core Audio knows.
    static func sniff(_ head: Data) -> String? {
        let bytes = [UInt8](head)
        func at(_ offset: Int, _ text: String) -> Bool {
            let want = Array(text.utf8)
            return bytes.count >= offset + want.count && Array(bytes[offset..<offset + want.count]) == want
        }
        if at(0, "fLaC") { return "flac" }
        if at(0, "RIFF") && at(8, "WAVE") || at(0, "RF64") { return "wav" }
        if at(0, "FORM") && (at(8, "AIFF") || at(8, "AIFC")) { return "aiff" }
        if at(4, "ftyp") { return "m4a" }
        if at(0, "caff") { return "caf" }
        if at(0, "OggS") { return at(28, "OpusHead") ? "opus" : "ogg" }
        if at(0, "ID3") { return "mp3" }
        // A frame sync: an ADTS AAC frame has layer bits 00, an MPEG audio
        // frame anything else.
        if bytes.count >= 2, bytes[0] == 0xFF, bytes[1] & 0xE0 == 0xE0 {
            return bytes[1] & 0x06 == 0 ? "aac" : "mp3"
        }
        return nil
    }

    /// The extension the library's word or the answer's type points to.
    static func guess(format: String?, mime: String?) -> String? {
        switch format?.lowercased() {
        case "flac": return "flac"
        case "mp3": return "mp3"
        case "aac": return "aac"
        case "m4a", "mp4", "alac": return "m4a"
        case "wav": return "wav"
        case "aif", "aiff": return "aiff"
        case "ogg": return "ogg"
        case "opus": return "opus"
        default: break
        }
        switch mime?.lowercased() {
        case "audio/flac", "audio/x-flac": return "flac"
        case "audio/mpeg", "audio/mp3": return "mp3"
        case "audio/aac", "audio/aacp": return "aac"
        case "audio/mp4", "audio/x-m4a", "audio/m4a": return "m4a"
        case "audio/wav", "audio/x-wav", "audio/wave": return "wav"
        case "audio/aiff", "audio/x-aiff": return "aiff"
        case "audio/ogg": return "ogg"
        case "audio/opus": return "opus"
        default: return nil
        }
    }

    /// The whole file at 48 kHz in two channels.
    ///
    /// Read at the file's own rate and converted once, here, with the best
    /// converter the system has. The web decodes at the file's rate and lets
    /// the browser bring it to 48 kHz on the way into Opus, which runs at 48
    /// kHz whatever it is given; the core takes 48 kHz and nothing else, so
    /// the same step happens before the samples are handed over instead.
    /// Converted in pieces, so a long file is never in memory as floats.
    ///
    /// The lossless stream's samples (`OriginalStream`) are the decoder's
    /// floats as they came. Core Audio scales both signs of a 16- or 24-bit
    /// sample by the same power of two, so the core's one-scale `to_int24`
    /// recovers every integer of the file exactly; Chrome's asymmetric scale
    /// is why the web needs its own recovery for a 16-bit file (lossless.ts
    /// `decodedToInt24`), and why this does not.
    static func decode(_ file: AVAudioFile) throws -> [Int16] {
        let source = file.processingFormat
        guard source.sampleRate > 0, source.channelCount > 0,
              let target = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48_000, channels: 2, interleaved: true),
              let converter = AVAudioConverter(from: source, to: target)
        else { throw CocoaError(.fileReadCorruptFile) }
        converter.sampleRateConverterQuality = AVAudioQuality.max.rawValue
        converter.sampleRateConverterAlgorithm = AVSampleRateConverterAlgorithm_Mastering
        if source.channelCount == 1 {
            // Left alone, a mono file becomes a left channel and silence.
            converter.channelMap = [0, 0]
        } else if source.channelCount > 2 {
            converter.downmix = true
        }

        let chunk: AVAudioFrameCount = 65_536
        let ratio = 48_000 / source.sampleRate
        guard let input = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: chunk),
              let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(Double(chunk) * ratio) + 4096)
        else { throw CocoaError(.fileReadCorruptFile) }

        var samples: [Int16] = []
        samples.reserveCapacity((Int(Double(file.length) * ratio) + 4096) * 2)
        var finished = false
        while true {
            if Task.isCancelled { throw CancellationError() }
            output.frameLength = 0
            var failure: NSError?
            let status = converter.convert(to: output, error: &failure) { _, said in
                if !finished {
                    do {
                        try file.read(into: input, frameCount: chunk)
                    } catch {
                        input.frameLength = 0
                    }
                    if input.frameLength > 0 {
                        said.pointee = .haveData
                        return input
                    }
                    finished = true
                }
                said.pointee = .endOfStream
                return nil
            }
            if status == .error { throw failure ?? CocoaError(.fileReadCorruptFile) }
            if output.frameLength > 0, let data = output.int16ChannelData {
                samples.append(contentsOf: UnsafeBufferPointer(start: data[0], count: Int(output.frameLength) * 2))
            }
            if status == .endOfStream { break }
        }
        guard !samples.isEmpty else { throw CocoaError(.fileReadCorruptFile) }
        return samples
    }

    /// How a file is said in the panel when no library described it: the
    /// library's words for what the file itself says (music.ts `describeAudio`).
    static func describe(_ decoded: Decoded) -> String {
        let description = decoded.file.streamDescription.pointee
        var bits = UInt32(description.mBitsPerChannel)
        if bits == 0 {
            // FLAC and ALAC carry their source depth in the format's flags.
            switch description.mFormatFlags {
            case kAppleLosslessFormatFlag_16BitSourceData: bits = 16
            case kAppleLosslessFormatFlag_20BitSourceData: bits = 20
            case kAppleLosslessFormatFlag_24BitSourceData: bits = 24
            case kAppleLosslessFormatFlag_32BitSourceData: bits = 32
            default: break
            }
        }
        return describeMusicAudio(audio: MusicAudio(format: decoded.kind, rate: decoded.rate,
                                                    channels: UInt32(decoded.file.channelCount), bits: bits, tier: ""))
    }
}
