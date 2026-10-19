import XCTest

/// What is open and what is typed outlive an iPhone Duo opening and closing.
///
/// A test cannot fold the simulator: the Duo simulator folds from Device
/// Hub's buttons, and SpringBoard refuses a pose from anything else on a build
/// that is not Apple's own. What it can do is what opening does to the app:
/// move its scene from the outer display to another one, where it is wide,
/// and back. The run's driver, apple/fold-ui-tests.sh, does that with
/// `devicectl device appResize` when a test says it is ready (the
/// `FOLD-STEP` lines this prints, each at the start of a line of its own),
/// and the test waits for its window to move there and back. Run with
/// TEST_RUNNER_FOLD_DRIVEN=1 by that driver; without it each test is
/// skipped, since nothing would ever open the phone.
///
/// Against the local deployment on 127.0.0.1:8080, signed in where the lobby
/// needs it (TEST_RUNNER_TOMOSHIBI_SIGNIN).
final class FoldStateTests: XCTestCase {
    private var driven: Bool { ProcessInfo.processInfo.environment["FOLD_DRIVEN"] != nil }

    private func launch(_ extra: [String: String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
        app.launch()
        return app
    }

    private func pastTheKeyboardTip(_ app: XCUIApplication) {
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 2) { tip.tap() }
    }

    /// The account page's field for the passphrase in use, known by its
    /// prompt: it carries no label of its own.
    private func passphrase(_ app: XCUIApplication) -> XCUIElement {
        app.secureTextFields.matching(NSPredicate(format: "placeholderValue == %@", "Current passphrase")).firstMatch
    }

    /// The app's window on the outer display, before the phone is opened.
    /// The window is told apart by its frame alone: on the display the app
    /// moves to, the frame is reported at another scale, so its width says
    /// nothing about the size class it was given.
    private var folded: CGRect = .zero

    private func window(_ app: XCUIApplication) -> CGRect { app.windows.firstMatch.frame }

    /// Asks the driver to open the phone, then waits for the app to move.
    private func open(_ app: XCUIApplication) {
        folded = window(app)
        print("FOLD-STEP open")
        let deadline = Date().addingTimeInterval(90)
        while window(app) == folded && Date() < deadline { usleep(500_000) }
        XCTAssertNotEqual(window(app), folded, "the app moved to the opened display")
        sleep(3)
    }

    /// Asks the driver to close the phone again, then waits for the app to be
    /// back on the outer display.
    private func close(_ app: XCUIApplication) {
        print("FOLD-STEP close")
        let deadline = Date().addingTimeInterval(90)
        while window(app) != folded && Date() < deadline { usleep(500_000) }
        XCTAssertEqual(window(app), folded, "the app came back to the outer display")
        sleep(3)
    }

    func testTheJoinBarKeepsWhatIsTyped() throws {
        try XCTSkipUnless(driven, "needs the fold driver")
        continueAfterFailure = false
        let app = launch([:])
        let field = app.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 30), "the lobby's join bar")
        field.tap()
        pastTheKeyboardTip(app)
        field.typeText("half-typed-room")
        open(app)
        // Opened, the lobby is the window with a sidebar.
        XCTAssertTrue(app.buttons["Start"].waitForExistence(timeout: 10), "the sidebar's Start")
        close(app)
        let again = app.textFields.firstMatch
        XCTAssertTrue(again.waitForExistence(timeout: 10), "the join bar, back")
        XCTAssertEqual(again.value as? String, "half-typed-room", "what was typed into the join bar")
    }

    func testTheSettingsKeepTheirSectionAndWhatIsTyped() throws {
        try XCTSkipUnless(driven, "needs the fold driver")
        continueAfterFailure = false
        let app = launch(["TOMOSHIBI_DEBUG_LOBBY": "settings", "TOMOSHIBI_DEBUG_SETTINGS": "account"])
        let current = passphrase(app)
        XCTAssertTrue(current.waitForExistence(timeout: 30), "the account page's passphrase field")
        current.tap()
        pastTheKeyboardTip(app)
        current.typeText("half-typed")
        open(app)
        // Wide, the settings are a sidebar beside the same page.
        XCTAssertTrue(app.buttons["General"].waitForExistence(timeout: 10), "the settings' sidebar")
        XCTAssertEqual((passphrase(app).value as? String)?.count, 10,
                       "the passphrase half typed, beside the sidebar")
        close(app)
        XCTAssertTrue(passphrase(app).waitForExistence(timeout: 10), "the account page, back")
        XCTAssertEqual((passphrase(app).value as? String)?.count, 10,
                       "the passphrase half typed, on the phone again")
    }

    func testTheStartSheetKeepsItsRoom() throws {
        try XCTSkipUnless(driven, "needs the fold driver")
        continueAfterFailure = false
        let app = launch(["TOMOSHIBI_DEBUG_LOBBY": "start"])
        let room = app.textFields.firstMatch
        XCTAssertTrue(room.waitForExistence(timeout: 30), "the start sheet's room")
        room.tap()
        pastTheKeyboardTip(app)
        room.typeText("folded-in-the-sheet")
        open(app)
        XCTAssertEqual(app.textFields.firstMatch.value as? String, "folded-in-the-sheet", "the room, opened out")
        close(app)
        XCTAssertEqual(app.textFields.firstMatch.value as? String, "folded-in-the-sheet", "the room, folded again")
        XCTAssertTrue(app.buttons["Start"].exists, "the sheet still open")
    }

    func testACallKeepsTheMessageBeingWritten() throws {
        try XCTSkipUnless(driven, "needs the fold driver")
        continueAfterFailure = false
        let app = launch(["TOMOSHIBI_AUTOJOIN": "fold-state-call", "TOMOSHIBI_NAME": "Fold Test",
                          "TOMOSHIBI_DEBUG_PANEL": "messages"])
        let say = app.textFields["Say something"]
        XCTAssertTrue(say.waitForExistence(timeout: 40), "the messages panel")
        say.tap()
        pastTheKeyboardTip(app)
        say.typeText("written before the fold")
        open(app)
        XCTAssertEqual(app.textFields["Say something"].value as? String, "written before the fold", "the draft, opened out")
        XCTAssertTrue(app.buttons["Leave"].exists, "still in the call")
        close(app)
        XCTAssertEqual(app.textFields["Say something"].value as? String, "written before the fold", "the draft, folded again")
        XCTAssertTrue(app.buttons["Leave"].exists, "still in the call")
    }
}
