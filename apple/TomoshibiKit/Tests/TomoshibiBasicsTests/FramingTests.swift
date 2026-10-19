import CoreGraphics
import XCTest
@testable import TomoshibiBasics

/// Whether a camera fills its place or is shown whole. The Android app's test
/// has the same cases in the same order, one for one; a case added here is
/// added there.
final class FramingTests: XCTestCase {
    /// The picture as shown, the place it is drawn in, and whether it fills.
    private let cases: [(picture: CGSize, view: CGSize, fills: Bool)] = [
        // The case that prompted the rule: a laptop's landscape camera on an
        // upright phone kept a little over half its width.
        (CGSize(width: 640, height: 480), CGSize(width: 300, height: 400), false),
        // The same on an opened fold held upright, nearly square.
        (CGSize(width: 640, height: 480), CGSize(width: 1856, height: 2160), false),
        (CGSize(width: 720, height: 960), CGSize(width: 1856, height: 2160), true),
        // The same way round, half or more kept: a tall picture on a tall
        // phone loses only the room around a face.
        (CGSize(width: 720, height: 960), CGSize(width: 968, height: 2376), true),
        (CGSize(width: 640, height: 480), CGSize(width: 968, height: 2376), false),
        (CGSize(width: 1280, height: 720), CGSize(width: 1440, height: 900), true),
        (CGSize(width: 720, height: 960), CGSize(width: 1440, height: 900), false),
        // A square is neither way round, so only three quarters kept will do.
        (CGSize(width: 480, height: 480), CGSize(width: 300, height: 400), true),
        (CGSize(width: 1280, height: 720), CGSize(width: 500, height: 500), false),
        (CGSize(width: 720, height: 960), CGSize(width: 1080, height: 2340), true),
        (CGSize(width: 720, height: 1280), CGSize(width: 968, height: 2376), true),
        (CGSize(width: 640, height: 480), CGSize(width: 1600, height: 900), true),
        // The same way round but less than half kept.
        (CGSize(width: 720, height: 1280), CGSize(width: 400, height: 1600), false),
    ]

    func testFillsOrShowsWholeByTheTwoShapes() {
        for (picture, view, fills) in cases {
            XCTAssertEqual(Framing.fills(picture: picture, in: view), fills,
                           "\(Int(picture.width))x\(Int(picture.height)) in \(Int(view.width))x\(Int(view.height)) should \(fills ? "fill" : "be shown whole")")
        }
    }

    // Before the first frame, or before the place is laid out: what every
    // camera did before.
    func testAnUnknownSizeFills() {
        XCTAssertTrue(Framing.fills(picture: .zero, in: CGSize(width: 300, height: 400)))
        XCTAssertTrue(Framing.fills(picture: CGSize(width: 640, height: 480), in: .zero))
        XCTAssertTrue(Framing.fills(picture: CGSize(width: 640, height: 0), in: CGSize(width: 300, height: 400)))
        XCTAssertTrue(Framing.fills(picture: CGSize(width: 640, height: 480), in: CGSize(width: 300, height: 0)))
    }
}
