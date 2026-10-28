import CoreGraphics
import Foundation
import SwiftUI
import XCTest
@testable import TomoshibiBasics

/// The lobby's animals: the Swift the apps draw is the JSON every
/// client draws (art/portraits.json), and what reads and plays it does what
/// art/README.md tells the other platforms it does.
final class PortraitArtTests: XCTestCase {
    /// art/portraits.json, found from this file: Tests/TomoshibiBasicsTests
    /// in apple/TomoshibiKit, three levels under the repository's top.
    private func json() throws -> [String: Any] {
        let top = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let data = try Data(contentsOf: top.appendingPathComponent("art/portraits.json"))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func number(_ value: Any?) throws -> Double {
        try XCTUnwrap(value as? NSNumber).doubleValue
    }

    /// PortraitArt.swift is written from the JSON by
    /// apple/generate-portraits.py; a change to one without the other fails
    /// here, number for number and stroke for stroke.
    func testTheSwiftIsTheJSON() throws {
        let art = try json()
        XCTAssertEqual(try number(art["format"]), Double(PortraitArt.format))
        let canvas = try XCTUnwrap(art["canvas"] as? [String: Any])
        XCTAssertEqual(try number(canvas["width"]), Double(PortraitArt.canvas.width))
        XCTAssertEqual(try number(canvas["height"]), Double(PortraitArt.canvas.height))
        XCTAssertEqual(try number(art["lineWidth"]), Double(PortraitArt.lineWidth))
        XCTAssertEqual(art["placement"] as? String, PortraitArt.placement)
        XCTAssertEqual(try number(art["gap"]), PortraitArt.gap)
        let people = try XCTUnwrap(art["portraits"] as? [[String: Any]])
        XCTAssertEqual(people.count, PortraitArt.portraits.count, "run apple/generate-portraits.py")
        for (said, drawn) in zip(people, PortraitArt.portraits) {
            XCTAssertEqual(said["name"] as? String, drawn.name)
            XCTAssertEqual(try number(said["hold"]), drawn.hold, drawn.name)
            XCTAssertEqual(try number(said["fade"]), drawn.fade, drawn.name)
            let strokes = try XCTUnwrap(said["strokes"] as? [[String: Any]])
            XCTAssertEqual(strokes.count, drawn.strokes.count, "\(drawn.name): run apple/generate-portraits.py")
            for (i, (s, d)) in zip(strokes, drawn.strokes).enumerated() {
                XCTAssertEqual(s["d"] as? String, d.d, "\(drawn.name) stroke \(i)")
                XCTAssertEqual(try number(s["start"]), d.start, "\(drawn.name) stroke \(i)")
                XCTAssertEqual(try number(s["duration"]), d.duration, "\(drawn.name) stroke \(i)")
            }
        }
    }

    /// Every stroke is path data as the README promises the other platforms:
    /// absolute M, L, C, Q and Z, a letter on every segment, single spaces,
    /// two decimals at most; it parses, and it stays on the canvas.
    func testEveryStrokeIsThePathDataTheReadmePromises() throws {
        let n = #"-?\d+(\.\d{1,2})?"#
        let segment = "(M \(n) \(n)|L \(n) \(n)|C( \(n)){6}|Q( \(n)){4}|Z)"
        let promise = try NSRegularExpression(pattern: "^M \(n) \(n)( \(segment))*$")
        let canvas = CGRect(origin: .zero, size: PortraitArt.canvas)
        for person in PortraitArt.portraits {
            XCTAssertFalse(person.strokes.isEmpty, person.name)
            XCTAssertNotNil(person.name.range(of: "^[a-z][a-z0-9-]*$", options: .regularExpression), person.name)
            for (i, stroke) in person.strokes.enumerated() {
                let range = NSRange(stroke.d.startIndex..., in: stroke.d)
                XCTAssertNotNil(promise.firstMatch(in: stroke.d, range: range), "\(person.name) stroke \(i): \(stroke.d)")
                let path = try XCTUnwrap(Portrait.path(stroke.d), "\(person.name) stroke \(i)")
                XCTAssertFalse(path.isEmpty, "\(person.name) stroke \(i)")
                XCTAssertTrue(canvas.insetBy(dx: -0.01, dy: -0.01).contains(path.boundingRect),
                              "\(person.name) stroke \(i) leaves the canvas: \(path.boundingRect)")
            }
        }
        XCTAssertEqual(Set(PortraitArt.portraits.map(\.name)).count, PortraitArt.portraits.count, "names are what debug switches call them by")
    }

    /// The pen draws one stroke after another, each taking a moment, and an
    /// animal is not so long in the drawing that the card seems stuck.
    func testThePenDrawsOneStrokeAfterAnother() {
        for person in PortraitArt.portraits {
            var end = 0.0
            for (i, stroke) in person.strokes.enumerated() {
                XCTAssertGreaterThanOrEqual(stroke.start, end - 1e-9, "\(person.name) stroke \(i) starts before the last ends")
                XCTAssertGreaterThan(stroke.duration, 0, "\(person.name) stroke \(i)")
                end = stroke.start + stroke.duration
            }
            XCTAssertEqual(person.writing, end, accuracy: 1e-9)
            XCTAssertLessThan(person.writing, 7, person.name)
            XCTAssertGreaterThan(person.hold, 0, person.name)
            XCTAssertGreaterThan(person.fade, 0, person.name)
        }
    }

    func testPathDataIsReadAsSVGReadsIt() throws {
        let path = try XCTUnwrap(Portrait.path("M 1 2 L 3 4 C 5 6 7 8 9 10 Q 11 12 13 14 Z M 20 20 21 21"))
        var seen: [String] = []
        path.forEach { element in
            switch element {
            case let .move(to): seen.append("M \(to.x) \(to.y)")
            case let .line(to): seen.append("L \(to.x) \(to.y)")
            case let .curve(to, c1, c2): seen.append("C \(c1.x) \(c1.y) \(c2.x) \(c2.y) \(to.x) \(to.y)")
            case let .quadCurve(to, c): seen.append("Q \(c.x) \(c.y) \(to.x) \(to.y)")
            case .closeSubpath: seen.append("Z")
            }
        }
        // Pairs after a move are lines; commas and minus signs part numbers.
        XCTAssertEqual(seen, ["M 1.0 2.0", "L 3.0 4.0", "C 5.0 6.0 7.0 8.0 9.0 10.0", "Q 11.0 12.0 13.0 14.0", "Z", "M 20.0 20.0", "L 21.0 21.0"])
        XCTAssertNotNil(Portrait.path("M1,2L-3-4"))
        // What the art does not use is refused rather than misread.
        for refused in ["m 1 2", "M 1 2 l 3 4", "M 1 2 A 1 1 0 0 1 3 4", "M 1 2 H 3", "M 1 2 L 3", "L 1 2", "M 1 2 C 1 2 3 4", "", "M 1 x", "M 1 2 Z 3"] {
            XCTAssertNil(Portrait.path(refused), refused)
        }
    }

    /// The pen eases in and out of a stroke (README: smoothstep).
    func testAStrokeIsDrawnEasedInAndOut() {
        let stroke = Portrait.Stroke(d: "M 0 0 L 10 0", start: 1, duration: 2)
        XCTAssertEqual(Portrait.drawn(stroke, at: 0.5), 0)
        XCTAssertEqual(Portrait.drawn(stroke, at: 1), 0)
        XCTAssertEqual(Portrait.drawn(stroke, at: 1.5), 0.15625, accuracy: 1e-12)
        XCTAssertEqual(Portrait.drawn(stroke, at: 2), 0.5, accuracy: 1e-12)
        XCTAssertEqual(Portrait.drawn(stroke, at: 3), 1)
        XCTAssertEqual(Portrait.drawn(stroke, at: .infinity), 1)
        XCTAssertEqual(Portrait.shown(fading: 0), 1)
        XCTAssertEqual(Portrait.shown(fading: 0.25), 1 - 0.15625, accuracy: 1e-12)
        XCTAssertEqual(Portrait.shown(fading: 1), 0)
    }

    /// The round as the README plays it: gap, drawing, hold, fade, the next
    /// animal, and the first again after the last; still in the gap and the
    /// hold, moving in the drawing and the fade.
    func testTheRound() {
        let one = Portrait(name: "one", hold: 1, fade: 0.5, strokes: [.init(d: "M 0 0 L 1 1", start: 0, duration: 2)])
        let two = Portrait(name: "two", hold: 2, fade: 1, strokes: [.init(d: "M 0 0 L 1 1", start: 0.5, duration: 1)])
        let round = PortraitRound(people: [one, two], gap: 0.25)
        XCTAssertEqual(round.length, 0.25 + 2 + 1 + 0.5 + 0.25 + 1.5 + 2 + 1, accuracy: 1e-12)

        var now = round.moment(at: 0.1)
        XCTAssertEqual(now.person, 0)
        XCTAssertLessThan(now.written, 0)
        XCTAssertEqual(now.shown, 1)
        XCTAssertEqual(round.motion(at: 0.1).moving, false)
        XCTAssertEqual(round.motion(at: 0.1).until, 0.25, accuracy: 1e-12)

        XCTAssertEqual(round.moment(at: 1.25).written, 1, accuracy: 1e-12)
        XCTAssertEqual(round.motion(at: 1.25).moving, true)
        XCTAssertEqual(round.motion(at: 1.25).until, 2.25, accuracy: 1e-12)
        XCTAssertEqual(round.motion(at: 3).moving, false)
        XCTAssertEqual(round.moment(at: 3).shown, 1)

        now = round.moment(at: 3.5)
        XCTAssertEqual(now.shown, 0.5, accuracy: 1e-12)
        XCTAssertEqual(round.motion(at: 3.5).moving, true)

        now = round.moment(at: 3.75 + 0.25 + 0.5)
        XCTAssertEqual(now.person, 1)
        XCTAssertEqual(now.written, 0.5, accuracy: 1e-12)
        XCTAssertEqual(now.shown, 1)

        now = round.moment(at: round.length + 0.3)
        XCTAssertEqual(now.person, 0)
        XCTAssertEqual(now.written, 0.05, accuracy: 1e-9)
    }
}
