/// When a source of the video watched together that has not begun playing is
/// given up on (WatchScreen.tsx `STALL_MS`), looked at once a second as the
/// web's interval looks.
///
/// Every age here is worked out saturating. The clock is read at different
/// moments for one beat: the beat reads it, the show may then move on to the
/// next video and build its player, and the player reads the clock again
/// when its source begins, a millisecond or more later than the beat's
/// reading. Subtracted as unsigned numbers, that source was a negative age,
/// and the subtraction trapped: the app quit whenever the show moved on by
/// itself, which it does when the holder's player never says a video ended.
/// A source that began after `now` has been running for no time at all.
public struct WatchStall: Equatable, Sendable {
    /// How long a source may go without beginning (`STALL_MS`).
    public let stallMs: UInt64
    /// When the source being tried began.
    public private(set) var started: UInt64
    private var looked: UInt64?

    public init(started: UInt64, stallMs: UInt64 = 8000) {
        self.started = started
        self.stallMs = stallMs
    }

    /// Another source is being tried, from `at`. When the stall was last
    /// looked at is kept: the second is the player's, not the source's.
    public mutating func restart(at: UInt64) {
        started = at
    }

    /// Whether a second has passed since the last look, which is then this
    /// one; the first look is always due.
    public mutating func due(_ now: UInt64) -> Bool {
        if let looked, Self.since(looked, now) < 1000 { return false }
        looked = now
        return true
    }

    /// Whether the source has gone `stallMs` without beginning.
    public func late(_ now: UInt64) -> Bool {
        Self.since(started, now) > stallMs
    }

    /// How long ago `then` was at `now`, and nothing for a `then` read after.
    public static func since(_ then: UInt64, _ now: UInt64) -> UInt64 {
        now > then ? now - then : 0
    }
}
