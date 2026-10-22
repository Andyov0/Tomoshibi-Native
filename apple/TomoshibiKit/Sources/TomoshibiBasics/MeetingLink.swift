import Foundation

/// The links a meeting and an invitation travel as, made and read where the
/// core is not: the Messages extension, which must not carry it (an extension
/// is given a fraction of the app's memory, and the core's WebRTC alone is
/// more than all of it).
///
/// The forms are the web's to the character, because one link is opened by a
/// browser, by every app, and by the pattern the deployment's
/// apple-app-site-association gives the system for universal links:
///
/// - `{base}?meeting={token}#/{room}`, meeting.ts `linkFor` and the core's
///   `meeting_link`; an invitation is the same with `invite=`;
/// - `tomoshibi://{host}/?meeting={token}#/{room}`, the same link under the
///   app's own scheme, which is how the web's landing page asks the app to
///   open it (landing.ts `appLink`) and how the extension hands a card to the
///   app.
public enum MeetingLink {
    public enum Kind: String, Sendable {
        case invite, meeting
    }

    /// What a link says.
    public struct Opened: Equatable, Sendable {
        public var kind: Kind
        public var token: String
        /// The room, from the fragment; empty where the link names none.
        public var room: String
        /// The deployment it belongs to, as an origin ending in "/": the form
        /// the core's `base_url` gives, so the two compare as strings.
        public var base: String

        public init(kind: Kind, token: String, room: String, base: String) {
            self.kind = kind
            self.token = token
            self.room = room
            self.base = base
        }
    }

    /// `encodeURIComponent`, which the web and the core (text.rs) encode a
    /// token with: everything but ASCII letters, digits and `-_.!~*'()` as
    /// percent-encoded UTF-8. Foundation's `urlQueryAllowed` lets `&`, `=`,
    /// `+` and `/` through, and a token holding one of those would end the
    /// query item early, or arrive with a space where its `+` was.
    public static func encodeComponent(_ value: String) -> String {
        var out = ""
        for byte in value.utf8 {
            switch byte {
            case UInt8(ascii: "A") ... UInt8(ascii: "Z"), UInt8(ascii: "a") ... UInt8(ascii: "z"),
                 UInt8(ascii: "0") ... UInt8(ascii: "9"),
                 UInt8(ascii: "-"), UInt8(ascii: "_"), UInt8(ascii: "."), UInt8(ascii: "!"),
                 UInt8(ascii: "~"), UInt8(ascii: "*"), UInt8(ascii: "'"), UInt8(ascii: "("), UInt8(ascii: ")"):
                out.append(Character(UnicodeScalar(byte)))
            default:
                out += String(format: "%%%02X", byte)
            }
        }
        return out
    }

    /// The link handed out for one, on the deployment at `base`.
    public static func link(_ kind: Kind, base: String, token: String, room: String) -> URL? {
        let root = base.hasSuffix("/") ? base : base + "/"
        return URL(string: "\(root)?\(kind.rawValue)=\(encodeComponent(token))#/\(room)")
    }

    /// The same link under the app's scheme (landing.ts `appLink`): the host
    /// and port of the deployment, its query and its fragment. The app puts
    /// the deployment's own scheme back before it compares the origin
    /// (AppModel `open`).
    public static func inApp(_ opened: Opened) -> URL? {
        guard let origin = URLComponents(string: opened.base), let host = origin.host, !host.isEmpty else { return nil }
        let authority = origin.port.map { "\(host):\($0)" } ?? host
        let fragment = opened.room.isEmpty ? "" : "#/\(opened.room)"
        return URL(string: "tomoshibi://\(authority)/?\(opened.kind.rawValue)=\(encodeComponent(opened.token))\(fragment)")
    }

    /// What a link holds, read as the web's landing page reads one (landing.ts
    /// `opened`: an invitation before a meeting, each trimmed, an empty one
    /// as none) and as the app does (AppModel `open`: the room from the
    /// fragment, the origin alone as the deployment). The app's own scheme
    /// reads as https, which is what it stands for. Nil for anything that is
    /// not an invitation or a meeting.
    public static func read(_ url: URL) -> Opened? {
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let scheme = parts.scheme?.lowercased(), ["http", "https", "tomoshibi"].contains(scheme),
              let host = parts.host, !host.isEmpty else { return nil }
        let items = parts.queryItems ?? []
        func value(_ name: String) -> String? {
            items.first { $0.name == name }?.value?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
        }
        let kind: Kind
        let token: String
        if let invite = value(Kind.invite.rawValue) {
            (kind, token) = (.invite, invite)
        } else if let meeting = value(Kind.meeting.rawValue) {
            (kind, token) = (.meeting, meeting)
        } else {
            return nil
        }
        let fragment = parts.fragment ?? ""
        let room = fragment.hasPrefix("/") ? String(fragment.dropFirst()) : fragment
        let authority = parts.port.map { "\(host.lowercased()):\($0)" } ?? host.lowercased()
        let base = "\(scheme == "tomoshibi" ? "https" : scheme)://\(authority)/"
        return Opened(kind: kind, token: token, room: room, base: base)
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
