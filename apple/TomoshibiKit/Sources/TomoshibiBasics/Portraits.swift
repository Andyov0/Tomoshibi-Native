import CoreGraphics
import SwiftUI

// The children the phone lobby's camera card draws while the camera is off
// (TomoshibiKit PreviewGlass): cartoon heads, a boy and a girl in turn, drawn
// in lines of glass one stroke after another, the way the iPhone writes its
// first "hello".
//
// What is drawn is not here. It is art/portraits.json at the top of the
// repository, which the Android and Windows clients play as well, so that
// every platform draws the same lines at the same moments; art/README.md says
// how to read it. PortraitArt.swift is that file as Swift, written by
// apple/generate-portraits.py, and PortraitArtTests fail when the two
// disagree. Here is only what reading and playing it takes: the path data
// parsed, how much of a stroke is down at a moment, and whose turn it is.

/// One child: their strokes in the order the pen draws them, when it draws
/// each, and how long the finished drawing stays before it fades.
public struct Portrait {
    public struct Stroke {
        /// The line as SVG path data: absolute M, L, C, Q and Z only.
        public let d: String
        /// When the pen starts the line, in seconds from the start of the
        /// drawing, and how long it takes to draw it.
        public let start: Double
        public let duration: Double
        /// The line itself, in the canvas's units; empty where `d` is not
        /// path data this can read (which PortraitArtTests rule out).
        public let path: Path

        public init(d: String, start: Double, duration: Double) {
            self.d = d
            self.start = start
            self.duration = duration
            path = Portrait.path(d) ?? Path()
        }
    }

    /// What a debug switch calls them by (TOMOSHIBI_DEBUG_PORTRAIT).
    public let name: String
    public let hold: Double
    public let fade: Double
    public let strokes: [Stroke]

    public init(name: String, hold: Double, fade: Double, strokes: [Stroke]) {
        self.name = name
        self.hold = hold
        self.fade = fade
        self.strokes = strokes
    }

    /// How long the drawing takes, from the first stroke's start to the end
    /// of the last one to finish.
    public var writing: Double {
        strokes.reduce(0) { max($0, $1.start + $1.duration) }
    }

    /// How much of `stroke` is drawn `seconds` into the drawing, from 0 to 1
    /// along its length: eased in and out (smoothstep), as a hand slows into
    /// the end of a line and out of its start.
    public static func drawn(_ stroke: Stroke, at seconds: Double) -> Double {
        guard stroke.duration > 0 else { return seconds >= stroke.start ? 1 : 0 }
        let u = min(max((seconds - stroke.start) / stroke.duration, 0), 1)
        return u * u * (3 - 2 * u)
    }

    /// The fading drawing's opacity, `u` from 0 to 1 through the fade:
    /// quickly at first, as a light dims.
    public static func shown(fading u: Double) -> Double {
        let u = min(max(u, 0), 1)
        return 1 - u * u * (3 - 2 * u)
    }

    // MARK: - Path data

    /// SVG path data as a path, for the commands the art is written in
    /// (absolute move, line, cubic and quadratic curve, close), the numbers
    /// apart by spaces or commas. Nil for anything else: another command, a
    /// relative one, a number short or over, or a line before a move.
    public static func path(_ d: String) -> Path? {
        var path = Path()
        var command: Character?
        var numbers: [CGFloat] = []
        var token = ""
        var started = false

        func arity(_ command: Character) -> Int? {
            switch command {
            case "M", "L": 2
            case "C": 6
            case "Q": 4
            case "Z": 0
            default: nil
            }
        }

        /// The command once it has its numbers.
        func draw(_ c: Character, _ n: [CGFloat]) -> Bool {
            guard c == "M" || started else { return false }
            switch c {
            case "M":
                path.move(to: CGPoint(x: n[0], y: n[1]))
                started = true
                // Pairs after a move's first are lines, as SVG reads them.
                command = "L"
            case "L":
                path.addLine(to: CGPoint(x: n[0], y: n[1]))
            case "C":
                path.addCurve(to: CGPoint(x: n[4], y: n[5]), control1: CGPoint(x: n[0], y: n[1]), control2: CGPoint(x: n[2], y: n[3]))
            case "Q":
                path.addQuadCurve(to: CGPoint(x: n[2], y: n[3]), control: CGPoint(x: n[0], y: n[1]))
            default:
                path.closeSubpath()
            }
            return true
        }

        func endNumber() -> Bool {
            guard !token.isEmpty else { return true }
            guard let c = command, let count = arity(c), count > 0,
                  let value = Double(token), value.isFinite else { return false }
            token = ""
            numbers.append(CGFloat(value))
            guard numbers.count == count else { return true }
            defer { numbers.removeAll(keepingCapacity: true) }
            return draw(c, numbers)
        }

        for character in d {
            switch character {
            case " ", ",", "\n", "\t":
                guard endNumber() else { return nil }
            case "M", "L", "C", "Q", "Z":
                guard endNumber(), numbers.isEmpty else { return nil }
                command = character
                if character == "Z", !draw(character, []) { return nil }
            case "-":
                guard endNumber() else { return nil }
                token = "-"
            case "0"..."9", ".":
                token.append(character)
            default:
                return nil
            }
        }
        guard endNumber(), numbers.isEmpty, started else { return nil }
        return path
    }
}

// MARK: - The round

/// The children one after another, round and round: for each, a moment of
/// empty glass (`gap`), the drawing, the finished drawing held, and a fade.
public struct PortraitRound {
    public let people: [Portrait]
    public let gap: Double
    /// When each turn starts, in seconds into a round.
    private let starts: [Double]
    public let length: Double

    public init(people: [Portrait], gap: Double) {
        self.people = people
        self.gap = gap
        var at = 0.0
        starts = people.map { person in
            defer { at += gap + person.writing + person.hold + person.fade }
            return at
        }
        length = at
    }

    /// The art as every platform plays it.
    public static let art = PortraitRound(people: PortraitArt.portraits, gap: PortraitArt.gap)

    private func turn(_ person: Int) -> Double {
        let p = people[person]
        return gap + p.writing + p.hold + p.fade
    }

    /// Whose turn it is `t` seconds into the cycle, how far into it, and
    /// when it started.
    private func place(_ t: Double) -> (person: Int, into: Double, start: Double) {
        guard length > 0 else { return (0, 0, 0) }
        let t = max(t, 0)
        let base = (t / length).rounded(.down) * length
        let inRound = t - base
        let person = starts.lastIndex { $0 <= inRound } ?? 0
        return (person, inRound - starts[person], base + starts[person])
    }

    /// Who is on the card `t` seconds in, how many seconds of their drawing
    /// are done, and how much of them is still there to see.
    public func moment(at t: Double) -> (person: Int, written: Double, shown: Double) {
        let (person, into, _) = place(t)
        let p = people[person]
        let written = into - gap
        let fading = p.fade > 0 ? (written - p.writing - p.hold) / p.fade : (written >= p.writing + p.hold ? 1 : 0)
        return (person, written, Portrait.shown(fading: fading))
    }

    /// Whether anything is moving `t` seconds in, and until when that stays
    /// so: the drawing and the fade move; the empty glass before a child and
    /// the finished drawing do not.
    public func motion(at t: Double) -> (moving: Bool, until: Double) {
        let (person, into, start) = place(t)
        let p = people[person]
        let marks = [gap, gap + p.writing, gap + p.writing + p.hold, turn(person)]
        let moving = [false, true, false, true]
        let phase = marks.firstIndex { into < $0 } ?? 3
        return (moving[phase], start + marks[phase])
    }
}
