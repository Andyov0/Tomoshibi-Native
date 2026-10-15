import XCTest
@testable import TomoshibiBasics

/// The web's own cases for music that makes way for speech
/// (web/src/live/duck.test.ts), against the Swift port, and the clock the
/// music is sent by.
///
/// What would go wrong unnoticed: the music turned down by itself, so that it
/// never comes back up; a click counted as somebody talking; a fade-in that
/// never finishes, which is music that is never heard; a switch that says off
/// while the music still ducks; and a song handed to the core faster than it
/// plays, which every listener hears at the wrong speed or not at all.
final class MusicDuckTests: XCTestCase {
    private var duck = MusicDuck()
    private var clock = 0.0

    /// Both clocks on together, tick by tick, with the microphones reading
    /// `levels` throughout.
    private func pass(_ ms: Double, _ levels: [Double]) {
        var t = 0.0
        while t < ms {
            clock += MusicDuck.tickMs
            _ = duck.tick(levels: levels, now: clock, elapsed: MusicDuck.tickMs)
            t += MusicDuck.tickMs
        }
    }

    func testComesDownWithinTheAttackOnceSomebodyTalksAndOnlyAsFarAsDucked() {
        pass(2 * MusicDuck.tickMs + MusicDuck.attackMs + MusicDuck.tickMs, [0.1])
        XCTAssertEqual(duck.factor("gfriend", now: clock), MusicDuck.ducked)
        XCTAssertTrue(duck.ducking)
    }

    func testWaitsAfterTheTalkingStopsThenComesBackOverTheReleaseNotAtOnce() {
        pass(400, [0.1])
        pass(MusicDuck.holdMs - 2 * MusicDuck.tickMs, [0])
        XCTAssertEqual(duck.factor("gfriend", now: clock), MusicDuck.ducked)

        pass(MusicDuck.releaseMs / 2, [0])
        let midway = duck.factor("gfriend", now: clock)
        XCTAssertGreaterThan(midway, MusicDuck.ducked)
        XCTAssertLessThan(midway, 1)

        pass(MusicDuck.releaseMs, [0])
        XCTAssertEqual(duck.factor("gfriend", now: clock), 1)
    }

    func testTakesAClickForNothing() {
        pass(MusicDuck.tickMs, [0.5])
        pass(MusicDuck.attackMs * 2, [0])
        XCTAssertEqual(duck.factor("gfriend", now: clock), 1)
    }

    func testIgnoresWhatIsBelowSpeech() {
        pass(2000, [MusicDuck.speech * 0.8])
        XCTAssertEqual(duck.factor("gfriend", now: clock), 1)
    }

    func testAnyOneMicrophoneIsEnough() {
        pass(500, [0, 0.1, 0])
        XCTAssertEqual(duck.factor("gfriend", now: clock), MusicDuck.ducked)
    }

    func testDoesNothingOnceSwitchedOff() {
        pass(500, [0.1])
        duck.enabled = false
        XCTAssertEqual(duck.factor("gfriend", now: clock), 1)
        XCTAssertFalse(duck.ducking)
    }

    func testFadesAStreamInFromNothingAndFinishesWithNoMicrophones() {
        duck.arrived(MusicDuck.local, now: clock)
        XCTAssertEqual(duck.factor(MusicDuck.local, now: clock), 0)
        pass(MusicDuck.fadeInMs / 2, [])
        XCTAssertEqual(duck.factor(MusicDuck.local, now: clock), 0.5, accuracy: 0.05)
        pass(MusicDuck.fadeInMs, [])
        XCTAssertEqual(duck.factor(MusicDuck.local, now: clock), 1)
        XCTAssertTrue(duck.settled(at: clock))
    }

    func testFadesAStreamInOnceNotAgainBetweenSongs() {
        duck.arrived("gholder", now: clock)
        pass(MusicDuck.fadeInMs + 100, [])
        duck.arrived("gholder", now: clock)
        XCTAssertEqual(duck.factor("gholder", now: clock), 1)
    }

    func testFadesInAgainAfterTheStreamWent() {
        duck.arrived("gholder", now: clock)
        pass(MusicDuck.fadeInMs + 100, [])
        duck.left("gholder")
        duck.arrived("gholder", now: clock)
        XCTAssertEqual(duck.factor("gholder", now: clock), 0)
    }
}

final class MusicPacerTests: XCTestCase {
    let ms: UInt64 = 1_000_000

    func testHandsOverNoMoreThanRealTimeHowEverOftenItIsAsked() {
        var pacer = MusicPacer(start: 0)
        var blocks = 0
        // Asked every millisecond for a second: a hundred 10 ms blocks, and
        // never one before its time.
        for step in 0...1000 {
            let now = UInt64(step) * ms
            blocks += pacer.due(at: now)
            XCTAssertLessThanOrEqual(blocks, Int(now / (10 * ms)))
        }
        XCTAssertEqual(blocks, 100)
    }

    func testAskedLateItMakesUpWhatIsOwed() {
        var pacer = MusicPacer(start: 0)
        XCTAssertEqual(pacer.due(at: 35 * ms), 3)
        XCTAssertEqual(pacer.due(at: 35 * ms), 0)
        XCTAssertEqual(pacer.due(at: 40 * ms), 1)
        XCTAssertEqual(pacer.nextDeadline, 50 * ms)
    }

    func testAfterAStallItLetsTheBacklogGoAndKeepsRealTimeAfterwards() {
        var pacer = MusicPacer(start: 0, mostBehind: 20)
        XCTAssertEqual(pacer.due(at: 100 * ms), 10)
        // Five seconds held up: at most twenty blocks at once, not five
        // hundred, and then one block per ten milliseconds again.
        XCTAssertEqual(pacer.due(at: 5100 * ms), 20)
        XCTAssertEqual(pacer.due(at: 5100 * ms), 0)
        XCTAssertEqual(pacer.due(at: 5150 * ms), 5)
    }
}
