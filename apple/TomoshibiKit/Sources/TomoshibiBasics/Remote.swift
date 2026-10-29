import Foundation
import Observation

/// What the deployment says to the apps between their releases (the core's
/// api/remote.rs, `GET /api/apps/remote`): wording laid over the app's own
/// phrases, features switched off, ceilings over what the app would
/// otherwise use, and a notice for the lobby. The owner, 2026-10-22: a small
/// change should not each time need a new build.
///
/// The last answer is kept, and read back before the first screen is drawn
/// (`restore`), so that a feature switched off stays off on a train and a
/// phrase laid over does not flicker back to the app's own at every start.
/// Then the deployment is asked: at start, every half hour after, and again
/// whenever the app comes to the front or back to the lobby (`refresh`). An
/// answer replaces what applied and is kept; a question that is not answered
/// changes nothing and says nothing, since what it would have said is
/// whatever was said last time. With nothing kept and nothing heard, the app
/// is exactly what it was built as.
///
/// The asking, the reading and the per-language working out are the core's
/// (RemoteSource); here are when to ask, what to keep, and what follows,
/// apart from the core so that the rules are tested by `swift test`.
@MainActor
@Observable
public final class Remote {
    /// What applies now: the last answer, or what was kept from the one
    /// before, or nothing.
    public private(set) var answer: RemoteAnswer = .nothing
    /// On the lobby's screen, the one place the notice is shown.
    private var inLobby = false
    /// Notices somebody closed, by id: not shown again, until the deployment
    /// puts up one with another id.
    private var closed: [String]

    @ObservationIgnored private let source: RemoteSource?
    @ObservationIgnored private let store: UserDefaults?
    @ObservationIgnored private let localization: Localization
    @ObservationIgnored private let every: Duration
    @ObservationIgnored private let soonest: TimeInterval
    @ObservationIgnored private let sleep: @Sendable (Duration) async throws -> Void
    @ObservationIgnored private let now: () -> Date

    @ObservationIgnored private var started = false
    @ObservationIgnored private var asking = false
    @ObservationIgnored private var lastAsked: Date?
    @ObservationIgnored private var schedule: Task<Void, Never>?
    @ObservationIgnored private var expiry: Task<Void, Never>?

    /// Told whenever an answer is applied, kept or heard, for what follows it
    /// outside this module: the app's updates, and the ceiling the iPhone's
    /// broadcast extension reads.
    @ObservationIgnored public var onApply: ((RemoteAnswer) -> Void)?

    public nonisolated static let every: Duration = .seconds(30 * 60)
    /// How soon a question asked for by coming back (`refresh`) may follow
    /// the one before: the app comes to the front and lands in the lobby in
    /// the same second it starts, which would be three questions for one
    /// answer.
    public nonisolated static let soonest: TimeInterval = 60
    /// Where the last answer and the notices closed are kept.
    static let keptKey = "tomoshibi.remote"
    static let closedKey = "tomoshibi.remote.closed"
    /// How many closed notices are remembered: an id names one notice for
    /// as long as that notice is up (api/remote.rs `RemoteNotice`), so ids
    /// closed long ago belong to notices long taken down, and only take up
    /// room.
    static let closedHeld = 20

    /// `store` is where the answer and the closed notices are kept, nil for
    /// nowhere (a rehearsal, whose notices and switches are no business of
    /// the owner's next start). `source` nil asks nothing and reads nothing:
    /// a debug build started by hand, which has no test deployment to ask.
    public init(source: RemoteSource?, store: UserDefaults?, localization: Localization = .shared,
                every: Duration = Remote.every, soonest: TimeInterval = Remote.soonest,
                now: @escaping () -> Date = Date.init,
                sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.source = source
        self.store = store
        self.localization = localization
        self.every = every
        self.soonest = soonest
        self.now = now
        self.sleep = sleep
        closed = store?.stringArray(forKey: Self.closedKey) ?? []
    }

    // MARK: What applies

    /// Whether a feature is offered (the answer's `off`).
    public func allows(_ feature: RemoteFeature) -> Bool {
        answer.allows(feature)
    }

    /// The ceiling over a shared screen.
    public var ceiling: ShareCeiling { answer.ceiling }

    /// The notice for the lobby, while the lobby is the screen: none in a
    /// call, none once it has run out, and none once closed here.
    public var notice: LobbyNotice? {
        guard inLobby, let notice = answer.notice, !closed.contains(notice.id) else { return nil }
        if let until = notice.until, until <= now() { return nil }
        return notice
    }

    /// The notice's close: this id is not shown again, on this device, even
    /// after the app is started again.
    public func close(_ notice: LobbyNotice) {
        guard !closed.contains(notice.id) else { return }
        closed.append(notice.id)
        if closed.count > Self.closedHeld { closed.removeFirst(closed.count - Self.closedHeld) }
        store?.set(closed, forKey: Self.closedKey)
    }

    // MARK: Asking

    /// Applies the answer kept from before, for the first screen to be drawn
    /// with: called before it is, as the language is settled before it is.
    public func restore() {
        guard let source, let kept = store?.string(forKey: Self.keptKey), let read = source.read(kept) else { return }
        apply(read)
    }

    /// Starts asking: now, and every half hour after. Once.
    public func start() {
        guard !started, source != nil else { return }
        started = true
        schedule = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                await self.ask()
                do { try await self.sleep(self.every) } catch { return }
            }
        }
    }

    /// Stops asking, for a test that is done with the model.
    public func stop() {
        schedule?.cancel()
        schedule = nil
        expiry?.cancel()
        expiry = nil
    }

    /// Asks again, on coming back to the front or to the lobby, unless a
    /// question is out or one went a moment ago.
    public func refresh() {
        guard started, !asking else { return }
        if let lastAsked, now().timeIntervalSince(lastAsked) < soonest { return }
        Task { await ask() }
    }

    /// Told whenever the screen changes. Coming back to the lobby asks again,
    /// since a call can be long and the lobby is where what was said shows.
    public func lobby(_ here: Bool) {
        guard here != inLobby else { return }
        inLobby = here
        if here { refresh() }
    }

    private func ask() async {
        guard let source, !asking else { return }
        asking = true
        lastAsked = now()
        defer { asking = false }
        guard let heard = try? await source.fetch() else { return }
        store?.set(heard.kept, forKey: Self.keptKey)
        apply(heard)
    }

    private func apply(_ heard: RemoteAnswer) {
        if heard != answer { answer = heard }
        localization.laidOver = heard.phrases
        expire()
        onApply?(heard)
    }

    /// Takes the notice down when it runs out, if nothing is heard before:
    /// a notice that says the server is down until nine would otherwise
    /// still be saying so at ten to somebody who had not left the lobby,
    /// since nothing else redraws it then.
    private func expire() {
        expiry?.cancel()
        expiry = nil
        guard let notice = answer.notice, let until = notice.until else { return }
        let left = until.timeIntervalSince(now())
        let sleep = self.sleep
        expiry = Task { [weak self] in
            if left > 0 {
                do { try await sleep(.milliseconds(Int64(left * 1000))) } catch { return }
            }
            guard let self, !Task.isCancelled, self.answer.notice?.id == notice.id else { return }
            self.answer.notice = nil
        }
    }
}

/// The features the deployment can switch off, by the names it uses. A name
/// the app does not know is a feature of a later app, and nothing to it.
public enum RemoteFeature: String, CaseIterable, Sendable {
    case screenShare = "screen-share"
    case backgroundBlur = "background-blur"
    case watchTogether = "watch-together"
    case music
    case appUpdates = "app-updates"
}

/// What the deployment said, as the app uses it: worked out for each of the
/// app's languages by the core when it was heard (RemoteSource), so that a
/// change of language needs nothing asked again.
public struct RemoteAnswer: Equatable, Sendable {
    /// The deployment's name for this answer.
    public var revision: String
    /// The phrases laid over the app's own, by language and then by English
    /// key (the core's `remote_phrases` for each language).
    public var phrases: [Language: [String: String]]
    /// The features switched off.
    public var off: Set<String>
    /// The numbers, by the deployment's names.
    public var params: [String: Double]
    public var notice: LobbyNotice?
    /// The answer as it came, to keep and read back (the core's `kept`).
    public var kept: String

    public init(revision: String = "", phrases: [Language: [String: String]] = [:], off: Set<String> = [],
                params: [String: Double] = [:], notice: LobbyNotice? = nil, kept: String = "") {
        self.revision = revision
        self.phrases = phrases
        self.off = off
        self.params = params
        self.notice = notice
        self.kept = kept
    }

    /// What applies when nothing has ever been heard: the app as it was built.
    public static let nothing = RemoteAnswer()

    public func allows(_ feature: RemoteFeature) -> Bool {
        !off.contains(feature.rawValue)
    }

    public var ceiling: ShareCeiling { ShareCeiling(params: params) }

    /// How long between looks for an update (update_check_hours), or nil for
    /// the app's own six hours. Never under ten minutes, the wait after a
    /// look that failed (Updates `retry`): a fraction of an hour written by
    /// mistake would otherwise have every copy of the app asking the
    /// deployment every few seconds.
    ///
    /// And never over a year, as on Windows (Remote.cs `UpdateInterval`). A
    /// huge figure, written to mean "never", is past what Int64 holds in
    /// seconds, and converting it trapped; the answer is kept and applied at
    /// the next start before the deployment is asked again, so the app
    /// crashed at every launch and a corrected answer could never reach it.
    public var updateInterval: Duration? {
        guard let hours = params["update_check_hours"], hours.isFinite, hours > 0 else { return nil }
        let year = 366.0 * 24 * 3600
        return .seconds(Int64(min(max(hours * 3600, 600), year).rounded()))
    }
}

/// A notice the lobby shows above everything else in it.
public struct LobbyNotice: Equatable, Sendable {
    /// The same while the notice is the same one; a closed notice is
    /// remembered by it.
    public var id: String
    /// Something to take care about (the deployment's "warning"), drawn as
    /// the app draws a problem, rather than only to know.
    public var warning: Bool
    /// The words by language: that language, else it without its region or
    /// script, else English (the core's `remote_notice_text` for each). A
    /// language missing here has nothing it can read, and is shown nothing.
    public var text: [Language: String]
    /// When it stops being shown; nil until it is taken down.
    public var until: Date?

    public init(id: String, warning: Bool = false, text: [Language: String], until: Date? = nil) {
        self.id = id
        self.warning = warning
        self.text = text
        self.until = until
    }
}

/// Where the answers come from: the core, or a test's stand-in.
@MainActor
public protocol RemoteSource: AnyObject {
    /// What the deployment says now; throws where it could not be asked or
    /// did not answer with something that can be read.
    func fetch() async throws -> RemoteAnswer
    /// An answer kept from before, read again; nil where it is not one. A
    /// notice that has run out since is left out.
    func read(_ kept: String) -> RemoteAnswer?
}
