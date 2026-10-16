import CoreGraphics
import XCTest
@testable import TomoshibiBasics

/// The web's cases for finding addresses in a message (Linked.test.tsx), and
/// the corner's rules: how long notices stay, which one a fourth pushes out,
/// where a pile of them sits, and the chat box's limit.
final class LinkedTests: XCTestCase {
    func testFindsACompleteAddress() {
        XCTAssertEqual(Linked.links("see https://example.com/a?b=1"), ["https://example.com/a?b=1"])
    }

    func testFindsMoreThanOne() {
        XCTAssertEqual(Linked.links("http://a.test and https://b.test"), ["http://a.test", "https://b.test"])
    }

    // A false link is worse than a missed one: it is clickable, it goes
    // somewhere, and it looks deliberate.
    func testDoesNotGuessAtAnythingThatIsNotOne() {
        XCTAssertEqual(Linked.links("see example.com or www.example.com"), [])
        XCTAssertEqual(Linked.links("port 4443 on the box"), [])
        XCTAssertEqual(Linked.links("ftp://files.test"), [])
        // The web's `\b`: not in the middle of a word.
        XCTAssertEqual(Linked.links("xhttps://example.com"), [])
    }

    // A sentence ending in an address is written with a stop after it, and
    // the stop is not part of the address.
    func testLeavesTrailingPunctuationOutOfTheAddress() {
        XCTAssertEqual(Linked.links("it is at https://example.com/docs."), ["https://example.com/docs"])
        XCTAssertEqual(Linked.links("(https://example.com)"), ["https://example.com"])
        XCTAssertEqual(Linked.links("https://example.com, then"), ["https://example.com"])
    }

    func testKeepsTheWordsAroundIt() {
        let pieces = Linked.pieces("go to https://example.com now")
        XCTAssertEqual(pieces, [.text("go to "), .link("https://example.com"), .text(" now")])
        let joined = pieces.map { piece -> String in
            switch piece {
            case let .text(words): words
            case let .link(href): href
            }
        }.joined()
        XCTAssertEqual(joined, "go to https://example.com now")
    }

    // In a browser `\b` is about ASCII, so an address typed straight after
    // Chinese or Japanese is found; ICU's own `\b` would not find it.
    func testFindsAnAddressStraightAfterChineseOrJapanese() {
        XCTAssertEqual(Linked.links("看这个https://example.com/x"), ["https://example.com/x"])
        XCTAssertEqual(Linked.links("これhttps://example.jp を"), ["https://example.jp"])
    }

    func testCaseDoesNotMatter() {
        XCTAssertEqual(Linked.links("HTTPS://Example.com/A"), ["HTTPS://Example.com/A"])
    }
}

final class CornerTests: XCTestCase {
    func testTheWebsDurations() {
        XCTAssertEqual(NoticeTimes.briefly, 2.2)
        XCTAssertEqual(NoticeTimes.aMoment, 3)
        XCTAssertEqual(NoticeTimes.atMost, 10)
        XCTAssertEqual(NoticeTimes.shown, 3)
    }

    func testTheFourthPushesOutTheOldestThatGoesByItself() {
        XCTAssertEqual(NoticeTimes.overflow([false, false, false]), [])
        XCTAssertEqual(NoticeTimes.overflow([false, false, false, false]), [0])
        // Somebody at the door stays; the next oldest goes instead.
        XCTAssertEqual(NoticeTimes.overflow([true, false, false, false]), [1])
        XCTAssertEqual(NoticeTimes.overflow([false, false, false, false, false]), [0, 1])
        // Nothing that stays is ever dropped, even past the limit.
        XCTAssertEqual(NoticeTimes.overflow([true, true, true, true]), [])
    }

    func testAClockWaitsWhileHeldAndKeepsWhatWasLeft() {
        var clock = NoticeClock(lasts: 3, now: 100, running: true)
        XCTAssertEqual(clock.due(now: 101), 2)
        clock.hold(now: 101)
        XCTAssertNil(clock.due(now: 150))
        XCTAssertEqual(clock.left, 2)
        clock.run(now: 150)
        XCTAssertEqual(clock.due(now: 150), 2)
        XCTAssertEqual(clock.due(now: 151.5), 0.5)
    }

    func testANoticeReadToItsLastMomentIsGivenASecondMore() {
        var clock = NoticeClock(lasts: 2.2, now: 0, running: true)
        clock.hold(now: 2.15)
        clock.run(now: 9)
        XCTAssertEqual(clock.due(now: 9), NoticeTimes.afterHolding)
    }

    func testAClockStartedHeldDoesNotRun() {
        let clock = NoticeClock(lasts: 2.2, now: 0, running: false)
        XCTAssertNil(clock.due(now: 5))
        XCTAssertFalse(clock.running)
    }

    func testAPilePeeksAndASpreadColumnStacks() {
        let heights: [CGFloat] = [44, 60, 52]
        XCTAssertEqual(NoticePile.lifts(heights, spread: false), [0, NoticePile.peek, 2 * NoticePile.peek])
        XCTAssertEqual(NoticePile.height(heights, spread: false), 44 + 2 * NoticePile.peek)
        XCTAssertEqual(NoticePile.lifts(heights, spread: true), [0, 44 + NoticePile.gap, 44 + 60 + 2 * NoticePile.gap])
        XCTAssertEqual(NoticePile.height(heights, spread: true), 44 + 60 + 52 + 2 * NoticePile.gap)
        XCTAssertEqual(NoticePile.lifts([], spread: true), [])
        XCTAssertEqual(NoticePile.height([], spread: false), 0)
        XCTAssertEqual(NoticePile.scale(place: 0, spread: false), 1)
        XCTAssertEqual(NoticePile.scale(place: 2, spread: false), 0.9, accuracy: 0.0001)
        XCTAssertEqual(NoticePile.scale(place: 2, spread: true), 1)
    }

    func testNoticesRiseOverMessagesOnlyWhereTheyShareWidth() {
        // A wide window: messages at the left, notices far to the right.
        let said = CGRect(x: 12, y: 600, width: 320, height: 90)
        XCTAssertEqual(CornerLanes.lift(notices: 1200, said: said), 0)
        // A phone: the notices' column starts inside the messages'.
        XCTAssertEqual(CornerLanes.lift(notices: 60, said: said), 90 + CornerLanes.gap)
        // Closer than the gap counts as touching.
        XCTAssertEqual(CornerLanes.lift(notices: said.maxX + 4, said: said), 90 + CornerLanes.gap)
        XCTAssertEqual(CornerLanes.lift(notices: 60, said: nil), 0)
        XCTAssertEqual(CornerLanes.lift(notices: 60, said: .zero), 0)
    }

    func testTheBoxStopsAtTwoThousandWithoutSplittingACharacter() {
        let long = String(repeating: "a", count: 2100)
        XCTAssertEqual(ChatRules.clamp(long).utf16.count, 2000)
        XCTAssertEqual(ChatRules.clamp("short"), "short")
        // One short of the limit, then an emoji of two UTF-16 units.
        let edge = String(repeating: "a", count: 1999) + "\u{1F600}"
        XCTAssertEqual(ChatRules.clamp(edge), String(repeating: "a", count: 1999))
        XCTAssertEqual(ChatRules.clamp(String(repeating: "\u{4F60}", count: 2001)).count, 2000)
    }

    func testARunFromOnePersonNamesThemOnce() {
        let speakers = ["a", "a", "b", "a", "a", "a"]
        XCTAssertEqual(speakers.indices.map { ChatRules.continues(speakers, at: $0) },
                       [false, true, false, false, true, true])
        XCTAssertFalse(ChatRules.continues([], at: 0))
    }
}
