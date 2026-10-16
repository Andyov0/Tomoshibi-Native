import Foundation

/// Work that must not overlap, done one piece at a time in the order it was
/// asked for (Sharing's changes of a running share).
///
/// A share's change of size publishes a new picture track, moves the capture
/// onto it and takes the old one down, with a network round trip between each
/// step. Two of them at once, the menu's and the uplink card's, each took down
/// the track it had found at the start, so the one published in between was
/// left on the stage with nothing feeding it until the call ended. Each now
/// waits for the one before it, and finds the track that one left.
///
/// Nothing that must always happen goes through here: stopping a share or
/// leaving the call never waits behind a change that may itself be waiting on
/// the network.
@MainActor
public final class Turns {
    private var held = false
    private var waiting: [CheckedContinuation<Void, Never>] = []

    public init() {}

    /// Whether a piece of work is running.
    public var busy: Bool { held }

    /// Runs `work` once everything asked for before it has finished.
    public func take<T>(_ work: () async throws -> T) async rethrows -> T {
        if held {
            await withCheckedContinuation { waiting.append($0) }
        } else {
            held = true
        }
        defer { handOn() }
        return try await work()
    }

    /// The next in line goes, still holding the turn, so that nothing asked
    /// for in between can slip in ahead of it.
    private func handOn() {
        if waiting.isEmpty {
            held = false
        } else {
            waiting.removeFirst().resume()
        }
    }
}
