import XCTest

/// The video watched together going into the small window as the app goes
/// to the home screen, through a real press of the home button: the system
/// decides which window opens as the app leaves, and how long the leaving
/// takes is part of that, which launching another app from a shell does not
/// reproduce.
///
/// Needs a simulator that has picture in picture at all: an iPad's. No
/// iPhone simulator has it (isPictureInPictureSupported is false in every one
/// of iOS 26.4, 27.1 and 27.2), so on one this is skipped, as it is without
/// a video to play: the local deployment on 127.0.0.1:8080 with its debug
/// stand-in for the watch gateway, the stand-in's control and log files
/// (TEST_RUNNER_TOMOSHIBI_DEBUG_WATCH_CONTROL, _LOG) and a link to play
/// (TEST_RUNNER_WATCH_LINK). Measured with the simulator's own log (the
/// system's, under com.apple.pegasus), which says which window it opened and
/// why none: the app's log line says only what the player believes.
final class WatchSmallWindowTests: XCTestCase {
    func testTheVideoGoesIntoTheSmallWindowAtHome() throws {
        let environment = ProcessInfo.processInfo.environment
        guard let control = environment["TOMOSHIBI_DEBUG_WATCH_CONTROL"],
              let log = environment["TOMOSHIBI_DEBUG_WATCH_LOG"],
              let link = environment["WATCH_LINK"] else {
            throw XCTSkip("needs TEST_RUNNER_TOMOSHIBI_DEBUG_WATCH_CONTROL, _LOG and TEST_RUNNER_WATCH_LINK")
        }
        let app = XCUIApplication()
        app.launchEnvironment = [
            "TOMOSHIBI_BASE": "http://127.0.0.1:8080",
            "TOMOSHIBI_AUTOJOIN": environment["WATCH_ROOM"] ?? "uitest-watch-small-window",
            "TOMOSHIBI_NAME": "UI Test",
            "TOMOSHIBI_DEBUG_WATCH_LINKS": "1",
        ]
        for (name, value) in environment where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        app.launch()
        XCTAssertTrue(app.buttons["Leave"].waitForExistence(timeout: 30), "in the call")

        try press("add \(link)", into: control)
        XCTAssertTrue(waitFor(in: log, timeout: 60) { $0.contains("playing=1") }, "the video plays")
        sleep(3)
        // YouTube's goes into the small window only from the stage's button
        // (YouTubeBackend `watchTheApp`), and then stays there as the app
        // leaves: TEST_RUNNER_WATCH_PIP_FIRST presses it first.
        if environment["WATCH_PIP_FIRST"] != nil {
            try press("pip", into: control)
            XCTAssertTrue(waitFor(in: log, timeout: 10) { $0.contains("active=true") }, "the stage's button opened the small window")
            sleep(2)
        }

        XCUIDevice.shared.press(.home)
        let opened = waitFor(in: log, timeout: 10) { $0.contains("active=true") }
        sleep(2)
        let home = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        home.name = "home"
        home.lifetime = .keepAlways
        add(home)
        XCTAssertTrue(opened, "the small window opened as the app went home")

        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10), "back in front")
        XCTAssertTrue(waitFor(in: log, timeout: 10) { text in
            text.split(separator: "\n").last { $0.contains(" pip ") }?.contains("active=false") == true
        }, "the small window closed as the app came back")
    }

    private func press(_ line: String, into control: String) throws {
        let handle = try XCTUnwrap(FileHandle(forWritingAtPath: control), "the control file")
        handle.seekToEndOfFile()
        handle.write(Data("\(line)\n".utf8))
        try handle.close()
    }

    private func waitFor(in path: String, timeout: TimeInterval, _ found: (String) -> Bool) -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if let data = FileManager.default.contents(atPath: path), found(String(decoding: data, as: UTF8.self)) { return true }
            Thread.sleep(forTimeInterval: 0.25)
        }
        return false
    }
}
