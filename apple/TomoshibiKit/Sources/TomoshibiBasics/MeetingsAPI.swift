import Foundation

/// One arranged meeting, as host and guest are told it (meeting.ts
/// `Arrangement`, the core's `Arrangement`). Kept as well as read: the app
/// leaves the meetings it last saw for the Messages extension (Handover).
public struct Meeting: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var room: String
    /// When, as an instant (RFC 3339); shown in the reader's own zone.
    public var at: String
    public var relay: String?
    public var started: Bool
    public var ended: Bool
    /// When the host may begin it, which is before `at`.
    public var from: String?
    /// The secret in the link, said only to the host.
    public var token: String?
    /// Whether whoever asked arranged it.
    public var mine: Bool

    public init(id: String, room: String, at: String, relay: String? = nil, started: Bool = false, ended: Bool = false,
                from: String? = nil, token: String? = nil, mine: Bool = false) {
        self.id = id
        self.room = room
        self.at = at
        self.relay = relay
        self.started = started
        self.ended = ended
        self.from = from
        self.token = token
        self.mine = mine
    }

    /// Read the way the core reads one (api/mod.rs `read`): a field of the
    /// wrong type is as good as absent, and absent is the empty value, so a
    /// deployment that adds a field or sends `"relay": ""` changes nothing.
    init(said: [String: Any]) {
        func text(_ key: String) -> String { said[key] as? String ?? "" }
        func someText(_ key: String) -> String? { (said[key] as? String).flatMap { $0.isEmpty ? nil : $0 } }
        func yes(_ key: String) -> Bool { said[key] as? Bool == true }
        self.init(id: text("id"), room: text("room"), at: text("at"), relay: someText("relay"), started: yes("started"),
                  ended: yes("ended"), from: someText("from"), token: someText("token"), mine: yes("mine"))
    }

    /// `at` as a date.
    public var instant: Date? { Self.instant(at) }

    /// An RFC 3339 instant with or without its fractions, as the server
    /// writes one and as `toISOString` does.
    public static func instant(_ text: String) -> Date? {
        let fractions = ISO8601DateFormatter()
        fractions.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractions.date(from: text) ?? ISO8601DateFormatter().date(from: text)
    }
}

/// What a request to the deployment came back as, when it was not an answer.
public enum MeetingsFault: Error, Equatable, Sendable {
    /// The deployment said no, with its code (`{"error": code}`): the app owns
    /// the sentence (MeetingRefusals), never the server.
    case refused(status: Int, code: String)
    /// Nothing answered.
    case network
    /// An answer of the wrong shape.
    case unexpected
}

/// The deployment's meetings, asked without the core, for the Messages
/// extension: its own arrangements, a new one, and what a link points at
/// (core/protocol/src/api/meetings.rs, which follows meeting.ts).
///
/// What the deployment sees from here is what it sees from the core and from
/// a browser: the same paths with the token escaped as encodeURIComponent
/// escapes it, `content-type: application/json` on every request, GETs
/// included, as meeting.ts sends them, and the arrangement's JSON in the
/// web's key order, room then at then relay. The session goes in the cookie
/// the server set (`meet-live.account`), from the Keychain the app shares,
/// and nothing the deployment answers is kept: a session's thirty sliding
/// days are the server's to keep.
///
/// Redirects are followed within the deployment and nowhere else, for the
/// core's reason (api/mod.rs): a request here carries the session, and a
/// proxy that sent it to another host would be handing that host somebody's
/// account.
public final class MeetingsAPI: Sendable {
    public let base: URL
    private let cookie: String?
    private let agent: String
    private let session: URLSession

    /// `base` is the deployment's origin; `cookie` the session's value, or nil
    /// to ask as nobody. `protocols` stand in for the network in the tests.
    public init(base: URL, cookie: String?, userAgent: String, protocols: [AnyClass] = []) {
        self.base = base
        self.cookie = cookie.flatMap { Self.cookieOctets($0) ? $0 : nil }
        agent = userAgent
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpShouldSetCookies = false
        configuration.httpCookieStorage = nil
        configuration.urlCache = nil
        // A card is waiting to be sent, and Messages may end an extension that
        // keeps it waiting; a server that has not answered in fifteen seconds
        // is better said to be out of reach.
        configuration.timeoutIntervalForRequest = 15
        if !protocols.isEmpty { configuration.protocolClasses = protocols }
        session = URLSession(configuration: configuration, delegate: StaysHome(origin: base), delegateQueue: nil)
    }

    deinit { session.finishTasksAndInvalidate() }

    /// The signed-in host's own arrangements, soonest first, each with its token.
    public func mine() async throws -> [Meeting] {
        let said = try await send("GET", "/api/meetings")
        return (said["meetings"] as? [[String: Any]] ?? []).map(Meeting.init(said:))
    }

    /// Arranges a meeting on `room` at `at`, on `relay` or wherever suits
    /// when it is empty. The time goes as `toISOString` writes it, an instant
    /// in UTC with milliseconds: the server refuses a time with no zone.
    public func arrange(room: String, at: Date, relay: String = "") async throws -> Meeting {
        let body = "{\"room\":\(Self.json(room)),\"at\":\(Self.json(Self.isoString(at))),\"relay\":\(Self.json(relay))}"
        return Meeting(said: try await send("POST", "/api/meetings", body: Data(body.utf8)))
    }

    /// What a meeting link points at; `no_such_meeting` for a dead one.
    public func arranged(token: String) async throws -> Meeting {
        Meeting(said: try await send("GET", "/api/meetings/" + MeetingLink.encodeComponent(token)))
    }

    /// `Date.prototype.toISOString`.
    static func isoString(_ date: Date) -> String {
        let format = ISO8601DateFormatter()
        format.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        format.timeZone = TimeZone(identifier: "UTC")
        return format.string(from: date)
    }

    /// One JSON string, escaped by the system's own writer.
    private static func json(_ text: String) -> String {
        (try? JSONSerialization.data(withJSONObject: text, options: [.fragmentsAllowed, .withoutEscapingSlashes]))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "\"\""
    }

    /// RFC 6265's cookie-octet, which the core holds a kept session to before
    /// it puts it back (`restore_account_cookie`); anything else in a header
    /// would end it and begin another.
    static func cookieOctets(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.allSatisfy { byte in
            byte == 0x21 || (0x23 ... 0x2B).contains(byte) || (0x2D ... 0x3A).contains(byte)
                || (0x3C ... 0x5B).contains(byte) || (0x5D ... 0x7E).contains(byte)
        }
    }

    private func send(_ method: String, _ path: String, body: Data? = nil) async throws -> [String: Any] {
        guard let url = URL(string: path, relativeTo: base)?.absoluteURL else { throw MeetingsFault.unexpected }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(agent, forHTTPHeaderField: "User-Agent")
        if let cookie { request.setValue("meet-live.account=\(cookie)", forHTTPHeaderField: "Cookie") }
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw MeetingsFault.network
        }
        guard let http = response as? HTTPURLResponse else { throw MeetingsFault.unexpected }
        let said = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        guard (200 ..< 300).contains(http.statusCode) else {
            // A redirect the guard declined comes back as itself: an answer
            // from somewhere that is not this deployment.
            if (300 ..< 400).contains(http.statusCode) { throw MeetingsFault.unexpected }
            // Go's plain-text 404 is a feature this deployment has not
            // configured (api/mod.rs `absent`); no code at all is the
            // general failure.
            let code = (said?["error"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                ?? (http.statusCode == 404 && said == nil ? "not_found" : "")
            throw MeetingsFault.refused(status: http.statusCode, code: code)
        }
        guard let said else { throw MeetingsFault.unexpected }
        return said
    }
}

/// Follows a redirect to the deployment's own origin and to nowhere else.
private final class StaysHome: NSObject, URLSessionTaskDelegate, Sendable {
    let origin: String

    init(origin: URL) {
        self.origin = Self.origin(of: origin)
    }

    static func origin(of url: URL) -> String {
        let parts = URLComponents(url: url, resolvingAgainstBaseURL: true)
        return "\(parts?.scheme ?? "")://\(parts?.host?.lowercased() ?? ""):\(parts?.port ?? (parts?.scheme == "http" ? 80 : 443))"
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest) async -> URLRequest? {
        guard let url = request.url, Self.origin(of: url) == origin else { return nil }
        return request
    }
}
