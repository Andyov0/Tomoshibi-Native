import Foundation
import TomoshibiBasics
import TomoshibiCore

/// The song desk's music going out: one published track, kept across songs,
/// fed in real time (sound.ts `Output`).
///
/// The web keeps one audio context and one published track for as long as the
/// songs that follow share a rate and bit depth, so that everybody listening
/// hears the next song without the stream starting again. Every song here is
/// brought to 48 kHz before it plays, so one track serves every song and is
/// only closed when the music stops.
///
/// Something always goes out, if only zeros. Paused, between one song and the
/// next, or past the end of a song while the desk fetches the next, the track
/// keeps flowing with silence: the web keeps a constant source connected for
/// the same reason, having measured that a track with nothing playing into it
/// stops being delivered at all, and a listener's stream that stops is given
/// up on.
///
/// The holder hears the same samples through the call's own audio engine (the
/// web's `monitor`), behind a volume of their own that changes nothing sent.
final class MusicOutput: @unchecked Sendable {
    /// 10 ms at 48 kHz, the frame the core's source cuts sound into.
    static let frame = 480

    let input: AudioInput
    private let monitor: PCMRing?
    private let ended: @Sendable (String) -> Void
    private let lock = NSLock()
    private var song: DecodedSong?
    private var key: String?
    private var position = 0
    private var paused = false
    private var told = false
    private var running = true

    /// `ended` is told the key of a song that played to its end, on the main
    /// thread.
    init(input: AudioInput, monitor: PCMRing?, ended: @escaping @Sendable (String) -> Void) {
        self.input = input
        self.monitor = monitor
        self.ended = ended
    }

    /// Starts the clock. The thread holds this output only while it fills a
    /// block, so an output nobody holds any more (the call gone without a
    /// word) ends its thread rather than sending into a closed track for
    /// ever.
    func start() {
        let thread = Thread { [weak self] in
            var pacer = MusicPacer(start: DispatchTime.now().uptimeNanoseconds)
            var block = [Int16](repeating: 0, count: MusicOutput.frame * 2)
            while let output = self {
                guard output.send(&pacer, &block) else { return }
                let wait = Int64(pacer.nextDeadline) - Int64(DispatchTime.now().uptimeNanoseconds)
                if wait > 0 { Thread.sleep(forTimeInterval: Double(wait) / 1e9) }
            }
        }
        thread.name = "Tomoshibi music"
        // The clock the song goes out by: late wake-ups are bursts every
        // listener has to absorb.
        thread.qualityOfService = .userInteractive
        thread.start()
    }

    /// Plays `song` from its start, in place of whatever was playing, and
    /// unpaused: the next song plays unpaused (jukebox.ts `Desk.pause`).
    func play(_ song: DecodedSong, key: String) {
        lock.withLock {
            self.song = song
            self.key = key
            position = 0
            paused = false
            told = false
        }
    }

    /// Pauses where it is, or goes on from there. The track keeps flowing
    /// with silence meanwhile; see the type's comment.
    func pause(_ paused: Bool) {
        lock.withLock { self.paused = paused }
    }

    /// Ends the thread; the caller unpublishes the track.
    func stop() {
        lock.withLock { running = false }
    }

    /// Where the song playing is, in seconds, and how long it is.
    var progress: (at: Double, of: Double)? {
        lock.withLock {
            guard let song else { return nil }
            return (Double(position) / 48_000, Double(song.frames) / 48_000)
        }
    }

    /// Hands over every block that is due. False once stopped.
    private func send(_ pacer: inout MusicPacer, _ block: inout [Int16]) -> Bool {
        for _ in 0..<pacer.due(at: DispatchTime.now().uptimeNanoseconds) {
            let filled = fill(&block)
            if case .stopped = filled { return false }
            input.push(samples: block)
            monitor?.write(block)
            if case let .ended(key) = filled {
                let ended = ended
                DispatchQueue.main.async { ended(key) }
            }
        }
        return lock.withLock { running }
    }

    private enum Filled {
        case stopped
        case playing
        /// The song with this key has just run out; said once.
        case ended(String)
    }

    /// The next 10 ms: the song where it is playing, silence anywhere else.
    private func fill(_ block: inout [Int16]) -> Filled {
        lock.withLock {
            guard running else { return .stopped }
            for index in block.indices { block[index] = 0 }
            guard let song, !paused, position < song.frames else { return .playing }
            let count = min(Self.frame, song.frames - position)
            song.samples.withUnsafeBufferPointer { all in
                for index in 0..<(count * 2) { block[index] = all[position * 2 + index] }
            }
            position += count
            if position >= song.frames, !told, let key {
                told = true
                return .ended(key)
            }
            return .playing
        }
    }
}
