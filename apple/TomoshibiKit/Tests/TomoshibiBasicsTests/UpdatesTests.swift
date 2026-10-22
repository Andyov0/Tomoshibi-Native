@testable import TomoshibiBasics
import XCTest

/*
 * When the Mac app looks for an update, what it says, and above all where it
 * says it and does anything about it (Updates). What these guard would go
 * wrong in front of somebody with every test of the core still passing: an
 * offer, a download or an install in the middle of a call; a version put away
 * with "Later" back at the next check; a failed check announced, or a check
 * somebody asked for answered with silence; a copy running from its disk
 * image downloading fifty megabytes it can never put in place; a build that
 * failed its checks kept to be tried again.
 *
 * The six hours are the test's own clock, which moves only when told to.
 */
@MainActor
final class UpdatesTests: XCTestCase {
    @MainActor
    final class Clock {
        var waits: [Duration] = []
        private var pending: [CheckedContinuation<Void, Error>] = []

        func sleep(_ duration: Duration) async throws {
            waits.append(duration)
            try await withCheckedThrowingContinuation { pending.append($0) }
        }

        /// Lets every wait that has begun end.
        func pass() {
            let waiting = pending
            pending = []
            for one in waiting { one.resume() }
        }
    }

    final class FakeTransfer: UpdateTransfer, @unchecked Sendable {
        let file: URL
        var cancelled = false
        /// A cancel that arrives after the download has already finished.
        var deaf = false
        var progress: (@Sendable (Int64, Int64) -> Void)?
        private var continuation: CheckedContinuation<Void, Error>?
        private var outcome: Result<Void, UpdateFailure>?

        init(_ offer: UpdateOffer) {
            file = FileManager.default.temporaryDirectory.appendingPathComponent("Tomoshibi-\(offer.version)-\(UUID().uuidString).dmg")
            FileManager.default.createFile(atPath: file.path, contents: Data([1]))
        }

        func run(progress: @escaping @Sendable (Int64, Int64) -> Void) async throws {
            self.progress = progress
            try await withCheckedThrowingContinuation { continuation in
                if let outcome { continuation.resume(with: outcome) } else { self.continuation = continuation }
            }
        }

        func finish(_ failure: UpdateFailure? = nil) {
            let result: Result<Void, UpdateFailure> = failure.map { .failure($0) } ?? .success(())
            if let continuation {
                self.continuation = nil
                continuation.resume(with: result)
            } else if outcome == nil {
                outcome = result
            }
        }

        func cancel() {
            cancelled = true
            if !deaf { finish(.cancelled) }
        }
    }

    final class FakeSource: UpdateSource {
        var answer: () throws -> UpdateOffer? = { nil }
        var asked = 0
        var cleared = 0
        var transfers: [FakeTransfer] = []

        func latest(running: String) async throws -> UpdateOffer? {
            asked += 1
            return try answer()
        }

        func download(_ offer: UpdateOffer) -> UpdateTransfer {
            let transfer = FakeTransfer(offer)
            transfers.append(transfer)
            return transfer
        }

        func clear() {
            cleared += 1
            for transfer in transfers { try? FileManager.default.removeItem(at: transfer.file) }
        }
    }

    final class FakeInstaller: UpdateInstaller {
        var misplacement: Misplacement?
        var installs: [URL] = []
        var answer: Installation = .done
        var revealed = 0
        var stopped = 0
        /// Holds the install until `stop`, as checking a copy takes seconds.
        var holds = false
        private var held: CheckedContinuation<Installation, Never>?
        /// How far each install says it has got, in turn.
        var reports: [Double] = []
        /// The progress of the install under way, for a test to report through.
        var report: (@MainActor (Double) -> Void)?
        var elsewhere = false
        var openedElsewhere = 0

        func install(_ file: URL, version: String, progress: @escaping @MainActor (Double) -> Void,
                     shown: @escaping @MainActor () async -> Void) async -> Installation {
            installs.append(file)
            report = progress
            for fraction in reports { progress(fraction) }
            // As MacInstaller: the bar full, and waited for, only on the way
            // to opening the new copy.
            if case .done = answer, !holds {
                progress(1)
                await shown()
            }
            guard holds else { return answer }
            return await withCheckedContinuation { held = $0 }
        }

        func openElsewhere() { openedElsewhere += 1 }

        func reveal() { revealed += 1 }

        func stop() {
            stopped += 1
            held?.resume(returning: .stopped)
            held = nil
        }
    }

    struct NoNetwork: Error {}

    let source = FakeSource()
    let installer = FakeInstaller()
    let clock = Clock()

    func offer(_ version: String) -> UpdateOffer {
        UpdateOffer(version: version, size: 30_000_000, notes: { $0 == "en" ? "Fixed." : nil })
    }

    func model(bars: (download: SteadyFill, install: SteadyFill)? = nil) -> Updates {
        let clock = clock
        // The bars showing each report as it comes unless a test gives its
        // own, so that what each step waits for is not a second of real time.
        let bars = bars ?? (SteadyFill(perSecond: .infinity), SteadyFill(perSecond: .infinity))
        return Updates(source: source, installer: installer, running: "0.1.0.7", bars: bars, sleep: { try await clock.sleep($0) })
    }

    /// Lets whatever the model started run as far as it can.
    func settle() async {
        for _ in 0..<40 { await Task.yield() }
    }

    func offered(_ version: String = "0.1.0.8") async -> Updates {
        source.answer = { self.offer(version) }
        let updates = model()
        updates.lobby(true)
        updates.start()
        await settle()
        XCTAssertEqual(updates.step, .offered(offer(version)))
        return updates
    }

    func testAsksAtStartAndEverySixHoursAndRetriesAFailureQuietly() async {
        var fail = true
        source.answer = { if fail { throw NoNetwork() }; return nil }
        let updates = model()
        updates.lobby(true)
        updates.start()
        await settle()
        XCTAssertEqual(source.asked, 1)
        XCTAssertEqual(source.cleared, 1, "what an earlier run left was not cleared")
        XCTAssertEqual(updates.step, .none)
        XCTAssertNil(updates.said, "a check nobody asked for said something")
        XCTAssertFalse(updates.shown)
        XCTAssertEqual(clock.waits, [Updates.retry], "a failed check was not tried again in ten minutes")

        fail = false
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 2)
        XCTAssertEqual(clock.waits, [Updates.retry, Updates.every], "an answered check was not left for six hours")
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 3)
        updates.start()
        await settle()
        XCTAssertEqual(source.asked, 3, "started twice")
        updates.stop()
    }

    func testAnOfferIsShownInTheLobbyAndNowhereElse() async {
        source.answer = { self.offer("0.1.0.8") }
        let updates = model()
        updates.start()
        await settle()
        // Found during a call: kept for the lobby.
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        XCTAssertFalse(updates.shown)
        updates.lobby(true)
        XCTAssertTrue(updates.shown)
        updates.lobby(false)
        XCTAssertFalse(updates.shown)
        XCTAssertEqual(updates.offer?.notes("en"), "Fixed.")
        updates.stop()
    }

    func testLaterPutsThatVersionAwayAndANewerOneIsOfferedAllTheSame() async {
        let updates = await offered("0.1.0.8")
        updates.putAway()
        XCTAssertFalse(updates.shown)
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 2)
        XCTAssertFalse(updates.shown, "a version put away came back at the next check")

        source.answer = { self.offer("0.1.0.9") }
        clock.pass()
        await settle()
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.9")))
        XCTAssertTrue(updates.shown)
        updates.stop()
    }

    func testACheckSomebodyAsksForAlwaysAnswers() async {
        let updates = model()
        updates.lobby(true)
        source.answer = { nil }
        updates.checkNow()
        XCTAssertEqual(updates.said, .checking)
        await settle()
        XCTAssertEqual(updates.said, .current)

        source.answer = { throw NoNetwork() }
        updates.checkNow()
        await settle()
        XCTAssertEqual(updates.said, .failed)

        source.answer = { self.offer("0.1.0.8") }
        updates.checkNow()
        await settle()
        updates.putAway()
        XCTAssertFalse(updates.shown)
        updates.checkNow()
        await settle()
        XCTAssertNil(updates.said)
        XCTAssertTrue(updates.shown, "asking did not bring back the version put away")
    }

    func testADownloadIsHeldOutsideTheLobbyAndCarriesOnWhenBack() async {
        let updates = await offered()
        updates.accept()
        await settle()
        let first = source.transfers[0]
        first.progress?(1_000, 30_000_000)
        await settle()
        XCTAssertEqual(updates.step, .downloading(offer("0.1.0.8"), received: 1_000, total: 30_000_000))

        updates.lobby(false)
        await settle()
        XCTAssertTrue(first.cancelled, "a download went on into a call")
        XCTAssertEqual(updates.step, .held(offer("0.1.0.8"), received: 1_000, total: 30_000_000))
        XCTAssertFalse(updates.shown)

        updates.lobby(true)
        await settle()
        XCTAssertEqual(source.transfers.count, 2, "the download did not carry on in the lobby")
        XCTAssertTrue(updates.shown)
        updates.stop()
    }

    func testCancelStopsTheDownloadAndOffersItAgain() async {
        let updates = await offered()
        updates.accept()
        await settle()
        updates.cancel()
        await settle()
        XCTAssertTrue(source.transfers[0].cancelled)
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        XCTAssertTrue(installer.installs.isEmpty)
        updates.stop()
    }

    /// The install waits for the download's bar to be drawn full, and a
    /// Cancel in that second keeps the build for Update instead of
    /// installing it (SteadyFill).
    func testTheInstallWaitsForTheBarAndCancelInThatSecondKeepsTheBuild() async {
        source.answer = { self.offer("0.1.0.8") }
        let download = SteadyFill(drives: false)
        let updates = model(bars: (download, SteadyFill(perSecond: .infinity)))
        updates.lobby(true)
        updates.start()
        await settle()
        updates.accept()
        await settle()
        source.transfers[0].progress?(3_000_000, 30_000_000)
        await settle()
        source.transfers[0].finish()
        await settle()
        XCTAssertEqual(download.target, 1)
        XCTAssertTrue(installer.installs.isEmpty, "installing with the bar at \(download.shown)")
        guard case .downloading = updates.step else { return XCTFail("\(updates.step)") }

        for _ in 0..<130 { download.advance(by: .milliseconds(10)) }
        await settle()
        XCTAssertEqual(installer.installs, [source.transfers[0].file], "installed once the bar was full")

        // Again, and Cancel while the bar is still going.
        updates.stop()
        installer.installs = []
        let again = SteadyFill(drives: false)
        let second = model(bars: (again, SteadyFill(perSecond: .infinity)))
        second.lobby(true)
        second.start()
        await settle()
        second.accept()
        await settle()
        let transfer = source.transfers.last!
        transfer.finish()
        await settle()
        second.cancel()
        await settle()
        XCTAssertEqual(second.step, .offered(offer("0.1.0.8")))
        for _ in 0..<130 { again.advance(by: .milliseconds(10)) }
        await settle()
        XCTAssertTrue(installer.installs.isEmpty, "installed after Cancel")
        second.accept()
        await settle()
        XCTAssertEqual(installer.installs, [transfer.file], "Update after Cancel installs what had arrived, downloading nothing")
        XCTAssertEqual(source.transfers.count, 2)
        second.stop()
    }

    func testACopyThatCannotBeReplacedSaysSoBeforeDownloadingAnything() async {
        installer.misplacement = .diskImage
        let updates = await offered()
        updates.accept()
        await settle()
        XCTAssertEqual(updates.step, .misplaced(offer("0.1.0.8"), .diskImage))
        XCTAssertTrue(source.transfers.isEmpty, "downloaded a build it could never put in place")
        XCTAssertTrue(updates.shown)
        updates.reveal()
        XCTAssertEqual(installer.revealed, 1)
        updates.putAway()
        XCTAssertFalse(updates.shown)
        updates.stop()
    }

    func testAnInstallSaysHowFarItHasGotAndNeverGoesBack() async {
        let updates = await offered()
        installer.holds = true
        installer.reports = [0.1, 0.4, 0.3, 0.7]
        updates.accept()
        await settle()
        source.transfers[0].finish()
        await settle()
        XCTAssertEqual(updates.step, .installing(offer("0.1.0.8")))
        XCTAssertEqual(updates.installed, 0.7, accuracy: 0.0001, "a report behind the last moved the bar back")
        installer.report?(1.4)
        XCTAssertEqual(updates.installed, 1, accuracy: 0.0001, "the bar went past its end")
        installer.stop()
        await settle()
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        // A report from the install given up does not move a bar no longer shown.
        installer.report?(0.9)
        XCTAssertEqual(updates.installed, 1, accuracy: 0.0001)
        updates.stop()
    }

    func testWhereAnotherAppInstallsUpdateOpensItAndDownloadsNothing() async {
        installer.elsewhere = true
        let updates = await offered()
        XCTAssertTrue(updates.shown)
        updates.accept()
        await settle()
        XCTAssertEqual(installer.openedElsewhere, 1)
        XCTAssertTrue(source.transfers.isEmpty, "downloaded a build TestFlight installs")
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        updates.putAway()
        XCTAssertFalse(updates.shown)
        updates.stop()
    }

    func testAnInstalledBuildIsInstalledInTheLobbyAndAFailureSaysWhy() async {
        let updates = await offered()
        installer.answer = .failed("The update is not signed by Tomoshibi's developer, so it was not installed.")
        updates.accept()
        await settle()
        let cleared = source.cleared
        source.transfers[0].finish()
        await settle()
        XCTAssertEqual(installer.installs, [source.transfers[0].file])
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8"), problem: "The update is not signed by Tomoshibi's developer, so it was not installed."))
        XCTAssertEqual(source.cleared, cleared + 1, "a build that failed its checks was kept")

        installer.answer = .done
        updates.accept()
        await settle()
        XCTAssertEqual(source.transfers.count, 2)
        source.transfers[1].finish()
        await settle()
        XCTAssertEqual(updates.step, .none)
        updates.stop()
    }

    func testAnInstallIsGivenUpWhenTheLobbyIsLeftAndTheBuildKept() async {
        let updates = await offered()
        installer.holds = true
        updates.accept()
        await settle()
        source.transfers[0].finish()
        await settle()
        XCTAssertEqual(updates.step, .installing(offer("0.1.0.8")))
        // Start pressed while the copy was being checked.
        updates.lobby(false)
        await settle()
        XCTAssertEqual(installer.stopped, 1, "an install went on outside the lobby")
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        XCTAssertFalse(updates.shown)

        installer.holds = false
        updates.lobby(true)
        updates.accept()
        await settle()
        XCTAssertEqual(source.transfers.count, 1, "downloaded again though it was here")
        XCTAssertEqual(installer.installs.count, 2)
        updates.stop()
    }

    /// The Mac's start and join sheet: the screen stays the lobby's until
    /// the call lands, and an install that reached its swap while the join
    /// was out replaced the app under the call being entered.
    func testAJoinPressedInTheLobbyGivesUpTheInstallBeforeItLands() async {
        let updates = await offered()
        installer.holds = true
        updates.accept()
        await settle()
        source.transfers[0].finish()
        await settle()
        XCTAssertEqual(updates.step, .installing(offer("0.1.0.8")))

        // Start pressed; the screen has not changed.
        updates.joining(true)
        await settle()
        XCTAssertEqual(installer.stopped, 1, "an install went on under a join from the lobby")
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8")))
        XCTAssertFalse(updates.shown)
        updates.accept()
        XCTAssertEqual(installer.installs.count, 1, "installed while a join was out")

        // Refused at the door: back in the lobby, and Update installs the
        // build kept.
        updates.joining(false)
        XCTAssertTrue(updates.shown)
        installer.holds = false
        updates.accept()
        await settle()
        XCTAssertEqual(source.transfers.count, 1, "downloaded again though it was here")
        XCTAssertEqual(installer.installs.count, 2)
        updates.stop()
    }

    func testADownloadIsHeldWhileAJoinFromTheLobbyIsOutAndGoesOnWhenItFails() async {
        let updates = await offered()
        updates.accept()
        await settle()
        updates.joining(true)
        await settle()
        XCTAssertTrue(source.transfers[0].cancelled, "a download went on under a join from the lobby")
        XCTAssertEqual(updates.step, .held(offer("0.1.0.8"), received: 0, total: 30_000_000))

        // The join lands: the screen changes, then the join is over, and
        // nothing starts again in between.
        updates.lobby(false)
        updates.joining(false)
        await settle()
        XCTAssertEqual(source.transfers.count, 1, "the download started again under the call")

        // The call ends: back in the lobby, the download carries on.
        updates.lobby(true)
        await settle()
        XCTAssertEqual(source.transfers.count, 2, "the download did not carry on in the lobby")

        // A join that is refused gives the lobby back as well.
        updates.joining(true)
        await settle()
        updates.joining(false)
        await settle()
        XCTAssertEqual(source.transfers.count, 3, "the download did not carry on after a failed join")
        XCTAssertTrue(updates.shown)
        updates.stop()
    }

    func testADamagedDownloadSaysSoAndASupersededOneAsksAgain() async {
        let updates = await offered()
        updates.accept()
        await settle()
        source.transfers[0].finish(.damaged)
        await settle()
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.8"), problem: "The download was damaged and has been deleted. Try again."))
        XCTAssertTrue(installer.installs.isEmpty)

        updates.accept()
        await settle()
        let asked = source.asked
        source.answer = { self.offer("0.1.0.9") }
        source.transfers[1].finish(.superseded)
        await settle()
        XCTAssertEqual(source.asked, asked + 1, "a superseded build was not asked about again")
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.9")))
        updates.stop()
    }

    func testABuildThatArrivesAsTheLobbyIsLeftIsNotInstalledUntilAskedThere() async {
        let updates = await offered()
        updates.accept()
        await settle()
        let transfer = source.transfers[0]
        transfer.deaf = true
        updates.lobby(false)
        transfer.finish()
        await settle()
        XCTAssertTrue(installer.installs.isEmpty, "installed outside the lobby")
        XCTAssertFalse(updates.shown)

        updates.lobby(true)
        await settle()
        XCTAssertTrue(updates.shown)
        XCTAssertTrue(installer.installs.isEmpty, "installed without being asked again")
        updates.accept()
        await settle()
        XCTAssertEqual(source.transfers.count, 1, "downloaded again though it was here")
        XCTAssertEqual(installer.installs, [transfer.file])
        updates.stop()
    }

    func testABuildTakenBackIsNotOfferedAnyMore() async {
        let updates = await offered()
        source.answer = { nil }
        clock.pass()
        await settle()
        XCTAssertEqual(updates.step, .none)
        XCTAssertFalse(updates.shown)
        updates.stop()
    }
    // MARK: What the deployment says (Remote's app-updates, update_check_hours)

    func testSwitchedOffNothingIsLookedForOrOfferedOrSaid() async {
        source.answer = { self.offer("0.1.0.8") }
        let updates = model()
        updates.lobby(true)
        updates.allow(false)
        updates.start()
        await settle()
        XCTAssertEqual(source.asked, 0, "looked for an update the deployment had switched off")
        XCTAssertTrue(clock.waits.isEmpty, "a schedule was kept for looks that will not happen")
        XCTAssertFalse(updates.speaks, "the settings went on talking about updates")
        updates.checkNow()
        await settle()
        XCTAssertEqual(source.asked, 0, "Check for updates still asked")
        XCTAssertNil(updates.said)
        updates.stop()
    }

    func testSwitchedOffAnOfferGoesAndSwitchedOnTheAppLooksAtOnce() async {
        let updates = await offered()
        XCTAssertTrue(updates.shown)
        updates.allow(false)
        XCTAssertFalse(updates.shown, "an offer stayed up after updates were switched off")
        updates.accept()
        await settle()
        XCTAssertTrue(source.transfers.isEmpty, "Update downloaded while updates were off")

        let asked = source.asked
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, asked, "the six-hourly look went on")

        source.answer = { self.offer("0.1.0.9") }
        updates.allow(true)
        await settle()
        XCTAssertEqual(source.asked, asked + 1, "switched back on, the app did not look at once")
        XCTAssertEqual(updates.step, .offered(offer("0.1.0.9")))
        XCTAssertTrue(updates.shown)
        updates.stop()
    }

    func testADownloadUnderWayGoesOnWhenUpdatesAreSwitchedOff() async {
        let updates = await offered()
        updates.accept()
        await settle()
        let transfer = source.transfers[0]
        transfer.progress?(1_000, 30_000_000)
        await settle()

        updates.allow(false)
        await settle()
        XCTAssertFalse(transfer.cancelled, "a download somebody agreed to was stopped")
        XCTAssertTrue(updates.shown, "its progress went from the lobby")
        XCTAssertTrue(updates.speaks, "and from the settings")

        transfer.finish()
        await settle()
        XCTAssertEqual(installer.installs, [transfer.file], "the download was not seen through to its install")
        updates.stop()
    }

    func testTheDeploymentsIntervalIsTheWaitBetweenLooks() async {
        source.answer = { nil }
        let updates = model()
        updates.lobby(true)
        updates.checkEvery(.seconds(2 * 60 * 60))
        updates.start()
        await settle()
        XCTAssertEqual(source.asked, 1)
        XCTAssertEqual(clock.waits, [.seconds(2 * 60 * 60)])

        // Changed while it waits: the next look is one new wait away, and
        // not at once, since the interval is said moments after every start.
        updates.checkEvery(.seconds(60 * 60))
        await settle()
        XCTAssertEqual(source.asked, 1, "a change of interval looked at once")
        XCTAssertEqual(clock.waits, [.seconds(2 * 60 * 60), .seconds(60 * 60)])
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 2, "the wait given up still looked when it ended, or the new one did not")
        XCTAssertEqual(clock.waits.last, .seconds(60 * 60))

        updates.checkEvery(.seconds(60 * 60))
        await settle()
        XCTAssertEqual(clock.waits.count, 3, "the same interval said again started another wait")
        updates.stop()
    }
}
