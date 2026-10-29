import XCTest

/// What a finger meets on an iPhone, pressed with real touches and read off
/// the screen: every small control pressed over a fingertip (Kit
/// `fingertip`, Apple's 44 points) though it is drawn smaller, a menu's rows a
/// fingertip high, and the states the bar's buttons are drawn in.
///
/// The fingertip is checked by pressing just outside what is drawn, where a
/// finger that meant the control lands, and seeing the control answer: the
/// accessibility frame says where a control is drawn, not where it can be
/// pressed. Only where a press there misses without it: the system gives a
/// lone control's touch some reach of its own, and a panel's close answered
/// a press five and a half points off its edge with the fingertip taken
/// away, so a test of it passed with the rule broken and is not here. The states are read as pixels on the button's plate, beside its
/// symbol, as AppearanceTests reads the screens: a panel's open button held
/// down in grey, not lit in amber (the owner, 2026-10-29: amber is only for
/// what is happening); a microphone that is off on the foreground's own
/// plate, white in the call's night; the lobby's Join, which cannot be
/// pressed with nothing typed, a grey plate rather than a faded amber.
///
/// Against the local deployment on 127.0.0.1:8080, or the one
/// TEST_RUNNER_TOMOSHIBI_BASE names; the lobby's test signed in as
/// LobbyTouchTests are (TEST_RUNNER_TOMOSHIBI_SIGNIN). The call of two's
/// button needs somebody else in the room the run names in
/// TEST_RUNNER_TOUCH_PEER_ROOM (a browser left in it), and is skipped
/// without one. Laid out for an iPhone 18 Pro held upright.
final class TouchTargetTests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
    }

    private func launch(_ extra: [String: String] = [:]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080", "TOMOSHIBI_NAME": "UI Test"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launch()
        return app
    }

    private func joined(_ room: String) -> XCUIApplication {
        let app = launch(["TOMOSHIBI_AUTOJOIN": room])
        XCTAssertTrue(app.buttons["More"].waitForExistence(timeout: 30), "the bar's More button")
        // The bar's arrival (Theme.settle) and the first layout's choice of
        // bar (ControlBar `fitting`).
        Thread.sleep(forTimeInterval: 1.5)
        return app
    }

    /// The square `side` across drawn at the middle of `element`, whatever
    /// its accessibility frame holds.
    private func drawn(_ element: XCUIElement, _ side: CGFloat) -> CGRect {
        let frame = element.frame
        return CGRect(x: frame.midX - side / 2, y: frame.midY - side / 2, width: side, height: side)
    }

    /// A press at a point of the screen, in points.
    private func press(_ app: XCUIApplication, at point: CGPoint) {
        app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: point.x, dy: point.y)).tap()
    }

    // MARK: Fingertips

    /// The compact bar's buttons are forty across and four apart: each takes
    /// the two points of the gap on its own side, so a press a point and a
    /// half off its drawn edge is still the button's.
    func testTheCompactBarsButtonsMeetAcrossTheirGaps() throws {
        let app = joined("uitest-touch-bar")
        let messages = app.buttons["Show messages"]
        XCTAssertTrue(messages.waitForExistence(timeout: 5))
        let circle = drawn(messages, 40)
        press(app, at: CGPoint(x: circle.maxX + 1.5, y: circle.midY))
        XCTAssertTrue(app.buttons["Hide messages"].waitForExistence(timeout: 5),
                      "a press in the gap on the messages button's side opens the messages")
    }

    /// The reactions' half of the hand is thirty-two along the bar (it was
    /// twenty), and the tray's six are a fingertip apart: a press between
    /// two of them, on the second's side, sends the second.
    func testTheReactionsAreAFingertipEach() throws {
        let app = joined("uitest-touch-tray")
        let arrow = app.buttons["Reactions"]
        XCTAssertTrue(arrow.waitForExistence(timeout: 5))
        XCTAssertGreaterThanOrEqual(arrow.frame.width, 32, "the reactions' arrow along the bar")
        arrow.tap()
        let first = app.buttons["\u{1F44D}"]
        XCTAssertTrue(first.waitForExistence(timeout: 5), "the reactions tray")
        Thread.sleep(forTimeInterval: 0.6)
        let cells = app.buttons.allElementsBoundByIndex.filter { $0.frame.minY == first.frame.minY && $0.frame.height == first.frame.height }
            .sorted { $0.frame.minX < $1.frame.minX }
        XCTAssertGreaterThanOrEqual(cells.count, 2, "the tray's reactions in a row")
        let gap = cells[1].frame.midX - cells[0].frame.midX
        XCTAssertGreaterThanOrEqual(gap, 44, "a reaction's middle a fingertip from the next")
        XCTAssertGreaterThanOrEqual(drawn(first, 40).width, 40)
    }

    /// A menu's rows are a fingertip high on a phone (they were thirty-four),
    /// and Leave's two as well.
    func testMenuRowsAreAFingertipHigh() throws {
        let app = joined("uitest-touch-rows")
        app.buttons["More"].tap()
        let sound = app.buttons["Sound"]
        XCTAssertTrue(sound.waitForExistence(timeout: 5), "the More menu")
        XCTAssertGreaterThanOrEqual(sound.frame.height, 44, "a row of the More menu")
        app.buttons["Leave"].tap()
        let leave = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Leave the call")).firstMatch
        XCTAssertTrue(leave.waitForExistence(timeout: 5), "Leave's popover")
        XCTAssertGreaterThanOrEqual(leave.frame.height, 44, "Leave the call")
        // Right-aligned to the button it hangs from.
        let button = app.buttons["Leave"]
        XCTAssertLessThanOrEqual(abs(leave.frame.maxX - button.frame.maxX), 12, "the popover's edge at the button's")
    }

    /// The other person of a call of two has the button that fills the
    /// screen at their picture's foot, thirty-six drawn, pressed over a
    /// fingertip, clear of the bar; and leaving the full screen gives the
    /// call of two back.
    func testTheCallOfTwoFillsTheScreenFromItsFoot() throws {
        guard let room = ProcessInfo.processInfo.environment["TOUCH_PEER_ROOM"] else {
            throw XCTSkip("needs TEST_RUNNER_TOUCH_PEER_ROOM, a room with somebody in it")
        }
        let app = joined(room)
        let fill = app.buttons["Fill the screen"]
        XCTAssertTrue(fill.waitForExistence(timeout: 20), "the call of two's own button")
        let bar = app.buttons["More"].frame
        XCTAssertLessThan(fill.frame.maxY, bar.minY - 4, "above the bar")
        XCTAssertGreaterThan(fill.frame.midX, app.frame.width / 2, "at the right of the picture")
        let circle = drawn(fill, 36)
        press(app, at: CGPoint(x: circle.midX, y: circle.minY - 3))
        let leave = app.buttons["Leave fullscreen"]
        XCTAssertTrue(leave.waitForExistence(timeout: 5), "a press just above the drawn button fills the screen")
        leave.tap()
        XCTAssertTrue(fill.waitForExistence(timeout: 5), "back to the call of two, the button at its foot again")
    }

    // MARK: States, as pixels

    /// The screen as pixels, and the mean colour of a few points of it.
    private struct Shot {
        let width: Int
        let height: Int
        let scale: CGFloat
        let bytes: [UInt8]

        init(_ app: XCUIApplication) {
            let image = XCUIScreen.main.screenshot().image.cgImage!
            width = image.width
            height = image.height
            scale = CGFloat(image.width) / app.frame.width
            var bytes = [UInt8](repeating: 0, count: width * height * 4)
            bytes.withUnsafeMutableBytes { buffer in
                let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8,
                                        bytesPerRow: image.width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
                context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            }
            self.bytes = bytes
        }

        /// The mean red, green and blue (0...255) of a small square at a
        /// point given in points.
        func colour(at point: CGPoint) -> (red: Double, green: Double, blue: Double) {
            var sum = (0.0, 0.0, 0.0)
            var count = 0.0
            let cx = Int(point.x * scale), cy = Int(point.y * scale)
            for y in (cy - 2)...(cy + 2) {
                for x in (cx - 2)...(cx + 2) where x >= 0 && y >= 0 && x < width && y < height {
                    let i = (y * width + x) * 4
                    sum.0 += Double(bytes[i]); sum.1 += Double(bytes[i + 1]); sum.2 += Double(bytes[i + 2])
                    count += 1
                }
            }
            return (sum.0 / count, sum.1 / count, sum.2 / count)
        }
    }

    /// A point on a round button's plate, clear of its symbol: a fifth of the
    /// way in from its left-hand edge, at its middle.
    private func plate(_ element: XCUIElement, _ side: CGFloat) -> CGPoint {
        let circle = drawn(element, side)
        return CGPoint(x: circle.minX + side * 0.2, y: circle.midY)
    }

    /// How far a colour is from grey: amber is a hundred and more, a grey a
    /// handful.
    private func chroma(_ c: (red: Double, green: Double, blue: Double)) -> Double {
        max(c.red, c.green, c.blue) - min(c.red, c.green, c.blue)
    }

    func testAnOpenPanelsButtonIsHeldInGreyNotLit() throws {
        let app = joined("uitest-state-panel")
        app.buttons["Show messages"].tap()
        let open = app.buttons["Hide messages"]
        XCTAssertTrue(open.waitForExistence(timeout: 5), "the messages, open")
        Thread.sleep(forTimeInterval: 0.8)
        XCTAssertTrue(open.isSelected, "said to be held down")
        let shot = Shot(app)
        let held = shot.colour(at: plate(open, 40))
        print("STATE-CHECK open panel's button \(held)")
        XCTAssertLessThan(chroma(held), 30, "the open panel's button is grey, not amber: \(held)")
        // Held down: a step lighter than a closed one beside it.
        let closed = shot.colour(at: plate(app.buttons["More"], 40))
        XCTAssertGreaterThan(held.green, closed.green + 4, "held is lighter than at rest: \(held) against \(closed)")
    }

    func testAMicrophoneThatIsOffIsTheForegroundsPlate() throws {
        let app = joined("uitest-state-mic")
        let mic = app.buttons["Unmute microphone"]
        XCTAssertTrue(mic.waitForExistence(timeout: 5), "the microphone, off, as a test joins")
        let shot = Shot(app)
        let off = shot.colour(at: plate(mic, 40))
        let on = shot.colour(at: plate(app.buttons["More"], 40))
        print("STATE-CHECK microphone off \(off), a plain button \(on)")
        // The call is night in either appearance: the foreground is white.
        XCTAssertGreaterThan(min(off.red, off.green, off.blue), 200, "a white plate: \(off)")
        XCTAssertLessThan(chroma(off), 20, "white, not red: \(off)")
        XCTAssertLessThan(on.green, 120, "a plain button stays the raised grey: \(on)")
    }

    func testTheLobbysJoinWithNothingTypedIsGrey() throws {
        guard ProcessInfo.processInfo.environment["TOMOSHIBI_SIGNIN"] != nil else {
            throw XCTSkip("needs TEST_RUNNER_TOMOSHIBI_SIGNIN")
        }
        XCUIDevice.shared.appearance = .dark
        defer { XCUIDevice.shared.appearance = .light }
        let app = launch()
        let join = app.buttons["Join"]
        XCTAssertTrue(join.waitForExistence(timeout: 30), "the join bar's Join")
        Thread.sleep(forTimeInterval: 1.5)
        XCTAssertFalse(join.isEnabled, "nothing typed, nothing to join")
        let waiting = Shot(app).colour(at: plate(join, 54))
        print("STATE-CHECK join, nothing typed \(waiting)")
        XCTAssertLessThan(chroma(waiting), 25, "a grey plate, not a faded amber: \(waiting)")
    }
}
