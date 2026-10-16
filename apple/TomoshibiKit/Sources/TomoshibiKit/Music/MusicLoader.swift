import AVFoundation
import Foundation
import TomoshibiCore

/// A song ready for the call.
///
/// Two forms of the same sound. `samples` is 48 kHz, 16-bit, two channels
/// interleaved: what the core's Opus source takes, and what the holder's own
/// speakers play. `original` is the file as it decoded, at its own rate, two
/// channels of floats interleaved, for the lossless stream beside the Opus
/// track: the web plays the decoded file through a context at the file's own
/// rate and its lossless sender reads it there (sound.ts `fetchLibraryTrack`,
/// `playDecoded`), so a listener hears the file's samples and not a
/// resampling of them. Only kept where a lossless stream is offered; a
/// five-minute song is a hundred megabytes of floats.
struct DecodedSong: Sendable {
    let samples: [Int16]
    let original: [Float]?
    /// The file's own samples a second, which the lossless stream runs at.
    let rate: UInt32
    /// What the file is, as the panel says it: "FLAC · 24-bit · 44.1 kHz".
    let quality: String?

    var frames: Int { samples.count / 2 }
    var originalFrames: Int { (original?.count ?? 0) / 2 }
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
        let original: [Float]?
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
        // the old name, which is then nothing.
        try FileManager.default.moveItem(at: downloaded, to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let opened = try AVAudioFile(forReading: file)
        let (samples, original) = try decode(opened, keepOriginal: keepOriginal)
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

    /// The whole file at 48 kHz in two channels, and, asked for, the file's
    /// own samples in two channels at its own rate.
    ///
    /// Read at the file's own rate and converted once, here, with the best
    /// converter the system has. The web decodes at the file's rate and lets
    /// the browser bring it to 48 kHz on the way into Opus, which runs at 48
    /// kHz whatever it is given; the core takes 48 kHz and nothing else, so
    /// the same step happens before the samples are handed over instead.
    /// Converted in pieces, so a long file is never in memory twice over as
    /// floats.
    ///
    /// The original is the decoder's floats as they came. Core Audio scales
    /// both signs of a 16- or 24-bit sample by the same power of two, so the
    /// core's one-scale `to_int24` recovers every integer of the file
    /// exactly; Chrome's asymmetric scale is why the web needs its own
    /// recovery for a 16-bit file (lossless.ts `decodedToInt24`), and why
    /// this does not.
    static func decode(_ file: AVAudioFile, keepOriginal: Bool) throws -> ([Int16], [Float]?) {
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

        // The original in two channels: a mono file in both, as the web's
        // two-channel destination upmixes it, and more than two folded down
        // as its speakers' rules fold them.
        var original: [Float] = []
        if keepOriginal { original.reserveCapacity(Int(file.length) * 2) }
        var folding: (AVAudioConverter, AVAudioPCMBuffer)?
        if keepOriginal, source.channelCount > 2,
           let stereo = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: source.sampleRate, channels: 2, interleaved: false),
           let fold = AVAudioConverter(from: source, to: stereo),
           let into = AVAudioPCMBuffer(pcmFormat: stereo, frameCapacity: chunk) {
            fold.downmix = true
            folding = (fold, into)
        }
        func keep(_ buffer: AVAudioPCMBuffer) {
            guard keepOriginal else { return }
            var read = buffer
            if let (fold, into) = folding {
                guard (try? fold.convert(to: into, from: buffer)) != nil else { return }
                read = into
            }
            guard let planes = read.floatChannelData else { return }
            let frames = Int(read.frameLength)
            let left = planes[0]
            let right = read.format.channelCount > 1 ? planes[1] : planes[0]
            let start = original.count
            original.append(contentsOf: repeatElement(0, count: frames * 2))
            original.withUnsafeMutableBufferPointer { all in
                for frame in 0..<frames {
                    all[start + frame * 2] = left[frame]
                    all[start + frame * 2 + 1] = right[frame]
                }
            }
        }

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
                        keep(input)
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
        return (samples, keepOriginal ? original : nil)
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
