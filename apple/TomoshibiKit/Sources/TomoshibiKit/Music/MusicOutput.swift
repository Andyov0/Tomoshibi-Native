import Foundation
import TomoshibiBasics
import TomoshibiCore

/// The song desk's music going out: one published track, kept across songs,
/// fed in real time (sound.ts `Output`), and the lossless stream beside it
/// where one is offered.
///
/// The web keeps one audio context and one published track for as long as the
/// songs that follow share a rate and bit depth, so that everybody listening
/// hears the next song without the stream starting again. Every song here is
/// brought to 48 kHz before it goes into Opus, so one track serves every song
/// and is only closed when the music stops. The lossless stream is the file
/// at its own rate, as the web's is; a song at another rate starts a new
/// stream on the same track (the core's sender begins a new stream on a
/// change of rate, and a listener starts a player afresh for it), which is
/// what the web's closing and reopening amounts to for a listener, without
/// the Opus track going away between the two.
///
/// Something always goes out, if only zeros. Paused, between one song and the
/// next, or past the end of a song while the desk fetches the next, both keep
/// flowing with silence: the web keeps a constant source connected for the
/// same reason, having measured that a track with nothing playing into it
/// stops being delivered at all, and that a lossless stream with nothing in
/// it for four seconds is given up on by every listener for the rest of the
/// track.
///
/// The holder hears the same samples through the call's own audio engine (the
/// web's `monitor`), behind a volume of their own that changes nothing sent.
final class MusicOutput: @unchecked Sendable {
    /// 10 ms at 48 kHz, the frame the core's source cuts sound into.
    static let frame = 480

    let input: AudioInput
    /// Whether the track was published with a lossless stream beside it.
    let lossless: Bool
    private let monitor: PCMRing?
    private let ended: @Sendable (String) -> Void
    private let lock = NSLock()
    private var song: DecodedSong?
    private var key: String?
    /// Where the song is, in 48 kHz frames for Opus and in the file's own for
    /// the lossless stream. Kept apart rather than one worked out from the
    /// other, so neither stream ever repeats or skips a sample to round.
    private var position = 0
    private var originalPosition = 0
    private var paused = false
    private var told = false
    private var running = true
    /// The rate the lossless stream runs at: the song's, kept through the
    /// silence after it until a song at another rate plays.
    private var losslessRate: UInt32 = 0
    /// Frames of the lossless stream sent since it began at this rate, and the
    /// 10 ms blocks of Opus sent meanwhile, which say how many are due: a rate
    /// like 44.1 kHz does not divide into 10 ms, and counting the two against
    /// each other keeps the stream at its rate without drifting.
    private var losslessSent: Int64 = 0
    private var blocksAtRate: Int64 = 0
    /// The longest the clock overslept since the debug log last asked: a
    /// listener's lossless stream that runs dry is either the line or this,
    /// and the log should say which.
    private var worstLate: UInt64 = 0

    /// `ended` is told the key of a song that played to its end, on the main
    /// thread.
    init(input: AudioInput, lossless: Bool, monitor: PCMRing?, ended: @escaping @Sendable (String) -> Void) {
        self.input = input
        self.lossless = lossless
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
                let deadline = pacer.nextDeadline
                let wait = Int64(deadline) - Int64(DispatchTime.now().uptimeNanoseconds)
                if wait > 0 { Thread.sleep(forTimeInterval: Double(wait) / 1e9) }
                let late = DispatchTime.now().uptimeNanoseconds &- deadline
                if late < 10_000_000_000 { output.noteLate(late) }
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
            originalPosition = 0
            paused = false
            told = false
            if lossless, song.original != nil, song.rate != losslessRate {
                losslessRate = song.rate
                losslessSent = 0
                blocksAtRate = 0
            }
        }
    }

    /// Pauses where it is, or goes on from there. Both streams keep flowing
    /// with silence meanwhile; see the type's comment.
    func pause(_ paused: Bool) {
        lock.withLock { self.paused = paused }
    }

    /// Ends the thread; the caller unpublishes the track.
    func stop() {
        lock.withLock { running = false }
    }

    private func noteLate(_ late: UInt64) {
        lock.withLock { worstLate = max(worstLate, late) }
    }

    /// How late the clock woke at worst since this was last asked, in
    /// milliseconds, for the song desk's debug log.
    func takeWorstLate() -> Double {
        lock.withLock {
            defer { worstLate = 0 }
            return Double(worstLate) / 1e6
        }
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
            if case .stopped = filled.state { return false }
            input.push(samples: block)
            monitor?.write(block)
            if let original = filled.original {
                input.pushLossless(samples: original.samples, channels: 2, rate: original.rate, timestampUs: original.at)
            }
            if case let .ended(key) = filled.state {
                let ended = ended
                DispatchQueue.main.async { ended(key) }
            }
        }
        return lock.withLock { running }
    }

    private enum State {
        case stopped
        case playing
        /// The song with this key has just run out; said once.
        case ended(String)
    }

    /// The lossless stream's share of one 10 ms block: the samples, their
    /// rate, and where they start on the stream's clock in microseconds,
    /// which the sender checks for holes.
    private struct Original {
        let samples: [Float]
        let rate: UInt32
        let at: Int64
    }

    /// The next 10 ms of both streams: the song where it is playing, silence
    /// anywhere else.
    private func fill(_ block: inout [Int16]) -> (state: State, original: Original?) {
        lock.withLock {
            guard running else { return (.stopped, nil) }
            let playing = song != nil && !paused
            let original = nextOriginal(playing: playing)

            for index in block.indices { block[index] = 0 }
            guard let song, playing, position < song.frames else { return (.playing, original) }
            let count = min(Self.frame, song.frames - position)
            song.samples.withUnsafeBufferPointer { all in
                for index in 0..<(count * 2) { block[index] = all[position * 2 + index] }
            }
            position += count
            if position >= song.frames, !told, let key {
                told = true
                return (.ended(key), original)
            }
            return (.playing, original)
        }
    }

    /// As many frames of the file as are due with this block. Called with the
    /// lock held.
    private func nextOriginal(playing: Bool) -> Original? {
        guard lossless, losslessRate > 0 else { return nil }
        blocksAtRate += 1
        let due = blocksAtRate * Int64(losslessRate) / 100
        let count = Int(due - losslessSent)
        guard count > 0 else { return nil }
        var samples = [Float](repeating: 0, count: count * 2)
        if playing, let song, let file = song.original, song.rate == losslessRate, originalPosition < song.originalFrames {
            let take = min(count, song.originalFrames - originalPosition)
            file.withUnsafeBufferPointer { all in
                samples.withUnsafeMutableBufferPointer { into in
                    for index in 0..<(take * 2) { into[index] = all[originalPosition * 2 + index] }
                }
            }
            originalPosition += take
        }
        let at = losslessSent * 1_000_000 / Int64(losslessRate)
        losslessSent = due
        return Original(samples: samples, rate: losslessRate, at: at)
    }
}
