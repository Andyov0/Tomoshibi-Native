import XCTest

/// The screens before a call, pressed with real touches on an iPhone: the
/// keyboard going away, and a room from the recent list. Signed in, which
/// these screens need: the run passes TEST_RUNNER_TOMOSHIBI_SIGNIN, an
/// account of the local deployment's and the file its passphrase is in.
final class LobbyTouchTests: XCTestCase {
    private func lobby(_ extra: [String: String] = [:]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launch()
        return app
    }

    /// The system's own first-use card about sliding across the keys covers
    /// the keyboard the first time it opens.
    private func pastTheKeyboardTip(_ app: XCUIApplication) {
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 1) { tip.tap() }
    }

    private func typingInTheJoinBar(_ app: XCUIApplication) {
        let field = app.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 30), "the lobby's join bar")
        field.tap()
        pastTheKeyboardTip(app)
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), "the keyboard, up")
    }

    func testKeyboardGoesOnATapOnNothing() throws {
        let app = lobby()
        typingInTheJoinBar(app)
        // Between the camera card and the join bar: with the keyboard up the
        // rest of the lower half is keys.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.538)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 5),
                      "a tap on the empty lobby puts the keyboard away")
    }

    func testKeyboardGoesOnATapOnTheCameraCard() throws {
        let app = lobby()
        typingInTheJoinBar(app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.3)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 5),
                      "a tap on the camera card puts the keyboard away")
    }

    func testKeyboardGoesInTheSheet() throws {
        // The sheet opened with a name typed, which takes the keyboard.
        let app = lobby(["TOMOSHIBI_DEBUG_LOBBY": "start", "TOMOSHIBI_DEBUG_ROOM": "keyboard-check"])
        pastTheKeyboardTip(app)
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 30), "the keyboard, up in the sheet")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.12)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 5),
                      "a tap above the sheet puts the keyboard away")
    }

    func testRecentRoomThatEndedStartsAgain() throws {
        // A name nobody is in: the sheet settles on Start, and Start goes in.
        let room = "again-\(Int(Date().timeIntervalSince1970))"
        let app = lobby(["TOMOSHIBI_DEBUG_LOBBY": "again", "TOMOSHIBI_DEBUG_ROOM": room])
        pastTheKeyboardTip(app)
        let start = app.buttons["Start"]
        XCTAssertTrue(start.waitForExistence(timeout: 30), "a recent room with no meeting in it offers Start")
        start.tap()
        XCTAssertTrue(app.buttons["More"].waitForExistence(timeout: 30), "and Start goes into the room")
    }

    func testRecentRoomCanBeRemoved() throws {
        let app = lobby(["TOMOSHIBI_DEBUG_RECENTS": "old-standup,design-review"])
        let cross = app.buttons["Remove old-standup from recent"]
        XCTAssertTrue(cross.waitForExistence(timeout: 30), "a cross on the recent room")
        cross.tap()
        XCTAssertTrue(app.staticTexts["old-standup"].waitForNonExistence(timeout: 5), "the room leaves the recent list")
        XCTAssertTrue(app.staticTexts["design-review"].exists, "and the other stays")
    }

    func testJoinBarOpensAPastedLink() throws {
        let app = lobby()
        typingInTheJoinBar(app)
        app.textFields.firstMatch.typeText("http://127.0.0.1:8080/#/pasted-link-room")
        app.buttons["Join"].firstMatch.tap()
        // The link's room, on the screen before a call, as a link handed over
        // by the system opens it.
        XCTAssertTrue(app.staticTexts["pasted-link-room"].waitForExistence(timeout: 10),
                      "a link pasted into the join bar opens its room")
    }
}

