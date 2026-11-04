import XCTest

/// The controls the screens stand in the system's column down the side of an
/// iPhone Duo's outer display (SystemColumn.swift), pressed with a finger.
/// They are drawn outside the screen's safe area, under the system's own
/// clock and beside its status bar, and only a real touch says whether one
/// drawn there can be pressed at all: a debug press reaches the model and not
/// the hit testing. Each test also says the control it pressed is in the
/// column, not the copy the screen keeps in its content elsewhere.
///
/// Only on the iPhone Duo, by the simulator's name for the device rather
/// than by whether a column was found: a test that skipped wherever the
/// column had gone missing would pass with the column broken. Against the
/// local deployment, signed in where the lobby needs it
/// (TEST_RUNNER_TOMOSHIBI_SIGNIN), as the lobby's other tests are.
final class SystemColumnTests: XCTestCase {
    private var runner: [String: String] { ProcessInfo.processInfo.environment }

    private func launch(_ extra: [String: String], signedIn: Bool = true) throws -> XCUIApplication {
        try XCTSkipUnless((runner["SIMULATOR_DEVICE_NAME"] ?? "").contains("Duo"),
                          "the column is the iPhone Duo's outer display's")
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in runner where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        if !signedIn { app.launchEnvironment["TOMOSHIBI_SIGNIN"] = nil }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
        app.launch()
        return app
    }

    /// The control, once it is there, and in the column: its middle in the
    /// last 84 points of the window, where the outer display's column is.
    private func inColumn(_ element: XCUIElement, _ app: XCUIApplication, _ what: String,
                          file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.waitForExistence(timeout: 30), what, file: file, line: line)
        let window = app.windows.firstMatch.frame
        XCTAssertGreaterThan(element.frame.midX, window.maxX - 84, "\(what), in the column", file: file, line: line)
    }

    private func pastTheKeyboardTip(_ app: XCUIApplication) {
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 2) { tip.tap() }
    }

    /// The lobby's Join, at the column's foot, opens the join sheet, whose
    /// field takes a pasted link where the phone's join bar took it; the
    /// sheet's close, under the clock, takes it down again.
    func testTheLobbysJoinOpensTheJoinSheet() throws {
        continueAfterFailure = false
        let app = try launch([:])
        let join = app.buttons["Join a meeting"]
        inColumn(join, app, "the lobby's Join")
        XCTAssertFalse(app.textFields["Room name or invite link"].exists, "no join bar in the page beside the column")
        join.tap()
        let cancel = app.buttons["Cancel"]
        inColumn(cancel, app, "the join sheet's close")
        let field = app.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5), "the join sheet's room")
        cancel.tap()
        XCTAssertTrue(field.waitForNonExistence(timeout: 5), "the sheet closed by the column's close")
        join.tap()
        XCTAssertTrue(app.textFields.firstMatch.waitForExistence(timeout: 5), "the join sheet, again")
        app.textFields.firstMatch.tap()
        pastTheKeyboardTip(app)
        // A link of the deployment the app is pointed at; another server's
        // is refused.
        let base = app.launchEnvironment["TOMOSHIBI_BASE"] ?? "http://127.0.0.1:8080"
        app.textFields.firstMatch.typeText("\(base)/#/column-link-room")
        app.buttons["Join"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["column-link-room"].waitForExistence(timeout: 10),
                      "a link pasted into the sheet opens its room")
    }

    /// The account, which the system stands under its clock, opens the
    /// settings, and the settings' close in the sheet's own column closes
    /// them.
    func testTheSettingsOpenAndCloseFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launch([:])
        let settings = app.buttons["Settings"]
        inColumn(settings, app, "the account, under the clock")
        settings.tap()
        let close = app.buttons["Close"]
        inColumn(close, app, "the settings' close")
        let general = app.staticTexts["General"]
        XCTAssertTrue(general.waitForExistence(timeout: 5), "the settings, open")
        close.tap()
        XCTAssertTrue(general.waitForNonExistence(timeout: 5), "the settings closed by the column's close")
    }

    /// The join screen's microphone and its way back, in the column: the
    /// microphone turns on and off, and Back leaves the screen.
    func testTheJoinScreensSwitchesAndBackAreInTheColumn() throws {
        continueAfterFailure = false
        let app = try launch(["TOMOSHIBI_PREPARE": "column-switches"], signedIn: false)
        let unmute = app.buttons["Unmute microphone"]
        inColumn(unmute, app, "the microphone, off")
        unmute.tap()
        let mute = app.buttons["Mute microphone"]
        inColumn(mute, app, "the microphone, on")
        mute.tap()
        XCTAssertTrue(app.buttons["Unmute microphone"].waitForExistence(timeout: 5), "and off again")
        let back = app.buttons["Back"]
        inColumn(back, app, "the way back")
        back.tap()
        XCTAssertTrue(app.staticTexts["column-switches"].waitForNonExistence(timeout: 10), "the join screen left")
    }

    /// The language, under the clock on the sign-in, opens its menu beside
    /// the column, and a choice in it closes it.
    func testTheLanguageMenuOpensFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launch([:], signedIn: false)
        let language = app.buttons["Language"]
        inColumn(language, app, "the language")
        language.tap()
        // The item's label carries the language the system is in after
        // its title.
        let same = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Same as the system")).firstMatch
        XCTAssertTrue(same.waitForExistence(timeout: 5), "the language menu, open")
        same.tap()
        XCTAssertTrue(same.waitForNonExistence(timeout: 5), "closed by the choice")
    }

    /// A Live Activity grows the camera's region down the column into the
    /// Dynamic Island (music playing: 90 points on the simulator, more on a
    /// phone), stood in for here 330 points further: the lobby keeps its
    /// controls in the column rather than giving it up for the join bar, its
    /// Start in sight at the capsule's foot and the rest a scroll away.
    func testTheLobbysColumnKeepsItsControlsUnderAGrownIsland() throws {
        continueAfterFailure = false
        let app = try launch(["TOMOSHIBI_DEBUG_ISLAND": "330"])
        let start = app.buttons["Start a meeting"]
        inColumn(start, app, "the lobby's Start, under the grown island")
        XCTAssertTrue(start.isHittable, "Start in sight")
        XCTAssertFalse(app.textFields["Room name or invite link"].exists, "no join bar: the column is kept")
        let join = app.buttons["Join a meeting"]
        XCTAssertFalse(join.isHittable, "Join scrolled out of the capsule's sight")
        // A finger drawn slowly down the capsule a button's length and a
        // half. Drawn at once from one point to the other, the drag reached
        // the capsule as a press on Start, and opened the start sheet.
        start.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3))
            .press(forDuration: 0.1, thenDragTo: start.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 1.8)),
                   withVelocity: .slow, thenHoldForDuration: 0.1)
        for _ in 0..<20 where !join.isHittable { usleep(500_000) }
        XCTAssertTrue(join.isHittable, "Join in sight after a scroll")
        join.tap()
        inColumn(app.buttons["Cancel"], app, "the join sheet's close")
    }
}
