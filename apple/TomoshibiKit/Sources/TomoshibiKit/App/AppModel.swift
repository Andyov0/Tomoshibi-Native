import Foundation
import Observation
import TomoshibiCore

/// Where the app is, and everything the screens before a call need.
///
/// Follows the web's App.tsx and doorway.ts, with one step the web never
/// needs: a browser is already at its deployment, while an app has to be told
/// which one. After that the order is the web's: who is signed in and what the
/// deployment allows decide between the lobby and the join form, an invitation
/// in a link wins over everything, and a call that the server moves is joined
/// again on its own.
@MainActor
@Observable
public final class AppModel {
    public enum Screen: Equatable, Sendable {
        /// The deployment the app opens on is being asked about; nothing to
        /// fill in yet.
        case starting
        case server, lobby, prejoin, knocking, waiting, call, ended
    }

    public private(set) var screen: Screen = .starting
    public private(set) var deployment: Deployment?
    public private(set) var info: DeploymentInfo?
    public private(set) var account: Account?
    public private(set) var call: CallModel?
    public private(set) var relays: RelayList?
    public private(set) var busy = false
    public var problem: String?

    public var serverAddress: String {
        didSet { if !rehearsal { UserDefaults.standard.set(serverAddress, forKey: Self.serverKey) } }
    }
    public var room = ""
    /// The web's own storage name, so the habit carries over.
    public var name: String {
        didSet { if !rehearsal { UserDefaults.standard.set(name, forKey: "meet-live.name") } }
    }
    public var passphrase = "" {
        didSet { if !rehearsal { Keychain.write(passphraseKey, passphrase) } }
    }
    /// Never stored, as the web never stores it (secrecy.ts).
    public var sealingWord = ""
    /// Nil picks whichever relay answers fastest.
    public var relayChoice: String?
    public var startWithMicrophone = true
    public var startWithCamera = false
    public private(set) var invite: String?
    public private(set) var invitation: InviteInfo?
    private var knockTask: Task<Void, Never>?
    /// The arranged meeting a `?meeting=` link points at, while it is waited
    /// for (meeting.ts, components/room/Waiting.tsx).
    public private(set) var arrangement: Arrangement?
    public private(set) var waitingSays: String?
    private var waitTask: Task<Void, Never>?

    private static let serverKey = "tomoshibi.server"

    /// The deployment this app is made for, which it opens on until somebody
    /// chooses another: the one whose web client it stands beside. Left empty,
    /// the app opened on a blank address field, and after the tests had run
    /// it opened on theirs.
    public static let home = "https://meet.example.com"

    /// A test is driving the app (TOMOSHIBI_BASE, honoured only in a debug
    /// build), and nothing it does is kept: not the server, the name, the
    /// passphrase, the session or the identity. The tests run the very app
    /// somebody uses, under the same bundle identifier, and the server they
    /// pointed it at was kept as the last one used, so the app opened by hand
    /// went to a loopback test deployment where no account existed, and
    /// signing in could only fail.
    let rehearsal: Bool

    /// The arranged meetings, recent rooms and measured relays the screens
    /// before a call show.
    @ObservationIgnored public private(set) lazy var lobby = LobbyModel(app: self)

    public init() {
        #if DEBUG
        rehearsal = ProcessInfo.processInfo.environment["TOMOSHIBI_BASE"] != nil
        #else
        rehearsal = false
        #endif
        serverAddress = UserDefaults.standard.string(forKey: Self.serverKey) ?? Self.home
        name = UserDefaults.standard.string(forKey: "meet-live.name") ?? ""
    }

    private var passphraseKey: String { "passphrase:" + (deployment?.baseUrl() ?? serverAddress) }
    private var cookieKey: String { "account:" + (deployment?.baseUrl() ?? serverAddress) }
    private var identityKey: String { "tomoshibi.identity." + (deployment?.baseUrl() ?? serverAddress) }

    // MARK: Reaching a deployment

    /// Picks up where the last launch left off.
    ///
    /// TOMOSHIBI_BASE and TOMOSHIBI_AUTOJOIN (a room) take the app straight
    /// into a call without a pointer, microphone and camera off so that no
    /// permission is asked for: what the interop scripts drive.
    public func start() async {
        let environment = ProcessInfo.processInfo.environment
        if rehearsal, let base = environment["TOMOSHIBI_BASE"] {
            serverAddress = base
            name = environment["TOMOSHIBI_NAME"] ?? "Native App"
            // TOMOSHIBI_MIC joins with the microphone on, for the echo
            // measurements, which need the real microphone hearing the real
            // speakers; asking for it means the permission prompt may show.
            startWithMicrophone = environment["TOMOSHIBI_MIC"] != nil
            startWithCamera = false
            await connect()
            // TOMOSHIBI_SIGNIN=<name>:<file holding the passphrase> signs in
            // first, so a test can see what somebody signed in sees; the
            // passphrase is read from a file to keep it off the command line.
            if let signIn = environment["TOMOSHIBI_SIGNIN"], let colon = signIn.firstIndex(of: ":"),
               let kept = try? String(contentsOfFile: String(signIn[signIn.index(after: colon)...]), encoding: .utf8) {
                await self.signIn(name: String(signIn[..<colon]), passphrase: kept.trimmingCharacters(in: .whitespacesAndNewlines))
            }
            // A passphrase to join with, so a test can join as the host of the
            // room it opens.
            if let typed = environment["TOMOSHIBI_PASSPHRASE"] { passphrase = typed }
            // TOMOSHIBI_SEALING_WORD seals the call with that word, so a test
            // can see what the features layered over a sealed call do.
            if let word = environment["TOMOSHIBI_SEALING_WORD"] { sealingWord = word }
            if let room = environment["TOMOSHIBI_AUTOJOIN"] {
                prepare(room: room)
                await join()
                // TOMOSHIBI_ORIGINAL: the same call with "original sound" on.
                if environment["TOMOSHIBI_ORIGINAL"] != nil { try? await call?.setOriginalSound(true) }
            } else if let room = environment["TOMOSHIBI_PREPARE"] {
                // Stops before the call, for a test to look at that screen.
                prepare(room: room)
            }
            return
        }
        if serverAddress.isEmpty {
            screen = .server
        } else {
            await connect()
        }
    }

    public func connect() async {
        busy = true
        defer { busy = false }
        do {
            var address = serverAddress.trimmingCharacters(in: .whitespacesAndNewlines)
            if !address.contains("://") { address = "https://" + address }
            let deployment = try Deployment(base: address)
            if let cookie = Keychain.read("account:" + deployment.baseUrl()) {
                try? deployment.restoreAccountCookie(value: cookie)
            }
            let info = try await deployment.deploymentInfo()
            self.deployment = deployment
            self.info = info
            serverAddress = deployment.baseUrl()
            passphrase = Keychain.read(passphraseKey) ?? ""
            account = try? await deployment.me()
            problem = nil
            screen = room.isEmpty ? .lobby : .prejoin
            Task { relays = try? await deployment.relays() }
        } catch {
            problem = explain(error)
            // Opening on a deployment that will not answer leaves somebody
            // where they can choose another, rather than on the mark.
            if screen == .starting { screen = .server }
        }
    }

    /// Asks again how much of the fleet is answering, for the light in the
    /// corner (Lobby.tsx `Fleet`, every minute).
    public func refreshRelays() async {
        guard let deployment else { return }
        if let fresh = try? await deployment.relays() { relays = fresh }
    }

    public func forgetServer() {
        deployment = nil
        info = nil
        account = nil
        screen = .server
    }

    // MARK: Links

    /// An invitation, an arranged meeting or a room, from a link opened with
    /// the app (`https://<deployment>/?invite=T#/room` and its kin, or the same
    /// under the `tomoshibi:` scheme).
    public func open(_ url: URL) async {
        guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return }
        let fragment = components.fragment ?? ""
        let linkRoom = fragment.hasPrefix("/") ? String(fragment.dropFirst()) : fragment
        let token = components.queryItems?.first { $0.name == "invite" }?.value
        let meeting = components.queryItems?.first { $0.name == "meeting" }?.value
        components.query = nil
        components.fragment = nil
        components.path = ""
        if components.scheme == "tomoshibi" { components.scheme = "https" }
        if let origin = components.url?.absoluteString, origin != deployment?.baseUrl() {
            serverAddress = origin
            await connect()
        }
        guard let deployment else { return }
        if let meeting {
            if !linkRoom.isEmpty { room = linkRoom }
            wait(for: meeting, on: deployment)
            return
        }
        if let token {
            invite = token
            invitation = try? await deployment.inviteInfo(token: token)
            if let name = invitation?.name, !name.isEmpty { self.name = name }
            room = invitation?.room ?? linkRoom
        } else if !linkRoom.isEmpty {
            // A link with a room and nothing else is a room name, and only
            // somebody signed in is taken anywhere by one of those.
            guard account != nil else {
                problem = t("{room} is by invitation. Ask the organiser for a link.", ["room": linkRoom])
                screen = .lobby
                return
            }
            room = linkRoom
        }
        if !room.isEmpty { screen = .prejoin }
    }

    // MARK: Account

    public func signIn(name: String, passphrase: String) async {
        guard let deployment else { return }
        busy = true
        defer { busy = false }
        do {
            account = try await deployment.signIn(name: name, passphrase: passphrase)
            if !rehearsal { Keychain.write(cookieKey, deployment.accountCookie()) }
            problem = nil
        } catch {
            problem = explain(error)
        }
    }

    public func signOut() async {
        guard let deployment else { return }
        try? await deployment.signOut()
        Keychain.write(cookieKey, nil)
        account = nil
    }

    // MARK: Joining

    /// Only somebody signed in, or holding an invitation, is taken to a room
    /// by its name: the app is a way in for the people who belong to the
    /// deployment and for the people they send links to, and a name typed by
    /// somebody nobody knows is neither (the owner's rule, 2026-10-15).
    public func prepare(room: String) {
        guard account != nil || invite != nil || rehearsal else {
            problem = t("Rooms here are by invitation. Open the link you were sent, or sign in.")
            return
        }
        self.room = room.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        problem = nil
        screen = .prejoin
    }

    /// Straight into a room by name from the start and join sheet, which has
    /// already shown the preview and taken the choices the join screen would.
    public func enter(room: String) async {
        guard account != nil || invite != nil || rehearsal else {
            problem = t("Rooms here are by invitation. Open the link you were sent, or sign in.")
            return
        }
        self.room = room.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        problem = nil
        await join()
    }

    public func join() async {
        guard let deployment else { return }
        busy = true
        defer { busy = false }
        do {
            // Signed in, the web sends the account's name and no passphrase:
            // the session is the proof (api.ts). A guest on an invitation with
            // a name takes that name.
            let signed = account != nil
            let grant = try await deployment.join(
                room: room,
                name: signed ? account!.name : name,
                passphrase: signed ? "" : passphrase,
                identity: UserDefaults.standard.string(forKey: identityKey),
                relay: relayChoice ?? "",
                invite: invite)
            // Every join's identity is kept: signing in or out makes the server
            // issue a new one, and a stale one sent back would be refused.
            if !rehearsal { UserDefaults.standard.set(grant.identity, forKey: identityKey) }
            let options = CallOptions(
                url: grant.url, token: grant.token, room: grant.room,
                sealingWord: sealingWord.isEmpty ? nil : sealingWord,
                forward: grant.forward.map { IceServer(url: $0.url, username: $0.username, credential: $0.credential) })
            let model = try await CallModel.join(options, deployment: deployment, region: grant.region)
            model.onMoved = { [weak self] relay in
                Task { await self?.rejoin(to: relay) }
            }
            call = model
            room = grant.room
            lobby.remember(room: grant.room)
            // The invitation is kept for as long as this call is: a move to
            // another relay and "Join again" both join once more, and an
            // invitation spent on the first join left a guest refused at the
            // door of the room they had just been in.
            problem = nil
            screen = .call
            if startWithMicrophone { try? await model.setMicrophone(true) }
            if startWithCamera { try? await model.setCamera(true) }
        } catch let error as ProtocolError {
            if case let .Refused(_, code) = error, code == "not_invited" || code == "not_in_scope" {
                problem = explain(error)
            } else {
                problem = explain(error)
            }
        } catch {
            problem = t("Could not join {room}.", ["room": room])
        }
    }

    /// The server moved the call to another relay; the web rejoins at once
    /// with the same passphrase and word, and lets the server choose.
    private func rejoin(to relay: String) async {
        let old = call
        movingTo = relay
        defer { movingTo = nil }
        call = nil
        await old?.leave()
        relayChoice = nil
        await join()
    }

    /// Where a move is taking the meeting while there is no call to show it
    /// over: between leaving the old room and entering the new one the room
    /// view is gone, and the screen was blank for those seconds. The web keeps
    /// its moving notice above every page for the same reason (App.tsx).
    public private(set) var movingTo: String?

    public func leave() async {
        await call?.leave()
        call = nil
        screen = .ended
        forgetRelayMeasurement()
    }

    public func backToLobby() {
        screen = .lobby
        problem = nil
        invite = nil
        invitation = nil
    }

    // MARK: An arranged meeting

    /// Asks after the meeting at the web's pace until it has begun (then
    /// joins with the invitation the server gives) or cannot any more.
    private func wait(for token: String, on deployment: Deployment) {
        waitTask?.cancel()
        screen = .waiting
        waitingSays = nil
        waitTask = Task { [weak self] in
            var firstSeen: Date?
            while !Task.isCancelled {
                guard let self else { return }
                do {
                    let arrangement = try await deployment.arranged(token: token)
                    self.arrangement = arrangement
                    self.room = arrangement.room
                    if arrangement.ended {
                        self.waitingSays = t("That meeting has ended.")
                        return
                    }
                    if let invite = arrangement.invite {
                        self.invite = invite
                        await self.join()
                        return
                    }
                    self.waitingSays = nil
                    let at = Self.instant(arrangement.at)
                    if let at, Date().timeIntervalSince(at) * 1000 > Double(hostNotComingAfterMs()) {
                        self.waitingSays = t("The host has not started this meeting.")
                        return
                    }
                    firstSeen = firstSeen ?? Date()
                    let pause = meetingAskEveryMs(at: arrangement.at, nowMs: Int64(Date().timeIntervalSince1970 * 1000))
                    try? await Task.sleep(for: .milliseconds(Int(pause)))
                } catch let error as ProtocolError {
                    if case let .Refused(_, code) = error, code == "no_such_meeting" {
                        self.waitingSays = t("That invitation is no longer good. Ask for another.")
                        return
                    }
                    self.waitingSays = t("Could not reach the meeting. Trying again.")
                    try? await Task.sleep(for: .seconds(5))
                } catch {
                    try? await Task.sleep(for: .seconds(5))
                }
            }
        }
    }

    /// The host of an arranged meeting starts it by joining.
    public func startArranged() async {
        waitTask?.cancel()
        await join()
    }

    public func stopWaiting() {
        waitTask?.cancel()
        arrangement = nil
        screen = .lobby
    }

    static func instant(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: text) ?? ISO8601DateFormatter().date(from: text)
    }

    // MARK: The door

    /// Asks the room's host to let this person in, then waits.
    public func knock() async {
        guard let deployment else { return }
        do {
            let ticket = try await deployment.knock(room: room, name: name)
            screen = .knocking
            knockTask?.cancel()
            knockTask = Task { [weak self] in
                let started = Date()
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(Int(knockAskEveryMs())))
                    guard let self, let answer = try? await deployment.atTheDoor(room: self.room, token: ticket.token) else { continue }
                    switch answer.state {
                    case .admitted:
                        self.invite = answer.invite
                        await self.join()
                        return
                    case .refused:
                        self.problem = t("Nobody let you in. Ask for a link.")
                        self.screen = .prejoin
                        return
                    case .knocking:
                        if Date().timeIntervalSince(started) * 1000 > Double(knockGiveUpAfterMs()) {
                            self.problem = t("Nobody let you in. Ask for a link.")
                            self.screen = .prejoin
                            return
                        }
                    }
                }
            }
        } catch {
            problem = explain(error)
        }
    }

    public func stopKnocking() {
        knockTask?.cancel()
        knockTask = nil
        screen = .prejoin
    }

    // MARK: Words

    /// What the server refused, in the reader's language (api.ts `explain`).
    public func explain(_ error: Error) -> String {
        guard let error = error as? ProtocolError else { return t("Something went wrong. Try again.") }
        switch error {
        case .Network:
            return t("Could not reach the server.")
        case let .Refused(_, code):
            switch code {
            case "rate_limited": return t("Too many attempts. Try again in a moment.")
            case "invalid_room": return t("Room names can only use lowercase letters, numbers and dashes.")
            case "room_not_open": return t("{room} isn't open. Ask the organiser for the link.", ["room": room])
            case "not_invited": return t("{room} is by invitation. Ask the organiser for a link.", ["room": room])
            case "not_in_scope": return t("Only members of {scope} can join {room}.", ["room": room, "scope": scopeOf(name: room) ?? ""])
            case "relay_not_allowed": return t("Access denied. That server is for administrators.")
            case "blocked": return t("This passphrase cannot join. Ask the organiser.")
            case "not_yours": return t("That name and passphrase do not go together.")
            case "server_error": return t("Something went wrong. Try again.")
            default: return t("Could not join {room}.", ["room": room])
            }
        default:
            return t("Something went wrong. Try again.")
        }
    }

    /// Whether the person could ask to be let in instead (PreJoin.tsx ~521).
    public var canKnock: Bool {
        problem == t("{room} is by invitation. Ask the organiser for a link.", ["room": room])
            || problem == t("Only members of {scope} can join {room}.", ["room": room, "scope": scopeOf(name: room) ?? ""])
    }
}
