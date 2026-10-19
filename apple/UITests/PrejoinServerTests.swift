import XCTest

/// The server picker on the screens before a call, pressed with a finger on
/// an iPhone, for somebody who came by a link rather than through the lobby:
/// an invitation, signed out and signed in, and a meeting's link. They never
/// see the lobby's picker, and before it was put here they could not choose.
///
/// Against a deployment with more than one relay and a relay out of service,
/// whose base the run hands down as TEST_RUNNER_TOMOSHIBI_BASE. The links
/// are the run's too, minted on that deployment just before: an invitation
/// as TEST_RUNNER_TOMOSHIBI_UITEST_INVITE, a meeting's link as
/// TEST_RUNNER_TOMOSHIBI_UITEST_MEETING, and an account to be signed in with
/// as TEST_RUNNER_TOMOSHIBI_UITEST_SIGNIN (name:file, as TOMOSHIBI_SIGNIN).
/// The relay names are the run's as well (TEST_RUNNER_TOMOSHIBI_UITEST_RELAYS,
/// the one to pick, then the one out of service).
final class PrejoinServerTests: XCTestCase {
    private var runner: [String: String] { ProcessInfo.processInfo.environment }

    private func opened(_ link: String, signedIn: Bool = false) throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080"]
        for (name, value) in runner where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        guard let link = runner[link] else { throw XCTSkip("the run gave no \(link)") }
        app.launchEnvironment["TOMOSHIBI_OPEN"] = link
        if signedIn {
            guard let account = runner["TOMOSHIBI_UITEST_SIGNIN"] else { throw XCTSkip("the run gave no account") }
            app.launchEnvironment["TOMOSHIBI_SIGNIN"] = account
        }
        app.launch()
        return app
    }

    private var relays: (pick: String, resting: String) {
        let names = (runner["TOMOSHIBI_UITEST_RELAYS"] ?? "beta,gamma").split(separator: ",").map(String.init)
        return (names[0], names[1])
    }

    private func button(_ app: XCUIApplication, beginning words: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", words)).firstMatch
    }

    /// The row's button, "Automatic" with the fastest reading beside it, opens
    /// the lobby's list; the relay out of service is in it and cannot be
    /// chosen; a relay pressed is the one the button then says.
    private func choosesAServer(_ app: XCUIApplication) {
        XCTAssertTrue(app.staticTexts["Server"].waitForExistence(timeout: 30), "the server row on the screen before the call")
        let automatic = button(app, beginning: "Automatic")
        XCTAssertTrue(automatic.waitForExistence(timeout: 5), "the picker, on Automatic")
        automatic.tap()

        XCTAssertTrue(button(app, beginning: "Automatic, Whichever answers fastest").waitForExistence(timeout: 10),
                      "the list opened by a finger, with Automatic first")
        let resting = button(app, beginning: relays.resting)
        XCTAssertTrue(resting.exists, "the relay out of service is listed")
        XCTAssertFalse(resting.isEnabled, "and cannot be chosen")

        let pick = button(app, beginning: relays.pick)
        XCTAssertTrue(pick.exists, "a relay to choose")
        pick.tap()
        XCTAssertTrue(button(app, beginning: relays.pick).waitForExistence(timeout: 5), "the button says the relay chosen")
        XCTAssertFalse(button(app, beginning: "Automatic").exists, "and no longer Automatic")
    }

    func testAnInvitationOffersTheServerPicker() throws {
        choosesAServer(try opened("TOMOSHIBI_UITEST_INVITE"))
    }

    // Signed in, an invitation's join screen used to leave the row out, on
    // the reasoning that somebody signed in had chosen in the lobby; they
    // came from the link, and had not.
    func testAnInvitationOffersItSignedInToo() throws {
        choosesAServer(try opened("TOMOSHIBI_UITEST_INVITE", signedIn: true))
    }

    func testAMeetingLinkOffersItWhileWaiting() throws {
        let app = try opened("TOMOSHIBI_UITEST_MEETING")
        XCTAssertTrue(app.buttons["Wait for the host"].waitForExistence(timeout: 30), "the meeting's waiting screen")
        choosesAServer(app)
    }
}
