import Foundation
import Observation
import SwiftUI
import TomoshibiCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Running a room and hearing it: the host's controls, the door, invitations,
/// every person's volume, reactions (web/src/live/host.ts, knock.ts,
/// hearing.ts, hands.ts; components/room/HostPanel.tsx, AtTheDoor.tsx,
/// SoundPanel.tsx, HandButton.tsx, Reacting.tsx, Moving.tsx).
///
/// The room view builds one of these in its initialiser, and SwiftUI may run
/// that initialiser more than once and keep only the first object. So nothing
/// happens here until `run()`, which the overlay calls from the one view that
/// is given the object SwiftUI kept: an initialiser that subscribed to the call
/// would leave a subscriber behind for every copy thrown away, each one
/// polling the door and answering every reaction twice.
@MainActor
@Observable
public final class HostFeature {
    /// Held, not borrowed: the view keeps this object for as long as it
    /// likes, through its leaving transition and through tasks a press
    /// started (ending the meeting ends the call under the task that asked),
    /// and an unowned reference to a call already let go killed the app.
    let call: CallModel

    /// What the server last said about this participant and this room
    /// (host.ts `useStanding`): nothing until it has said.
    private(set) var standing = HostStanding(yours: false, admin: false)

    /// Who is knocking, for as long as this participant can let them in.
    private(set) var waiting: [Knocker] = []
    /// The knock being answered, so its buttons do nothing twice.
    private(set) var answering: String?

    /// The link made by the last press of "Invite somebody", until it is
    /// revoked.
    private(set) var link: String?
    private(set) var copied = false

    /// The one action under way, named by what it acts on (HostPanel.tsx
    /// `busy`): an identity, or "invite", "revoke", "move", "close".
    private(set) var busy: String?

    /// The relays a meeting can be moved to, once asked for.
    private(set) var relays: [Relay]?

    /// What is said over the call about something a press did (notices.ts
    /// `actionFailed`, `actionDone`).
    private(set) var notices: [HostNotice] = []

    /// Asked once rather than done at once: ending a meeting for eleven people
    /// cannot be undone, and its button sits beside three that can, so the
    /// press that matters is the second (HostPanel.tsx `ending`).
    var ending = false
    /// Whether the list of relays to move to is open.
    var choosingRelay = false

    /// Whether the host panel is on screen: the door's notice stands down
    /// while it is, because the panel lists the same people (HostPanel.tsx
    /// `Hosting`, which raises the notice only while the panel is shut).
    var panelOpen = false
    /// Whether the sound panel is: what is said over the call moves aside.
    var soundOpen = false

    /// A move older than this is no longer shown, whatever became of it
    /// (App.tsx: the overlay's twenty-second limit). Set from the overlay.
    private(set) var movingGaveUp: Date?

    private var started = false
    /// The sounds this feature asked to stop at the server, so it only ever
    /// lets flow again what it stopped itself.
    private var stopped: Set<String> = []

    public init(call: CallModel) {
        self.call = call
    }

    var reactions: ReactionBoard { ReactionBoard.of(call.call) }

    /// Whether this room is held under a scope, where every member runs it and
    /// there is nobody to hand it to (HostPanel.tsx `scoped`).
    var scoped: Bool { !scopeOf(name: call.room).isEmpty }

    /// Where a move is taking the meeting, while that is worth showing.
    var movingTo: String? {
        guard let moving = call.moving else { return nil }
        if let gaveUp = movingGaveUp, gaveUp >= moving.at { return nil }
        return moving.relay
    }

    // MARK: Running

    /// Everything this feature does while the call is on screen: listening to
    /// the call, asking who runs the room, watching the door every four
    /// seconds while this participant may open it. Returns when the task it
    /// runs in is cancelled, which is the call view going.
    func run() async {
        begin()
        await refreshStanding()
        while !Task.isCancelled, call.endedReason == nil {
            if standing.yours {
                await lookAtTheDoor()
            } else if !waiting.isEmpty {
                waiting = []
            }
            // AtTheDoor.tsx `LOOK_EVERY`.
            try? await Task.sleep(for: .seconds(4))
        }
    }

    private func begin() {
        guard !started else { return }
        started = true
        call.listen { [weak self] event in self?.heard(event) }
        applyHearing()
        HostDebug.start(self)
    }

    private func heard(_ event: CallEvent) {
        switch event {
        case .participantLeft, .stateChanged:
            // Somebody leaving is the ordinary reason a room changes hands,
            // and a connection that came back may have missed whatever
            // happened while it was away (host.ts `useStanding`).
            Task { await refreshStanding() }
        case let .data(topic, identity, payload):
            // A handover, said by the server and only by the server: a
            // participant's packet always arrives with its sender, and this
            // one with none (host.ts `told`).
            if topic == "host", identity == nil {
                Task { await refreshStanding() }
            }
            if let identity, let what = receivedReaction(topic: topic, from: identity, payload: payload) {
                reactions.show(what, from: identity)
            }
        case .participantJoined, .trackPublished, .trackSubscribed:
            // A sound that has just arrived was given a player at full volume
            // and is flowing; whatever was decided about it is put back
            // (Audible.tsx `HEARING_EVENTS`).
            applyHearing()
        case let .trackUnpublished(_, sid):
            stopped.remove(sid)
        default:
            break
        }
    }

    private var bearer: String { call.call.token() }

    func refreshStanding() async {
        guard let said = try? await call.deployment.hostStanding(room: call.room, bearer: bearer) else { return }
        let became = said.yours && !standing.yours
        standing = said
        if !said.yours {
            waiting = []
            link = nil
            copied = false
        }
        // Looked at once at once rather than at the next round, so a person
        // handed a room with somebody already outside sees them now.
        if became { await lookAtTheDoor() }
    }

    // MARK: The door

    private func lookAtTheDoor() async {
        guard standing.yours else { return }
        // A round that did not answer is not an empty door (AtTheDoor.tsx).
        guard let said = try? await call.deployment.knocking(room: call.room, bearer: bearer) else { return }
        if said != waiting { waiting = said }
    }

    func answer(_ knocker: Knocker, admit: Bool) async {
        guard answering == nil else { return }
        answering = knocker.id
        defer { answering = nil }
        do {
            _ = try await call.deployment.answerKnock(room: call.room, id: knocker.id, bearer: bearer, admit: admit)
            // Taken off the list now rather than at the next round, so a press
            // does something the moment it is pressed.
            waiting.removeAll { $0.id == knocker.id }
        } catch {
            await lookAtTheDoor()
        }
    }

    // MARK: The host's actions

    /// One action at a time, and what went wrong said in words chosen for its
    /// code (HostPanel.tsx `act`).
    private func act(_ who: String, _ work: () async throws -> Void) async {
        guard busy == nil else { return }
        busy = who
        defer { busy = nil }
        do {
            try await work()
        } catch {
            say(Self.explain(error), danger: true)
        }
    }

    func invite() async {
        await act("invite") {
            let made = try await call.deployment.createInvite(room: call.room, bearer: bearer)
            let address = call.deployment.inviteLink(token: made.token, room: call.room)
            link = address
            // Copied as well as shown: whoever pressed this is about to paste
            // it somewhere.
            copied = Self.copy(address)
        }
    }

    func revoke() async {
        await act("revoke") {
            let gone = try await call.deployment.revokeInvites(room: call.room, bearer: bearer)
            link = nil
            copied = false
            // Said, because "nothing happened" and "three links stopped
            // working" look the same otherwise.
            say(gone == 1 ? t("One link stopped working.") : t("{count} links stopped working.", ["count": String(gone)]))
        }
    }

    /// Quietens somebody's microphone from the server, not by asking them.
    func silence(_ person: Person) async {
        guard let microphone = person.track(.microphone) else { return }
        await act(person.id) {
            try await call.deployment.silence(room: call.room, bearer: bearer, identity: person.id, track: microphone.sid)
        }
    }

    func turnOut(_ person: Person) async {
        await act(person.id) {
            try await call.deployment.turnOut(room: call.room, bearer: bearer, identity: person.id)
        }
    }

    func handOver(to person: Person) async {
        await act(person.id) {
            _ = try await call.deployment.handOver(room: call.room, bearer: bearer, to: person.id)
        }
        // The server says so to the room, which asks again; asked here as
        // well, so the panel does not wait on a packet to stop offering
        // controls that now answer "not yours".
        await refreshStanding()
    }

    func move(to relay: Relay) async {
        await act("move") {
            _ = try await call.deployment.moveRoom(room: call.room, bearer: bearer, relay: relay.name)
        }
    }

    func dissolve() async {
        await act("close") {
            try await call.deployment.dissolve(room: call.room, bearer: bearer)
        }
    }

    /// The relays on offer, asked once, when the panel that offers them is
    /// first opened (HostPanel.tsx `Elsewhere`).
    func loadRelays() async {
        guard relays == nil else { return }
        relays = (try? await call.deployment.relays())?.relays ?? []
    }

    /// Where the meeting can be put: neither a machine under maintenance nor,
    /// for anybody but an administrator, one held in reserve. The server
    /// refuses those regardless; this is about not showing a door that will
    /// not open.
    var usableRelays: [Relay] {
        (relays ?? []).filter { !$0.maintenance && (standing.admin || !$0.adminOnly) }
    }

    /// Whether somebody joined without a passphrase, so that the room cannot
    /// be handed to them (HostPanel.tsx `HandOver`, by the signature's prefix).
    func isGuest(_ person: Person) -> Bool {
        signatureOf(identity: person.id)?.proven == false
    }

    func refuseHandOver() {
        say(t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one."), danger: true)
    }

    // MARK: Hearing

    func setting(_ person: Person, _ sound: Sound) -> HearingSetting {
        Hearing.shared.setting(person.id, sound)
    }

    func setVolume(_ person: Person, _ sound: Sound, _ volume: Double) {
        var setting = Hearing.shared.setting(person.id, sound)
        setting.volume = volume
        Hearing.shared.set(person.id, sound, setting)
        applyHearing()
    }

    func setBlocked(_ person: Person, _ sound: Sound, _ blocked: Bool) {
        var setting = Hearing.shared.setting(person.id, sound)
        setting.blocked = blocked
        Hearing.shared.set(person.id, sound, setting)
        applyHearing()
    }

    /// The track each sound is carried on (hearing.ts `SOURCE`).
    static func track(of person: Person, _ sound: Sound) -> TrackInfo? {
        switch sound {
        case .voice: person.track(.microphone)
        case .screen: person.track(.screenShareAudio)
        case .music: person.musicTrack
        }
    }

    /// Puts every setting back on the call: all of them, every time, rather
    /// than working out which one an event was about (Audible.tsx `apply`).
    ///
    /// A block is asked of the media server, so nothing is downloaded. The
    /// core can only ask that of a track it is subscribed to, and it
    /// subscribes to every sound the moment it is published and plays it at
    /// once; so a blocked sound is also turned to nothing here, which covers
    /// the moment between its first packets and the server hearing the block.
    /// Only a sound this stopped is ever let flow again, so a track something
    /// else held back stays held back.
    func applyHearing() {
        for person in call.people {
            for sound in Sound.allCases {
                guard let track = Self.track(of: person, sound) else { continue }
                let setting = Hearing.shared.setting(person.id, sound)
                call.audio.setVolume(sid: track.sid, Float(setting.blocked ? 0 : setting.volume))
                if setting.blocked {
                    // Repeating it costs nothing: the media library sends the
                    // server an update only when the answer changes. Noted
                    // before it is asked, not after it succeeded: the library
                    // switches the track off here before it tells the server,
                    // so a request that then fails has still stopped the sound
                    // on this side, and has to be undone when it is let back.
                    stopped.insert(track.sid)
                    try? call.call.setRemoteEnabled(sid: track.sid, enabled: false)
                } else if stopped.remove(track.sid) != nil {
                    try? call.call.setRemoteEnabled(sid: track.sid, enabled: true)
                }
            }
        }
    }

    // MARK: Notices

    func say(_ text: String, danger: Bool = false) {
        let notice = HostNotice(text: text, danger: danger)
        // Three fit above the island; the fourth pushes the oldest out
        // (Notices.tsx `visibleToasts`).
        notices.append(notice)
        if notices.count > 3 { notices.removeFirst(notices.count - 3) }
        Task { [weak self] in
            // notices.ts `AT_MOST`: what a press did is over, so it goes.
            try? await Task.sleep(for: .seconds(10))
            self?.notices.removeAll { $0.id == notice.id }
        }
    }

    func giveUpMoving(at: Date) {
        movingGaveUp = at
    }

    /// The server sends a code; this owns the sentence (HostPanel.tsx
    /// `explain`), each written out because each wants a different next move.
    static func explain(_ error: Error) -> String {
        guard case let .Refused(_, code)? = error as? ProtocolError else {
            return t("That could not be done. Try again.")
        }
        switch code {
        case "media_unreachable": return t("That server did not answer. The meeting has not been moved.")
        case "no_such_relay": return t("That server is no longer on this deployment.")
        case "relay_not_allowed": return t("That server is reserved, and this meeting may not be put on it.")
        case "no_such_person": return t("They have already left.")
        case "not_proven": return t("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one.")
        case "not_yours": return t("You no longer run this room.")
        case "scoped_room": return t("Every member of this room's scope runs it, so there is nobody to hand it to.")
        case "rate_limited": return t("Too many attempts. Try again in a moment.")
        default: return t("That could not be done. Try again.")
        }
    }

    /// Puts the link on the pasteboard. TOMOSHIBI_DEBUG_PASTEBOARD names a
    /// private pasteboard to use instead, so a test run on somebody's own Mac
    /// does not replace whatever they had copied.
    private static func copy(_ text: String) -> Bool {
        #if os(macOS)
        let board: NSPasteboard
        if let name = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PASTEBOARD"], !name.isEmpty {
            board = NSPasteboard(name: NSPasteboard.Name(name))
        } else {
            board = NSPasteboard.general
        }
        board.clearContents()
        return board.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        return true
        #endif
    }
}

/// Something said over the call about a press: an error, with a red edge, or
/// something that worked where working is not visible on its own.
struct HostNotice: Identifiable, Equatable {
    let id = UUID()
    let text: String
    let danger: Bool
}

/// The sound panel's record for this app, kept where the web keeps it and
/// under the web's name (hearing.ts). One for the whole app rather than one
/// per call, as the web's is one per page: a call that is moved, or comes
/// back after a drop, is joined again as the same identity, and its settings
/// should be waiting for it.
@MainActor
@Observable
final class Hearing {
    static let shared = Hearing()

    private var book: HearingBook
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        book = HearingBook(stored: defaults.string(forKey: HearingBook.storageKey))
    }

    /// Filed under the signature for a proven identity, which is the same on
    /// every visit, and under the identity itself otherwise, for the call
    /// only (hearing.ts `filing`).
    static func filing(_ identity: String) -> HearingFiling {
        if let signature = signatureOf(identity: identity), signature.proven {
            return HearingFiling(key: signature.trip, keeps: true)
        }
        return HearingFiling(key: identity, keeps: false)
    }

    func setting(_ identity: String, _ sound: Sound) -> HearingSetting {
        book.setting(Self.filing(identity), sound)
    }

    func set(_ identity: String, _ sound: Sound, _ setting: HearingSetting) {
        if book.set(Self.filing(identity), sound, setting) {
            if let stored = book.stored {
                defaults.set(stored, forKey: HearingBook.storageKey)
            } else {
                defaults.removeObject(forKey: HearingBook.storageKey)
            }
        }
    }
}
