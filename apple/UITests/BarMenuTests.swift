import XCTest

/// The call's bar, pressed with real touches on an iPhone: what the stage's
/// own debug presses cannot tell, because they reach the model and not the
/// hit testing a finger goes through. Every popover of the bar hangs outside
/// its capsule, and inside the bar's glass none of them could be touched
/// (Glass.swift `glassBacking`): the More menu's items, the reactions and
/// Leave's choices were drawn and did nothing.
///
/// Against the local deployment on 127.0.0.1:8080, as everything in interop,
/// or the one TEST_RUNNER_TOMOSHIBI_BASE names.
final class BarMenuTests: XCTestCase {
    private func joined(_ room: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = [
            "TOMOSHIBI_MUTE": "1",
            "TOMOSHIBI_BASE": "http://127.0.0.1:8080",
            "TOMOSHIBI_AUTOJOIN": room,
            "TOMOSHIBI_NAME": "UI Test",
        ]
        // Switches for the app handed down from the run (TEST_RUNNER_<name>
        // on the xcodebuild line), to try the same presses another way.
        for (name, value) in ProcessInfo.processInfo.environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        app.launch()
        XCTAssertTrue(app.buttons["More"].waitForExistence(timeout: 30), "the bar's More button")
        return app
    }

    func testMoreMenuItemOpensItsPanel() throws {
        let app = joined("uitest-bar-more")
        app.buttons["More"].tap()
        let sound = app.buttons["Sound"]
        XCTAssertTrue(sound.waitForExistence(timeout: 5), "the More menu's Sound item")
        sound.tap()
        XCTAssertTrue(app.buttons["Close sound"].waitForExistence(timeout: 5),
                      "pressing Sound in the More menu opens the sound panel")
    }

    func testMoreMenuOpensDevices() throws {
        let app = joined("uitest-bar-devices")
        app.buttons["More"].tap()
        let devices = app.buttons["Devices"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5), "the More menu's Devices item")
        devices.tap()
        XCTAssertTrue(app.staticTexts["Original sound"].waitForExistence(timeout: 5),
                      "pressing Devices in the More menu opens the devices panel")
    }

    func testReactionIsSentFromTheTray() throws {
        let app = joined("uitest-bar-reactions")
        app.buttons["Reactions"].tap()
        let thumbs = app.buttons["\u{1F44D}"]
        XCTAssertTrue(thumbs.waitForExistence(timeout: 5), "the reactions tray")
        thumbs.tap()
        XCTAssertTrue(thumbs.waitForNonExistence(timeout: 5), "pressing a reaction sends it and closes the tray")
    }

    /// The tray is one of the bar's popovers, as on Android and Windows:
    /// opening another puts it away, it puts another away, and a press
    /// anywhere off it closes it. Kept apart, it stayed open beside More and
    /// Leave, and only its own arrow or a reaction closed it.
    func testTheTrayIsOneOfTheBarsPopovers() throws {
        let app = joined("uitest-bar-tray")
        let thumbs = app.buttons["\u{1F44D}"]
        let sound = app.buttons["Sound"]
        app.buttons["Reactions"].tap()
        XCTAssertTrue(thumbs.waitForExistence(timeout: 5), "the reactions tray")
        app.buttons["More"].tap()
        XCTAssertTrue(sound.waitForExistence(timeout: 5), "the More menu")
        XCTAssertTrue(thumbs.waitForNonExistence(timeout: 5), "the tray stayed open beside the More menu")
        app.buttons["Reactions"].tap()
        XCTAssertTrue(thumbs.waitForExistence(timeout: 5), "the reactions tray, over the More menu")
        XCTAssertTrue(sound.waitForNonExistence(timeout: 5), "the More menu stayed open beside the tray")
        // On the room, well away from the bar and the tray.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).tap()
        XCTAssertTrue(thumbs.waitForNonExistence(timeout: 5), "a press off the tray left it open")
    }

    func testLeaveFromItsPopover() throws {
        let app = joined("uitest-bar-leave")
        app.buttons["Leave"].tap()
        // Its label carries its detail after the title.
        let leave = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Leave the call")).firstMatch
        XCTAssertTrue(leave.waitForExistence(timeout: 5), "Leave's popover")
        leave.tap()
        XCTAssertTrue(app.staticTexts["You left the call."].waitForExistence(timeout: 10),
                      "pressing Leave the call leaves")
    }
}
