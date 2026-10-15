import AVFoundation
import Foundation
import TomoshibiCore

/// A song ready for the call: 48 kHz, 16-bit, two channels interleaved, which
/// is what the core's music source takes.
struct DecodedSong: Sendable {
    let samples: [Int16]
    /// What the file is, as the panel says it: "FLAC · 24-bit · 44.1 kHz".
    let quality: String?

    var frames: Int { samples.count / 2 }
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
    static func load(source: String, id: String, quality: MusicQuality, from deployment: Deployment) async throws -> Outcome {
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

        // Named for what it is: Core Audio opens a file by its extension, and
        // a FLAC or an MP3 in a file called .tmp is not opened at all.
        let kind = audio.format == "flac" || http.mimeType == "audio/flac" ? "flac" : "mp3"
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("tomoshibi-music-\(UUID().uuidString)")
            .appendingPathExtension(kind)
        try FileManager.default.moveItem(at: downloaded, to: file)
        defer { try? FileManager.default.removeItem(at: file) }

        let samples = try decode(file)
        return .ready(DecodedSong(samples: samples, quality: describeMusicAudio(audio: audio)))
    }

    private static func url(_ text: String) throws -> URL {
        guard let url = URL(string: text) else { throw URLError(.badURL) }
        return url
    }

    /// The whole file at 48 kHz in two channels.
    ///
    /// Read at the file's own rate and converted once, here, with the best
    /// converter the system has. The web decodes at the file's rate and lets
    /// the browser bring it to 48 kHz on the way into Opus, which runs at 48
    /// kHz whatever it is given; the core takes 48 kHz and nothing else, so
    /// the same step happens before the samples are handed over instead.
    /// Converted in pieces, so a long file is never in memory twice over as
    /// floats.
    static func decode(_ url: URL) throws -> [Int16] {
        let file = try AVAudioFile(forReading: url)
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
}
