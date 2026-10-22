import Foundation
import Observation
import os
@testable import TomoshibiBasics
import XCTest

/*
 * What the deployment says to the apps between releases, and what the app
 * does with it (Remote). What these guard would go wrong in front of
 * somebody with every test of the core still passing: a feature switched off
 * coming back on a train because nothing was kept, or the app's own words
 * flickering back at every start; a question that went unanswered taking
 * away what the last answer said; a notice put away coming back at the next
 * start, or a new one never shown because the old one was put away; a
 * notice drawn over a call; a notice that has run out still saying so to
 * somebody who never left the lobby.
 *
 * The half hour is the test's own clock, which moves only when told to, and
 * so is the time of day.
 */
@MainActor
final class RemoteTests: XCTestCase {
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

    /// The deployment, as the core would hand it over: answers by what was
    /// kept of them, and the next answer to a question.
    final class FakeSource: RemoteSource {
        var kept: [String: RemoteAnswer] = [:]
        var answer: () throws -> RemoteAnswer = { throw Unanswered() }
        var asked = 0

        func fetch() async throws -> RemoteAnswer {
            asked += 1
            return try answer()
        }

        func read(_ kept: String) -> RemoteAnswer? { self.kept[kept] }
    }

    struct Unanswered: Error {}

    private let suite = "TomoshibiBasicsTests.Remote"
    private let languageSuite = "TomoshibiBasicsTests.Remote.Language"
    private var store: UserDefaults!
    private var localization: Localization!
    let source = FakeSource()
    let clock = Clock()
    var time = Date(timeIntervalSince1970: 1_792_627_200) // 2026-10-22T00:00:00Z

    override func setUp() async throws {
        store = UserDefaults(suiteName: suite)
        store.removePersistentDomain(forName: suite)
        UserDefaults(suiteName: languageSuite)?.removePersistentDomain(forName: languageSuite)
        localization = Localization(defaults: UserDefaults(suiteName: languageSuite)!, preferredLanguages: [])
        localization.language = .zhHans
    }

    override func tearDown() async throws {
        store.removePersistentDomain(forName: suite)
        UserDefaults(suiteName: languageSuite)?.removePersistentDomain(forName: languageSuite)
    }

    func model(store: UserDefaults? = nil, kept: Bool = true) -> Remote {
        let clock = clock
        return Remote(source: source, store: kept ? (store ?? self.store) : nil, localization: localization,
                      now: { [unowned self] in self.time }, sleep: { try await clock.sleep($0) })
    }

    func settle() async {
        for _ in 0..<40 { await Task.yield() }
    }

    let reworded = RemoteAnswer(revision: "r1", phrases: [.zhHans: ["Leave": "走了"], .ja: ["Leave": "さようなら"]],
                                off: ["screen-share", "music", "something-a-later-app-has"],
                                params: ["screen_share_max_fps": 30, "update_check_hours": 2], kept: "r1")

    func notice(_ id: String, until: Date? = nil, warning: Bool = false) -> RemoteAnswer {
        RemoteAnswer(revision: id, notice: LobbyNotice(id: id, warning: warning,
                                                        text: [.en: "Down tonight", .zhHans: "今晚维护", .zhHant: "Down tonight", .ja: "Down tonight"],
                                                        until: until),
                     kept: id)
    }

    // MARK: Kept, asked, applied

    func testWithNothingKeptOrHeardTheAppIsAsItWasBuilt() async {
        let remote = model()
        remote.restore()
        remote.lobby(true)
        XCTAssertEqual(remote.answer, .nothing)
        for feature in RemoteFeature.allCases { XCTAssertTrue(remote.allows(feature), feature.rawValue) }
        XCTAssertEqual(remote.ceiling, .none)
        XCTAssertNil(remote.answer.updateInterval)
        XCTAssertNil(remote.notice)
        XCTAssertEqual(localization.t("Leave"), Phrasebook.tables[.zhHans]?["Leave"])
    }

    func testWhatWasKeptAppliesBeforeAnythingIsAsked() async {
        store.set("r1", forKey: Remote.keptKey)
        source.kept["r1"] = reworded
        let remote = model()
        remote.restore()
        XCTAssertEqual(source.asked, 0)
        XCTAssertEqual(localization.t("Leave"), "走了", "the kept wording was not laid over before the first screen")
        XCTAssertFalse(remote.allows(.screenShare))
        XCTAssertFalse(remote.allows(.music))
        XCTAssertTrue(remote.allows(.watchTogether))
        XCTAssertTrue(remote.allows(.backgroundBlur), "a name the app does not know switched off something else")
        XCTAssertEqual(remote.ceiling, ShareCeiling(fps: 30))
        XCTAssertEqual(remote.answer.updateInterval, .seconds(2 * 60 * 60))
    }

    func testAnAnswerIsAppliedKeptAndReadBackOffline() async {
        var applied: [RemoteAnswer] = []
        source.answer = { self.reworded }
        let remote = model()
        remote.onApply = { applied.append($0) }
        remote.start()
        await settle()
        XCTAssertEqual(source.asked, 1, "not asked at start")
        XCTAssertEqual(remote.answer, reworded)
        XCTAssertEqual(applied, [reworded], "what follows outside was not told")
        XCTAssertEqual(store.string(forKey: Remote.keptKey), "r1", "the answer was not kept")
        remote.stop()

        // The next start, with no network: what was kept, as it was.
        source.answer = { throw Unanswered() }
        source.kept["r1"] = reworded
        let offline = model()
        offline.restore()
        offline.start()
        await settle()
        XCTAssertEqual(offline.answer, reworded, "offline, the kept answer did not apply")
        XCTAssertFalse(offline.allows(.screenShare))
        XCTAssertEqual(store.string(forKey: Remote.keptKey), "r1", "an unanswered question wrote over what was kept")
        offline.stop()
    }

    func testAQuestionThatGoesUnansweredChangesNothing() async {
        source.answer = { self.reworded }
        let remote = model()
        remote.start()
        await settle()
        var applied = 0
        remote.onApply = { _ in applied += 1 }

        source.answer = { throw Unanswered() }
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 2)
        XCTAssertEqual(remote.answer, reworded, "an unanswered question took away what was said")
        XCTAssertEqual(localization.t("Leave"), "走了")
        XCTAssertEqual(applied, 0)
        remote.stop()
    }

    func testARehearsalKeepsNothing() async {
        source.answer = { self.reworded }
        let remote = model(kept: false)
        remote.start()
        await settle()
        XCTAssertEqual(remote.answer, reworded)
        XCTAssertNil(store.string(forKey: Remote.keptKey))
        remote.stop()
    }

    func testAsksAtStartAndEveryHalfHour() async {
        source.answer = { self.reworded }
        let remote = model()
        remote.start()
        await settle()
        XCTAssertEqual(source.asked, 1)
        XCTAssertEqual(clock.waits, [Remote.every])
        clock.pass()
        await settle()
        XCTAssertEqual(source.asked, 2)
        remote.start()
        await settle()
        XCTAssertEqual(source.asked, 2, "started twice")
        remote.stop()
    }

    func testComingBackAsksAgainButNotTwiceInAMinute() async {
        source.answer = { self.reworded }
        let remote = model()
        remote.refresh()
        await settle()
        XCTAssertEqual(source.asked, 0, "asked before the app had started")
        remote.start()
        await settle()
        XCTAssertEqual(source.asked, 1)

        // Coming to the front and to the lobby in the same second as the
        // start: the one answer serves.
        remote.refresh()
        remote.lobby(true)
        await settle()
        XCTAssertEqual(source.asked, 1, "asked again a moment after asking")

        time += 61
        remote.lobby(false)
        await settle()
        XCTAssertEqual(source.asked, 1, "leaving the lobby asked")
        remote.lobby(true)
        await settle()
        XCTAssertEqual(source.asked, 2, "back in the lobby, not asked again")
        time += 61
        remote.refresh()
        await settle()
        XCTAssertEqual(source.asked, 3, "back in front, not asked again")
        remote.stop()
    }

    // MARK: The phrases

    func testThePhrasesFollowTheLanguageAndANewAnswer() async {
        source.answer = { self.reworded }
        let remote = model()
        let changed = OSAllocatedUnfairLock(initialState: 0)
        withObservationTracking {
            _ = localization.t("Leave")
        } onChange: {
            changed.withLock { $0 += 1 }
        }
        remote.start()
        await settle()
        XCTAssertEqual(changed.withLock { $0 }, 1, "a view saying the phrase was not redrawn when it was laid over")
        XCTAssertEqual(localization.t("Leave"), "走了")

        localization.language = .ja
        XCTAssertEqual(localization.t("Leave"), "さようなら", "the other language's wording did not follow")
        localization.language = .zhHant
        XCTAssertEqual(localization.t("Leave"), Phrasebook.tables[.zhHant]?["Leave"], "a language not reworded is the app's own")

        source.answer = { RemoteAnswer(revision: "r2", kept: "r2") }
        clock.pass()
        await settle()
        localization.language = .zhHans
        XCTAssertEqual(localization.t("Leave"), Phrasebook.tables[.zhHans]?["Leave"], "wording taken back stayed")
        remote.stop()
    }

    // MARK: The notice

    func testTheNoticeIsShownInTheLobbyAndNowhereElse() async {
        source.answer = { self.notice("n1") }
        let remote = model()
        remote.start()
        await settle()
        XCTAssertNil(remote.notice, "shown before the lobby was")
        remote.lobby(true)
        XCTAssertEqual(remote.notice?.id, "n1")
        XCTAssertEqual(remote.notice?.text[.zhHans], "今晚维护")
        remote.lobby(false)
        XCTAssertNil(remote.notice, "shown in a call")
        remote.lobby(true)
        XCTAssertEqual(remote.notice?.id, "n1", "back from the call, it was gone")
        remote.stop()
    }

    func testAClosedNoticeStaysClosedAndANewOneIsShown() async throws {
        source.answer = { self.notice("n1") }
        let remote = model()
        remote.lobby(true)
        remote.start()
        await settle()
        let shown = try XCTUnwrap(remote.notice)
        let changed = OSAllocatedUnfairLock(initialState: 0)
        withObservationTracking {
            _ = remote.notice
        } onChange: {
            changed.withLock { $0 += 1 }
        }
        remote.close(shown)
        XCTAssertNil(remote.notice)
        XCTAssertEqual(changed.withLock { $0 }, 1, "the lobby was not redrawn without it")

        clock.pass()
        await settle()
        XCTAssertNil(remote.notice, "the same notice came back at the next answer")
        remote.stop()

        // The next start, the same notice kept and asked again.
        store.set("n1", forKey: Remote.keptKey)
        source.kept["n1"] = notice("n1")
        let again = model()
        again.lobby(true)
        again.restore()
        again.start()
        await settle()
        XCTAssertNil(again.notice, "a closed notice came back at the next start")

        source.answer = { self.notice("n2") }
        clock.pass()
        await settle()
        XCTAssertEqual(again.notice?.id, "n2", "a new notice was not shown because the old one was closed")
        again.stop()
    }

    func testOnlyTheLatestClosedNoticesAreRemembered() {
        let remote = model()
        for one in 0..<(Remote.closedHeld + 5) {
            remote.close(LobbyNotice(id: "n\(one)", text: [.en: "x"]))
        }
        let closed = store.stringArray(forKey: Remote.closedKey) ?? []
        XCTAssertEqual(closed.count, Remote.closedHeld)
        XCTAssertEqual(closed.last, "n\(Remote.closedHeld + 4)")
    }

    func testANoticeThatHasRunOutIsNotShownAndGoesWhenItRunsOut() async {
        source.answer = { self.notice("gone", until: self.time - 1) }
        let remote = model()
        remote.lobby(true)
        remote.start()
        await settle()
        XCTAssertNil(remote.notice, "a notice already over was shown")
        remote.stop()

        let clock = Clock()
        source.answer = { self.notice("tonight", until: self.time + 3600) }
        let timed = Remote(source: source, store: store, localization: localization,
                           now: { [unowned self] in self.time }, sleep: { try await clock.sleep($0) })
        timed.lobby(true)
        timed.start()
        await settle()
        XCTAssertEqual(timed.notice?.id, "tonight")
        XCTAssertTrue(clock.waits.contains(.seconds(3600)), "nothing was set to take it down when it runs out")

        // Its hour passes in the lobby with nothing heard since.
        source.answer = { throw Unanswered() }
        time += 3600
        clock.pass()
        await settle()
        XCTAssertNil(timed.answer.notice, "the notice outlived its end")
        XCTAssertNil(timed.notice)
        timed.stop()
    }
}
