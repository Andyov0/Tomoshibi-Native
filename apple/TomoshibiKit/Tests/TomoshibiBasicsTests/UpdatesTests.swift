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

        func install(_ file: URL, version: String) async -> Installation {
            installs.append(file)
            return answer
        }

        func reveal() { revealed += 1 }
    }

    struct NoNetwork: Error {}

    let source = FakeSource()
    let installer = FakeInstaller()
    let clock = Clock()

    func offer(_ version: String) -> UpdateOffer {
        UpdateOffer(version: version, size: 30_000_000, notes: { $0 == "en" ? "Fixed." : nil })
    }

    func model() -> Updates {
        let clock = clock
        return Updates(source: source, installer: installer, running: "0.1.0.7", sleep: { try await clock.sleep($0) })
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
}
