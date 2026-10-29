#if os(macOS)
import Foundation
import Security

/// Whether a Mac build that arrived as an update may replace the running app:
/// signed with this team's Developer ID, for this app, and notarized.
///
/// The download is already checked against the size and SHA-256 the
/// deployment named, which says the file is the one the deployment meant to
/// hand out. This says it is one the developer made: a deployment's manifest
/// written wrongly, or by somebody who should not have, cannot put an app of
/// anybody else's in place of this one, because that app would have to be
/// signed by the team's Developer ID certificate, whose key is not on the
/// deployment, and accepted by Apple's notary service.
///
/// The requirement is the Developer ID one Xcode writes into every such
/// app's designated requirement (`codesign -d -r-` on a release build):
/// Apple's anchor, the Developer ID intermediate (1.2.840.113635.100.6.2.6),
/// a Developer ID Application leaf (…6.1.13), and the team in the leaf's
/// organisational unit; with the bundle identifier besides, so that another
/// app of the same team is not taken for this one. Notarization is what
/// `spctl --assess` says, which reads the stapled ticket.
///
/// A debug build may be told to accept a development signature instead
/// (TOMOSHIBI_DEBUG_UPDATE_DEVELOPMENT_SIGNED=1), so that the whole update can
/// be tried between two debug builds, which no notary will look at. The
/// switch, and the development requirement with it, exist only in a debug
/// build: in a release build neither is compiled, so nothing in the
/// environment can relax what a release accepts (UpdateTrustTests holds
/// both configurations to it).
public enum UpdateTrust {
    public static let team = "SM86NB3CPZ"

    public enum Signing: Equatable, Sendable {
        case developerID
        #if DEBUG
        case development
        #endif
    }

    /// The signing a build must carry to be installed, by the process's own
    /// environment unless another is given. The environment is read only in
    /// a debug build, so a release build does not look at it at all.
    public static func signing(environment: [String: String]? = nil) -> Signing {
        #if DEBUG
        let environment = environment ?? ProcessInfo.processInfo.environment
        if environment["TOMOSHIBI_DEBUG_UPDATE_DEVELOPMENT_SIGNED"] == "1" { return .development }
        #endif
        return .developerID
    }

    static func requirement(_ signing: Signing, identifier: String) -> String {
        let app = "anchor apple generic and identifier \"\(identifier)\""
        switch signing {
        case .developerID:
            return app + " and certificate 1[field.1.2.840.113635.100.6.2.6] exists"
                + " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
                + " and certificate leaf[subject.OU] = \"\(team)\""
        #if DEBUG
        case .development:
            // Apple's worldwide developer relations intermediate and an
            // Apple Development leaf, of the same team.
            return app + " and certificate 1[field.1.2.840.113635.100.6.2.1] exists"
                + " and certificate leaf[field.1.2.840.113635.100.6.1.12] exists"
                + " and certificate leaf[subject.OU] = \"\(team)\""
        #endif
        }
    }

    /// Why the app at `url` is not one to install, or nil where it is: its
    /// signature, all of it and every architecture, valid and meeting the
    /// requirement for `signing`; and, for a Developer ID build, notarized.
    public static func refusal(of url: URL, identifier: String, signing: Signing) -> Refusal? {
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(url as CFURL, [], &code) == errSecSuccess, let code else { return .unsigned }

        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(Self.requirement(signing, identifier: identifier) as CFString, [], &requirement) == errSecSuccess,
              let requirement else { return .unsigned }

        // Strict, every architecture, and the frameworks and helpers inside
        // as well as the app's own executable: a valid outer signature over a
        // nested binary that was swapped is otherwise accepted.
        let flags = SecCSFlags(rawValue: kSecCSCheckAllArchitectures | kSecCSStrictValidate | kSecCSCheckNestedCode)
        var problem: Unmanaged<CFError>?
        let status = SecStaticCodeCheckValidityWithErrors(code, flags, requirement, &problem)
        problem?.release()
        switch status {
        case errSecSuccess: break
        case errSecCSUnsigned: return .unsigned
        case errSecCSReqFailed: return .notOurs
        default: return .broken
        }

        if signing == .developerID, !notarized(url) { return .notNotarized }
        return nil
    }

    public enum Refusal: Equatable, Sendable {
        /// No signature at all.
        case unsigned
        /// Signed, but not by this team's certificate for this app.
        case notOurs
        /// Signed, and the signature does not hold: something inside was
        /// changed after it was made.
        case broken
        /// Signed by the team and not accepted by Apple's notary service.
        case notNotarized
    }

    /// `spctl --assess --type execute`, which accepts a Developer ID app
    /// only with a notarization ticket, and says which kind it accepted.
    static func notarized(_ url: URL) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/spctl")
        process.arguments = ["--assess", "--type", "execute", "--verbose=2", url.path]
        let said = Pipe()
        process.standardOutput = said
        process.standardError = said
        do { try process.run() } catch { return false }
        let output = said.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        // "accepted" alone is not enough: it is also what an app the person
        // has once allowed by hand is, whatever signed it.
        return process.terminationStatus == 0
            && String(decoding: output, as: UTF8.self).contains("source=Notarized Developer ID")
    }
}
#endif
