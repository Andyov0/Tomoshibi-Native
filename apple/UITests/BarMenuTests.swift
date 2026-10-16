import XCTest

/// The call's bar, pressed with real touches on an iPhone: what the stage's
/// own debug presses cannot tell, because they reach the model and not the
/// hit testing a finger goes through. Every popover of the bar hangs outside
/// its capsule, and inside the bar's glass none of them could be touched
/// (Glass.swift `glassBacking`): the More menu's items, the reactions and
/// Leave's choices were drawn and did nothing.
///
/// Against the local deployment on 127.0.0.1:8080, as everything in interop.
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
        // Only sending one closes the tray; nothing else closes it.
        XCTAssertTrue(thumbs.waitForNonExistence(timeout: 5), "pressing a reaction sends it and closes the tray")
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
