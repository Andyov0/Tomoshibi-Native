import Messages
import Observation
import TomoshibiBasics
import UIKit

/// What the extension shows and does: the account's meetings and a new one
/// for now on the way out, a card that was pressed on the way in.
@MainActor
@Observable
final class InviteModel {
    /// Where the meetings stand.
    enum Status: Equatable {
        /// Nobody is signed in to the app, or the app has not run since it was
        /// installed.
        case signedOut
        /// The deployment has ended the session the app left.
        case expired
        case loading
        case ready
        case failed
    }

    /// What the deployment says of a card that was pressed.
    enum Reading: Equatable {
        case asking
        case known(Meeting)
        /// Cancelled, or swept a day after its time.
        case gone
        case unreachable
        /// A link to a deployment this build is not for.
        case elsewhere
    }

    /// The deployment, as an origin: home's, from the build (the extension's
    /// Info.plist says the host Deployment.xcconfig gave it), and in a debug
    /// build the test deployment the app was last driven against, which the
    /// app leaves in the app group because Messages starts an extension with
    /// none of the test's environment (Handover `testBase`). Nothing else: a
    /// deployment named by a card is not one this build talks to.
    let base: String?
    private(set) var record: Handover.Record?
    private(set) var status: Status = .loading
    /// The meetings still ahead or under way, soonest first.
    private(set) var meetings: [Meeting] = []
    /// A meeting being started for now.
    private(set) var starting = false
    /// The meeting a card is being made of.
    private(set) var sending: String?
    /// Something to say at the foot, which fades.
    private(set) var problem: String?
    var style: MSMessagesAppPresentationStyle = .compact {
        didSet {
            // Back to compact is back to sending; a card read in the larger
            // view is put away with it, or the next press on the arrow would
            // open on it again.
            if style == .compact { opened = nil }
        }
    }
    /// The card pressed to open this, while it is shown.
    private(set) var opened: MeetingLink.Opened?
    private(set) var reading: Reading = .asking

    private let insert: (MSMessage) async -> Bool
    private let insertText: (String) async -> Bool
    private let open: (URL) -> Void
    private let collapse: () -> Void
    private let handover = Handover.shared
    private var problemTask: Task<Void, Never>?

    init(insert: @escaping (MSMessage) async -> Bool, insertText: @escaping (String) async -> Bool,
         open: @escaping (URL) -> Void, collapse: @escaping () -> Void) {
        self.insert = insert
        self.insertText = insertText
        self.open = open
        self.collapse = collapse
        base = Self.home
        adopt(base.flatMap(handover.record(for:)))
    }

    static var home: String? {
        #if DEBUG
        if let test = Handover.shared.testBase.flatMap(Handover.origin) { return test }
        #endif
        guard let host = Bundle.main.object(forInfoDictionaryKey: "TomoshibiHost") as? String, !host.isEmpty else { return nil }
        return Handover.origin("https://" + host)
    }

    /// What the app left: the account, its meetings as last seen, and the
    /// language and colour chosen in it, worn here as the app wears them and
    /// written nowhere, since they are the app's to keep.
    private func adopt(_ record: Handover.Record?) {
        self.record = record
        Localization.shared.keeps = false
        if let chosen = record?.language.flatMap(Language.init(rawValue:)) {
            Localization.shared.language = chosen
        } else if !Localization.shared.followsSystem {
            Localization.shared.followSystem()
        }
        Appearance.shared.keeps = false
        if let accent = record?.accent.flatMap(Accent.init(rawValue:)) { Appearance.shared.accent = accent }
        meetings = Self.upcoming(record?.meetings ?? [])
        status = record?.account == nil ? .signedOut : meetings.isEmpty ? .loading : .ready
    }

    private var api: MeetingsAPI? {
        guard let base, let url = URL(string: base) else { return nil }
        return MeetingsAPI(base: url, cookie: handover.cookie(for: base), userAgent: Self.userAgent)
    }

    private static func upcoming(_ all: [Meeting]) -> [Meeting] {
        all.filter { !$0.ended }.sorted { ($0.instant ?? .distantFuture) < ($1.instant ?? .distantFuture) }
    }

    // MARK: Sending

    /// Reads again what the app left and asks the deployment for the
    /// account's meetings, as the lobby's schedule asks for them.
    func refresh() async {
        adopt(base.flatMap(handover.record(for:)))
        guard record?.account != nil, let base, handover.cookie(for: base) != nil, let api else {
            status = .signedOut
            return
        }
        if meetings.isEmpty { status = .loading }
        do {
            meetings = Self.upcoming(try await api.mine())
            status = .ready
        } catch MeetingsFault.refused(_, "not_signed_in") {
            status = .expired
        } catch {
            status = .failed
        }
    }

    var arranges: Bool { record?.arranges ?? false }

    /// A meeting for now, on a name nobody has used, sent as a card. Arranged
    /// rather than opened: the host is not in it yet, and an arranged
    /// meeting's link is the one way in that exists before they are
    /// (internal/app/meeting.go). Whoever opens it waits until the host
    /// joins, and the host's own join starts it.
    func startNow() async {
        guard !starting, sending == nil, let api else { return }
        guard arranges else {
            say(t("Arranging meetings has not been opened to you."))
            return
        }
        starting = true
        defer { starting = false }
        do {
            let made = try await api.arrange(room: RoomNames.generated(), at: Date())
            meetings = Self.upcoming(meetings + [made])
            await send(made)
        } catch let MeetingsFault.refused(_, code) {
            if code == "not_signed_in" { status = .expired }
            say(MeetingRefusals.sentence(code))
        } catch {
            say(t("Could not reach the server."))
        }
    }

    /// The card for one, put in the field the person sends from.
    func send(_ meeting: Meeting) async {
        guard let base, let token = meeting.token,
              let link = MeetingLink.link(.meeting, base: base, token: token, room: meeting.room) else { return }
        sending = meeting.id
        defer { sending = nil }
        let layout = MSMessageTemplateLayout()
        layout.image = InvitePicture.image()
        layout.caption = meeting.room
        layout.subcaption = meeting.instant.map(MeetingTimes.said) ?? meeting.at
        let message = MSMessage()
        message.layout = layout
        message.url = link
        // What a notification, and a device that cannot draw the card, say.
        message.summaryText = t("You were invited to {room}.", ["room": meeting.room])
        message.accessibilityLabel = "\(message.summaryText ?? "") \(layout.subcaption ?? "")"
        guard await insert(message) else {
            say(t("Something went wrong. Try again."))
            return
        }
        // The link as well, as text beside the card. A card is drawn and
        // opened by its extension, and a phone without the app has none: the
        // simulator's Messages hides such a card altogether, and on a phone
        // Messages offers the app from the App Store (as developers on
        // Apple's forums report it), where this app, given out through
        // TestFlight, is not. A friend without it would have been sent
        // something that opens nothing. As text, the link opens in that
        // phone's browser, on the deployment's page that offers TestFlight or
        // the browser; with the app, it opens in the app as any of the
        // deployment's links does. After the invitation's sentence rather
        // than alone: an address alone in the field is made a link preview
        // of its own, and the field holds one such thing, so it took the
        // card's place.
        _ = await insertText("\(message.summaryText ?? "") \(link.absoluteString)")
        collapse()
    }

    /// The app, for signing in or out.
    func openApp() {
        guard let base, var parts = URLComponents(string: base) else { return }
        parts.scheme = "tomoshibi"
        if let url = parts.url { open(url) }
    }

    // MARK: A card that was pressed

    func show(_ message: MSMessage?) {
        guard let url = message?.url, let link = MeetingLink.read(url) else {
            opened = nil
            return
        }
        opened = link
        reading = .asking
        guard link.base == base, let api else {
            reading = .elsewhere
            return
        }
        Task {
            let token = link.token
            do {
                let said = try await api.arranged(token: token)
                guard opened?.token == token else { return }
                reading = .known(said)
            } catch MeetingsFault.refused(_, "no_such_meeting") {
                if opened?.token == token { reading = .gone }
            } catch {
                if opened?.token == token { reading = .unreachable }
            }
        }
    }

    /// Into the meeting, in the app: the card's own link under the app's
    /// scheme. The app reads it as it reads the link from anywhere else
    /// (AppModel `open`), waiting for the host or starting the meeting for
    /// the one who arranged it.
    func join() {
        guard let opened, let url = MeetingLink.inApp(opened) else { return }
        open(url)
    }

    // MARK: Saying

    private func say(_ sentence: String) {
        problemTask?.cancel()
        problem = sentence
        problemTask = Task {
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            problem = nil
        }
    }

    /// What the app says it is (ClientDescription), for the deployment's
    /// management pages: the extension is the same app on the same phone.
    private static var userAgent: String {
        let info = Bundle.main.infoDictionary
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        return ClientAgent.userAgent(
            platform: pad ? "iPad" : "iPhone",
            version: ClientAgent.version(info?["CFBundleShortVersionString"] as? String ?? "",
                                         build: info?["CFBundleVersion"] as? String ?? ""),
            system: "\(pad ? "iPadOS" : "iOS") \(UIDevice.current.systemVersion)",
            device: model)
    }

    /// ClientDescription `model`: a simulator's hw.machine is the Mac's.
    private static var model: String {
        #if targetEnvironment(simulator)
        if let simulated = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] { return simulated + " Simulator" }
        #endif
        var size = 0
        guard sysctlbyname("hw.machine", nil, &size, nil, 0) == 0, size > 0 else { return "" }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname("hw.machine", &bytes, &size, nil, 0) == 0 else { return "" }
        return String(decoding: bytes.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
    }
}
