import XCTest

/// Tomoshibi in Messages, pressed with real touches in the simulator's own
/// Messages: the extension opened from the conversation's apps, a meeting
/// started for now and sent as a card, the card pressed open, and Join taking
/// the app into the meeting it names.
///
/// What it needs, which a run sets up before it starts:
///
/// - the local deployment on 127.0.0.1:8080, with an account allowed to
///   arrange meetings, named with the file its passphrase is in as
///   TEST_RUNNER_TOMOSHIBI_SIGNIN=<name>:<file>, and one meeting arranged for
///   it called "design-review";
/// - the same switches in the simulator's own environment
///   (`xcrun simctl spawn <udid> launchctl setenv TOMOSHIBI_BASE
///   http://127.0.0.1:8080`, and TOMOSHIBI_SIGNIN, TOMOSHIBI_MUTE=1 and
///   TOMOSHIBI_DEBUG_HANDOVER=1 likewise), because Join has the system open
///   the app, and an app the system starts gets that environment and not the
///   test's. Without them a debug build opened that way goes to home.
///
/// The extension itself is started by Messages with no environment at all,
/// which is why the app is launched first: signed in, it leaves the account
/// and the test deployment's address for the extension (AppModel `handOver`).
final class MessagesInviteTests: XCTestCase {
    private func shot(_ name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    private func tomoshibi() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080", "TOMOSHIBI_DEBUG_HANDOVER": "1"]
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        return app
    }

    /// Messages, in its first conversation, with Tomoshibi opened from the
    /// conversation's apps (the "+" beside the field, then the list, which is
    /// scrolled until Tomoshibi is clear of the keyboard).
    private func openTheExtension(_ messages: XCUIApplication) {
        // Started afresh: Messages left with the extension open shows it again
        // on coming back, and an extension the run has just reinstalled is
        // drawn there as a blank pane.
        messages.launch()
        let add = messages.buttons["add"]
        if !add.waitForExistence(timeout: 3) {
            messages.cells.firstMatch.tap()
            XCTAssertTrue(add.waitForExistence(timeout: 5), "a conversation, with its apps button")
        }
        if messages.buttons["Continue"].exists { messages.buttons["Continue"].tap() }
        add.tap()
        let apps = messages.cells.matching(NSPredicate(format: "identifier CONTAINS %@", "MSMessageExtensionBalloonPlugin"))
        XCTAssertTrue(apps.firstMatch.waitForExistence(timeout: 5), "the conversation's apps")
        let ours = messages.cells.matching(NSPredicate(format: "identifier CONTAINS %@", "app.tomoshibi.ios.messages")).firstMatch
        var swipes = 0
        while (!ours.exists || ours.frame.maxY > messages.frame.maxY - 150) && swipes < 6 {
            apps.element(boundBy: 2).swipeUp()
            swipes += 1
        }
        XCTAssertTrue(ours.exists, "Tomoshibi among the conversation's apps")
        // Pressed where it is drawn: the keyboard's window lies over the list
        // for the accessibility hierarchy, so the cell never reads as hittable.
        ours.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
    }

    func testAMeetingStartedFromMessagesIsSentAndJoined() throws {
        continueAfterFailure = false
        let app = tomoshibi()
        app.launch()
        // The lobby, which is for somebody signed in; the meeting may be
        // further down its short list than earlier runs' meetings for now.
        XCTAssertTrue(app.buttons["Start a meeting"].waitForExistence(timeout: 30), "the app, signed in")
        shot("01-app-signed-in")
        XCUIDevice.shared.press(.home)

        let messages = XCUIApplication(bundleIdentifier: "com.apple.MobileSMS")
        openTheExtension(messages)
        let start = messages.buttons["Start a meeting"]
        XCTAssertTrue(start.waitForExistence(timeout: 15), "the extension, signed in as the app is")
        XCTAssertTrue(messages.staticTexts["design-review"].waitForExistence(timeout: 10), "the account's meeting, to send")
        sleep(1)
        shot("02-extension-compact")

        start.tap()
        let send = messages.buttons["sendButton"]
        XCTAssertTrue(send.waitForExistence(timeout: 15), "the card, in the field to send")
        sleep(1)
        shot("03-card-in-the-field")
        send.tap()
        sleep(2)
        shot("04-card-sent")

        // The card as the conversation shows it, pressed as a friend would.
        // Messages draws a card as a link labelled with its captions, the
        // room first, beside the link sent with it as text; the newest card
        // is the last of them.
        let cards = messages.collectionViews["TranscriptCollectionView"].links
            .matching(NSPredicate(format: "label MATCHES %@", "^[a-z]+-[a-z]+-[a-z]+-[0-9]{4},.*"))
        XCTAssertGreaterThan(cards.count, 0, "the card in the conversation")
        let card = cards.element(boundBy: cards.count - 1)
        let room = card.label.components(separatedBy: ",").first ?? ""
        print("STEP card \(card.label)")
        card.tap()
        let join = messages.buttons["Join"]
        XCTAssertTrue(join.waitForExistence(timeout: 15), "the card opened, with Join")
        sleep(2)
        shot("05-card-opened")

        join.tap()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15), "Join brings the app forward")
        let begin = app.buttons["Start the meeting"]
        XCTAssertTrue(begin.waitForExistence(timeout: 20), "the app on the meeting the card named, for its host to start")
        sleep(1)
        shot("06-app-on-the-meeting")
        begin.tap()
        XCTAssertTrue(app.buttons["More"].waitForExistence(timeout: 30), "and into the call")
        sleep(2)
        shot("07-in-the-call")

        // TOMOSHIBI_GUEST=<name>: somebody without the app opens the card's
        // link in a browser and joins under that name, which the run drives
        // once this says the host is in (the web's own page, as a friend
        // without the app would see it); the host's app sees them arrive.
        if let guest = ProcessInfo.processInfo.environment["TOMOSHIBI_GUEST"] {
            print("STEP in-call \(room)")
            // Now rather than when the run ends: the runner's output is a
            // pipe, and whoever drives the browser is reading it for this.
            fflush(stdout)
            XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", guest)).firstMatch
                .waitForExistence(timeout: 180), "the guest who opened the card's link in a browser, in the call")
            sleep(2)
            shot("08-guest-arrived")
            // Long enough for the browser to see the host too.
            sleep(15)
        }
    }

    /// A meeting's link shared from the app's schedule: the share sheet
    /// draws it as the invitation, with the mark, from the metadata handed
    /// over with the link (LinkShare), which is what Messages sends as the
    /// link's preview. Without it the sheet, and Messages, say only the
    /// address's host. (The simulator's share sheet offers no Messages, so the
    /// sheet's own preview is what can be seen here.)
    func testASharedLinkIsDrawnAsTheInvitation() throws {
        continueAfterFailure = false
        let app = tomoshibi()
        app.launchEnvironment["TOMOSHIBI_DEBUG_LOBBY"] = "scheduled"
        app.launch()
        let meeting = app.cells.containing(.staticText, identifier: "design-review").firstMatch
        XCTAssertTrue(meeting.waitForExistence(timeout: 30), "the arranged meeting in the schedule")
        meeting.buttons["Share link"].tap()
        XCTAssertTrue(app.otherElements["LP.CaptionBar.TopCaption"].waitForExistence(timeout: 10), "the share sheet, with a preview")
        XCTAssertEqual(app.otherElements["LP.CaptionBar.TopCaption"].label, "You were invited to design-review.",
                       "the preview says the invitation, not the address")
        sleep(1)
        shot("11-share-sheet")
    }
}
