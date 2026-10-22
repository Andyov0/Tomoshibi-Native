import Foundation

/// The user agent the apps send, for a request made without the core: the
/// Messages extension's. The core puts it on everything else
/// (core/protocol/src/api/agent.rs), and the deployment's management pages
/// read it to say what somebody is using; an extension that sent URLSession's
/// own ("TomoshibiMessages/1 CFNetwork/… Darwin/…") would read there as a
/// device of its own that nobody owns.
///
/// The same rules as the core's `agent_of`, so that the extension and the app
/// on one phone say the same thing: `Tomoshibi/<version> (<platform>; <system>;
/// <model>)`, each part printable ASCII with none of the three characters the
/// server splits on, and an empty part left out.
public enum ClientAgent {
    public static func userAgent(platform: String, version: String, system: String, device: String) -> String {
        let detail = [clean(platform)] + [system, device].map(clean).filter { !$0.isEmpty }
        return "Tomoshibi/\(clean(version).replacingOccurrences(of: " ", with: ".")) (\(detail.joined(separator: "; ")))"
    }

    /// The core's `version_with_build`: the version alone for its first build
    /// and the build as a fourth part for a later one, 0.1.4 and then 0.1.4.2,
    /// which is what the app calls itself (ClientDescription).
    public static func version(_ version: String, build: String) -> String {
        let version = version.trimmingCharacters(in: .whitespaces)
        guard let number = UInt64(build.trimmingCharacters(in: .whitespaces)), number > 1 else { return version }
        return "\(version).\(number)"
    }

    /// agent.rs `clean`: a header value that is not printable ASCII refuses to
    /// be built, and `(`, `)` and `;` would split the platform wrongly at the
    /// server. A phone named in another script loses the name and keeps the
    /// rest.
    static func clean(_ part: String) -> String {
        let kept = part.unicodeScalars
            .filter { ($0.isASCII && $0.value > 0x20 && $0.value < 0x7F) || $0 == " " }
            .filter { !["(", ")", ";"].contains($0) }
        return String(String.UnicodeScalarView(kept)).split(separator: " ").joined(separator: " ")
    }
}
