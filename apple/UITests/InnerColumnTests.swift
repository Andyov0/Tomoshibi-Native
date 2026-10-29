import XCTest

/// The column down the right of an iPhone Duo's inner display on its side
/// (VerticalBar, SystemColumn): the screens keep it for themselves there,
/// beside their content, and stand their controls in it as they stand them in
/// the outer display's. Upright, the inner display keeps its controls where
/// any iPhone has them.
///
/// The system says nothing of its bars on the simulator's stand-ins for the
/// inner display, so TOMOSHIBI_DEBUG_VERTICAL_EDGE=trailing stands in for its
/// word that they go down the trailing edge of a window wider than tall, as
/// it says on the inner display on its side. Against the local deployment,
/// signed in where the lobby needs it (TEST_RUNNER_TOMOSHIBI_SIGNIN).
class ColumnTestCase: XCTestCase {
    var runner: [String: String] { ProcessInfo.processInfo.environment }
    /// The column's width, the outer display's.
    let column: CGFloat = 84

    func launch(_ extra: [String: String], signedIn: Bool = true) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["TOMOSHIBI_MUTE": "1", "TOMOSHIBI_BASE": "http://127.0.0.1:8080",
                                 "TOMOSHIBI_DEBUG_VERTICAL_EDGE": "trailing"]
        for (name, value) in runner where name.hasPrefix("TOMOSHIBI_") {
            app.launchEnvironment[name] = value
        }
        if !signedIn { app.launchEnvironment["TOMOSHIBI_SIGNIN"] = nil }
        for (name, value) in extra { app.launchEnvironment[name] = value }
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
        app.launch()
        return app
    }

    /// The app's window, the largest of its windows: in a call the first
    /// window the query found was one with no size at all, and a column
    /// measured from it held everything.
    func window(_ app: XCUIApplication) -> CGRect {
        app.windows.allElementsBoundByIndex.map { $0.frame }.max { $0.width * $0.height < $1.width * $1.height } ?? .zero
    }

    /// The window's width in points, where its frame is reported at another
    /// scale: on the Duo's resizable display the frames came back in points
    /// on its side and at a third of them upright (a window 223 wide for 669
    /// points). Nil where they are points.
    var across: CGFloat? { nil }

    /// The column's share of the window, in the window's own units.
    func columnWidth(_ app: XCUIApplication, across: CGFloat? = nil) -> CGFloat {
        guard let points = across ?? self.across else { return column }
        return window(app).width * column / points
    }

    /// The control, once it is there, in the column: its middle in the
    /// window's last 84 points. Given the few seconds a column takes to
    /// come after the window turns or the screen arrives, sliding in from
    /// the edge; one that never comes is still a failure.
    func inColumn(_ element: XCUIElement, _ app: XCUIApplication, _ what: String,
                  file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.waitForExistence(timeout: 20), what, file: file, line: line)
        let deadline = Date().addingTimeInterval(3)
        while element.frame.midX <= window(app).maxX - columnWidth(app) && Date() < deadline { usleep(250_000) }
        XCTAssertGreaterThan(element.frame.midX, window(app).maxX - columnWidth(app), "\(what), in the column",
                             file: file, line: line)
    }

    /// The control, once it is there, out of where a column would be, in a
    /// window `across` points wide.
    func outOfColumn(_ element: XCUIElement, _ app: XCUIApplication, across: CGFloat? = nil, _ what: String,
                     file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.waitForExistence(timeout: 20), what, file: file, line: line)
        XCTAssertLessThan(element.frame.maxX, window(app).maxX - columnWidth(app, across: across), "\(what), not in a column",
                          file: file, line: line)
    }

    func microphone(_ app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label == %@ OR label == %@", "Unmute microphone", "Mute microphone")).firstMatch
    }

    func camera(_ app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label == %@ OR label == %@", "Turn camera on", "Turn camera off")).firstMatch
    }

    /// The button labelled `label` furthest to the right, of two that are:
    /// the call's bar and the messages both say Hide messages.
    func rightmost(_ label: String, _ app: XCUIApplication) -> XCUIElement {
        let all = app.buttons.matching(NSPredicate(format: "label == %@", label)).allElementsBoundByIndex
        return all.max { $0.frame.midX < $1.frame.midX } ?? app.buttons[label]
    }

    /// Where the leftmost of the texts that say `label` starts: the wide
    /// settings say a section's name in their list and over its page.
    func leftmost(_ label: String, _ app: XCUIApplication) -> CGFloat {
        app.staticTexts.matching(NSPredicate(format: "label == %@", label)).allElementsBoundByIndex
            .map { $0.frame.minX }.min() ?? .infinity
    }

    /// The lobby laid out as the outer display's, with no sidebar: its
    /// controls all in the column, the account at its head and the rest at
    /// its foot, the server only in the lists beside the column, as on the
    /// outer display, and the sidebar's Start nowhere.
    func lobbysColumn(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        inColumn(microphone(app), app, "the microphone", file: file, line: line)
        inColumn(camera(app), app, "the camera", file: file, line: line)
        let account = app.buttons["Settings"]
        let start = app.buttons["Start a meeting"]
        inColumn(account, app, "the account", file: file, line: line)
        inColumn(app.buttons["Schedule a meeting"], app, "arranging", file: file, line: line)
        inColumn(app.buttons["Join a meeting"], app, "joining", file: file, line: line)
        inColumn(start, app, "starting", file: file, line: line)
        let middle = window(app).midY
        XCTAssertLessThan(account.frame.midY, middle, "the account at the column's head", file: file, line: line)
        XCTAssertGreaterThan(start.frame.midY, middle, "starting at the column's foot", file: file, line: line)
        // The column's head held a server as well, under the account, and
        // the inner display had two ways to the same choice where the outer
        // display has the lists' row alone.
        let servers = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Server"))
        XCTAssertTrue(servers.firstMatch.waitForExistence(timeout: 20), "the lists' server row", file: file, line: line)
        for server in servers.allElementsBoundByIndex {
            XCTAssertLessThan(server.frame.midX, window(app).maxX - columnWidth(app),
                              "no server in the column (\(server.label))", file: file, line: line)
        }
        XCTAssertFalse(app.buttons["Start"].exists, "no sidebar, and no Start at its head", file: file, line: line)
        XCTAssertFalse(app.textFields["Room name or invite link"].exists, "no join bar beside the column",
                       file: file, line: line)
    }
}

/// On the iPhone Duo, the app moved to the simulator's resizable display at
/// the inner display's size, on its side and upright, through
/// apple/fold-ui-tests.sh (the `FOLD-STEP` lines this prints): where the
/// screens' controls stand. Only where: a UI test's touches go to the Duo's
/// first display, the outer one, whatever display the app is on (the
/// synthesized events' display in a run's results), so a press on the
/// stand-in lands nowhere. WideColumnTests presses the same column on an
/// iPad. Run with TEST_RUNNER_FOLD_DRIVEN=1 by that driver; without it each
/// test is skipped, since nothing would ever open the phone.
final class InnerColumnTests: ColumnTestCase {
    private var folded: CGRect = .zero
    /// The inner display's width on its side.
    override var across: CGFloat? { 951 }

    private func launchDriven(_ extra: [String: String], signedIn: Bool = true) throws -> XCUIApplication {
        try XCTSkipUnless(runner["FOLD_DRIVEN"] != nil, "needs the fold driver")
        return launch(extra, signedIn: signedIn)
    }

    /// Asks the driver to open the phone, on its side or upright, and waits
    /// for the app to move there.
    private func open(_ app: XCUIApplication, tall: Bool = false) {
        folded = window(app)
        print(tall ? "FOLD-STEP open-tall" : "FOLD-STEP open")
        // At once: the driver (apple/fold-ui-tests.sh) reads this line from
        // the test's output, and buffered it arrived after the test had
        // waited its ninety seconds and failed, as MessagesInviteTests found.
        fflush(stdout)
        let deadline = Date().addingTimeInterval(90)
        while window(app) == folded && Date() < deadline { usleep(500_000) }
        XCTAssertNotEqual(window(app), folded, "the app moved to the opened display")
        sleep(3)
    }

    private func close(_ app: XCUIApplication) {
        print("FOLD-STEP close")
        fflush(stdout)
        let deadline = Date().addingTimeInterval(90)
        while window(app) != folded && Date() < deadline { usleep(500_000) }
        sleep(2)
    }

    /// The lobby opened on its side is the outer display's, everything in
    /// the column and no sidebar (VerticalBar `sideways`).
    func testTheLobbysControlsAreAllInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchDriven([:])
        XCTAssertTrue(app.buttons["Join a meeting"].waitForExistence(timeout: 30), "the lobby, folded")
        open(app)
        lobbysColumn(app)
        close(app)
    }

    /// The schedule open on the outer display stays open as the phone is
    /// opened, over the whole of the inner display with its close in the
    /// column: it was closed by the opening, and the inner display opened on
    /// the lobby's home.
    func testTheScheduleStaysOpenAcrossTheInnerDisplay() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_DEBUG_LOBBY": "scheduled"])
        let arrange = app.buttons["Arrange"]
        XCTAssertTrue(arrange.waitForExistence(timeout: 30), "the schedule, folded")
        open(app)
        XCTAssertTrue(arrange.waitForExistence(timeout: 10), "the schedule, still open")
        inColumn(app.buttons["Close"], app, "the schedule's close")
        XCTAssertLessThan(arrange.frame.minX, window(app).midX, "the schedule's form, at the left of the whole screen")
        close(app)
    }

    /// The settings over the whole of the inner display on its side, their
    /// close in the column, rather than a sheet in the middle of it.
    func testTheSettingsCoverTheInnerDisplay() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_DEBUG_LOBBY": "settings"])
        let general = app.staticTexts["General"].firstMatch
        XCTAssertTrue(general.waitForExistence(timeout: 30), "the settings, folded")
        open(app)
        XCTAssertTrue(general.waitForExistence(timeout: 10), "the settings, still open")
        inColumn(app.buttons["Close"], app, "the settings' close")
        XCTAssertLessThan(leftmost("General", app), window(app).width * 0.12, "the settings from the screen's left edge")
        close(app)
    }

    /// The server page's way back and Measure again, in the column.
    func testTheServerPagesWayBackIsInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_DEBUG_LOBBY": "servers"])
        let automatic = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Automatic")).firstMatch
        XCTAssertTrue(automatic.waitForExistence(timeout: 30), "the server page, folded")
        open(app)
        inColumn(app.buttons["Back"], app, "the way back")
        inColumn(app.buttons["Measure again"], app, "measuring again")
        close(app)
    }

    func testTheJoinScreensSwitchesAndBackAreInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_PREPARE": "inner-column-switches"], signedIn: false)
        XCTAssertTrue(app.staticTexts["inner-column-switches"].waitForExistence(timeout: 30), "the join screen, folded")
        open(app)
        inColumn(microphone(app), app, "the microphone")
        inColumn(app.buttons["Back"], app, "the way back")
        close(app)
    }

    func testTheStartSheetsCloseIsInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_DEBUG_LOBBY": "start"])
        XCTAssertTrue(app.textFields.firstMatch.waitForExistence(timeout: 30), "the start sheet, folded")
        open(app)
        inColumn(app.buttons["Cancel"], app, "the start sheet's close")
        inColumn(microphone(app), app, "the start sheet's microphone")
        close(app)
    }

    func testTheCallsBarIsInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_AUTOJOIN": "inner-column-call", "TOMOSHIBI_NAME": "Inner Column"])
        XCTAssertTrue(app.buttons["Leave"].waitForExistence(timeout: 40), "the call, folded")
        open(app)
        inColumn(app.buttons["Leave"], app, "Leave")
        inColumn(microphone(app), app, "the call's microphone")
        close(app)
    }

    /// Upright, the inner display has an ordinary iPhone's bars across the
    /// top and the foot, and the lobby's switches stay at the foot of the
    /// stage, out of where a column would be.
    func testUprightTheControlsStayWhereTheyWere() throws {
        continueAfterFailure = false
        let app = try launchDriven([:])
        XCTAssertTrue(app.buttons["Join a meeting"].waitForExistence(timeout: 30), "the lobby, folded")
        open(app, tall: true)
        outOfColumn(microphone(app), app, across: 669, "the microphone, upright")
        XCTAssertTrue(app.buttons["Start"].waitForExistence(timeout: 10), "the sidebar's Start, upright")
        close(app)
    }

    /// Upright, the schedule open on the outer display is the section
    /// beside the sidebar once the phone is opened, not the lobby's home.
    func testUprightTheScheduleStaysOpen() throws {
        continueAfterFailure = false
        let app = try launchDriven(["TOMOSHIBI_DEBUG_LOBBY": "scheduled"])
        let arrange = app.buttons["Arrange"]
        XCTAssertTrue(arrange.waitForExistence(timeout: 30), "the schedule, folded")
        open(app, tall: true)
        XCTAssertTrue(app.buttons["Start"].waitForExistence(timeout: 10), "the sidebar, upright")
        XCTAssertTrue(arrange.waitForExistence(timeout: 10), "the schedule, still open beside it")
        close(app)
    }
}

/// The same column pressed with a finger, on an iPad held on its side: a
/// window wider than tall at a regular width, as the Duo's inner display on
/// its side is, with no column of the system's on either side, so the
/// screens are laid out exactly as they are on the inner display, the lobby
/// without its sidebar and its controls in a column of its own (the switch
/// standing in for the system's word). The only place a test can touch it:
/// on the Duo the app has to be moved to a display the test's touches do not
/// reach (InnerColumnTests). Only on an iPad, by the simulator's name for
/// the device.
final class WideColumnTests: ColumnTestCase {
    private func launchWide(_ extra: [String: String], signedIn: Bool = true) throws -> XCUIApplication {
        try XCTSkipUnless((runner["SIMULATOR_DEVICE_NAME"] ?? "").contains("iPad"), "an iPad on its side")
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = launch(extra, signedIn: signedIn)
        let deadline = Date().addingTimeInterval(20)
        while !(window(app).width > window(app).height) && Date() < deadline { usleep(500_000) }
        XCTAssertGreaterThan(window(app).width, window(app).height, "the iPad on its side")
        return app
    }

    override func tearDown() {
        XCUIDevice.shared.orientation = .portrait
        super.tearDown()
    }

    /// The lobby without its sidebar: everything in the column, and the
    /// microphone turning on and off there. The server is the lists' row
    /// (testTheServerPageGoesBackFromTheColumn).
    func testTheLobbysSwitchesArePressedInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:])
        lobbysColumn(app)
        let mic = microphone(app)
        let before = mic.label
        mic.tap()
        let after = app.buttons[before == "Mute microphone" ? "Unmute microphone" : "Mute microphone"]
        XCTAssertTrue(after.waitForExistence(timeout: 5), "the microphone, pressed")
        inColumn(after, app, "the microphone, pressed")
        after.tap()
        XCTAssertTrue(app.buttons[before].waitForExistence(timeout: 5), "and back")
    }

    /// Arranging, at the column's foot, opens the scheduled meetings over
    /// the whole screen, and their close at the column's head takes them
    /// down, back to the lobby and its column.
    func testTheScheduledMeetingsOpenAndCloseFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:])
        let arranging = app.buttons["Schedule a meeting"]
        inColumn(arranging, app, "arranging")
        arranging.tap()
        let arrange = app.buttons["Arrange"]
        XCTAssertTrue(arrange.waitForExistence(timeout: 10), "the scheduled meetings, open")
        XCTAssertLessThan(arrange.frame.minX, window(app).midX, "over the whole screen, from its left")
        let shut = app.buttons["Close"]
        inColumn(shut, app, "the scheduled meetings' close")
        shut.tap()
        XCTAssertTrue(arrange.waitForNonExistence(timeout: 10), "the scheduled meetings closed")
        inColumn(microphone(app), app, "home again, its microphone")
    }

    /// The account, at the column's head, opens the settings over the whole
    /// screen; their close in the column takes them down.
    func testTheAccountOpensTheSettingsAndTheyCloseFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:])
        let account = app.buttons["Settings"]
        inColumn(account, app, "the account")
        account.tap()
        let general = app.staticTexts["General"].firstMatch
        XCTAssertTrue(general.waitForExistence(timeout: 10), "the settings, open")
        XCTAssertLessThan(leftmost("General", app), window(app).width * 0.12, "over the whole screen, from its left")
        let close = app.buttons["Close"]
        inColumn(close, app, "the settings' close")
        close.tap()
        XCTAssertTrue(general.waitForNonExistence(timeout: 10), "the settings closed by the column's close")
        inColumn(account, app, "the account, home again")
    }

    /// The settings and the schedule stay open as the iPad is turned upright
    /// and back: upright the settings are a sheet and the schedule the
    /// section beside the sidebar, on its side both cover the whole screen
    /// with their close in the column. The turn hands one over to the other
    /// where it stands, and the sheet it takes down does not close them.
    func testTheSettingsAndTheScheduleStayOpenAsItTurns() throws {
        continueAfterFailure = false
        for (opened, shown) in [("settings", "General"), ("scheduled", "Arrange")] {
            let app = try launchWide(["TOMOSHIBI_DEBUG_LOBBY": opened])
            let it = opened == "settings" ? app.staticTexts[shown].firstMatch : app.buttons[shown]
            XCTAssertTrue(it.waitForExistence(timeout: 30), "\(opened), open on its side")
            inColumn(app.buttons["Close"], app, "\(opened)' close, on its side")
            XCUIDevice.shared.orientation = .portrait
            let deadline = Date().addingTimeInterval(20)
            while !(window(app).height > window(app).width) && Date() < deadline { usleep(500_000) }
            XCTAssertGreaterThan(window(app).height, window(app).width, "the iPad upright")
            sleep(2)
            XCTAssertTrue(it.exists, "\(opened), still open upright")
            if opened == "scheduled" {
                XCTAssertTrue(app.buttons["Start"].exists, "the schedule beside the sidebar, upright")
            }
            XCUIDevice.shared.orientation = .landscapeLeft
            let back = Date().addingTimeInterval(20)
            while !(window(app).width > window(app).height) && Date() < back { usleep(500_000) }
            sleep(2)
            XCTAssertTrue(it.exists, "\(opened), still open on its side again")
            let close = app.buttons["Close"]
            inColumn(close, app, "\(opened)' close, on its side again")
            close.tap()
            XCTAssertTrue(it.waitForNonExistence(timeout: 10), "\(opened), closed by the column's close")
            inColumn(app.buttons["Settings"], app, "home again, the account")
        }
    }

    /// Joining and starting, at the column's foot, open the join and start
    /// sheet, whose close at the column's head takes it down.
    func testJoiningAndStartingArePressedInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:])
        for (label, what) in [("Join a meeting", "joining"), ("Start a meeting", "starting")] {
            let button = app.buttons[label]
            inColumn(button, app, what)
            button.tap()
            let field = app.textFields.firstMatch
            XCTAssertTrue(field.waitForExistence(timeout: 10), "the sheet, opened by \(what)")
            let cancel = app.buttons["Cancel"]
            inColumn(cancel, app, "the sheet's close, after \(what)")
            cancel.tap()
            XCTAssertTrue(field.waitForNonExistence(timeout: 10), "the sheet closed, after \(what)")
        }
    }

    /// The lists' server row pushes the server page, whose way back and
    /// Measure again are the column's, and the way back comes home.
    func testTheServerPageGoesBackFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:])
        let rows = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Server")).allElementsBoundByIndex
        let row = rows.min { $0.frame.midX < $1.frame.midX }
        XCTAssertNotNil(row, "the lists' server row")
        guard let row else { return }
        XCTAssertLessThan(row.frame.maxX, window(app).maxX - columnWidth(app), "the row, beside the column")
        row.tap()
        let back = app.buttons["Back"]
        inColumn(back, app, "the server page's way back")
        inColumn(app.buttons["Measure again"], app, "measuring again")
        back.tap()
        XCTAssertTrue(back.waitForNonExistence(timeout: 10), "the server page left")
        inColumn(app.buttons["Settings"], app, "home again, the account")
    }

    /// The start sheet's close, under the column's head, takes it down.
    func testTheStartSheetClosesFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide(["TOMOSHIBI_DEBUG_LOBBY": "start"])
        XCTAssertTrue(app.textFields.firstMatch.waitForExistence(timeout: 30), "the start sheet")
        let cancel = app.buttons["Cancel"]
        inColumn(cancel, app, "the start sheet's close")
        cancel.tap()
        XCTAssertTrue(app.textFields.firstMatch.waitForNonExistence(timeout: 10), "the sheet closed by the column's close")
    }

    /// The join screen's microphone turns on and off in the column, and its
    /// way back there leaves the screen.
    func testTheJoinScreensSwitchesAndBackArePressedInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide(["TOMOSHIBI_PREPARE": "wide-column-switches"], signedIn: false)
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
        XCTAssertTrue(app.staticTexts["wide-column-switches"].waitForNonExistence(timeout: 10), "the join screen left")
    }

    /// The call's bar stands in the column: its messages open from there,
    /// and Leave there leaves, to the screen after the call, whose way back
    /// is in the column as well.
    func testTheCallsBarIsPressedInTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide(["TOMOSHIBI_AUTOJOIN": "wide-column-call", "TOMOSHIBI_NAME": "Wide Column"])
        let leave = app.buttons["Leave"]
        inColumn(leave, app, "Leave")
        inColumn(microphone(app), app, "the call's microphone")
        let messages = app.buttons["Show messages"]
        inColumn(messages, app, "the call's messages")
        messages.tap()
        XCTAssertTrue(app.textFields["Say something"].waitForExistence(timeout: 10), "the messages, opened from the column")
        // The bar's, of whatever else has the same label. Not judged by
        // where it stands: the keyboard the messages may bring up makes
        // the column too short for the bar, which goes back across the
        // foot until it goes, as it does on the outer display.
        rightmost("Hide messages", app).tap()
        XCTAssertTrue(app.textFields["Say something"].waitForNonExistence(timeout: 10), "the messages closed")
        inColumn(leave, app, "Leave, the messages closed")
        leave.tap()
        let leaving = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Leave the call")).firstMatch
        XCTAssertTrue(leaving.waitForExistence(timeout: 5), "Leave's popover, beside the column")
        leaving.tap()
        let back = app.buttons["Back"]
        inColumn(back, app, "the way back after the call")
        back.tap()
        XCTAssertTrue(app.staticTexts["You left the call."].waitForNonExistence(timeout: 10), "the screen after the call left")
    }

    /// The sign-in's language under the column's head opens its menu, and
    /// its paste control stands at the column's foot. The language in the
    /// screen's corner is as near the edge as the column, so the paste
    /// control, at the foot of the card where there is no column, says
    /// which of the two the language is.
    func testTheLanguageMenuOpensFromTheColumn() throws {
        continueAfterFailure = false
        let app = try launchWide([:], signedIn: false)
        inColumn(app.buttons["Paste"], app, "the paste control")
        let language = app.buttons["Language"]
        inColumn(language, app, "the language")
        language.tap()
        let same = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Same as the system")).firstMatch
        XCTAssertTrue(same.waitForExistence(timeout: 5), "the language menu, open")
        same.tap()
        XCTAssertTrue(same.waitForNonExistence(timeout: 5), "closed by the choice")
    }
}
