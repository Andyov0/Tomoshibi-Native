import CoreGraphics
import SwiftUI

// The people the camera card draws while the camera is off (PreviewGlass):
// heads and shoulders, as a person sits in front of a camera, each somebody
// else's hair, glasses, beard or hat, so that the card says "this is where
// you will be" without saying who.
//
// Each is a line drawing in very few strokes, written the way the iPhone
// writes its first "hello": one line that starts at the chin, goes up the
// face, over the hair, down the other side and the neck and off at a
// shoulder, then the other shoulder, then the two or three short strokes
// that make the person somebody (a fringe, glasses, a collar). The pen
// moves at one speed throughout and lifts between strokes, so a person
// takes as long to draw as there is of them. The lines are along the
// middle of the stroke, not the outline of a letter-like shape, so that
// drawing one is one line growing rather than two edges.
//
// Drawn in a square a hundred units a side, y down, the shoulders running
// off its foot as a picture of somebody sitting crops them; the card fits
// the square to its height and stands it on its foot.

/// Moves and curves in the portrait's square.
struct Pen {
    fileprivate(set) var path = Path()

    mutating func move(_ x: CGFloat, _ y: CGFloat) {
        path.move(to: CGPoint(x: x, y: y))
    }

    mutating func line(_ x: CGFloat, _ y: CGFloat) {
        path.addLine(to: CGPoint(x: x, y: y))
    }

    mutating func curve(_ x1: CGFloat, _ y1: CGFloat, _ x2: CGFloat, _ y2: CGFloat, _ x: CGFloat, _ y: CGFloat) {
        path.addCurve(to: CGPoint(x: x, y: y), control1: CGPoint(x: x1, y: y1), control2: CGPoint(x: x2, y: y2))
    }

    mutating func quad(_ cx: CGFloat, _ cy: CGFloat, _ x: CGFloat, _ y: CGFloat) {
        path.addQuadCurve(to: CGPoint(x: x, y: y), control: CGPoint(x: cx, y: cy))
    }

    /// A full turn of a circle of radius `r` hung below the pen, back to
    /// where it started: an earring from the foot of an ear.
    mutating func hoop(_ r: CGFloat) {
        guard let at = path.currentPoint else { return }
        let k = r * 0.5523
        let c = CGPoint(x: at.x, y: at.y + r)
        curve(at.x + k, at.y, c.x + r, c.y - k, c.x + r, c.y)
        curve(c.x + r, c.y + k, c.x + k, c.y + r, c.x, c.y + r)
        curve(c.x - k, c.y + r, c.x - r, c.y + k, c.x - r, c.y)
        curve(c.x - r, c.y - k, at.x - k, at.y, at.x, at.y)
    }

    /// A full circle round (`cx`, `cy`), over the top first: from its left
    /// point clockwise, or from its right point the other way. A lens of a
    /// pair of glasses, which the pen enters from the bridge's side.
    mutating func ring(_ cx: CGFloat, _ cy: CGFloat, _ r: CGFloat, clockwise: Bool) {
        let k = r * 0.5523
        if clockwise {
            curve(cx - r, cy - k, cx - k, cy - r, cx, cy - r)
            curve(cx + k, cy - r, cx + r, cy - k, cx + r, cy)
            curve(cx + r, cy + k, cx + k, cy + r, cx, cy + r)
            curve(cx - k, cy + r, cx - r, cy + k, cx - r, cy)
        } else {
            curve(cx + r, cy - k, cx + k, cy - r, cx, cy - r)
            curve(cx - k, cy - r, cx - r, cy - k, cx - r, cy)
            curve(cx - r, cy + k, cx - k, cy + r, cx, cy + r)
            curve(cx + k, cy + r, cx + r, cy + k, cx + r, cy)
        }
    }
}

/// One person: their strokes in the order they are drawn, and a name to call
/// them by in a debug switch.
struct Portrait {
    let name: String
    let strokes: [Path]
    /// How long each stroke is, in the square's units.
    let lengths: [Double]

    init(_ name: String, _ strokes: [(inout Pen) -> Void]) {
        self.name = name
        self.strokes = strokes.map { draw in
            var pen = Pen()
            draw(&pen)
            return pen.path
        }
        lengths = self.strokes.map(Self.length)
    }

    /// How fast the pen moves, in the square's units a second, and how long
    /// it is off the paper between strokes.
    static let speed: Double = 220
    static let lift: Double = 0.1

    /// How long the drawing takes, pen lifts and all.
    var writing: Double {
        lengths.reduce(0) { $0 + $1 / Self.speed } + Self.lift * Double(max(strokes.count - 1, 0))
    }

    /// Everything drawn of this person `seconds` into the drawing.
    func path(after seconds: Double) -> Path {
        var all = Path()
        var start = 0.0
        for (stroke, length) in zip(strokes, lengths) {
            let takes = length / Self.speed
            let done = min(max((seconds - start) / takes, 0), 1)
            if done >= 1 {
                all.addPath(stroke)
            } else if done > 0 {
                all.addPath(stroke.trimmedPath(from: 0, to: done))
            }
            start += takes + Self.lift
        }
        return all
    }

    /// The length of a path, its curves walked in short straight steps.
    private static func length(_ path: Path) -> Double {
        var total = 0.0
        var at = CGPoint.zero
        var start = CGPoint.zero
        func walk(_ point: (Double) -> CGPoint) {
            var last = at
            for i in 1...24 {
                let next = point(Double(i) / 24)
                total += hypot(next.x - last.x, next.y - last.y)
                last = next
            }
            at = last
        }
        path.forEach { element in
            switch element {
            case let .move(to):
                at = to
                start = to
            case let .line(to):
                total += hypot(to.x - at.x, to.y - at.y)
                at = to
            case let .quadCurve(to, c):
                let p = at
                walk { t in
                    let u = 1 - t
                    return CGPoint(x: u * u * p.x + 2 * u * t * c.x + t * t * to.x,
                                   y: u * u * p.y + 2 * u * t * c.y + t * t * to.y)
                }
            case let .curve(to, c1, c2):
                let p = at
                walk { t in
                    let u = 1 - t
                    let a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t
                    return CGPoint(x: a * p.x + b * c1.x + c * c2.x + d * to.x,
                                   y: a * p.y + b * c1.y + c * c2.y + d * to.y)
                }
            case .closeSubpath:
                total += hypot(start.x - at.x, start.y - at.y)
                at = start
            }
        }
        return total
    }
}

// MARK: - The figure everybody shares

/// The parts of a person most of them have in common, as pieces of a stroke.
private enum Figure {
    /// Across the chin from the left side of the neck to the right, where
    /// every drawing starts: in the middle of the card, where a pen set down
    /// at a shoulder would be hidden under the card's buttons.
    static func chin(_ pen: inout Pen, neck: CGFloat = 44.5, top: CGFloat = 57.5, depth: CGFloat = 2.2) {
        pen.move(neck, top)
        pen.curve(neck + 1.5, top + depth, 100 - neck - 1.5, top + depth, 100 - neck, top)
    }

    /// Down the left side of the neck and the left shoulder, off the foot of
    /// the picture.
    static func leftShoulderDown(_ pen: inout Pen, neck: CGFloat = 44.5, spread: CGFloat = 0) {
        pen.line(neck, 66)
        pen.curve(neck - 1, 72.5, 36, 75.5, 27, 77.5)
        pen.curve(13 - spread, 80.5, 6 - spread, 89, 4 - spread, 100)
    }

    /// From the right of the chin up the jaw and round the ear to the
    /// temple, where the hair begins.
    static func rightSideUp(_ pen: inout Pen, neck: CGFloat = 44.5, top: CGFloat = 57.5, ear: CGFloat = 37.8, temple: CGFloat = 35) {
        pen.curve(100 - neck + 4, top - 2.1, 63.2, ear + 12.2, 63.7, ear + 8.2)
        pen.curve(67.5, ear + 7.8, 67.8, ear + 0.6, 63.8, ear)
        pen.line(64, temple)
    }

    /// From the left temple round the ear and down the jaw to the neck.
    static func leftSideDown(_ pen: inout Pen, neck: CGFloat = 44.5, top: CGFloat = 57.5, ear: CGFloat = 37.8, temple: CGFloat = 35) {
        pen.line(36, temple)
        pen.line(36.2, ear)
        pen.curve(32.2, ear + 0.6, 32.5, ear + 7.8, 36.3, ear + 8.2)
        pen.curve(36.8, ear + 12.2, neck - 4, top - 2.1, neck, top)
    }

    /// The right side of the neck and the right shoulder, down off the foot
    /// of the picture.
    static func rightShoulderDown(neck: CGFloat = 44.5, top: CGFloat = 57.5, spread: CGFloat = 0) -> (inout Pen) -> Void {
        { pen in
            pen.move(100 - neck, top)
            pen.line(100 - neck, 66)
            pen.curve(100 - neck + 1, 72.5, 64, 75.5, 73, 77.5)
            pen.curve(87 + spread, 80.5, 94 + spread, 89, 96 + spread, 100)
        }
    }
}

// MARK: - Everybody

// Every main stroke goes the same way: from the chin up the right side of
// the face, over the hair, down the left side, and on down the neck and off
// at the left shoulder; then the right shoulder, then what is worn.
extension Portrait {
    /// Short hair cut close, a crew-neck shirt.
    static let cropped = Portrait("cropped", [
        { pen in
            Figure.chin(&pen)
            Figure.rightSideUp(&pen)
            pen.curve(67, 22, 60.5, 14, 51, 13.5)
            pen.curve(41.5, 13, 33.5, 22, 36, 35)
            Figure.leftSideDown(&pen)
            Figure.leftShoulderDown(&pen)
        },
        Figure.rightShoulderDown(),
        { pen in
            pen.move(36.6, 30)
            pen.curve(41, 27.5, 45, 24, 49, 23)
            pen.curve(55, 24.5, 60, 26.5, 63.4, 30)
        },
        { pen in
            pen.move(38.5, 76.4)
            pen.quad(50, 84, 61.5, 76.4)
        },
    ])

    /// Long straight hair parted in the middle, over the shoulders, and a
    /// V-necked top.
    static let long = Portrait("long", [
        { pen in
            // From the left cheek round the chin, up the right side of the
            // face to the parting, down the left side and on down the hair
            // that falls in front of the shoulder.
            pen.move(37.4, 46)
            pen.curve(38.6, 53.5, 43, 58.2, 50, 58.5)
            pen.curve(57, 58.2, 61.4, 53.5, 62.6, 46)
            pen.curve(62.3, 41, 62.3, 37, 62.5, 33)
            pen.curve(61.5, 23, 57, 16, 50, 13.5)
            pen.curve(43, 16, 38.5, 23, 37.5, 33)
            pen.curve(37.2, 37, 37.3, 41.5, 37.4, 46)
            pen.curve(37.5, 57, 36.5, 68, 34.5, 78)
        },
        { pen in
            pen.move(62.6, 46)
            pen.curve(62.7, 57, 63.5, 68, 65.5, 78)
        },
        { pen in
            // The hair from the crown down the outside to its ends, and the
            // shoulder it falls over; the same on the other side.
            pen.move(50, 12.5)
            pen.curve(38, 12, 31, 20.5, 31, 36)
            pen.curve(31, 54, 29.5, 72, 27, 88)
            pen.curve(15, 89, 8, 93, 5, 100)
        },
        { pen in
            pen.move(50, 12.5)
            pen.curve(62, 12, 69, 20.5, 69, 36)
            pen.curve(69, 54, 70.5, 72, 73, 88)
            pen.curve(85, 89, 92, 93, 95, 100)
        },
        { pen in
            pen.move(45, 58)
            pen.line(45, 66)
            pen.curve(45, 70, 47.5, 73.5, 50, 77)
            pen.curve(52.5, 73.5, 55, 70, 55, 66)
            pen.line(55, 58)
        },
    ])

    /// Round glasses, hair swept to one side, a high neck.
    static let glasses = Portrait("glasses", [
        { pen in
            Figure.chin(&pen, neck: 43.5, top: 58, depth: 1.8)
            Figure.rightSideUp(&pen, neck: 43.5, top: 58, temple: 36)
            pen.curve(67.5, 21, 62, 13.5, 52, 13)
            pen.curve(42, 12.5, 33, 21, 36, 36)
            Figure.leftSideDown(&pen, neck: 43.5, top: 58, temple: 36)
            Figure.leftShoulderDown(&pen, neck: 43.5)
        },
        Figure.rightShoulderDown(neck: 43.5, top: 58),
        { pen in
            pen.move(41, 16.5)
            pen.curve(44, 25, 53, 28.5, 64.6, 29)
        },
        { pen in
            // One lens from its inner side round over the top, the bridge,
            // and the other lens the same way.
            pen.move(48.4, 40.5)
            pen.ring(43, 40.5, 5.4, clockwise: false)
            pen.quad(50, 38.6, 51.6, 40.5)
            pen.ring(57, 40.5, 5.4, clockwise: true)
        },
        { pen in
            pen.move(43.5, 63.5)
            pen.quad(50, 66.5, 56.5, 63.5)
            pen.line(56.5, 68.5)
            pen.quad(50, 71.5, 43.5, 68.5)
        },
    ])

    /// Hair pulled back and tied high, the tail swinging out behind; a
    /// V-neck.
    static let ponytail = Portrait("ponytail", [
        { pen in
            Figure.chin(&pen)
            Figure.rightSideUp(&pen, ear: 37.5)
            // Up the back of the head and down the inside of the tail, back
            // up its outside to the tie, and over the top.
            pen.curve(64.6, 30.5, 66.5, 26.5, 68, 24)
            pen.curve(72.5, 33, 74, 44, 72.5, 52)
            pen.curve(76, 47, 77.5, 41, 78, 34)
            pen.curve(79, 22, 73, 15, 66, 14)
            pen.curve(64.5, 10.5, 60, 11, 58.5, 15.5)
            pen.curve(56, 14.2, 54, 14, 51, 14)
            pen.curve(42, 14, 34, 21, 36, 35)
            Figure.leftSideDown(&pen, ear: 37.5)
            Figure.leftShoulderDown(&pen, spread: -2)
        },
        Figure.rightShoulderDown(spread: -2),
        { pen in
            pen.move(37, 29)
            pen.curve(43, 22.5, 52, 21.5, 59, 17)
        },
        { pen in
            pen.move(40.5, 75.5)
            pen.line(50, 84.5)
            pen.line(59.5, 75.5)
        },
    ])

    /// A full beard, short hair lifted at the front, an open collar.
    static let bearded = Portrait("bearded", [
        { pen in
            // The chin is the foot of the beard, below where the others'
            // is, and the face's sides are the beard's.
            pen.move(44.5, 62.6)
            pen.curve(46.5, 65.6, 53.5, 65.6, 55.5, 62.6)
            pen.curve(60, 60, 63.5, 53, 63.6, 45)
            pen.curve(67.5, 44.6, 67.8, 37.1, 63.9, 36.5)
            pen.line(64, 33)
            pen.curve(66.5, 24, 64.5, 16, 59, 13)
            pen.curve(55, 9.5, 47, 10, 44, 14.5)
            pen.curve(38, 16, 34.5, 22, 36, 33)
            pen.line(36.1, 36.5)
            pen.curve(32.2, 37.1, 32.5, 44.6, 36.4, 45)
            pen.curve(36.5, 53, 40, 60, 44.5, 62.6)
            Figure.leftShoulderDown(&pen, spread: 2)
        },
        Figure.rightShoulderDown(top: 62.6, spread: 2),
        { pen in
            // Where the beard begins on the cheeks, and the moustache.
            pen.move(36.5, 42.5)
            pen.curve(39, 46.5, 41, 50, 43.5, 51)
            pen.curve(46, 48.5, 54, 48.5, 56.5, 51)
            pen.curve(59, 50, 61, 46.5, 63.5, 42.5)
        },
        { pen in
            pen.move(46.5, 55.5)
            pen.quad(50, 57, 53.5, 55.5)
        },
        { pen in
            pen.move(39.5, 73)
            pen.line(45, 81)
            pen.line(50, 75.5)
            pen.line(55, 81)
            pen.line(60.5, 73)
        },
    ])

    /// Hair gathered into a bun on the crown, round earrings, a wide
    /// neckline.
    static let bun = Portrait("bun", [
        { pen in
            Figure.chin(&pen, neck: 45)
            // Up the jaw to the foot of the ear, the earring hung from it,
            // and round the ear.
            pen.curve(59, 55.4, 63.2, 49.7, 63.7, 45.5)
            pen.hoop(1.5)
            pen.curve(67.5, 45.1, 67.8, 37.9, 63.8, 37.3)
            pen.line(64, 35)
            pen.curve(65.5, 26, 62, 19.5, 56, 17.4)
            pen.curve(58.5, 8.5, 41.5, 8.5, 44, 17.4)
            pen.curve(38, 19.5, 34.5, 26, 36, 35)
            pen.line(36.2, 37.3)
            pen.curve(32.2, 37.9, 32.5, 45.1, 36.3, 45.5)
            pen.hoop(1.5)
            pen.curve(36.8, 49.7, 41, 55.4, 45, 57.5)
            Figure.leftShoulderDown(&pen, neck: 45, spread: -1)
        },
        Figure.rightShoulderDown(neck: 45, spread: -1),
        { pen in
            pen.move(37.5, 26.5)
            pen.curve(42, 21.5, 46, 19.5, 50, 19)
            pen.curve(54, 19.5, 58, 21.5, 62.5, 26.5)
        },
        { pen in
            pen.move(33, 78)
            pen.quad(50, 86, 67, 78)
        },
    ])

    /// A knitted hat with a turned-up cuff and a bobble.
    static let beanie = Portrait("beanie", [
        { pen in
            // Up the face to the hat's cuff, up the cuff's side, over the
            // crown and the bobble, and down the other side of the cuff.
            Figure.chin(&pen)
            Figure.rightSideUp(&pen, ear: 37.5, temple: 33.6)
            pen.line(66, 33.5)
            pen.line(66.5, 26)
            pen.curve(65.5, 15.5, 58.5, 9.6, 53.2, 9.6)
            pen.curve(55.5, 6, 53, 2.4, 50, 2.4)
            pen.curve(47, 2.4, 44.5, 6, 46.8, 9.6)
            pen.curve(41.5, 9.6, 34.5, 15.5, 33.5, 26)
            pen.line(34, 33.5)
            pen.line(36, 33.6)
            Figure.leftSideDown(&pen, ear: 37.5, temple: 33.6)
            Figure.leftShoulderDown(&pen, spread: 1)
        },
        Figure.rightShoulderDown(spread: 1),
        { pen in
            pen.move(33.5, 26)
            pen.quad(50, 22.5, 66.5, 26)
        },
        { pen in
            pen.move(34, 33.5)
            pen.quad(50, 30, 66, 33.5)
        },
    ])

    /// Everybody, in the order they are drawn.
    static let everybody: [Portrait] = [cropped, long, glasses, ponytail, bearded, bun, beanie]
}
