import Foundation
import Observation

/// The Mac app's own updates: looking for a newer build, saying so, and with
/// the person's agreement downloading it, checking it and putting it in
/// place of the running copy (Updates.kt on Android follows the same rules).
///
/// The asking and the fetching are the core's (core/protocol/src/api/updates.rs),
/// which compares the versions and refuses a file that is not the build;
/// what is here is when to ask, what to say, and the one rule the owner set
/// above the rest: nothing about an update interrupts a call. An offer is
/// shown only in the lobby; one found during a call waits for the lobby; a
/// download is held when the person leaves the lobby and carries on where it
/// stopped when they are back; nothing is installed anywhere but in the
/// lobby, since installing ends the app and opens the new one.
///
/// Asked once at start and every six hours after. A check that fails is kept
/// quiet and asked again in ten minutes, so that a laptop without a network
/// is not told so every time it wakes. "Later" puts that version away until
/// the app is next started; a newer one is offered all the same. A check
/// somebody asks for in the settings always answers, "You have the latest
/// version." included, and brings back a version put away.
///
/// Here, apart from the core and AppKit (UpdateSource, UpdateInstaller), so
/// that the rules are tested by `swift test`, where a bundle linking the core
/// timed out before its first test.
@MainActor
@Observable
public final class Updates {
    public enum Step: Equatable, Sendable {
        case none
        /// A newer build, and what went wrong with the last try at it.
        case offered(UpdateOffer, problem: String? = nil)
        case downloading(UpdateOffer, received: Int64, total: Int64)
        /// A download the person left the lobby during: stopped, kept, and
        /// carried on when they are back.
        case held(UpdateOffer, received: Int64, total: Int64)
        /// This copy of the app cannot be replaced where it is; said before
        /// anything is downloaded, with what to do about it.
        case misplaced(UpdateOffer, Misplacement)
        /// Checked, and being put in place of this copy, which then opens the
        /// new one and quits.
        case installing(UpdateOffer)
    }

    /// What a check somebody asked for found, for the settings to say.
    public enum Said: Equatable, Sendable { case checking, current, failed }

    public private(set) var step: Step = .none
    /// The answer to the last check asked for in the settings; nil where none
    /// was asked or it found a build.
    public private(set) var said: Said?
    /// Versions put away with "Later", for this run of the app only.
    private var later: Set<String> = []
    /// In the lobby, where an update may be shown, downloaded and installed:
    /// on its screen, and not on the way out of it (`joining`).
    private var free = false
    /// On the lobby's screen, as the app last said (`lobby`).
    private var onLobby = false
    /// A join pressed in the lobby is out (`joining`).
    private var settingOff = false

    public let running: String
    @ObservationIgnored private let source: UpdateSource
    @ObservationIgnored private let installer: UpdateInstaller
    @ObservationIgnored private let sleep: @Sendable (Duration) async throws -> Void
    @ObservationIgnored private let every: Duration
    @ObservationIgnored private let retry: Duration

    public static let every: Duration = .seconds(6 * 60 * 60)
    public static let retry: Duration = .seconds(10 * 60)

    @ObservationIgnored private var started = false
    @ObservationIgnored private var schedule: Task<Void, Never>?
    @ObservationIgnored private var transfer: UpdateTransfer?
    @ObservationIgnored private var transferTask: Task<Void, Never>?
    /// Set when the transfer is stopped because the person left the lobby,
    /// not because they cancelled.
    @ObservationIgnored private var holding = false
    /// A build that arrived as the person left the lobby, kept so that
    /// "Update" back there installs it rather than downloading it again.
    @ObservationIgnored private var ready: (version: String, file: URL)?

    public init(source: UpdateSource, installer: UpdateInstaller, running: String,
                every: Duration = Updates.every, retry: Duration = Updates.retry,
                sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.source = source
        self.installer = installer
        self.running = running
        self.every = every
        self.retry = retry
        self.sleep = sleep
    }

    /// Whether the banner is up: something to say, in the lobby, and not put away.
    public var shown: Bool {
        guard free else { return false }
        switch step {
        case .none, .held: return false
        case let .offered(offer, _): return !later.contains(offer.version)
        default: return true
        }
    }

    /// The build being offered, downloaded or installed, whatever the step.
    public var offer: UpdateOffer? {
        switch step {
        case .none: nil
        case let .offered(offer, _), let .downloading(offer, _, _), let .held(offer, _, _),
             let .misplaced(offer, _), let .installing(offer): offer
        }
    }

    /// Starts asking: now, and every six hours after. Once for the life of
    /// the model.
    public func start() {
        guard !started else { return }
        started = true
        // Whatever an earlier run downloaded was installed, failed, or was
        // left behind; none of it is trusted to be the build offered now.
        source.clear()
        schedule = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let answered = await self.check(asked: false)
                do { try await self.sleep(answered ? self.every : self.retry) } catch { return }
            }
        }
    }

    /// Stops asking, for a test that is done with the model.
    public func stop() {
        schedule?.cancel()
        schedule = nil
    }

    /// Told whenever the screen changes: the lobby is the only place an
    /// update is shown or worked on.
    public func lobby(_ inLobby: Bool) {
        onLobby = inLobby
        reconsider()
    }

    /// Told when a join is out and again when it is over, landed or not.
    ///
    /// A join pressed in the lobby leaves it only once it lands, seconds
    /// later: the Mac's start and join sheet asks whether the meeting is
    /// live, then the deployment for a grant, then the room to let it in,
    /// all with the lobby's screen behind the sheet. An install that came to
    /// its swap in those seconds saw a lobby, replaced the app and quit it,
    /// and the call being entered ended as it began. So a join out counts as
    /// the lobby left, from the press; one that is refused or called off is
    /// back in it, and a download held for it carries on.
    public func joining(_ out: Bool) {
        settingOff = out
        reconsider()
    }

    private func reconsider() {
        let inLobby = onLobby && !settingOff
        guard inLobby != free else { return }
        free = inLobby
        if !inLobby, case let .downloading(offer, received, total) = step {
            holding = true
            step = .held(offer, received: received, total: total)
            transfer?.cancel()
        } else if !inLobby, case .installing = step {
            // Checking the copy takes seconds, and replacing the app ends it:
            // a meeting started in them would be ended by it. Given up before
            // anything is replaced, the build kept for Update back here.
            installer.stop()
        } else if inLobby, case let .held(offer, _, _) = step {
            download(offer)
        }
    }

    /// The settings' "Check for updates".
    public func checkNow() {
        guard said != .checking else { return }
        said = .checking
        Task { await check(asked: true) }
    }

    /// "Update": downloads the build offered, where this copy can be replaced.
    public func accept() {
        guard free, case let .offered(offer, _) = step else { return }
        if let misplaced = installer.misplacement {
            step = .misplaced(offer, misplaced)
            return
        }
        if let ready, ready.version == offer.version, FileManager.default.fileExists(atPath: ready.file.path) {
            install(offer, ready.file)
        } else {
            download(offer)
        }
    }

    /// "Later": puts this version away until the app is next started.
    public func putAway() {
        guard let offer else { return }
        switch step {
        case .downloading, .held, .installing: return
        default: break
        }
        if ready?.version == offer.version {
            ready = nil
            source.clear()
        }
        later.insert(offer.version)
        step = .offered(offer)
    }

    /// "Cancel", on a download. What arrived is kept, so "Update" again
    /// carries on from there.
    public func cancel() {
        guard case .downloading = step else { return }
        holding = false
        transfer?.cancel()
    }

    /// "Show in Finder", for somebody told the app cannot replace itself.
    public func reveal() {
        installer.reveal()
    }

    @discardableResult
    private func check(asked: Bool) async -> Bool {
        let found: UpdateOffer?
        do {
            found = try await source.latest(running: running)
        } catch {
            if asked { said = .failed }
            return false
        }
        guard let found else {
            if asked { said = .current }
            // A build offered and since taken back is not offered any more.
            if case .offered = step { step = .none }
            return true
        }
        if asked {
            said = nil
            later.remove(found.version)
        }
        switch step {
        case .none:
            step = .offered(found)
        case let .offered(offer, _) where offer.version != found.version:
            step = .offered(found)
        case let .misplaced(offer, misplaced) where offer.version != found.version:
            step = .misplaced(found, misplaced)
        default:
            // Whatever is under way goes on; a newer build is offered once it
            // is over.
            break
        }
        return true
    }

    private func download(_ offer: UpdateOffer) {
        // The one before is waited for rather than cancelled: cancelling the
        // task lets go of the core's future without stopping the work the
        // core runs it on, which would go on appending to the same part as
        // the new download did. A transfer is stopped by its own cancel, and
        // its task ends when the core says it has.
        let previous = transferTask
        transfer?.cancel()
        let transfer = source.download(offer)
        self.transfer = transfer
        holding = false
        step = .downloading(offer, received: 0, total: offer.size)
        transferTask = Task { [weak self] in
            await previous?.value
            var failure: UpdateFailure?
            do {
                try await transfer.run { received, total in
                    Task { @MainActor [weak self] in
                        guard let self, self.transfer === transfer,
                              case let .downloading(now, _, _) = self.step, now.version == offer.version else { return }
                        self.step = .downloading(offer, received: received, total: total)
                    }
                }
            } catch let fault as UpdateFailure {
                failure = fault
            } catch {
                failure = .other
            }
            guard let self, self.transfer === transfer else { return }
            self.transfer = nil
            self.finished(offer, transfer.file, failure)
        }
    }

    private func finished(_ offer: UpdateOffer, _ file: URL, _ failure: UpdateFailure?) {
        switch failure {
        case nil:
            install(offer, file)
        case .cancelled:
            if holding, case .held = step { return }
            step = .offered(offer)
        case .superseded:
            // The deployment has a newer build than this one now; ask what it
            // is rather than offer one nobody is handing out.
            step = .none
            Task { await check(asked: false) }
        case let .some(fault):
            step = .offered(offer, problem: Self.explain(fault))
        }
    }

    private func install(_ offer: UpdateOffer, _ file: URL) {
        guard free else {
            // Arrived in the moment the person left the lobby: kept, and
            // installed only once they are back there and say so, never
            // under a call.
            ready = (offer.version, file)
            step = .offered(offer)
            return
        }
        if let misplaced = installer.misplacement {
            step = .misplaced(offer, misplaced)
            ready = nil
            source.clear()
            return
        }
        ready = nil
        step = .installing(offer)
        Task {
            switch await installer.install(file, version: offer.version) {
            case .done:
                // The new copy is opening and this one is quitting.
                step = .none
            case let .failed(sentence):
                source.clear()
                step = .offered(offer, problem: sentence)
            case .stopped:
                ready = (offer.version, file)
                step = .offered(offer)
            }
        }
    }

    static func explain(_ fault: UpdateFailure) -> String {
        switch fault {
        case .broken: t("The download stopped. Check your connection and try again.")
        case .damaged: t("The download was damaged and has been deleted. Try again.")
        case .storage: t("The update could not be saved on this device.")
        default: t("The update could not be downloaded. Try again later.")
        }
    }
}

/// A newer build, as the app speaks of it.
public struct UpdateOffer: Equatable, Sendable {
    public let version: String
    /// Bytes.
    public let size: Int64
    private let notesIn: @Sendable (String) -> String?
    /// What the source needs to download it; nobody else reads it.
    public let handle: (any Sendable)?

    public init(version: String, size: Int64, notes: @escaping @Sendable (String) -> String? = { _ in nil }, handle: (any Sendable)? = nil) {
        self.version = version
        self.size = size
        self.notesIn = notes
        self.handle = handle
    }

    /// What changed, for somebody reading `language` ("zh-Hans"); nil where
    /// there is nothing to say in a language they read.
    public func notes(_ language: String) -> String? { notesIn(language) }

    public static func == (a: Self, b: Self) -> Bool { a.version == b.version && a.size == b.size }
}

/// Why a download did not finish (the core's UpdateError, as the app tells
/// them apart).
public enum UpdateFailure: Error, Equatable, Sendable {
    case cancelled, broken, damaged, superseded, storage, other
    case refused(String)
}

/// Why this copy of the app cannot put a new one in its place.
public enum Misplacement: Equatable, Sendable {
    /// Opened from the disk image it came in, which cannot be written to.
    case diskImage
    /// Opened where it was downloaded, and run by macOS from a hidden copy
    /// (App Translocation), which is replaced with nothing.
    case translocated
    /// Somewhere this person may not change, such as an Applications folder
    /// another account installed it in.
    case readOnly
}

/// Where builds come from: the core, or a test's stand-in.
@MainActor
public protocol UpdateSource: AnyObject {
    /// The newer build there is, nil where this one is the newest; throws
    /// where the deployment could not be asked.
    func latest(running: String) async throws -> UpdateOffer?
    /// A download of `offer`, which starts when it is run.
    func download(_ offer: UpdateOffer) -> UpdateTransfer
    /// Removes every build and part of one that was downloaded.
    func clear()
}

/// One download, run once.
public protocol UpdateTransfer: AnyObject, Sendable {
    /// Where the build is once `run` returns.
    var file: URL { get }
    /// Downloads the build, throwing an UpdateFailure where it does not
    /// arrive whole. `progress` is called from anywhere.
    func run(progress: @escaping @Sendable (Int64, Int64) -> Void) async throws
    func cancel()
}

/// How a build is put in place of the running app: on the Mac, mounted,
/// checked and swapped in (MacInstaller); in a test, a stand-in.
@MainActor
public protocol UpdateInstaller: AnyObject {
    /// Why this copy cannot be replaced, or nil where it can.
    var misplacement: Misplacement? { get }
    /// Checks the build and puts it in place, then opens it and quits; returns
    /// only where it did not.
    func install(_ file: URL, version: String) async -> Installation
    /// Shows this copy of the app in Finder.
    func reveal()
    /// Gives up the install under way before anything is replaced, which
    /// then returns `stopped`; too late to, it changes nothing.
    func stop()
}

public enum Installation: Equatable, Sendable {
    case done
    case failed(String)
    /// Given up by `stop`, the build untouched.
    case stopped
}
