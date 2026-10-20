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
        case unreachable, lobby, prejoin, knocking, waiting, call, ended
    }

    /// Why the last call ended (App.tsx, the `ended` handler): three endings
    /// that look alike from the inside, a call that simply stopped, and want
    /// three different sentences. Somebody told only "You left the call."
    /// after the host closed the room reads the call as having broken.
    public enum Ending: Equatable, Sendable {
        /// They pressed leave.
        case left
        /// The room was closed for everybody (ROOM_DELETED, which the web
        /// calls DISCONNECT_ROOM_CLOSED).
        case closed
        /// They alone were taken out (PARTICIPANT_REMOVED).
        case removed
        /// Anything else: the connection failed and the client gave up.
        case lost
    }

    public private(set) var screen: Screen = .starting {
        didSet { updates?.lobby(screen == .lobby) }
    }
    /// The app's own updates (Updates), told of every change of screen,
    /// since the lobby is the only place one is shown or worked on. On the
    /// Mac only: the iPhone is updated by TestFlight and the App Store. None
    /// in a debug build started by hand, which has no test deployment to ask
    /// and would ask the real one.
    public private(set) var updates: Updates?
    public private(set) var deployment: Deployment?
    public private(set) var info: DeploymentInfo?
    public private(set) var account: Account?
    public private(set) var call: CallModel?
    public private(set) var relays: RelayList? {
        didSet { dropRelayNotOffered() }
    }
    /// Something is being asked of the deployment on the person's behalf, and
    /// the buttons that would ask it again wait. Counted rather than set and
    /// cleared, because the asking overlaps: a join that was called off is
    /// still out until it comes back (`cancelJoin`), and a flag cleared by a
    /// sign-in or a connect finishing in the meantime would offer a second
    /// join while the first could still land.
    public var busy: Bool { asking > 0 }
    private var asking = 0
    public private(set) var ending: Ending = .left

    /// Something to tell the person before a call, at the foot of the screen.
    /// Most of them fade (notices.ts `actionFailed`: a press that did not take
    /// is over, and pressing again is the remedy); `problemStays` marks the
    /// ones about why they are not in a meeting, which stay until dismissed
    /// (notices.ts `joinFailed`: "a message about why they are not in the
    /// meeting should not disappear while they are reading it"). The lobby's
    /// toast cleared everything after five seconds, so an invitation that
    /// opens tomorrow said when, and was gone before it had been read.
    public var problem: String? {
        didSet { if !telling { problemStays = false } }
    }
    public private(set) var problemStays = false
    @ObservationIgnored private var telling = false

    /// Says something that stays until the person closes it.
    public func tell(_ sentence: String) {
        telling = true
        problem = sentence
        problemStays = true
        telling = false
    }

    /// The deployment this app talks to: always `home`, except to a test that
    /// names its own. Nobody can choose it: the address field, "Change server"
    /// and links that took the app to another server are gone, because the
    /// apps are for this deployment and not for anybody's own (the core holds
    /// a release build to the same address, `home_deployment`).
    public private(set) var serverAddress: String
    public var room = ""
    /// The web's own storage name, so the habit carries over. Only ever what
    /// the person typed: an invitation's name is `invitedName` and is never
    /// written here (PreJoin.tsx: "Not a name somebody else chose for them.
    /// It is this link's, and keeping it would greet them by it on every visit
    /// after"). Opening such a link used to write it over the saved name.
    public var name: String {
        didSet { if !rehearsal { UserDefaults.standard.set(name, forKey: "meet-live.name") } }
    }
    public var passphrase = "" {
        didSet { if !rehearsal { Keychain.write(passphraseKey, passphrase) } }
    }
    /// Never stored, as the web never stores it (secrecy.ts).
    public var sealingWord = ""
    /// Nil picks whichever relay answers fastest. Kept under the web's own
    /// name (api.ts `rememberRelay`), so that a machine somebody picked is
    /// the one the next screen and the next launch say, rather than coming
    /// back as "Automatic". The web keeps it for the tab's sitting; an app
    /// has no tab, and on a phone the system ends the process whenever it
    /// likes, so it is kept, and dropped again whenever the deployment stops
    /// offering that relay (`dropRelayNotOffered`).
    public var relayChoice: String? {
        didSet {
            guard !rehearsal, relayChoice != oldValue else { return }
            UserDefaults.standard.set(relayChoice, forKey: Self.relayKey)
        }
    }
    /// What starts on when somebody joins (preferences.ts `meet-live.devices`):
    /// the microphone on and the camera off unless somebody chose otherwise.
    /// Written as it is switched, as the web's account page writes it, so the
    /// settings and the join screen are one choice and not two that disagree.
    public var startWithMicrophone: Bool {
        didSet { rememberDevices() }
    }
    public var startWithCamera: Bool {
        didSet { rememberDevices() }
    }
    /// Where the device choices are kept (DeviceMemory): nowhere in a
    /// rehearsal, so a test that turns the camera on or picks a microphone
    /// does not leave the owner's next call starting that way.
    /// TOMOSHIBI_DEBUG_DEVICE_MEMORY names a defaults suite to keep them in
    /// instead, so a test can see a choice carried from one launch to the
    /// next without touching the real one.
    let deviceMemory: DeviceMemory
    /// The store DeviceMemory keeps them in, which the settings write their
    /// device choices into as well, so that both read one record.
    let deviceDefaults: UserDefaults?
    public private(set) var invite: String?
    public private(set) var invitation: InviteInfo?
    private var knockTask: Task<Void, Never>?
    /// The arranged meeting a `?meeting=` link points at, while it is waited
    /// for (meeting.ts, components/room/Waiting.tsx).
    public private(set) var arrangement: Arrangement?
    public private(set) var waiting = Waiting()
    private var meetingToken: String?
    private var waitTask: Task<Void, Never>?
    /// Moved on by every new picture, so the old one is not shown from the
    /// image cache under the same address (`avatarAddress`).
    public private(set) var pictureVersion = 0

    /// Where the address somebody had typed used to be kept; read no more,
    /// and cleared, so that no copy of another server lingers.
    private static let serverKey = "tomoshibi.server"
    static let relayKey = "meet-live.relay"

    /// The deployment this app is made for, from the core, which keeps the
    /// one address every client is held to.
    public static let home = homeDeployment()

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

    /// What the settings show and write, one for the app however many times
    /// the settings are opened.
    @ObservationIgnored private(set) lazy var preferences = CallPreferences(keeps: !rehearsal, deviceDefaults: deviceDefaults)

    public init() {
        #if DEBUG
        rehearsal = ProcessInfo.processInfo.environment["TOMOSHIBI_BASE"] != nil
        #else
        rehearsal = false
        #endif
        serverAddress = Self.home
        if !rehearsal { UserDefaults.standard.removeObject(forKey: Self.serverKey) }
        name = UserDefaults.standard.string(forKey: "meet-live.name") ?? ""
        #if DEBUG
        let suite = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_DEVICE_MEMORY"].flatMap { UserDefaults(suiteName: $0) }
        #else
        let suite: UserDefaults? = nil
        #endif
        deviceDefaults = rehearsal ? suite : .standard
        deviceMemory = DeviceMemory(defaults: deviceDefaults)
        let joining = deviceMemory.joining
        startWithMicrophone = joining.microphone
        startWithCamera = joining.camera
        // A rehearsal joins wherever the test says, and a relay the person
        // picked on their own deployment means nothing on a test's.
        relayChoice = rehearsal ? nil : UserDefaults.standard.string(forKey: Self.relayKey)
        // The language and the look are the owner's too: a test that pressed
        // a language tile or a colour would leave the app opening in them the
        // next time it was opened by hand.
        Appearance.shared.keeps = !rehearsal
        Localization.shared.keeps = !rehearsal
        #if os(macOS)
        // Asked of the deployment the app talks to: the test's in a
        // rehearsal, and in a release build home, the only one its core will
        // speak to.
        #if DEBUG
        let base = rehearsal ? ProcessInfo.processInfo.environment["TOMOSHIBI_BASE"] : nil
        #else
        let base: String? = Self.home
        #endif
        if let base {
            updates = Updates(source: CoreUpdateSource(base: base), installer: MacInstaller(), running: ClientDescription.version)
        }
        #endif
    }

    private var passphraseKey: String { "passphrase:" + (deployment?.baseUrl() ?? serverAddress) }
    private var cookieKey: String { "account:" + (deployment?.baseUrl() ?? serverAddress) }
    private var identityKey: String { "tomoshibi.identity." + (deployment?.baseUrl() ?? serverAddress) }
    /// A rehearsal's identity, for its own joins only, as a fresh browser tab
    /// has one of its own. It read the identity this machine had kept while
    /// it never wrote one, so two test apps in one room joined as the same
    /// participant, and the server put the first out as a duplicate the
    /// moment the second arrived: a listener that "never subscribed" to the
    /// music had simply replaced its holder (2026-10-17).
    private var rehearsalIdentity: String?

    // MARK: What is switched on when somebody joins

    /// Kept in the web's own record (DeviceMemory, preferences.ts), which
    /// the settings page and the join screen both write, so they are one
    /// choice; a rehearsal keeps nothing.
    private func rememberDevices() {
        deviceMemory.remember(DeviceMemory.Joining(camera: startWithCamera, microphone: startWithMicrophone))
    }

    /// sound.ts `rememberedOriginal`: whether the voice goes out as it is.
    /// Never the owner's choice in a rehearsal, whose echo measurements say
    /// for themselves which voice they want (TOMOSHIBI_ORIGINAL).
    var originalSoundWanted: Bool {
        deviceMemory.originalSound
    }

    // MARK: Reaching a deployment

    /// Picks up where the last launch left off.
    ///
    /// TOMOSHIBI_BASE and TOMOSHIBI_AUTOJOIN (a room) take the app straight
    /// into a call without a pointer, microphone and camera off so that no
    /// permission is asked for: what the interop scripts drive.
    ///
    /// Once for the life of the model, however many windows ask. Every window
    /// the Mac opens runs its root's task, and one opened from the Dock after
    /// the last was closed asked the deployment about itself all over again
    /// and put the join screen up over a call that was still live: the
    /// microphone stayed on, with no way back to the call but the menu's
    /// Leave. A second window is shown whatever screen the model is on.
    public func start() async {
        guard !started else { return }
        started = true
        updates?.start()
        let environment = ProcessInfo.processInfo.environment
        #if DEBUG
        if rehearsal { AppDebug.start(self) }
        #endif
        if rehearsal, let base = environment["TOMOSHIBI_BASE"] {
            serverAddress = base
            name = environment["TOMOSHIBI_NAME"] ?? "Native App"
            // TOMOSHIBI_MIC joins with the microphone on, for the echo
            // measurements, which need the real microphone hearing the real
            // speakers; asking for it means the permission prompt may show.
            startWithMicrophone = environment["TOMOSHIBI_MIC"] != nil
            startWithCamera = false
            // TOMOSHIBI_DEBUG_DEVICE_MEMORY: joins as the suite it names
            // remembers, to check that the join screen's choice is carried.
            if environment["TOMOSHIBI_DEBUG_DEVICE_MEMORY"] != nil, environment["TOMOSHIBI_MIC"] == nil {
                let joining = deviceMemory.joining
                startWithMicrophone = joining.microphone
                startWithCamera = joining.camera
            }
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
            // TOMOSHIBI_DEBUG_SEAL=<word>: a sealing word already typed, so a
            // test can see the screens that say the call is sealed.
            if let word = environment["TOMOSHIBI_DEBUG_SEAL"] { sealingWord = word }
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
            } else if let link = environment["TOMOSHIBI_OPEN"], let url = URL(string: link) {
                // TOMOSHIBI_OPEN=<link>: an invitation or meeting link opened
                // as the system would hand it over, for a test to see what a
                // guest sees, a bad link included.
                await open(url)
            } else if let ended = environment["TOMOSHIBI_DEBUG_ENDED"] {
                // TOMOSHIBI_DEBUG_ENDED=left|closed|removed|lost: the screen
                // after a call, for each of the ways one ends.
                if let room = environment["TOMOSHIBI_ROOM"] { self.room = room }
                ending = switch ended {
                case "closed": .closed
                case "removed": .removed
                case "lost": .lost
                default: .left
                }
                screen = .ended
            }
            return
        }
        await connect()
    }

    @ObservationIgnored private var started = false

    public func connect() async {
        // Nothing here may take the screen off a call that is live; the
        // only way from a call to another deployment is to leave it first.
        guard call == nil, movingTo == nil else { return }
        asking += 1
        defer { asking -= 1 }
        do {
            var address = serverAddress.trimmingCharacters(in: .whitespacesAndNewlines)
            if !address.contains("://") { address = "https://" + address }
            // Before the deployment is made, which reads it then.
            ClientDescription.tell()
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
            tell(explain(error))
            // Opening on a deployment that will not answer leaves somebody
            // where they can try again, rather than on the mark.
            if screen == .starting { screen = .unreachable }
        }
    }

    /// Asks again how much of the fleet is answering, for the light in the
    /// corner (Lobby.tsx `Fleet`, every minute).
    public func refreshRelays() async {
        guard let deployment else { return }
        if let fresh = try? await deployment.relays() { relays = fresh }
    }

    /// Whether there is a machine to choose at all (PreJoin.tsx
    /// `offerChoice`): more than one relay, on a deployment that acts on the
    /// choice. One relay is not a choice, and a picker on a deployment that
    /// ignores it is a control that does nothing.
    public var offersRelayChoice: Bool {
        guard let relays else { return false }
        return relays.measure && relays.relays.count > 1
    }

    /// Whether the join screen and the meeting's waiting screen offer the
    /// server picker: where there is a choice, to somebody who did not come
    /// to them through the lobby. Somebody signed in who starts or joins from
    /// the lobby has its picker in the sheet they press Join in, and goes
    /// straight into the call; somebody who opened an invitation or a
    /// meeting's link, signed in or not, or who is not signed in at all, never
    /// sees the lobby's picker, and could not choose (the owner, 2026-10-19).
    /// The web's join screen offers it in the same place (PreJoin.tsx,
    /// `offerChoice`), and it is the lobby's own choice, `relayChoice`, so a
    /// machine picked on either is the one the other says.
    public var offersRelayChoiceAtTheDoor: Bool {
        offersRelayChoice && (account == nil || arrivedByLink)
    }

    /// The screen before a call was reached by a link (an invitation, a
    /// meeting, a room's address), handed over by the system or pasted, and
    /// not by the lobby's own join. Kept until the person goes back to the
    /// lobby, so a join that is refused, or a move that fails, comes back to
    /// the same screen with the same picker on it.
    public private(set) var arrivedByLink = false

    /// A relay remembered from before that this deployment does not offer,
    /// or no longer takes calls on, would be sent with every join and
    /// refused, or honoured for a machine nobody can see in the picker.
    private func dropRelayNotOffered() {
        guard let choice = relayChoice, let relays else { return }
        if !relays.relays.contains(where: { $0.name == choice && !$0.maintenance }) { relayChoice = nil }
    }

    // MARK: Links

    /// An invitation, an arranged meeting or a room, from a link opened with
    /// the app (`https://<deployment>/?invite=T#/room` and its kin, or the same
    /// under the `tomoshibi:` scheme).
    ///
    /// Not over a call. A link opened while one was live took the screen to
    /// the link's join screen and left the call publishing underneath, and
    /// joining from there put the first call out of sight for good. The call
    /// stays, and says how to get to the link; there is one call at a time,
    /// and leaving it is the person's decision rather than a link's. A join
    /// still out is called off instead: the link is where they now mean to
    /// go.
    public func open(_ url: URL) async {
        #if DEBUG
        linksOpened += 1
        #endif
        if let call {
            call.announce(CallToast(t("Leave this call to open that link."), lasts: NoticeTimes.atMost))
            return
        }
        // Between the two rooms of a move there is no call to say it in, and
        // the move is that call coming back.
        guard movingTo == nil else { return }
        cancelJoin()
        guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return }
        let fragment = components.fragment ?? ""
        let linkRoom = fragment.hasPrefix("/") ? String(fragment.dropFirst()) : fragment
        let token = components.queryItems?.first { $0.name == "invite" }?.value
        let meeting = components.queryItems?.first { $0.name == "meeting" }?.value
        components.query = nil
        components.fragment = nil
        components.path = ""
        if components.scheme == "tomoshibi" { components.scheme = "https" }
        // A link opened at launch can arrive before the deployment has
        // answered; it is this deployment's or nobody's.
        if deployment == nil { await connect() }
        guard let deployment else { return }
        // A link to another server is refused rather than followed: following
        // one took the app there, and the app is for this deployment alone.
        // Compared without the trailing slash the deployment's address always
        // has and a link's origin never does.
        let here = deployment.baseUrl().trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard components.url?.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")) == here else {
            tell(t("That link is for another server."))
            return
        }
        if let meeting {
            if !linkRoom.isEmpty { room = linkRoom }
            arrivedByLink = true
            wait(for: meeting, on: deployment)
            return
        }
        if let token {
            let said = try? await deployment.inviteInfo(token: token)
            // An invitation that is no good is still somebody sent here on
            // purpose, and they are told which of three things it is, because
            // they are three different next moves (App.tsx): come back later,
            // the meeting is over, or ask for another link. Thrown away, the
            // link opened a join screen that could only be refused.
            if let error = said?.error ?? (said == nil ? "no_such_invite" : nil) {
                invite = nil
                invitation = nil
                if error == "invite_not_yet", let from = said?.from, let at = Self.instant(from) {
                    tell(t("That invitation opens {when}. Come back then.", ["when": LobbyModel.said(at)]))
                } else if error == "invite_expired" {
                    tell(t("That meeting has ended."))
                } else if error == "rate_limited" {
                    tell(t("Too many attempts. Try again in a moment."))
                } else {
                    tell(t("That invitation is no longer good. Ask for another."))
                }
                arrivedByLink = false
                screen = .lobby
                return
            }
            invite = token
            invitation = said
            room = said?.room ?? linkRoom
        } else if !linkRoom.isEmpty {
            // A link with a room and nothing else is a room name, and only
            // somebody signed in is taken anywhere by one of those.
            guard account != nil else {
                tell(t("{room} is by invitation. Ask the organiser for a link.", ["room": linkRoom]))
                arrivedByLink = false
                screen = .lobby
                return
            }
            room = linkRoom
        }
        if !room.isEmpty {
            arrivedByLink = true
            screen = .prejoin
        }
    }

    /// The name an invitation was made for, which is the name the guest will
    /// wear whatever is typed: the server signs it into the token
    /// (PreJoin.tsx `locked`).
    public var invitedName: String? {
        guard let name = invitation?.name, !name.isEmpty else { return nil }
        return name
    }

    /// Somebody who arrived on an invitation link, who has a name to choose and
    /// nothing else to answer (PreJoin.tsx `guest`).
    public var arrivedInvited: Bool { invitation != nil }

    /// The name this person will be called in the room: the account's, the
    /// invitation's, or what they typed, in that order.
    public var joiningName: String {
        account?.name ?? invitedName ?? name.trimmingCharacters(in: .whitespaces)
    }

    // MARK: Account

    public func signIn(name: String, passphrase: String) async {
        guard let deployment else { return }
        asking += 1
        defer { asking -= 1 }
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
        // A rehearsal never wrote the session it signs out of, and a test
        // that signed out deleted the one this machine had kept.
        if !rehearsal { Keychain.write(cookieKey, nil) }
        account = nil
    }

    /// Account.tsx `Picture`: the picture as a data URL the account page would
    /// send, already scaled; the sentence to say if it did not take.
    public func setPicture(_ dataURL: String) async -> String? {
        guard let deployment else { return nil }
        do {
            account = try await deployment.setAvatar(image: dataURL)
            pictureVersion += 1
            return nil
        } catch {
            return t("That picture could not be used.")
        }
    }

    public func removePicture() async -> String? {
        guard let deployment else { return nil }
        do {
            account = try await deployment.removeAvatar()
            pictureVersion += 1
            return nil
        } catch {
            return t("That picture could not be used.")
        }
    }

    /// Account.tsx `Passphrase`, with its `explain`.
    public func changePassphrase(current: String, next: String) async -> String? {
        guard let deployment else { return nil }
        do {
            account = try await deployment.changePassphrase(current: current, next: next)
            return nil
        } catch let ProtocolError.Refused(_, code) {
            switch code {
            case "not_yours": return t("That is not your current passphrase.")
            case "passphrase_too_short": return t("At least eight characters.")
            case "passphrase_unchanged": return t("That is the passphrase you already have.")
            case "passphrase_in_use": return t("Somebody else already uses that passphrase. Choose another.")
            default: return t("That could not be changed. Try again.")
            }
        } catch {
            return t("That could not be changed. Try again.")
        }
    }

    /// Account.tsx `GuestLinks`: a standing link into a scope's room.
    public func makeGuestLink(room: String, name: String, from: Date?, until: Date?) async -> Result<MadeInvite, GuestLinkRefusal> {
        guard let deployment else { return .failure(GuestLinkRefusal(t("That link could not be made. Try again."))) }
        do {
            let made = try await deployment.createStandingInvite(
                room: room, name: name.trimmingCharacters(in: .whitespaces),
                fromMs: from.map { Int64($0.timeIntervalSince1970 * 1000) },
                untilMs: until.map { Int64($0.timeIntervalSince1970 * 1000) })
            return .success(made)
        } catch {
            return .failure(GuestLinkRefusal(Self.explainLink(error)))
        }
    }

    /// Stops every link into a room, returning how many stopped.
    public func stopGuestLinks(room: String) async -> Result<UInt32, GuestLinkRefusal> {
        guard let deployment else { return .failure(GuestLinkRefusal(t("That link could not be made. Try again."))) }
        do {
            return .success(try await deployment.revokeInvites(room: room, bearer: nil))
        } catch {
            return .failure(GuestLinkRefusal(Self.explainLink(error)))
        }
    }

    /// Account.tsx `explainLink`.
    static func explainLink(_ error: Error) -> String {
        guard case let ProtocolError.Refused(_, code) = error else { return t("That link could not be made. Try again.") }
        switch code {
        case "not_yours": return t("You can make links only into rooms of your own scopes.")
        case "name_too_long": return t("That name is too long.")
        case "bad_time": return t("That window cannot be used. It has to end after it starts, and after now.")
        case "invalid_room": return t("Room names can only use lowercase letters, numbers and dashes.")
        case "rate_limited": return t("Too many attempts. Try again in a moment.")
        case "not_allowed": return t("Guest links have not been opened to you.")
        default: return t("That link could not be made. Try again.")
        }
    }

    /// Whether the account holds a permission, by what the server said; a
    /// server older than permissions said nothing, which reads as the
    /// defaults (manage/grants.ts `DEFAULTS`: everything but Emby).
    public func holds(_ permission: String) -> Bool {
        guard let account else { return false }
        if let can = account.can { return can.contains(permission) }
        return permission != "emby"
    }

    // MARK: Joining

    /// Only somebody signed in, or holding an invitation, is taken to a room
    /// by its name: the app is a way in for the people who belong to the
    /// deployment and for the people they send links to, and a name typed by
    /// somebody nobody knows is neither (the owner's rule, 2026-10-15).
    public func prepare(room: String) {
        // Never over a call that is live (`open`).
        guard call == nil, movingTo == nil else { return }
        guard account != nil || invite != nil || rehearsal else {
            tell(t("Rooms here are by invitation. Open the link you were sent, or sign in."))
            return
        }
        self.room = room.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        problem = nil
        arrivedByLink = false
        screen = .prejoin
    }

    /// Straight into a room by name from the start and join sheet, which has
    /// already shown the preview and taken the choices the join screen would.
    public func enter(room: String) async {
        guard account != nil || invite != nil || rehearsal else {
            tell(t("Rooms here are by invitation. Open the link you were sent, or sign in."))
            return
        }
        self.room = room.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        problem = nil
        arrivedByLink = false
        await join()
    }

    /// A join is out and has not yet landed in its call, or was called off
    /// and has not yet come back.
    ///
    /// One at a time. "Join again" pressed twice ran two joins as the same
    /// participant; the server put out whichever arrived first, and the
    /// ended screen it then raised could end the one that survived. A join
    /// called off stays out until it returns, rather than letting the next
    /// one go beside it: two connections under one identity, the later
    /// replacing the earlier, would put the wanted call out in favour of the
    /// one that was called off.
    public private(set) var joining = false

    /// Moved on by `cancelJoin`; a join compares the value it started with
    /// after every wait, and one that no longer stands lands nowhere.
    @ObservationIgnored private var joinAttempt = 0

    /// Calls off a join that has not yet landed: Back on the join screen,
    /// Cancel on the start and join sheet, leaving the door or the wait.
    /// Those buttons stayed live and did nothing to the join, and somebody who
    /// pressed Cancel was put in the call a moment later with the microphone
    /// on. Nothing once the call is showing: that is left with Leave.
    public func cancelJoin() {
        guard joining else { return }
        joinAttempt += 1
    }

    public func join() async {
        await join(relay: nil)
    }

    /// `relay` is the relay to ask for on this join alone, in place of the
    /// one chosen; a move sends "" (App.tsx ~620).
    private func join(relay: String?) async {
        guard let deployment, !joining else { return }
        joining = true
        asking += 1
        let attempt = joinAttempt
        var landed = false
        defer {
            if !landed { joining = false }
            asking -= 1
        }
        // A join while a call is live replaces it; the call it replaces is
        // left first, rather than going on publishing out of sight.
        if let old = call {
            call = nil
            await old.leave()
        }
        deviceMemory.remember(DeviceMemory.Joining(camera: startWithCamera, microphone: startWithMicrophone))
        do {
            // Signed in, the web sends the account's name and no passphrase:
            // the session is the proof (api.ts). A guest on an invitation with
            // a name takes that name. A guest of a room held under a scope is
            // minted a mark of their own and a passphrase is ignored, so none
            // is sent and the one kept here is left alone (PreJoin.tsx
            // `scopedGuest`).
            let signed = account != nil
            let scopedGuest = arrivedInvited && !scopeOf(name: room).isEmpty
            let grant = try await deployment.join(
                room: room,
                name: joiningName,
                passphrase: signed || scopedGuest ? "" : passphrase,
                identity: rehearsal ? rehearsalIdentity : UserDefaults.standard.string(forKey: identityKey),
                relay: relay ?? relayChoice ?? "",
                invite: invite)
            #if DEBUG
            lastJoinRelay = relay ?? relayChoice ?? ""
            joinsAnswered += 1
            #endif
            // Every join's identity is kept: signing in or out makes the server
            // issue a new one, and a stale one sent back would be refused.
            if rehearsal { rehearsalIdentity = grant.identity } else { UserDefaults.standard.set(grant.identity, forKey: identityKey) }
            guard attempt == joinAttempt else { return }
            let options = CallOptions(
                url: grant.url, token: grant.token, room: grant.room,
                sealingWord: sealingWord.isEmpty ? nil : sealingWord,
                forward: grant.forward.map { IceServer(url: $0.url, username: $0.username, credential: $0.credential) })
            let model = try await CallModel.join(options, deployment: deployment, region: grant.region)
            #if DEBUG
            // TOMOSHIBI_DEBUG_JOIN_SLOW=<ms>: a room that takes that much
            // longer to enter, for a test to call the join off while it is.
            if rehearsal, let slow = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_JOIN_SLOW"].flatMap(Int.init) {
                try? await Task.sleep(for: .milliseconds(slow))
            }
            #endif
            // Called off while the room was being entered: the room was
            // entered all the same, and is left at once.
            guard attempt == joinAttempt else {
                await model.leave()
                return
            }
            model.relay = grant.relay
            model.holding = grant.holding
            // Where the choices are kept, before anything is chosen: the
            // original sound below remembers itself, and remembered it into
            // the owner's settings from a rehearsal that keeps nothing,
            // because the call's memory was still the standard one.
            model.memory = deviceMemory
            // Before the microphone is published, so it goes out the way it
            // was asked for the first time rather than being republished.
            if originalSoundWanted { try? await model.setOriginalSound(true) }
            guard attempt == joinAttempt else {
                await model.leave()
                return
            }
            model.onMoved = { [weak self, weak model] relay in
                guard let model else { return }
                Task { await self?.rejoin(from: model, to: relay) }
            }
            call = model
            landed = true
            joining = false
            room = grant.room
            lobby.remember(room: grant.room)
            // The invitation is kept for as long as this call is: a move to
            // another relay and "Join again" both join once more, and an
            // invitation spent on the first join left a guest refused at the
            // door of the room they had just been in.
            problem = nil
            screen = .call
            // After joining rather than before, so somebody is in the room the
            // moment they join and their devices come up a beat later, and each
            // on its own, never through the catch below (App.tsx `onJoin`): a
            // refused microphone or a camera another app holds is one person's
            // problem with their own machine, not a room that would not open.
            // A device that will not start says so in the call (deviceFailed)
            // and the call goes on with that button off. Each only while this
            // is still the call: one left, or moved, while the microphone was
            // coming up was sent a camera afterwards all the same.
            model.restoreDevices()
            if startWithMicrophone, call === model {
                await switchMicrophone(on: true)
            }
            if startWithCamera, call === model {
                do { try await model.setCamera(true) } catch {}
            }
        } catch let error as ProtocolError {
            // Why they are not in the meeting, which stays until read; a
            // join that was called off has nothing to say.
            guard attempt == joinAttempt else { return }
            tell(explain(error))
        } catch {
            guard attempt == joinAttempt else { return }
            tell(t("Could not join {room}.", ["room": room]))
        }
    }

    #if DEBUG
    /// The relay the last join asked for, for a test of the move, and how
    /// many joins the deployment has answered, for a test that a press made
    /// twice asks once.
    @ObservationIgnored private(set) var lastJoinRelay: String?
    @ObservationIgnored private(set) var joinsAnswered = 0
    /// How many links the app was handed, for a test that a universal link
    /// arrives once and not twice (SwiftUI could hand one to `onOpenURL` and
    /// to a user-activity handler both).
    @ObservationIgnored private(set) var linksOpened = 0
    #endif

    /// The server moved the call to another relay; the web rejoins at once
    /// with the same passphrase and word, and lets the server choose.
    ///
    /// For this join only. The choice was set to nothing here, and once the
    /// choice was kept between launches that meant a move forgot for good the
    /// machine somebody had picked; the web asks for "" on the rejoin and
    /// says outright that a machine chosen by hand stays chosen (App.tsx
    /// ~620 and ~676).
    ///
    /// Only for the call that is showing: a call already replaced or left
    /// that is told of a move afterwards has nothing to come back to, and
    /// answering it left the current call to rejoin the old one's room.
    private func rejoin(from moved: CallModel, to relay: String) async {
        guard call === moved else { return }
        movingTo = relay
        defer { movingTo = nil }
        call = nil
        await moved.leave()
        await join(relay: "")
        // A join that failed left the screen on the call with no call in it,
        // which drew nothing at all: a black window. Back to the join screen,
        // with the reason, and the Join button that tries again.
        if call == nil, screen == .call { screen = .prejoin }
    }

    // MARK: The switches outside the call's own bar

    /// The microphone being switched by the join or by the menu's ⌘D, while
    /// it is: the menu item waits. The join's microphone came up without the
    /// bar's knowing, and ⌘D pressed while it (or the first-run permission
    /// prompt) was pending asked for the microphone a second time; the call
    /// publishes a track whenever it holds none yet, so that could be a
    /// second microphone, and in a test it left the switch saying off over a
    /// microphone that was sending.
    public private(set) var switchingMicrophone = false

    /// What ⌘D does.
    public func toggleMicrophone() async {
        guard let call, !switchingMicrophone else { return }
        await switchMicrophone(on: !call.microphoneOn)
    }

    private func switchMicrophone(on: Bool) async {
        guard let call, !switchingMicrophone else { return }
        switchingMicrophone = true
        defer { switchingMicrophone = false }
        // A refusal says so itself, in the call (CallModel `deviceFailed`).
        try? await call.setMicrophone(on)
    }

    /// Where a move is taking the meeting while there is no call to show it
    /// over: between leaving the old room and entering the new one the room
    /// view is gone, and the screen was blank for those seconds. The web keeps
    /// its moving notice above every page for the same reason (App.tsx).
    public private(set) var movingTo: String?

    public func leave() async {
        await end(.left)
    }

    /// The call ended without anybody here pressing leave: the reason is the
    /// core's, the media server's disconnect reason by name.
    ///
    /// `model` is the call that ended, which ends the screen only while it is
    /// the call showing. One that has already been replaced (put out as a
    /// duplicate by the join that replaced it) ended the call that replaced
    /// it, which was the one still working.
    public func ended(because reason: String, in model: CallModel? = nil) async {
        if let model, model !== call { return }
        let ending: Ending = if reason.contains("RoomDeleted") || reason.contains("RoomClosed") {
            .closed
        } else if reason.contains("ParticipantRemoved") {
            .removed
        } else if reason.contains("ClientInitiated") {
            .left
        } else {
            .lost
        }
        await end(ending)
    }

    private func end(_ how: Ending) async {
        ending = how
        await call?.leave()
        call = nil
        screen = .ended
        // A relay that has just dropped a call is the one thing known about
        // the fleet; without forgetting the measurement it would go on being
        // named the fastest for minutes (App.tsx, the lost connection).
        forgetRelayMeasurement()
    }

    public func backToLobby() {
        guard call == nil else { return }
        cancelJoin()
        screen = .lobby
        problem = nil
        invite = nil
        invitation = nil
        arrivedByLink = false
        stopWaitingTasks()
        arrangement = nil
    }

    // MARK: An arranged meeting

    /// Waiting.tsx, which is a join screen whose button waits.
    ///
    /// Nothing is asked of the meeting but what it is until somebody presses:
    /// the web's first draft joined the moment the host arrived, and somebody
    /// still typing their name was joined with none, and somebody arriving
    /// late was put in the call before they had looked at their own camera.
    /// This used to do the same, polling from the moment the link opened.
    public struct Waiting: Equatable, Sendable {
        public enum Gone: Equatable, Sendable { case cancelled, ended, notComing }
        /// The button has been pressed and the door is being asked.
        public var asking = false
        public var gone: Gone?
        /// The last read failed; said, and tried again.
        public var unreachable = false
    }

    private func wait(for token: String, on deployment: Deployment) {
        stopWaitingTasks()
        meetingToken = token
        arrangement = nil
        waiting = Waiting()
        problem = nil
        screen = .waiting
        // Read on arrival so the screen can say what it is for, and again,
        // slowly, until it can (Waiting.tsx `RETRY_EVERY`).
        waitTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                if await self.look() != nil {
                    // TOMOSHIBI_DEBUG_WAIT=press: the button pressed once the
                    // meeting is known, for a test of the whole wait.
                    if self.rehearsal, ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_WAIT"] == "press" {
                        Task { await self.goArranged() }
                    }
                    return
                }
                if self.waiting.gone != nil { return }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    /// Reads the meeting once.
    private func look() async -> Arrangement? {
        guard let deployment, let meetingToken else { return nil }
        do {
            let said = try await deployment.arranged(token: meetingToken)
            arrangement = said
            room = said.room
            waiting.unreachable = false
            if said.ended { waiting.gone = .ended }
            return said
        } catch let ProtocolError.Refused(_, code) where code == "no_such_meeting" {
            waiting.gone = .cancelled
            return nil
        } catch {
            waiting.unreachable = true
            return nil
        }
    }

    /// Whether the meeting has begun and carries the way in, so the button is
    /// the ordinary Join.
    public var arrangedReady: Bool {
        guard let arrangement else { return false }
        return arrangement.started && arrangement.invite != nil
    }

    /// When the host may begin it: the server begins a meeting only from
    /// `from`, and a Start pressed earlier joins and begins nothing.
    public var arrangedOpens: Date? {
        guard let arrangement else { return nil }
        return Self.instant(arrangement.from ?? arrangement.at)
    }

    /// The one button: join where it has begun, begin it where it is the
    /// host's, otherwise wait for the host.
    public func goArranged() async {
        guard let arrangement else { return }
        if arrangedReady, let invite = arrangement.invite {
            self.invite = invite
            await join()
            return
        }
        // The host does not wait for the host.
        if arrangement.mine {
            await join()
            return
        }
        askForArranged()
    }

    /// Asks at the web's pace until the meeting begins (then joins with the
    /// invitation it carries), ends, or the host is two hours late.
    private func askForArranged() {
        waitTask?.cancel()
        waiting.asking = true
        waiting.gone = nil
        waitTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let said = await self.look()
                if Task.isCancelled { return }
                if said?.ended == true || self.waiting.gone != nil {
                    self.waiting.asking = false
                    return
                }
                if let said, said.started, let invite = said.invite {
                    self.waiting.asking = false
                    self.invite = invite
                    await self.join()
                    return
                }
                if let said, let at = Self.instant(said.at), Date().timeIntervalSince(at) * 1000 > Double(hostNotComingAfterMs()) {
                    self.waiting.asking = false
                    self.waiting.gone = .notComing
                    return
                }
                // A read that failed is not a reason to ask faster.
                let pause = said.map { meetingAskEveryMs(at: $0.at, nowMs: Int64(Date().timeIntervalSince1970 * 1000)) } ?? 30_000
                try? await Task.sleep(for: .milliseconds(Int(pause)))
            }
        }
    }

    /// "Ask again", after the host was said not to be coming.
    public func askAgain() {
        askForArranged()
    }

    public func stopWaiting() {
        // The button that waits may already have turned into a join.
        cancelJoin()
        stopWaitingTasks()
        arrangement = nil
        meetingToken = nil
        waiting = Waiting()
        arrivedByLink = false
        screen = .lobby
    }

    private func stopWaitingTasks() {
        waitTask?.cancel()
        waitTask = nil
        waiting.asking = false
    }

    static func instant(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: text) ?? ISO8601DateFormatter().date(from: text)
    }

    // MARK: The door

    /// Asks the room's host to let this person in, then waits. Under the name
    /// the room will know them by: somebody signed in knocked as the guest
    /// name this device last typed, and was let in as somebody else.
    public func knock() async {
        guard let deployment else { return }
        let from = screen
        do {
            let ticket = try await deployment.knock(room: room, name: joiningName)
            // Somebody who went back while the knock was being sent is not
            // put at the door afterwards.
            guard screen == from else { return }
            screen = .knocking
            knockTask?.cancel()
            let giveUpAfter = knockGiveUp
            knockTask = Task { [weak self] in
                let started = Date()
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(Int(knockAskEveryMs())))
                    guard let self, !Task.isCancelled else { return }
                    // A round that did not answer is not the end of the
                    // waiting: the person inside has not gone anywhere.
                    if let answer = try? await deployment.atTheDoor(room: self.room, token: ticket.token), !Task.isCancelled {
                        switch answer.state {
                        case .admitted:
                            self.invite = answer.invite
                            await self.join()
                            // Let in and then refused at the join: the reason
                            // is on the join screen, with its button, rather
                            // than under a door nobody is answering any more.
                            if self.call == nil, self.screen == .knocking { self.screen = .prejoin }
                            return
                        case .refused:
                            self.tell(t("Nobody let you in. Ask for a link."))
                            self.screen = .prejoin
                            return
                        case .knocking:
                            break
                        }
                    }
                    if Task.isCancelled { return }
                    // knock.ts `GIVE_UP_AFTER`, counted whatever the rounds
                    // said. It was counted only on an answer of "still
                    // knocking", so a server that never answered at all,
                    // which is the one case the door cannot end, left the
                    // person waiting with a spinner for as long as the app
                    // was open (Knocking.tsx: "this one is only for a server
                    // that never answers").
                    if Date().timeIntervalSince(started) > giveUpAfter {
                        self.tell(t("Nobody let you in. Ask for a link."))
                        self.screen = .prejoin
                        return
                    }
                }
            }
        } catch {
            guard screen == from else { return }
            problem = t("That could not be sent.")
        }
    }

    /// How long the door is asked, in seconds: knock.ts `GIVE_UP_AFTER`, or
    /// what TOMOSHIBI_DEBUG_KNOCK_GIVE_UP says in a rehearsal, so a test can
    /// see the end of a wait that otherwise takes ten minutes.
    private var knockGiveUp: TimeInterval {
        #if DEBUG
        if rehearsal, let seconds = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_KNOCK_GIVE_UP"].flatMap(Double.init) {
            return seconds
        }
        #endif
        return Double(knockGiveUpAfterMs()) / 1000
    }

    public func stopKnocking() {
        knockTask?.cancel()
        knockTask = nil
        // Let in a moment ago, and already joining.
        cancelJoin()
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
            case "not_in_scope": return t("Only members of {scope} can join {room}.", ["room": room, "scope": scopeOf(name: room)])
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
            || problem == t("Only members of {scope} can join {room}.", ["room": room, "scope": scopeOf(name: room)])
    }
}

/// A guest link refused, as a sentence.
public struct GuestLinkRefusal: Error {
    public let sentence: String
    init(_ sentence: String) { self.sentence = sentence }
}
