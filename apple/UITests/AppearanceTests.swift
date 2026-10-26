import XCTest

/// The day and the night on an iPhone, read off the screen: every screen
/// around a call in the system's appearance from its first frame, following
/// it as it changes in front and behind, into a call and out of it, and the
/// call alone kept dark; the system's own furniture (the clock, the
/// keyboard, a sheet) with it. Read as pixels, because the screens are told
/// their appearance apart from the window the system draws its furniture
/// in, and the two have to agree; and with the scene turned to the other
/// appearance as the system turns it to photograph the app on its way into
/// the background, which is what the app showed wrongly for a moment on
/// coming back after the system had changed (WindowLook).
///
/// Signed in as LobbyTouchTests are (TEST_RUNNER_TOMOSHIBI_SIGNIN). The
/// loading screen is held by a deployment that never answers, which the run
/// names in TEST_RUNNER_APPEARANCE_SILENT_BASE (a socket that takes the
/// connection and says nothing); without it that test is skipped. Every
/// reading is printed as LOOK-CHECK, and every screen read is kept in the
/// results. Laid out for an iPhone 18 Pro held upright: the status bar's
/// clock at the top left, an empty stretch of page low in the middle.
final class AppearanceTests: XCTestCase {
    enum Look: String {
        case light, dark

        var device: XCUIDevice.Appearance { self == .light ? .light : .dark }
        var other: Look { self == .light ? .dark : .light }
    }

    /// Every reading that came out wrong, failed once at the end of the
    /// test: each failure in a UI test sits for minutes collecting the
    /// accessibility hierarchy, and a run that is meant to show every wrong
    /// screen at once would take an hour.
    private var wrong: [String] = []

    override func setUp() {
        super.setUp()
        continueAfterFailure = true
        wrong = []
        XCUIDevice.shared.orientation = .portrait
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
        XCTAssertTrue(wrong.isEmpty, "\(wrong.count) wrong: " + wrong.joined(separator: "; "))
        super.tearDown()
    }

    // MARK: Launching

    private func launch(in look: Look, _ extra: [String: String] = [:]) -> XCUIApplication {
        XCUIDevice.shared.appearance = look.device
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_DEBUG_NO_CAMERA": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launch()
        return app
    }

    private func inLobby(_ app: XCUIApplication) {
        XCTAssertTrue(app.buttons["Settings"].waitForExistence(timeout: 30), "the lobby")
        settled()
    }

    private func inCall(_ app: XCUIApplication) {
        XCTAssertTrue(app.buttons["More"].waitForExistence(timeout: 30), "the call")
        settled()
    }

    private func leave(_ app: XCUIApplication) {
        app.buttons["Leave"].tap()
        let leave = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Leave the call")).firstMatch
        XCTAssertTrue(leave.waitForExistence(timeout: 5), "Leave's popover")
        leave.tap()
        XCTAssertTrue(app.staticTexts["You left the call."].waitForExistence(timeout: 10), "the screen after the call")
    }

    /// Long enough for a screen's arrival (Theme.settle) and the system's own
    /// change of appearance to have finished.
    private func settled(_ seconds: Double = 1.5) {
        Thread.sleep(forTimeInterval: seconds)
    }

    private func switchSystem(to look: Look) {
        XCUIDevice.shared.appearance = look.device
        settled()
    }

    /// Home, the system's appearance changed while the app is behind, and
    /// back to the app.
    private func switchSystemBehind(_ app: XCUIApplication, to look: Look) {
        XCUIDevice.shared.press(.home)
        settled(2)
        XCUIDevice.shared.appearance = look.device
        settled(2)
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10), "back in front")
        settled(2)
    }

    // MARK: Reading the screen

    /// The screen as pixels.
    private struct Shot {
        let width: Int
        let height: Int
        let bytes: [UInt8]

        init(_ screenshot: XCUIScreenshot) {
            let image = screenshot.image.cgImage!
            width = image.width
            height = image.height
            var bytes = [UInt8](repeating: 0, count: width * height * 4)
            bytes.withUnsafeMutableBytes { buffer in
                let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8,
                                        bytesPerRow: image.width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
                context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            }
            self.bytes = bytes
        }

        /// The luminance (0...255) of the pixel at x, y, counted from the top.
        func luminance(_ x: Int, _ y: Int) -> Double {
            let i = (y * width + x) * 4
            return 0.2126 * Double(bytes[i]) + 0.7152 * Double(bytes[i + 1]) + 0.0722 * Double(bytes[i + 2])
        }

        /// Every pixel's luminance in a rectangle given in fractions of the
        /// screen.
        func region(_ x0: Double, _ y0: Double, _ x1: Double, _ y1: Double) -> [Double] {
            var values: [Double] = []
            let (left, right) = (Int(x0 * Double(width)), min(width - 1, Int(x1 * Double(width))))
            let (top, bottom) = (Int(y0 * Double(height)), min(height - 1, Int(y1 * Double(height))))
            for y in stride(from: top, through: bottom, by: 1) {
                for x in stride(from: left, through: right, by: 1) { values.append(luminance(x, y)) }
            }
            return values
        }

        func mean(_ x0: Double, _ y0: Double, _ x1: Double, _ y1: Double) -> Double {
            let values = region(x0, y0, x1, y1)
            return values.reduce(0, +) / Double(max(1, values.count))
        }
    }

    private func shot(_ name: String) -> Shot {
        let screenshot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
        return Shot(screenshot)
    }

    private func look(of luminance: Double) -> Look { luminance > 128 ? .light : .dark }

    private func check(_ step: String, _ what: String, want: Look, got: Look, value: Double) {
        print("LOOK-CHECK \(name) \(step) \(what) want=\(want.rawValue) got=\(got.rawValue) value=\(Int(value))")
        if got != want { wrong.append("\(step): \(what) \(got.rawValue) (\(Int(value)))") }
    }

    /// The clock's words in the status bar, read as the colour furthest
    /// from the bar's own ground: dark words are the light appearance's.
    private func statusBar(_ shot: Shot, _ step: String, want: Look) {
        let values = shot.region(0.10, 0.015, 0.32, 0.055).sorted()
        let ground = values[values.count / 2]
        let words = abs(values.first! - ground) > abs(values.last! - ground) ? values.first! : values.last!
        check(step, "status bar", want: want, got: words < ground ? .light : .dark, value: words)
    }

    /// The page in an empty stretch of it, low on the screen, and above it
    /// under the status bar, where a bar of another colour was once seen
    /// across the top of a light lobby.
    private func page(_ shot: Shot, _ step: String, want: Look, top: Bool = true) {
        let low = shot.mean(0.3, 0.79, 0.7, 0.81)
        check(step, "page", want: want, got: look(of: low), value: low)
        if top {
            // Under the clock, and just below the status bar where a
            // navigation bar's ground would be.
            for (y0, y1) in [(0.058, 0.068), (0.078, 0.09)] {
                let high = shot.mean(0.02, y0, 0.08, y1)
                print("LOOK-CHECK \(name) \(step) page-top value=\(Int(high)) page=\(Int(low))")
                if abs(high - low) >= 6 { wrong.append("\(step): the top of the page is not the page (\(Int(high)) against \(Int(low)))") }
            }
        }
    }

    /// The keyboard, from the lobby's join bar: up, read, and put away.
    private func keyboard(_ app: XCUIApplication, _ step: String, want: Look) {
        let field = app.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10), "\(step): the join bar")
        field.tap()
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 1) { tip.tap() }
        let keys = app.keyboards.firstMatch
        XCTAssertTrue(keys.waitForExistence(timeout: 5), "\(step): the keyboard")
        settled(1)
        let screen = XCUIScreen.main.screenshot()
        let reading = Shot(screen)
        let attachment = XCTAttachment(screenshot: screen)
        attachment.name = "\(step) keyboard"
        attachment.lifetime = .keepAlways
        add(attachment)
        let frame = keys.frame
        let size = app.frame.size
        let value = reading.mean(frame.minX / size.width + 0.02, frame.minY / size.height + 0.01,
                                 frame.maxX / size.width - 0.02, frame.maxY / size.height - 0.05)
        check(step, "keyboard", want: want, got: look(of: value), value: value)
        statusBar(reading, step + " (typing)", want: want)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.538)).tap()
        XCTAssertTrue(keys.waitForNonExistence(timeout: 5), "\(step): the keyboard put away")
        settled(1)
    }

    /// The settings, a sheet, opened from the lobby and closed again.
    private func settingsSheet(_ app: XCUIApplication, _ step: String, want: Look) {
        app.buttons["Settings"].tap()
        let close = app.buttons["Close"]
        XCTAssertTrue(close.waitForExistence(timeout: 10), "\(step): the settings")
        settled()
        let reading = shot("\(step) settings")
        let value = reading.mean(0.005, 0.80, 0.025, 0.90)
        check(step, "settings sheet", want: want, got: look(of: value), value: value)
        statusBar(reading, step + " (settings)", want: want)
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 5), "\(step): the settings closed")
        settled()
    }

    /// The start sheet, full screen over the lobby, opened and closed.
    private func startSheet(_ app: XCUIApplication, _ step: String, want: Look) {
        app.buttons["Start a meeting"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Start"].waitForExistence(timeout: 10), "\(step): the start sheet")
        settled()
        let reading = shot("\(step) start sheet")
        let value = reading.mean(0.3, 0.85, 0.7, 0.87)
        check(step, "start sheet", want: want, got: look(of: value), value: value)
        statusBar(reading, step + " (start sheet)", want: want)
        app.buttons["Cancel"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Start"].waitForNonExistence(timeout: 5), "\(step): the start sheet closed")
        settled()
    }

    /// The lobby and everything the system draws over it.
    private func lobbyIs(_ app: XCUIApplication, _ step: String, _ want: Look, sheets: Bool = true) {
        let reading = shot("\(step) lobby")
        page(reading, step, want: want)
        statusBar(reading, step, want: want)
        keyboard(app, step, want: want)
        if sheets {
            settingsSheet(app, step, want: want)
            startSheet(app, step, want: want)
        }
    }

    private func endedIs(_ step: String, _ want: Look) {
        let reading = shot("\(step) ended")
        page(reading, step + " ended", want: want, top: false)
        statusBar(reading, step + " ended", want: want)
    }

    /// The call, dark whatever the system's appearance, its status bar too.
    private func callIsDark(_ step: String) {
        let reading = shot("\(step) call")
        let value = reading.mean(0.0, 0.06, 1.0, 0.09)
        check(step, "call", want: .dark, got: look(of: value), value: value)
        statusBar(reading, step + " call", want: .dark)
    }

    // MARK: The matrix

    /// Opened in each appearance, the lobby is in it from the start.
    func testLaunchInEitherAppearance() {
        for look in [Look.light, .dark] {
            let app = launch(in: look)
            inLobby(app)
            lobbyIs(app, "launched \(look.rawValue)", look)
            app.terminate()
        }
    }

    /// Changed with the app in front: the lobby, its sheets and the keyboard
    /// follow.
    func testSystemChangesInFront() {
        let app = launch(in: .light)
        inLobby(app)
        switchSystem(to: .dark)
        lobbyIs(app, "front to dark", .dark)
        switchSystem(to: .light)
        lobbyIs(app, "front to light", .light)
        // With the settings open, the sheet itself changes under the finger.
        app.buttons["Settings"].tap()
        XCTAssertTrue(app.buttons["Close"].waitForExistence(timeout: 10))
        settled()
        switchSystem(to: .dark)
        let reading = shot("settings open, to dark")
        let value = reading.mean(0.005, 0.80, 0.025, 0.90)
        check("settings open, to dark", "settings sheet", want: .dark, got: look(of: value), value: value)
        app.buttons["Close"].tap()
        settled()
        lobbyIs(app, "after the open sheet changed", .dark, sheets: false)
    }

    /// Changed with the app behind, and back to it.
    func testSystemChangesBehind() {
        let app = launch(in: .light)
        inLobby(app)
        switchSystemBehind(app, to: .dark)
        lobbyIs(app, "behind to dark", .dark)
        switchSystemBehind(app, to: .light)
        lobbyIs(app, "behind to light", .light)
    }

    /// Into a call and out of it: the call dark, and everything after it in
    /// the system's appearance again, following it as before.
    func testLeavingACallInEitherAppearance() {
        for look in [Look.light, .dark] {
            let app = launch(in: look, ["TOMOSHIBI_AUTOJOIN": "appearance-\(look.rawValue)-\(Int(Date().timeIntervalSince1970))",
                                        "TOMOSHIBI_NAME": "Day"])
            inCall(app)
            callIsDark("\(look.rawValue) call")
            leave(app)
            settled()
            endedIs("\(look.rawValue) after the call", look)
            app.buttons["Back"].tap()
            inLobby(app)
            lobbyIs(app, "\(look.rawValue) after the call", look)
            // And it still follows the system.
            switchSystem(to: look.other)
            lobbyIs(app, "\(look.rawValue) after the call, then \(look.other.rawValue)", look.other)
            switchSystem(to: look)
            lobbyIs(app, "\(look.rawValue) after the call, back to \(look.rawValue)", look, sheets: false)
            switchSystemBehind(app, to: look.other)
            lobbyIs(app, "\(look.rawValue) after the call, behind to \(look.other.rawValue)", look.other, sheets: false)
            app.terminate()
        }
    }

    /// The system changed during the call: the call stays dark, and what
    /// follows it is in the new appearance.
    func testSystemChangesDuringACall() {
        for look in [Look.light, .dark] {
            let app = launch(in: look, ["TOMOSHIBI_AUTOJOIN": "appearance-during-\(look.rawValue)-\(Int(Date().timeIntervalSince1970))",
                                        "TOMOSHIBI_NAME": "Day"])
            inCall(app)
            switchSystem(to: look.other)
            callIsDark("\(look.rawValue) call, changed to \(look.other.rawValue)")
            leave(app)
            settled()
            endedIs("\(look.rawValue) call changed to \(look.other.rawValue)", look.other)
            app.buttons["Back"].tap()
            inLobby(app)
            lobbyIs(app, "\(look.rawValue) call changed to \(look.other.rawValue)", look.other)
            switchSystem(to: look)
            lobbyIs(app, "\(look.rawValue) call changed, and back", look, sheets: false)
            app.terminate()
        }
    }

    /// Changed behind the app during a call, then out of the call.
    func testSystemChangesBehindDuringACall() {
        let app = launch(in: .light, ["TOMOSHIBI_AUTOJOIN": "appearance-behind-\(Int(Date().timeIntervalSince1970))",
                                      "TOMOSHIBI_NAME": "Day"])
        inCall(app)
        switchSystemBehind(app, to: .dark)
        callIsDark("call, behind to dark")
        switchSystemBehind(app, to: .light)
        callIsDark("call, behind to light")
        leave(app)
        settled()
        endedIs("call behind, left", .light)
        app.buttons["Back"].tap()
        inLobby(app)
        lobbyIs(app, "call behind, left", .light)
        app.terminate()
    }

    /// The scene turned to the other appearance and back with the screen's
    /// left alone, as the system turns it to photograph the app on its way
    /// into the background (TOMOSHIBI_DEBUG_PHOTOGRAPH): the screens turn
    /// with it, or the photograph that shows on coming back after the system
    /// has changed is of the other appearance.
    func testTheScreensTurnForThePhotograph() {
        for look in [Look.light, .dark] {
            let app = launch(in: look, ["TOMOSHIBI_DEBUG_PHOTOGRAPH": "12:6"])
            let launched = Date()
            inLobby(app)
            Thread.sleep(until: launched.addingTimeInterval(14.5))
            let during = shot("\(look.rawValue), photographed \(look.other.rawValue)")
            page(during, "\(look.rawValue), photographed \(look.other.rawValue)", want: look.other)
            statusBar(during, "\(look.rawValue), photographed \(look.other.rawValue)", want: look.other)
            Thread.sleep(until: launched.addingTimeInterval(21))
            let after = shot("\(look.rawValue), after the photograph")
            page(after, "\(look.rawValue), after the photograph", want: look)
            statusBar(after, "\(look.rawValue), after the photograph", want: look)
            app.terminate()
        }
    }

    /// The screen after a call, changed while it is shown.
    func testEndedScreenFollows() {
        let app = launch(in: .light, ["TOMOSHIBI_DEBUG_ENDED": "left", "TOMOSHIBI_ROOM": "appearance-ended"])
        XCTAssertTrue(app.staticTexts["You left the call."].waitForExistence(timeout: 30))
        settled()
        endedIs("ended, launched light", .light)
        switchSystem(to: .dark)
        endedIs("ended, to dark", .dark)
        switchSystem(to: .light)
        endedIs("ended, to light", .light)
        switchSystemBehind(app, to: .dark)
        endedIs("ended, behind to dark", .dark)
        app.terminate()
    }

    /// The loading screen, held by a deployment that never answers, changed
    /// while it is shown.
    func testLoadingScreenFollows() throws {
        guard let silent = ProcessInfo.processInfo.environment["APPEARANCE_SILENT_BASE"] else {
            throw XCTSkip("needs TEST_RUNNER_APPEARANCE_SILENT_BASE")
        }
        let app = launch(in: .light, ["TOMOSHIBI_BASE": silent, "TOMOSHIBI_SIGNIN": ""])
        XCTAssertTrue(app.activityIndicators.firstMatch.waitForExistence(timeout: 10), "the loading screen")
        settled()
        let first = shot("loading, launched light")
        page(first, "loading, launched light", want: .light, top: false)
        statusBar(first, "loading, launched light", want: .light)
        switchSystem(to: .dark)
        let dark = shot("loading, to dark")
        page(dark, "loading, to dark", want: .dark, top: false)
        statusBar(dark, "loading, to dark", want: .dark)
        switchSystem(to: .light)
        let light = shot("loading, to light")
        page(light, "loading, to light", want: .light, top: false)
        statusBar(light, "loading, to light", want: .light)
        XCTAssertTrue(app.activityIndicators.firstMatch.exists, "still loading")
        app.terminate()
    }
}
