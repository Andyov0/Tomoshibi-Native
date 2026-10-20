#if os(macOS)
import Foundation
import TomoshibiBasics
import TomoshibiCore

/// Builds from the deployment, through the core (api/updates.rs), which asks
/// as the app, compares the versions, and lets nothing through that is not
/// the build it described.
///
/// A deployment of its own rather than the app's: an update is for whoever
/// has the app, signed in or not, and reachable or not when it started, and
/// needs no cookie.
@MainActor
final class CoreUpdateSource: UpdateSource {
    private let base: String
    /// ~/Library/Caches/app.tomoshibi.mac/Updates: the app's own, which the
    /// system may empty when it is short of room, and which is emptied at
    /// every start anyway (Updates `start`).
    private let folder: URL
    private var made: Deployment?

    init(base: String) {
        self.base = base
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        folder = caches.appendingPathComponent(Bundle.main.bundleIdentifier ?? "app.tomoshibi.mac").appendingPathComponent("Updates")
    }

    private func deployment() throws -> Deployment {
        if let made { return made }
        // Before the deployment is made, which reads it then, so that the
        // user agent carries the version the deployment is asked about.
        ClientDescription.tell()
        let deployment = try Deployment(base: base)
        made = deployment
        return deployment
    }

    func latest(running: String) async throws -> UpdateOffer? {
        switch try await deployment().checkForUpdate(platform: .mac, running: running) {
        case .upToDate:
            return nil
        case let .available(build):
            return UpdateOffer(version: build.version, size: Int64(build.size),
                               notes: { language in updateNotes(build: build, language: language) }, handle: build)
        }
    }

    func download(_ offer: UpdateOffer) -> UpdateTransfer {
        let file = folder.appendingPathComponent("Tomoshibi-\(offer.version).dmg")
        guard let build = offer.handle as? AppBuild, let deployment = try? deployment() else { return Unreachable(file: file) }
        return CoreTransfer(download: deployment.downloadUpdate(build: build, path: file.path), file: file)
    }

    func clear() {
        let inside = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        for one in inside { try? FileManager.default.removeItem(at: one) }
    }
}

private final class CoreTransfer: UpdateTransfer, @unchecked Sendable {
    let download: UpdateDownload
    let file: URL

    init(download: UpdateDownload, file: URL) {
        self.download = download
        self.file = file
    }

    func run(progress: @escaping @Sendable (Int64, Int64) -> Void) async throws {
        do {
            try await download.run(progress: Watcher(said: progress))
        } catch let fault as UpdateError {
            throw switch fault {
            case .Cancelled: UpdateFailure.cancelled
            case .Network: UpdateFailure.broken
            case .Mismatch: UpdateFailure.damaged
            case .Superseded: UpdateFailure.superseded
            case .Storage: UpdateFailure.storage
            case let .Refused(_, code): UpdateFailure.refused(code)
            case .Unexpected: UpdateFailure.other
            }
        }
    }

    func cancel() { download.cancel() }

    /// The core's progress, handed on as it comes; Updates takes it to the
    /// main actor.
    final class Watcher: UpdateProgress, @unchecked Sendable {
        let said: @Sendable (Int64, Int64) -> Void
        init(said: @escaping @Sendable (Int64, Int64) -> Void) { self.said = said }
        func progressed(received: UInt64, total: UInt64) { said(Int64(received), Int64(total)) }
    }
}

/// A download that cannot start because no deployment could be made of the
/// address, which in a release build cannot happen: the address is the core's
/// own.
private final class Unreachable: UpdateTransfer, @unchecked Sendable {
    let file: URL
    init(file: URL) { self.file = file }
    func run(progress: @escaping @Sendable (Int64, Int64) -> Void) async throws { throw UpdateFailure.other }
    func cancel() {}
}
#endif
