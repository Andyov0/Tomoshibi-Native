#if os(macOS)
import AppKit
import Foundation
import os
import TomoshibiBasics
import TomoshibiCore

private let log = Logger(subsystem: "app.tomoshibi", category: "update")

/// Puts a downloaded Mac build in place of the running app and opens it.
///
/// Written here rather than taken from Sparkle. Sparkle fetches its own feed
/// and archive through its own URLSession, which would put the Mac's update
/// beside the core instead of through it, under another user agent the
/// deployment's update addresses refuse; it wants an appcast and an EdDSA
/// signature over every archive where the deployment already names a size and
/// a SHA-256 over TLS; and its interface would have had to be replaced for the
/// app's own anyway. What it does that this does not is ask for an
/// administrator's password to replace an app the person may not change;
/// here that is said, with what to do instead (`misplacement`).
///
/// The build arrives as the same disk image the download page offers. It is
/// mounted where Finder will not show it, the app copied out of it beside the
/// running one (on the same volume, so the swap is a rename), the image let
/// go, and the copy checked before anything is touched: the bundle identifier
/// is this app's, the version is the one offered, and the signature is the
/// team's Developer ID, intact and notarized (UpdateTrust). Only then is the
/// running bundle swapped for it, the old one thrown away, and a small shell
/// left waiting for this process to end before it opens the new one.
@MainActor
final class MacInstaller: UpdateInstaller {
    private let app = Bundle.main.bundleURL
    /// Set by `stop` while a build is being checked; read once it has been.
    private var stopping = false

    var misplacement: Misplacement? { Self.misplacement(of: app) }

    func reveal() {
        NSWorkspace.shared.activateFileViewerSelecting([Self.original(of: app) ?? app])
    }

    func stop() {
        stopping = true
    }

    /// Says how far it has got as it goes (UpdateInstalling's bar): the image
    /// opened, the app copied out of it by what has arrived of its size, the
    /// copy checked, and put in place.
    func install(_ file: URL, version: String, progress: @escaping @MainActor (Double) -> Void,
                 shown: @escaping @MainActor () async -> Void) async -> Installation {
        stopping = false
        let app = app
        let identifier = Bundle.main.bundleIdentifier ?? "app.tomoshibi.mac"
        let signing = UpdateTrust.signing()
        progress(0.02)
        let report: @Sendable (Double) -> Void = { fraction in Task { @MainActor in progress(fraction) } }
        let prepared = await Task.detached { Self.prepare(file, version: version, identifier: identifier, signing: signing, beside: app, report: report) }.value
        let staged: URL
        switch prepared {
        case let .failure(refusal):
            log.error("update refused: \(String(describing: refusal), privacy: .public)")
            return .failed(refusal.sentence)
        case let .success(url):
            staged = url
        }
        // The person left the lobby while the copy was checked, perhaps for
        // a call, which replacing the app would end.
        if stopping {
            try? FileManager.default.removeItem(at: staged.deletingLastPathComponent())
            return .stopped
        }

        progress(0.92)
        let placed = await Task.detached { () -> Bool in
            do {
                _ = try FileManager.default.replaceItemAt(app, withItemAt: staged, backupItemName: nil, options: [])
                return true
            } catch {
                log.error("the update could not be put in place: \(error.localizedDescription, privacy: .public)")
                return false
            }
        }.value
        try? FileManager.default.removeItem(at: staged.deletingLastPathComponent())
        guard placed else { return .failed(t("Tomoshibi could not put the update in place. Nothing was changed.")) }

        // Launch Services is told the bundle changed, so that the version the
        // Dock and Finder show is the new one before it has been opened.
        LSRegisterURL(app as CFURL, true)
        log.info("update to \(version, privacy: .public) is in place; opening it")
        progress(1)
        reopen(app)
        // The bar moves at one speed (SteadyFill), so a fast copy leaves it
        // behind; the window goes once it is drawn full, and a moment after.
        // The new copy opens only when this one has ended (reopen).
        await shown()
        try? await Task.sleep(for: .milliseconds(200))
        NSApp.terminate(nil)
        return .done
    }

    /// Leaves a shell waiting for this process to end, which then opens the
    /// app again through Launch Services, as a double-click would.
    ///
    /// A debug build hands its TOMOSHIBI_* switches on to the new copy with
    /// `open --env`: without TOMOSHIBI_BASE a debug build talks to the real
    /// deployment, and a test that updated one would have had it open on
    /// production the moment it came back.
    private func reopen(_ app: URL) {
        var opening = ["-n"]
        #if DEBUG
        for (key, value) in ProcessInfo.processInfo.environment where key.hasPrefix("TOMOSHIBI_") {
            opening += ["--env", "\(key)=\(value)"]
        }
        #endif
        opening += [app.path, "--args", "-ApplePersistenceIgnoreState", "YES"]
        let shell = Process()
        shell.executableURL = URL(fileURLWithPath: "/bin/sh")
        shell.arguments = ["-c", "while /bin/kill -0 \"$0\" 2>/dev/null; do /bin/sleep 0.2; done; exec /usr/bin/open \"$@\"",
                           String(ProcessInfo.processInfo.processIdentifier)] + opening
        shell.standardOutput = FileHandle.nullDevice
        shell.standardError = FileHandle.nullDevice
        do { try shell.run() } catch { log.error("could not leave anything to reopen the app: \(error.localizedDescription, privacy: .public)") }
    }

    // MARK: Where the app is

    /// Why this copy cannot replace itself where it is, or nil where it can.
    nonisolated static func misplacement(of app: URL) -> Misplacement? {
        if translocated(app) { return .translocated }
        // Opened straight from the disk image it was downloaded in.
        if (try? app.resourceValues(forKeys: [.volumeIsReadOnlyKey]))?.volumeIsReadOnly == true { return .diskImage }
        // The swap renames entries in the folder the app is in and the app's
        // own: both have to be this person's to change. An Applications
        // folder another account installed it into is not.
        let files = FileManager.default
        guard files.isWritableFile(atPath: app.deletingLastPathComponent().path), files.isWritableFile(atPath: app.path) else { return .readOnly }
        return nil
    }

    /// App Translocation: an app opened from where a browser left it, still
    /// quarantined, is run from a randomised read-only copy, and replacing
    /// that copy changes nothing the person will open next time. Asked of the
    /// Security framework, whose functions for it are not in its headers, and
    /// recognised by its path where they cannot be found.
    nonisolated static func translocated(_ app: URL) -> Bool {
        typealias Ask = @convention(c) (CFURL, UnsafeMutablePointer<Bool>, UnsafeMutablePointer<Unmanaged<CFError>?>?) -> Bool
        if let security = dlopen("/System/Library/Frameworks/Security.framework/Security", RTLD_LAZY),
           let symbol = dlsym(security, "SecTranslocateIsTranslocatedURL") {
            var answer = false
            if unsafeBitCast(symbol, to: Ask.self)(app as CFURL, &answer, nil) { return answer }
        }
        return app.path.contains("/AppTranslocation/")
    }

    /// Where a translocated app really is, for Finder to show.
    nonisolated static func original(of app: URL) -> URL? {
        guard translocated(app),
              let security = dlopen("/System/Library/Frameworks/Security.framework/Security", RTLD_LAZY),
              let symbol = dlsym(security, "SecTranslocateCreateOriginalPathForURL") else { return nil }
        typealias Ask = @convention(c) (CFURL, UnsafeMutablePointer<Unmanaged<CFError>?>?) -> Unmanaged<CFURL>?
        return unsafeBitCast(symbol, to: Ask.self)(app as CFURL, nil)?.takeRetainedValue() as URL?
    }

    // MARK: The build

    enum Refusal: Error, CustomStringConvertible {
        case unopened(String)
        case notTheBuild(String)
        case untrusted(UpdateTrust.Refusal)

        var sentence: String {
            switch self {
            case .unopened: t("The update could not be opened.")
            case .notTheBuild: t("The update is not the version that was offered, so it was not installed.")
            case .untrusted(.notNotarized): t("Apple has not approved this update, so it was not installed.")
            case .untrusted: t("The update is not signed by Tomoshibi's developer, so it was not installed.")
            }
        }

        var description: String {
            switch self {
            case let .unopened(why): "unopened: \(why)"
            case let .notTheBuild(why): "not the build: \(why)"
            case let .untrusted(why): "untrusted: \(why)"
            }
        }
    }

    /// The app out of the disk image, beside `beside` and checked, or why not.
    nonisolated static func prepare(_ image: URL, version: String, identifier: String,
                                    signing: UpdateTrust.Signing, beside running: URL,
                                    report: @Sendable (Double) -> Void = { _ in }) -> Result<URL, Refusal> {
        let files = FileManager.default
        guard let place = try? files.url(for: .itemReplacementDirectory, in: .userDomainMask, appropriateFor: running, create: true) else {
            return .failure(.unopened("no folder beside the app"))
        }
        let mountAt = files.temporaryDirectory.appendingPathComponent("tomoshibi-update-\(UUID().uuidString)")
        try? files.createDirectory(at: mountAt, withIntermediateDirectories: true)
        defer { try? files.removeItem(at: mountAt) }

        // Not browsed, so it never appears in Finder's sidebar or on the
        // desktop; read only; at a random place, so a volume already called
        // Tomoshibi (the person's own copy of the image, open) is not it.
        let attached = run("/usr/bin/hdiutil", ["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountrandom", mountAt.path, "-plist", image.path])
        guard attached.status == 0,
              let plist = try? PropertyListSerialization.propertyList(from: attached.output, format: nil) as? [String: Any],
              let entities = plist["system-entities"] as? [[String: Any]],
              let mounted = entities.compactMap({ $0["mount-point"] as? String }).first else {
            try? files.removeItem(at: place)
            return .failure(.unopened("hdiutil attach said \(attached.status)"))
        }
        defer { detach(mounted) }
        report(0.1)

        let volume = URL(fileURLWithPath: mounted)
        let inside = (try? files.contentsOfDirectory(at: volume, includingPropertiesForKeys: nil)) ?? []
        guard let found = inside.first(where: { $0.lastPathComponent == "Tomoshibi.app" }) ?? inside.first(where: { $0.pathExtension == "app" }) else {
            try? files.removeItem(at: place)
            return .failure(.unopened("no app in the image"))
        }
        let staged = place.appendingPathComponent(running.lastPathComponent)
        guard copy(found, to: staged, report: { report(0.1 + 0.55 * $0) }) else {
            try? files.removeItem(at: place)
            return .failure(.unopened("the app would not copy out"))
        }

        report(0.68)
        let refusal = check(staged, version: version, identifier: identifier, signing: signing)
        if let refusal {
            try? files.removeItem(at: place)
            return .failure(refusal)
        }
        // Checked: what is in place of the running app is the developer's
        // and Apple's, and macOS need not ask again whether to open
        // something downloaded from the internet.
        _ = run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", staged.path])
        report(0.9)
        return .success(staged)
    }

    /// ditto, saying how far it has got as the part of the app's size that
    /// has arrived, every 150 ms until it ends.
    nonisolated static func copy(_ from: URL, to: URL, report: @Sendable (Double) -> Void) -> Bool {
        let total = max(1, size(of: from))
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = [from.path, to.path]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return false }
        while process.isRunning {
            Thread.sleep(forTimeInterval: 0.15)
            report(min(1, Double(size(of: to)) / Double(total)))
        }
        return process.terminationStatus == 0
    }

    /// The bytes of the files in a folder, all the way down.
    nonisolated static func size(of folder: URL) -> Int64 {
        let keys: [URLResourceKey] = [.fileSizeKey, .isRegularFileKey]
        guard let walk = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: keys) else { return 0 }
        var bytes: Int64 = 0
        for case let file as URL in walk {
            guard let values = try? file.resourceValues(forKeys: Set(keys)), values.isRegularFile == true else { continue }
            bytes += Int64(values.fileSize ?? 0)
        }
        return bytes
    }

    /// Whether the app at `staged` is the build offered, signed as it must be.
    nonisolated static func check(_ staged: URL, version: String, identifier: String, signing: UpdateTrust.Signing) -> Refusal? {
        guard let info = NSDictionary(contentsOf: staged.appendingPathComponent("Contents/Info.plist")) as? [String: Any] else {
            return .notTheBuild("no Info.plist")
        }
        // The version and the build, joined as ClientDescription says them
        // (the core's version_with_build: 0.1.4, then 0.1.4.2), against the
        // version offered read the same way, where 0.1.4.1, as the deployment
        // writes a first build for the Macs from before 0.1.2, is 0.1.4.
        let said = versionWithBuild(version: info["CFBundleShortVersionString"] as? String ?? "",
                                    build: info["CFBundleVersion"] as? String ?? "")
        guard info["CFBundleIdentifier"] as? String == identifier else { return .notTheBuild("another app") }
        guard said == Self.firstBuildAlone(version) else { return .notTheBuild("version \(said), not \(version)") }
        if let refusal = UpdateTrust.refusal(of: staged, identifier: identifier, signing: signing) { return .untrusted(refusal) }
        return nil
    }

    /// A version offered as the apps from 0.1.4 call themselves: four parts
    /// with a first build as the fourth (0.1.4.1) are the three alone.
    nonisolated static func firstBuildAlone(_ version: String) -> String {
        let parts = version.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return version }
        return versionWithBuild(version: parts.prefix(3).joined(separator: "."), build: String(parts[3]))
    }

    private nonisolated static func detach(_ mounted: String) {
        if run("/usr/bin/hdiutil", ["detach", mounted, "-quiet"]).status != 0 {
            _ = run("/usr/bin/hdiutil", ["detach", mounted, "-force", "-quiet"])
        }
    }

    private nonisolated static func run(_ tool: String, _ arguments: [String]) -> (status: Int32, output: Data) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        let out = Pipe()
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return (-1, Data()) }
        let output = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, output)
    }
}
#endif
