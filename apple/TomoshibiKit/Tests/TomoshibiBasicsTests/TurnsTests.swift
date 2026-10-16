@testable import TomoshibiBasics
import XCTest

/*
 * One change of a running share at a time (Sharing.retune, Turns). What this
 * guards is the screen track left on the stage with nothing feeding it: two
 * changes at once each took down the track they had found when they began,
 * so the one published in between stayed up until the call ended.
 */
@MainActor
final class TurnsTests: XCTestCase {
    /// A share's change of size, as Sharing does it: publish a new track,
    /// wait on the network, move to it, take the old one down. On the main
    /// actor, as Sharing is, so the two only meet where one waits.
    @MainActor
    private final class Share {
        var running = "first"
        var published: Set<String> = ["first"]

        func change(to fresh: String) async {
            let old = running
            published.insert(fresh)
            try? await Task.sleep(for: .milliseconds(20))
            running = fresh
            published.remove(old)
        }
    }

    func testTwoChangesAtOnceLeaveOnlyTheLastTrackPublished() async {
        let share = Share()
        let turns = Turns()
        async let menu: Void = turns.take { await share.change(to: "menu") }
        async let card: Void = turns.take { await share.change(to: "card") }
        _ = await (menu, card)
        XCTAssertEqual(share.published, [share.running], "a track nobody feeds was left published")
    }

    /// The same two changes without the turns: the fault itself, so that the
    /// test above is known to be testing something.
    func testWithoutTurnsTheFaultShowsItself() async {
        let share = Share()
        async let menu: Void = share.change(to: "menu")
        async let card: Void = share.change(to: "card")
        _ = await (menu, card)
        XCTAssertGreaterThan(share.published.count, 1)
    }

    func testRunsInTheOrderAskedAndOneAtATime() async {
        let turns = Turns()
        var order: [Int] = []
        var running = 0
        var most = 0
        await withTaskGroup(of: Void.self) { group in
            for index in 0..<5 {
                group.addTask { @MainActor in
                    await turns.take {
                        running += 1
                        most = max(most, running)
                        order.append(index)
                        try? await Task.sleep(for: .milliseconds(5))
                        running -= 1
                    }
                }
                // Each asked for after the one before has queued.
                await Task.yield()
                await Task.yield()
            }
        }
        XCTAssertEqual(most, 1)
        XCTAssertEqual(order, [0, 1, 2, 3, 4])
        XCTAssertFalse(turns.busy)
    }

    func testAPieceThatThrowsHandsTheTurnOn() async {
        struct Refused: Error {}
        let turns = Turns()
        do {
            try await turns.take { throw Refused() }
            XCTFail("the error was swallowed")
        } catch {}
        XCTAssertFalse(turns.busy)
        let after = await turns.take { 42 }
        XCTAssertEqual(after, 42)
    }
}
