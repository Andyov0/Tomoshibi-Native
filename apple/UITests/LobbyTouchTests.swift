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

    func testRecentRoomIsRemovedBySwipingIt() throws {
        let app = lobby(["TOMOSHIBI_DEBUG_RECENTS": "old-standup,design-review"])
        let row = app.staticTexts["old-standup"]
        XCTAssertTrue(row.waitForExistence(timeout: 30), "the recent room")
        let remove = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Remove")).firstMatch
        XCTAssertFalse(remove.exists, "Remove is out of sight until the row is swiped")
        row.swipeLeft()
        XCTAssertTrue(remove.waitForExistence(timeout: 3), "swiped to the left, the row shows Remove")
        remove.tap()
        XCTAssertTrue(row.waitForNonExistence(timeout: 5), "the room leaves the recent list")
        XCTAssertTrue(app.staticTexts["design-review"].exists, "and the other stays")
    }

    /// The scheduled meetings on a phone, pressed with a finger: a meeting's
    /// copy button, Cancel out of sight until the meeting is swiped to the
    /// left and then pressed, and the page's own close. Needs a meeting
    /// called "swipe-check" arranged for the signed-in account; the run
    /// arranges it through the deployment before it starts.
    func testScheduledMeetingIsCancelledBySwipingIt() throws {
        continueAfterFailure = false
        let app = lobby(["TOMOSHIBI_DEBUG_LOBBY": "scheduled"])
        let meeting = app.cells.containing(.staticText, identifier: "swipe-check").firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 30), "the arranged meeting in the list")
        XCTAssertFalse(app.buttons["Cancel meeting"].exists, "Cancel out of sight before the swipe")
        meeting.buttons["Copy link"].tap()
        // "Link copied" for 1.6 s, then the button is Copy link again.
        let copied = meeting.buttons["Link copied"].waitForExistence(timeout: 1.5)
        XCTAssertTrue(copied, "the copy button pressed")
        meeting.swipeLeft()
        let cancel = app.buttons["Cancel meeting"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5), "Cancel, shown by the swipe")
        cancel.tap()
        XCTAssertTrue(app.staticTexts["swipe-check"].waitForNonExistence(timeout: 10), "the meeting called off")
        app.buttons["Close"].tap()
        XCTAssertTrue(app.staticTexts["Pick a time and hand out the link before it starts."].waitForNonExistence(timeout: 5),
                      "the page closed by its own close")
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

    /// Sign out asks first, from the account page of the settings; Cancel
    /// takes the question away and leaves the account signed in. The marks
    /// go to the log, for a recording of the screen to be cut at them (that
    /// is how the card going dark under the glass on Cancel was found).
    func testSignOutCancelKeepsTheAccount() throws {
        let app = lobby(["TOMOSHIBI_DEBUG_SETTINGS": "account"])
        let settings = app.buttons["Settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 30), "the lobby's settings button")
        settings.tap()
        let signOut = app.buttons["Sign out"]
        XCTAssertTrue(signOut.waitForExistence(timeout: 10), "the account page's Sign out")
        sleep(1)
        print("MOTION-MARK ask \(Date().timeIntervalSince1970)")
        signOut.tap()
        let cancel = app.buttons["signout.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5), "the question, asked")
        sleep(1)
        print("MOTION-MARK cancel \(Date().timeIntervalSince1970)")
        cancel.tap()
        XCTAssertTrue(cancel.waitForNonExistence(timeout: 3), "the question, gone on Cancel")
        sleep(1)
        XCTAssertTrue(app.buttons["Sign out"].exists, "still signed in, the account page still there")
    }
}
