import Foundation

/// When the next block of music is due, by a real clock.
///
/// The core's music source sends whatever it is handed at once: it has no
/// clock of its own (its queue is set to nothing, so that a voice is not
/// delayed). A song pushed as fast as it decodes would go out in a few
/// seconds, at the wrong speed, and the listeners' jitter buffers would throw
/// most of it away. So the song is handed over one 10 ms block at a time, as
/// many as the time since it started says are due, which is what a browser's
/// audio context does by playing the buffer in real time.
///
/// A thread that was held up (a busy machine, a laptop lid) owes a burst. Up
/// to `mostBehind` blocks are made up at once, which the listeners' jitter
/// buffers absorb; anything beyond is let go and the clock starts again from
/// there, so the song resumes where it stopped rather than racing through
/// seconds of it to catch up.
public struct MusicPacer: Sendable {
    public let blockNanos: UInt64
    public let mostBehind: Int
    private var base: UInt64
    private var sent: UInt64 = 0

    public init(start: UInt64, blockNanos: UInt64 = 10_000_000, mostBehind: Int = 20) {
        self.base = start
        self.blockNanos = blockNanos
        self.mostBehind = mostBehind
    }

    /// How many blocks are due by `now`, which are then counted as sent.
    public mutating func due(at now: UInt64) -> Int {
        guard now > base else { return 0 }
        let owed = (now - base) / blockNanos
        guard owed > sent else { return 0 }
        var due = owed - sent
        if due > UInt64(mostBehind) {
            // Let the rest go: the clock restarts as if only `mostBehind`
            // blocks had been owed.
            base += (due - UInt64(mostBehind)) * blockNanos
            due = UInt64(mostBehind)
        }
        sent += due
        return Int(due)
    }

    /// When the block after those sent falls due.
    public var nextDeadline: UInt64 {
        base + (sent + 1) * blockNanos
    }
}
