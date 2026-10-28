import Foundation
import SwiftUI
import XCTest
@testable import TomoshibiBasics

/// The lobby's animals hold still while nobody can see them and carry on
/// from the same stroke when somebody can again (PortraitPlayback).
final class PortraitPlaybackTests: XCTestCase {
    private let zero = Date(timeIntervalSinceReferenceDate: 1_000_000)

    private func at(_ seconds: Double) -> Date { zero.addingTimeInterval(seconds) }

    /// The drawing moves only in front, in sight, in the active scene and
    /// without Reduce Motion; any one of them missing stops it.
    func testTheDrawingMovesOnlyWhenItCanBeSeen() {
        XCTAssertTrue(PortraitPlayback.runs(held: false, inSight: true, phase: .active, reduceMotion: false))
        XCTAssertFalse(PortraitPlayback.runs(held: true, inSight: true, phase: .active, reduceMotion: false), "covered")
        XCTAssertFalse(PortraitPlayback.runs(held: false, inSight: false, phase: .active, reduceMotion: false), "scrolled away")
        XCTAssertFalse(PortraitPlayback.runs(held: false, inSight: true, phase: .inactive, reduceMotion: false), "inactive")
        XCTAssertFalse(PortraitPlayback.runs(held: false, inSight: true, phase: .background, reduceMotion: false), "background")
        XCTAssertFalse(PortraitPlayback.runs(held: false, inSight: true, phase: .active, reduceMotion: true), "Reduce Motion")
    }

    /// On a Mac the drawing is watched only in the key window, open and not
    /// wholly covered. Each way of losing it is a way a window loses one of
    /// the three: another app brought to the front, the Settings window or a
    /// sheet over the lobby take the key; the Dock takes the window; another
    /// window lying over all of it takes its visibility.
    func testAMacWindowIsWatchedOnlyInFront() {
        XCTAssertTrue(PortraitPlayback.watched(key: true, miniaturized: false, visible: true))
        XCTAssertFalse(PortraitPlayback.watched(key: false, miniaturized: false, visible: true),
                       "another app in front, or the Settings, or a sheet over the lobby")
        XCTAssertFalse(PortraitPlayback.watched(key: false, miniaturized: true, visible: false), "minimized")
        XCTAssertFalse(PortraitPlayback.watched(key: true, miniaturized: true, visible: true), "minimized, though still key")
        XCTAssertFalse(PortraitPlayback.watched(key: true, miniaturized: false, visible: false), "wholly covered")
    }

    /// Stopped, the clock keeps its place however long the window is away;
    /// started again, it carries on from there rather than from the start or
    /// from where it would have got to.
    func testTheClockKeepsItsPlaceWhileStopped() {
        var playback = PortraitPlayback()
        XCTAssertFalse(playback.running)
        XCTAssertEqual(playback.elapsed(at: at(5)), 0)

        playback.run(at: at(0))
        XCTAssertTrue(playback.running)
        XCTAssertEqual(playback.elapsed(at: at(3)), 3, accuracy: 1e-9)

        playback.stop(at: at(3))
        XCTAssertFalse(playback.running)
        XCTAssertEqual(playback.elapsed(at: at(3)), 3, accuracy: 1e-9)
        XCTAssertEqual(playback.elapsed(at: at(600)), 3, accuracy: 1e-9, "still while nobody watches")

        playback.run(at: at(600))
        XCTAssertEqual(playback.elapsed(at: at(600)), 3, accuracy: 1e-9, "carried on, not started over")
        XCTAssertEqual(playback.elapsed(at: at(602)), 5, accuracy: 1e-9)
    }

    /// Asked twice, run and stop do nothing the second time: a view that
    /// tells it again (a redraw, two notifications in a row) moves nothing.
    func testRunAndStopTwiceAreRunAndStopOnce() {
        var playback = PortraitPlayback()
        playback.run(at: at(0))
        playback.run(at: at(10))
        XCTAssertEqual(playback.elapsed(at: at(12)), 12, accuracy: 1e-9)
        playback.stop(at: at(12))
        playback.stop(at: at(50))
        XCTAssertEqual(playback.elapsed(at: at(80)), 12, accuracy: 1e-9)
    }

    /// What is on the screen is the same the moment before the window goes
    /// away and the moment it comes back, mid-stroke and all.
    func testTheSameStrokeIsOnTheScreenAcrossAPause() {
        let round = PortraitRound.art
        var playback = PortraitPlayback()
        playback.run(at: at(0))
        // Part of the way into the first animal's drawing.
        let away = round.gap + PortraitArt.portraits[0].writing / 2
        let before = round.moment(at: playback.elapsed(at: at(away)))
        playback.stop(at: at(away))
        playback.run(at: at(away + 3600))
        let after = round.moment(at: playback.elapsed(at: at(away + 3600)))
        XCTAssertEqual(before.person, after.person)
        XCTAssertEqual(before.written, after.written, accuracy: 1e-9)
        XCTAssertEqual(before.shown, after.shown, accuracy: 1e-9)
        XCTAssertGreaterThan(before.written, 0, "mid-drawing")
        XCTAssertLessThan(before.written, PortraitArt.portraits[0].writing, "mid-drawing")
    }
}
