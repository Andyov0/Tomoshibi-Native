import Foundation

/// Music that makes way for speech (web/src/live/duck.ts).
///
/// Whenever anybody in the call is talking, this person included, the song
/// desk's music is turned down where it is heard, and brought back up a moment
/// after they stop: what a radio presenter's desk does under a voice. Each
/// participant decides for itself, from the microphones it can hear, and turns
/// down only its own playback, so what goes out is still the file and nobody's
/// choice reaches anybody else.
///
/// Music also arrives gently: a stream that starts is faded in over a second
/// and a half rather than starting at full level in the middle of a sentence.
///
/// This is the arithmetic alone, driven by whoever reads the microphones, with
/// the times given in milliseconds by the caller's clock, so that the tests can
/// hold it to the web's own cases without waiting for real time to pass.
public struct MusicDuck: Sendable {
    /// How far the music comes down while somebody talks.
    public static let ducked = 0.3
    /// Down quickly, so the first word is heard; up slowly, so a pause is not
    /// a lurch.
    public static let attackMs = 150.0
    public static let holdMs = 700.0
    public static let releaseMs = 1200.0
    public static let fadeInMs = 1500.0
    /// How often the microphones are read and the level moved.
    public static let tickMs = 40.0
    /// The RMS of a frame that counts as talking, about -38 dB below full
    /// scale: a voice after the system's processing sits well above it, and
    /// the residue of music after echo cancelling well below.
    public static let speech = 0.012
    /// Readings in a row above `speech` before it is taken for speech: a click
    /// is one.
    public static let onset = 2
    /// The key a participant's own music output is filed under, beside the
    /// identities of whoever else plays music (duck.ts `LOCAL`).
    public static let local = "\u{0}local"

    /// Whether the music is turned down under speech at all; the switch in
    /// the panel, kept as `meet-live.music-duck`.
    public var enabled: Bool
    public private(set) var level = 1.0
    private var lastSpeech = -Double.infinity
    private var loud = 0
    private var arrivals: [String: Double] = [:]

    public init(enabled: Bool = true) {
        self.enabled = enabled
    }

    /// Whether somebody is talking, as far as the music is concerned.
    public var ducking: Bool { enabled && level < 1 }

    /// One reading of every microphone that is on, and the level moved by the
    /// time since the last. Returns whether any music's factor moved.
    public mutating func tick(levels: [Double], now: Double, elapsed: Double) -> Bool {
        let speaking = levels.contains { $0 > Self.speech }
        loud = speaking ? loud + 1 : 0
        return step(speaking: loud >= Self.onset, now: now, elapsed: elapsed)
    }

    /// Moves the level one tick, given whether anybody is talking (duck.ts
    /// `step`): toward `ducked` at the attack's pace, held there until
    /// `holdMs` after the last word, then back at the release's pace.
    public mutating func step(speaking: Bool, now: Double, elapsed: Double) -> Bool {
        let before = level
        if speaking { lastSpeech = now }
        let target = now - lastSpeech < Self.holdMs ? Self.ducked : 1
        if level > target {
            level = max(target, level - (1 - Self.ducked) * elapsed / Self.attackMs)
        } else if level < target {
            level = min(target, level + (1 - Self.ducked) * elapsed / Self.releaseMs)
        }
        return level != before || fading(at: now)
    }

    /// What to multiply a music stream's volume by now: the duck, and the
    /// fade-in of a stream that has just started.
    public func factor(_ key: String, now: Double) -> Double {
        let fade = arrivals[key].map { min(1, max(0, (now - $0) / Self.fadeInMs)) } ?? 1
        return (enabled ? level : 1) * fade
    }

    /// A music stream has started from `key`. Faded in once; again only after
    /// it has gone, so the songs that follow on the same stream are not.
    public mutating func arrived(_ key: String, now: Double) {
        if arrivals[key] == nil { arrivals[key] = now }
    }

    public mutating func left(_ key: String) {
        arrivals[key] = nil
    }

    /// Whether some stream is still fading in, and so still moving.
    public func fading(at now: Double) -> Bool {
        arrivals.values.contains { now - $0 < Self.fadeInMs + Self.tickMs }
    }

    /// Whether anything still moves without a microphone saying so.
    public func settled(at now: Double) -> Bool {
        level == 1 && !fading(at: now)
    }
}
