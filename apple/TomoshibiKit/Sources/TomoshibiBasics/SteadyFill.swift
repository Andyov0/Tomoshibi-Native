import Foundation
import Observation

/// A bar's fill that moves at one speed. The owner, 2026-10-22: the update
/// bars "做得不够丝滑 虽然下的很快 但是尽量做匀速" — a build arrives in a few
/// reports, and a bar animated to each report jumped from one to the next.
/// What is shown here follows what is reported at `perSecond` at most, so a
/// bar takes at least 1.2 s from empty to full however fast the bytes came,
/// and is never ahead of them: a bar that said more than had arrived would be
/// a promise the download might not keep. A report below what is shown (a
/// download begun again) is shown at once rather than drawn backwards.
///
/// `filled()` returns once the bar is seen full, so that the step after it —
/// installing, opening the new copy — waits for the bar instead of cutting it
/// off half way (Updates). The Android and Windows apps follow the same rule.
@MainActor
@Observable
public final class SteadyFill {
    /// From 0 to 1: what the bar draws.
    public private(set) var shown: Double = 0
    /// The last report, from 0 to 1.
    @ObservationIgnored public private(set) var target: Double = 0
    @ObservationIgnored private let perSecond: Double
    @ObservationIgnored private let drives: Bool
    @ObservationIgnored private var ticking: Task<Void, Never>?
    /// Which mover is the current one, so that one stopped by `reset` and
    /// finishing late does not forget the next one and leave two moving the
    /// bar at twice the speed.
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var waiting: [CheckedContinuation<Void, Never>] = []

    /// The speed for every update bar: the whole bar in 1.2 s.
    public nonisolated static let perSecond = 1 / 1.2

    /// `drives` false leaves the moving to `advance`, for the tests; a speed
    /// of `.infinity` shows every report as it comes.
    public init(perSecond: Double = SteadyFill.perSecond, drives: Bool = true) {
        self.perSecond = perSecond
        self.drives = drives
    }

    /// Takes a report: what has got there, from 0 to 1.
    public func follow(_ fraction: Double) {
        let fraction = min(1, max(0, fraction.isFinite ? fraction : 0))
        target = fraction
        if fraction < shown || perSecond.isInfinite { shown = fraction }
        settleIfDone()
        guard drives, shown < target, ticking == nil else { return }
        generation += 1
        let mine = generation
        // Moved by the time that has passed rather than by the ticks, so a
        // late tick does not slow the bar.
        ticking = Task { [weak self] in
            let clock = ContinuousClock()
            var last = clock.now
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(8))
                guard let self, self.generation == mine, !Task.isCancelled else { return }
                let now = clock.now
                self.advance(by: now - last)
                last = now
                if self.shown >= self.target { break }
            }
            if let self, self.generation == mine { self.ticking = nil }
        }
    }

    /// Empty again, for the next download or install. Whoever was waiting for
    /// the bar to fill is let go: what they were waiting to follow is over.
    public func reset() {
        ticking?.cancel()
        ticking = nil
        generation += 1
        target = 0
        shown = 0
        let done = waiting
        waiting = []
        done.forEach { $0.resume() }
    }

    /// Moves the bar on by what `elapsed` allows, no further than the report.
    public func advance(by elapsed: Duration) {
        let seconds = Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18
        shown = min(target, shown + max(0, seconds) * perSecond)
        settleIfDone()
    }

    /// Returns once the bar is drawn full; at once where it already is.
    public func filled() async {
        if shown >= 1 { return }
        await withCheckedContinuation { waiting.append($0) }
    }

    private func settleIfDone() {
        guard shown >= 1, !waiting.isEmpty else { return }
        let done = waiting
        waiting = []
        done.forEach { $0.resume() }
    }
}
