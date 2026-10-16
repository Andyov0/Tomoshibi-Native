import Foundation

/// When a call's audio engine that stopped under it is started again, and
/// when that is given up (CallAudio).
///
/// The engine stops itself whenever the hardware under it changes: a headset
/// plugged in, AirPods switching profile as the microphone opens, voice
/// processing building its own aggregate device as it starts. It is started
/// again at once, up to three times in ten seconds. Past that the machine is
/// still settling, and starting it again into every change was a loop, so it
/// is left stopped until the hardware has been quiet for three seconds and
/// then tried once more. A start that fails waits the same way.
///
/// It used to stop there: the fourth change in ten seconds left the engine
/// stopped for the rest of the call while everything above it believed it
/// ran, and a call that had just had a headset plugged in went silent both
/// ways with the microphone's button on. Now it is given up only when the try
/// after the quiet fails as well, or the hardware churns past the budget again
/// within half a minute of it, and giving up is said (`giveUp`), because a
/// call with no sound that says nothing is the fault this is for.
public struct EngineRecovery: Equatable, Sendable {
    public enum Step: Equatable, Sendable {
        /// Start the engine again now.
        case restart
        /// Leave it stopped and call `quiet` after this many seconds without
        /// another change.
        case wait(Double)
        /// Leave it stopped and say so: it will not come back by itself.
        case giveUp
        /// Nothing to do: a wait is already running, or it was given up.
        case none
    }

    /// Restarts allowed within `window` before waiting for quiet.
    public static let budget = 3
    public static let window: Double = 10
    /// How long the hardware has to stay unchanged before the one more try.
    public static let quietFor: Double = 3
    /// How long after that try another give-up is the last.
    public static let settledFor: Double = 30

    private var restarts: [Double] = []
    /// Stopped, waiting for the hardware to be quiet.
    public private(set) var waiting = false
    /// Given up until something starts the engine again.
    public private(set) var gaveUp = false
    private var triedAt: Double?

    public init() {}

    /// The engine stopped under the call at `now`, or the hardware changed
    /// again while it was waiting.
    public mutating func changed(at now: Double) -> Step {
        if gaveUp { return .none }
        if waiting { return .wait(Self.quietFor) }
        restarts = restarts.filter { now - $0 < Self.window } + [now]
        if restarts.count <= Self.budget { return .restart }
        return settle(at: now)
    }

    /// A start of the engine threw at `now`.
    public mutating func failed(at now: Double) -> Step {
        if gaveUp || waiting { return .none }
        return settle(at: now)
    }

    /// The hardware was quiet for `quietFor`: the one more try.
    public mutating func quiet(at now: Double) -> Step {
        guard waiting else { return .none }
        waiting = false
        triedAt = now
        return .restart
    }

    /// The engine started, by whatever started it.
    public mutating func started() {
        waiting = false
        gaveUp = false
    }

    private mutating func settle(at now: Double) -> Step {
        if let triedAt, now - triedAt < Self.settledFor {
            waiting = false
            gaveUp = true
            return .giveUp
        }
        waiting = true
        return .wait(Self.quietFor)
    }
}
