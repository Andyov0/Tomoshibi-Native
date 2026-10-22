import Foundation
import TomoshibiCore
#if canImport(UIKit)
import UIKit
#endif

/// What this app tells the deployment it is, for the deployment's management
/// pages: the platform, the version as the download page writes it (0.1.0
/// build 8 is 0.1.0.8), the system and the model. The core puts it in the user
/// agent of every request (core/protocol/src/api/agent.rs); left untold it
/// says only the platform it was built for, which on iOS cannot tell an iPad
/// from an iPhone.
@MainActor
enum ClientDescription {
    /// The version as the update check and the deployment write it: the
    /// version alone for its first build and the build as a fourth part for
    /// a later one, 0.1.4 then 0.1.4.2 (the core's version_with_build; the
    /// owner, 2026-10-22: a small fix keeps the version and takes the next
    /// build). People are told it as "0.1.4 (2)" (versionLabel).
    static var version: String {
        let info = Bundle.main.infoDictionary
        return versionWithBuild(version: info?["CFBundleShortVersionString"] as? String ?? "",
                                build: info?["CFBundleVersion"] as? String ?? "")
    }

    static func tell() {
        let version = version
        #if os(iOS)
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        describeClient(
            platform: pad ? "iPad" : "iPhone",
            version: version,
            system: "\(pad ? "iPadOS" : "iOS") \(UIDevice.current.systemVersion)",
            device: model())
        #else
        let system = ProcessInfo.processInfo.operatingSystemVersion
        describeClient(
            platform: "Mac",
            version: version,
            system: "macOS \(system.majorVersion).\(system.minorVersion)"
                + (system.patchVersion > 0 ? ".\(system.patchVersion)" : ""),
            device: named("hw.model"))
        #endif
    }

    #if os(iOS)
    /// The phone's model. A simulator's hw.machine is the Mac's processor,
    /// "arm64", which on the management page read as a device of that name;
    /// it says which model it is standing in for elsewhere.
    private static func model() -> String {
        #if targetEnvironment(simulator)
        if let simulated = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] {
            return simulated + " Simulator"
        }
        #endif
        return named("hw.machine")
    }
    #endif

    /// The model the system reports under `key`: "iPhone17,1" for hw.machine
    /// on a phone, "Mac16,7" for hw.model on a Mac. Empty where it will not
    /// say.
    private static func named(_ key: String) -> String {
        var size = 0
        guard sysctlbyname(key, nil, &size, nil, 0) == 0, size > 0 else { return "" }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname(key, &bytes, &size, nil, 0) == 0 else { return "" }
        return String(cString: bytes)
    }
}
