import XCTest

/// The deployment's notice at the head of the iPhone's lobby and sign-in,
/// with and without a newer build said under it, pressed and looked at with
/// real touches and the phone turned. The local deployment has nothing to
/// say to the apps and no build to offer, so both are handed to the app
/// itself (TOMOSHIBI_DEBUG_REMOTE, TOMOSHIBI_DEBUG_UPDATE_OFFER, AppModel);
/// signed in as LobbyTouchTests are, with TEST_RUNNER_TOMOSHIBI_SIGNIN.
///
/// What is guarded is what a unit test cannot see: a close that is drawn
/// and cannot be pressed, and two cards at the top of one page lying one
/// over the other. The update was laid over the iPhone's page under its
/// title, and with a notice there it covered the notice. That a closed
/// notice stays closed across starts is RemoteTests', since a rehearsal
/// keeps nothing. Each test leaves pictures of what it saw in its results.
final class LobbyNoticeTests: XCTestCase {
    private static let notice = #"{ "revision": "ui", "notice": { "id": "ui-ID", "level": "warning", "text": { "en": "The servers are down for maintenance tonight from 22:00 to 23:00." } } }"#
    private static let nothing = #"{ "revision": "ui" }"#

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
        super.tearDown()
    }

    private func launch(saying answer: String, offering version: String? = nil, signedIn: Bool = true) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        if !signedIn { app.launchEnvironment["TOMOSHIBI_SIGNIN"] = nil }
        // A new id each time, so that nothing closed before is in the way.
        app.launchEnvironment["TOMOSHIBI_DEBUG_REMOTE"] = answer.replacingOccurrences(of: "ID", with: UUID().uuidString)
        app.launchEnvironment["TOMOSHIBI_DEBUG_UPDATE_OFFER"] = version
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        return app
    }

    /// The cards come in with Theme.settle, a third of a second; the page
    /// turning takes longer.
    private func settled() {
        Thread.sleep(forTimeInterval: 1.2)
    }

    private func turned(_ orientation: UIDeviceOrientation) {
        XCUIDevice.shared.orientation = orientation
        Thread.sleep(forTimeInterval: 2)
    }

    private func shoot(_ name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    /// Both drawn, neither over the other, and `upper` wholly above `lower`
    /// unless `beside` (the iPhone on its side puts the cards in the column
    /// beside the preview).
    private func apart(_ upper: XCUIElement, _ lower: XCUIElement, _ said: String, beside: Bool = false,
                       file: StaticString = #filePath, line: UInt = #line) {
        let a = upper.frame, b = lower.frame
        XCTAssertFalse(a.isEmpty || b.isEmpty, "\(said): not both drawn (\(a), \(b))", file: file, line: line)
        XCTAssertFalse(a.intersects(b), "\(said): \(a) and \(b) lie one over the other", file: file, line: line)
        if !beside {
            XCTAssertLessThanOrEqual(a.maxY, b.minY + 0.5, "\(said): \(a) is not above \(b)", file: file, line: line)
        }
    }

    func testTheNoticeIsClosedWithAFinger() throws {
        let app = launch(saying: Self.notice)
        let notice = app.otherElements["lobby.notice"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30), "the notice, under the lobby's title")
        let close = app.buttons["lobby.notice.close"]
        XCTAssertTrue(close.isHittable, "the notice's close cannot be pressed")
        close.tap()
        XCTAssertTrue(notice.waitForNonExistence(timeout: 5), "a tap on the close put the notice away")
        XCTAssertTrue(app.buttons["Start a meeting"].isHittable, "the lobby under it")
    }

    func testTheNoticeAndAnUpdateStandOneAboveTheOther() throws {
        let app = launch(saying: Self.notice, offering: "9.9.9")
        let notice = app.otherElements["lobby.notice"]
        let update = app.otherElements["update.banner"]
        let start = app.buttons["Start a meeting"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30), "the notice")
        XCTAssertTrue(update.waitForExistence(timeout: 30), "the update")
        settled()
        apart(notice, update, "upright")
        apart(update, start, "upright, the update and the preview's Start")
        shoot("both, upright")

        turned(.landscapeLeft)
        apart(notice, update, "on its side")
        apart(update, start, "on its side, the update and the preview's Start", beside: true)
        XCTAssertTrue(start.isHittable, "on its side, the preview's Start cannot be pressed")
        shoot("both, on its side")
        turned(.portrait)

        // The notice closed: the update moves up into its place.
        settled()
        let before = update.frame.minY
        app.buttons["lobby.notice.close"].tap()
        XCTAssertTrue(notice.waitForNonExistence(timeout: 5))
        settled()
        XCTAssertTrue(update.exists, "the update went with the notice")
        XCTAssertLessThan(update.frame.minY, before - 20, "the update did not move up into the notice's place")
        apart(update, start, "the update alone")
        shoot("the update alone, upright")
        turned(.landscapeLeft)
        apart(update, start, "the update alone on its side", beside: true)
        shoot("the update alone, on its side")
    }

    func testTheNoticeAloneAndNeitherLeaveThePageWhereItWas() throws {
        let app = launch(saying: Self.notice)
        let notice = app.otherElements["lobby.notice"]
        let start = app.buttons["Start a meeting"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30), "the notice")
        settled()
        XCTAssertFalse(app.otherElements["update.banner"].exists, "an update with none offered")
        apart(notice, start, "upright")
        shoot("the notice alone, upright")
        turned(.landscapeLeft)
        apart(notice, start, "on its side", beside: true)
        shoot("the notice alone, on its side")
        turned(.portrait)

        app.terminate()
        let bare = launch(saying: Self.nothing)
        let bareStart = bare.buttons["Start a meeting"]
        XCTAssertTrue(bareStart.waitForExistence(timeout: 30), "the lobby")
        settled()
        XCTAssertFalse(bare.otherElements["lobby.notice"].exists)
        XCTAssertFalse(bare.otherElements["update.banner"].exists)
        shoot("neither, upright")
    }

    func testTheSignInSaysBothOneAboveTheOther() throws {
        let app = launch(saying: Self.notice, offering: "9.9.9", signedIn: false)
        let notice = app.otherElements["lobby.notice"]
        let update = app.otherElements["update.banner"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30), "the notice, to somebody not signed in")
        XCTAssertTrue(update.waitForExistence(timeout: 30), "the update, to somebody not signed in")
        settled()
        apart(notice, update, "the sign-in, upright")
        let language = app.buttons["Language"]
        XCTAssertTrue(language.exists, "the language menu in the corner")
        XCTAssertFalse(language.frame.intersects(notice.frame), "the notice lies over the language menu")
        apart(update, app.staticTexts["Sign in"].firstMatch, "the sign-in, the update and the heading")
        shoot("the sign-in, both, upright")
        turned(.landscapeLeft)
        apart(notice, update, "the sign-in, on its side")
        shoot("the sign-in, both, on its side")
    }
}
