import Foundation
import Observation
import TomoshibiBasics
import TomoshibiCore

/// What the screens before a call show beside their forms: the meetings this
/// account has arranged (Schedule.tsx), the rooms joined lately, and the
/// relays as measured from here (ServerPicker.tsx). Kept apart from the views
/// because the Mac's sidebar and the phone's home show the same things laid
/// out differently, and a list fetched twice is a list that disagrees with
/// itself for a moment.
@MainActor
@Observable
public final class LobbyModel {
    @ObservationIgnored weak var app: AppModel?

    init(app: AppModel) {
        self.app = app
        recents = Self.loadRecents()
    }

    // MARK: Arranged meetings

    public enum Status: Equatable { case loading, ready, failed, signedOut }

    public private(set) var arrangements: [Arrangement] = []
    public private(set) var status: Status = .loading
    @ObservationIgnored private var reads = 0

    /// The ones still ahead or under way, soonest first.
    public var upcoming: [Arrangement] { arrangements.filter { !$0.ended } }

    public func refreshArrangements() async {
        guard let deployment = app?.deployment, app?.account != nil else { return }
        reads += 1
        let mine = reads
        status = .loading
        do {
            let found = try await deployment.arrangements()
            guard mine == reads else { return }
            arrangements = found
            status = .ready
        } catch let ProtocolError.Refused(_, code) where code == "not_signed_in" {
            guard mine == reads else { return }
            status = .signedOut
        } catch {
            guard mine == reads else { return }
            status = .failed
        }
    }

    /// Arranges one and returns it; the error is the sentence to say.
    public func arrange(room: String, at: Date, relay: String?) async -> Result<Arrangement, ArrangeError> {
        guard let deployment = app?.deployment else { return .failure(ArrangeError(t("Something went wrong. Try again."))) }
        do {
            let made = try await deployment.arrangeMeeting(room: room, atMs: Int64(at.timeIntervalSince1970 * 1000), relay: relay ?? "")
            reads += 1
            arrangements = (arrangements + [made]).sorted { $0.at < $1.at }
            status = .ready
            return .success(made)
        } catch {
            return .failure(ArrangeError(Self.explain(error)))
        }
    }

    public func cancel(_ one: Arrangement) async -> String? {
        guard let deployment = app?.deployment else { return nil }
        do {
            try await deployment.cancelMeeting(id: one.id)
            reads += 1
            arrangements.removeAll { $0.id == one.id }
            return nil
        } catch {
            return Self.explain(error)
        }
    }

    /// meeting.ts `linkFor`: the page, the meeting's token, the room.
    public func link(_ one: Arrangement) -> URL? {
        guard let base = app?.deployment?.baseUrl(), let token = one.token else { return nil }
        let root = base.hasSuffix("/") ? base : base + "/"
        let encoded = token.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? token
        return URL(string: "\(root)?meeting=\(encoded)#/\(one.room)")
    }

    /// meeting.ts `whenSaid`: the date, the time and the zone, in the
    /// reader's language.
    public static func said(_ date: Date) -> String {
        date.formatted(Date.FormatStyle(date: .abbreviated, time: .shortened, locale: Localization.shared.language.locale)
            .timeZone(.specificName(.short)))
    }

    /// calendar.ts `icsFor`, written where the system can hand it to a
    /// calendar.
    public func calendarFile(_ one: Arrangement) -> URL? {
        guard let link = link(one)?.absoluteString, let start = AppModel.instant(one.at) else { return nil }
        let format = DateFormatter()
        format.locale = Locale(identifier: "en_US_POSIX")
        format.timeZone = TimeZone(identifier: "UTC")
        format.dateFormat = "yyyyMMdd'T'HHmmss'Z'"
        let escape: (String) -> String = {
            $0.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: ";", with: "\\;")
                .replacingOccurrences(of: ",", with: "\\,").replacingOccurrences(of: "\n", with: "\\n")
        }
        let lines = [
            "BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Tomoshibi//Arranged meeting//EN", "CALSCALE:GREGORIAN",
            "METHOD:PUBLISH", "BEGIN:VEVENT", "UID:\(one.id)@tomoshibi", "DTSTAMP:\(format.string(from: Date()))",
            "DTSTART:\(format.string(from: start))", "SUMMARY:\(escape(one.room))", "URL:\(link)",
            "DESCRIPTION:\(escape(link))", "END:VEVENT", "END:VCALENDAR",
        ]
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(one.room).ics")
        guard (try? (lines.joined(separator: "\r\n") + "\r\n").write(to: file, atomically: true, encoding: .utf8)) != nil else { return nil }
        return file
    }

    /// Schedule.tsx `explainArranging`.
    static func explain(_ error: Error) -> String {
        guard case let ProtocolError.Refused(_, code) = error else { return t("Something went wrong. Try again.") }
        switch code {
        case "room_already_arranged": return t("That room already has a meeting arranged.")
        case "bad_time": return t("That time is not one a meeting can be arranged for.")
        case "meetings_need_invites": return t("Meetings here ask for an account, so a link cannot let anybody in.")
        case "not_yours": return t("That is somebody else's room.")
        case "too_many_meetings": return t("You have as many meetings arranged as you can have.")
        case "invalid_room": return t("Room names can only use lowercase letters, numbers and dashes.")
        case "scoped_room": return t("A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.")
        case "relay_not_allowed": return t("Access denied. That server is for administrators.")
        case "not_signed_in": return t("Your session has expired. Sign in again.")
        case "not_allowed": return t("Arranging meetings has not been opened to you.")
        default: return t("Something went wrong. Try again.")
        }
    }

    // MARK: Rooms lately

    /// A room this device joined, kept so it can be joined again in one
    /// press: a call app's recent list, which the web has no place for. Kept
    /// on this device only and per deployment, and never anything but the
    /// name and when.
    public struct Recent: Codable, Hashable, Identifiable, Sendable {
        public var room: String
        public var server: String
        public var at: Date
        public var id: String { server + "#" + room }
    }

    public private(set) var recents: [Recent]
    private static let recentsKey = "tomoshibi.recents"

    /// The ones joined on the deployment showing now, latest first.
    public var recentHere: [Recent] {
        let here = app?.deployment?.baseUrl() ?? ""
        return recents.filter { $0.server == here }.sorted { $0.at > $1.at }
    }

    func remember(room: String) {
        guard let server = app?.deployment?.baseUrl(), app?.rehearsal == false else { return }
        recents.removeAll { $0.server == server && $0.room == room }
        recents.insert(Recent(room: room, server: server, at: Date()), at: 0)
        recents = Array(recents.prefix(24))
        if let data = try? JSONEncoder().encode(recents) {
            UserDefaults.standard.set(data, forKey: Self.recentsKey)
        }
    }

    private static func loadRecents() -> [Recent] {
        guard let data = UserDefaults.standard.data(forKey: recentsKey) else { return [] }
        return (try? JSONDecoder().decode([Recent].self, from: data)) ?? []
    }

    // MARK: The relays from here

    /// Round trips by relay name, nil until measured; a relay that did not
    /// answer is present with no number.
    public private(set) var measured: [String: Double?]?
    public private(set) var measuring = false
    /// The order the first measurement settled on, so a row does not move out
    /// from under a finger already reaching for it.
    public private(set) var order: [String] = []

    public func measure() async {
        guard !measuring, let relays = app?.relays?.relays, !relays.isEmpty else { return }
        measuring = true
        defer { measuring = false }
        let timings = await relayTimings(relays: relays)
        var found: [String: Double?] = [:]
        for timing in timings { found[timing.name] = timing.millis }
        measured = found
        if order.isEmpty {
            order = timings.sorted { ($0.millis ?? .infinity) < ($1.millis ?? .infinity) }.map(\.name)
        }
    }

    /// The relays under their region, in the settled order.
    public func grouped() -> [(region: String, relays: [Relay])] {
        let relays = app?.relays?.relays ?? []
        let ranked = order.isEmpty ? relays : relays.sorted {
            (order.firstIndex(of: $0.name) ?? .max) < (order.firstIndex(of: $1.name) ?? .max)
        }
        var out: [(region: String, relays: [Relay])] = []
        for relay in ranked {
            let region = (relay.region ?? "").split(separator: "/").map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }.map { say($0) }.joined(separator: " · ")
            if let index = out.firstIndex(where: { $0.region == region }) {
                out[index].relays.append(relay)
            } else {
                out.append((region, [relay]))
            }
        }
        return out
    }

    /// The fastest relay that answered, which is what "automatic" settles on.
    ///
    /// Never one kept for administrators, and never one out of service
    /// (relays.ts `fastest`): the join measures without them, so naming one
    /// beside "Automatic" told an administrator the call would go somewhere it
    /// never would, and the measured winner on a fleet with an admin-only
    /// relay was often exactly that one.
    public var fastest: (name: String, millis: Double)? {
        guard let measured else { return nil }
        let usable = Set((app?.relays?.relays ?? []).filter { !$0.adminOnly && !$0.maintenance }.map(\.name))
        return measured.compactMap { name, millis in usable.contains(name) ? millis.map { (name, $0) } : nil }
            .min { $0.1 < $1.1 }
    }

    public func label(_ name: String) -> String {
        guard let relay = app?.relays?.relays.first(where: { $0.name == name }) else { return name }
        return say(relay.label ?? relay.name)
    }
}

/// A refusal to arrange, as a sentence.
public struct ArrangeError: Error {
    public let sentence: String
    init(_ sentence: String) { self.sentence = sentence }
}
