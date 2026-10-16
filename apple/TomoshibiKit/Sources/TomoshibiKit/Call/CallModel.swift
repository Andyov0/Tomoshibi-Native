import AVFoundation
import Foundation
import Observation
import SwiftUI
import TomoshibiCore

/// One person in the call, as the screens see them.
public struct Person: Identifiable, Equatable, Sendable {
    public let id: String
    public var name: String
    public var attributes: [String: String]
    public var tracks: [String: TrackInfo]
    public var quality: LinkQuality = .good
    public var speaking = false
    /// Whether their sealed frames can be read; nil until the call says.
    public var readable: Bool?

    public var handUp: Bool { attributes["hand"] == "up" }

    public func track(_ source: MediaSource) -> TrackInfo? {
        tracks.values.first { $0.source == source }
    }

    public var musicTrack: TrackInfo? {
        tracks.values.first { $0.source == .unknown && $0.name.hasPrefix("music") }
    }
}

/// The unit of layout: a picture, not a person (the web's `Surface`). Someone
/// sharing their screen with their camera on is two of these.
public struct Surface: Identifiable, Equatable, Sendable {
    public enum Kind: Sendable { case camera, screen }
    public let id: String
    public let person: Person
    public let kind: Kind
    /// The picture's track, when there is one and it is not muted.
    public let sid: String?
}

/// One thing said in the corner of a call: a notice (web/src/live/notices.ts).
///
/// The web's rule is about lifetime rather than severity. Something that
/// happened is said here and fades; something still true is a banner over the
/// stage (`CallModel.notice`, "Reconnecting"). A notice about something over
/// carries no way to close it, because a target that lives two seconds is one
/// nobody hits; one somebody may want to deal with now, or not at all, has
/// the close button (`closable`).
public struct CallToast: Identifiable, Equatable, Sendable {
    /// What it is about, said by its edge (Notices.tsx: "the edge is the whole
    /// of the severity system"): plain, the signal colour for somebody else
    /// taking the stage (`is-signal`) or anything else happening now, red for
    /// a failure.
    public enum Kind: Sendable { case info, signal, warning, failure }

    /// Kept when it is said again under the same key, so the card stays where
    /// it is and only its words change.
    public internal(set) var id = UUID()
    public var title: String
    public var detail: String?
    public var kind: Kind
    /// Seconds before it goes by itself, or nil to stay until dismissed.
    public var lasts: Double?
    /// The words on its button, when it has one.
    public var actionTitle: String?
    /// A second toast with the same key replaces the first, in place: one
    /// "Reconnecting" rather than three.
    public var key: String?
    /// Whether it carries a close button (notices.ts `closeButton`). Unless
    /// told, only one that would otherwise never go.
    public var closable: Bool
    /// Something drawn under its words, by the name whoever raised it gave
    /// `CallModel.attach`: the people at the door, with their two answers.
    public var attachment: String?

    public init(_ title: String, detail: String? = nil, kind: Kind = .info, lasts: Double? = NoticeTimes.briefly,
                actionTitle: String? = nil, key: String? = nil, closable: Bool? = nil, attachment: String? = nil) {
        self.title = title
        self.detail = detail
        self.kind = kind
        self.lasts = lasts
        self.actionTitle = actionTitle
        self.key = key
        self.closable = closable ?? (lasts == nil)
        self.attachment = attachment
    }
}

public struct ChatLine: Identifiable, Equatable, Sendable {
    public let id: String
    public let identity: String
    public let name: String
    public let text: String
    public let at: Date
    public let mine: Bool
}

/// A call and everything the screens draw from it.
///
/// The core speaks on its own threads; every event is carried to the main
/// thread in the order it was said, applied here, and SwiftUI redraws from
/// this object. Remote sound is played the moment it is subscribed, because a
/// participant nobody can hear is a fault nobody can see.
@MainActor
@Observable
public final class CallModel {
    public let call: Call
    public let room: String
    public let localIdentity: String
    public let localName: String
    /// The deployment the call was joined through: host controls, watching
    /// together and the song desk all speak to it.
    public let deployment: Deployment
    /// The join answer's region for the relay this call came in through, when
    /// the server said: a mainland one sends YouTube through the relay
    /// (relays.ts `mainland`, WatchScreen.tsx ~240).
    public let region: String?
    /// Whether the call is sealed; watching together, the song desk's
    /// broadcast and the lossless stream switch themselves off in one.
    public let sealed: Bool

    public private(set) var state: CallState = .connected
    public private(set) var people: [Person] = []
    public private(set) var messages: [ChatLine] = []
    public private(set) var endedReason: String?
    public private(set) var localTracks: [String: TrackInfo] = [:]

    /// What others said in the last six seconds, for the corner while the
    /// messages are closed (live/chat.ts SAID_FOR, components/room/Said.tsx).
    public private(set) var arriving: [ChatLine] = []
    /// Something was said while the messages were closed (useChat.ts).
    public private(set) var unread = false
    /// Whether the messages panel is open; set by the room view.
    public var chatOpen = false {
        didSet {
            if chatOpen {
                unread = false
                arriving.removeAll()
            }
        }
    }
    static let saidFor: Duration = .seconds(6)

    /// Something still true, said as a line across the top of the call for as
    /// long as it is (the web's banners: Notices.tsx). What happened and is
    /// over is a toast instead, in the corner.
    public var notice: String?

    /// What the call says in the corner, oldest first (the web's toasts,
    /// notices.ts): somebody arriving, a device that would not start, a song
    /// that could not be played. At most three at a time.
    public private(set) var toasts: [CallToast] = []
    /// Whether the corner is being read, under the pointer or a finger: every
    /// countdown waits until it is not (sonner pauses its timers while the
    /// stack is expanded).
    public private(set) var toastsHeld = false
    @ObservationIgnored private var toastActions: [UUID: @MainActor () -> Void] = [:]
    @ObservationIgnored private var toastClocks: [UUID: NoticeClock] = [:]
    @ObservationIgnored private var toastTimers: [UUID: Task<Void, Never>] = [:]
    /// What a toast's `attachment` names, drawn under its words.
    @ObservationIgnored private(set) var attachments: [String: @MainActor () -> AnyView] = [:]
    /// Where the corner's messages are drawn, in the window, while there are
    /// any: the notices rise above them where the two would meet (CornerLanes).
    var saidFrame: CGRect?

    /// Says `toast`; one with an action calls it when its button is pressed.
    public func announce(_ toast: CallToast, action: (@MainActor () -> Void)? = nil) {
        var toast = toast
        if let key = toast.key, let index = toasts.firstIndex(where: { $0.key == key }) {
            toast.id = toasts[index].id
            stopClock(toast.id)
            toasts[index] = toast
        } else {
            toasts.append(toast)
            // The fourth pushes out the oldest one that goes by itself; one
            // somebody has to act on is never what is lost.
            for index in NoticeTimes.overflow(toasts.map { $0.lasts == nil }).reversed() {
                forget(toasts[index].id)
                toasts.remove(at: index)
            }
        }
        toastActions[toast.id] = action
        if let lasts = toast.lasts {
            toastClocks[toast.id] = NoticeClock(lasts: lasts, now: Self.now(), running: !toastsHeld)
            runClock(toast.id)
        }
        #if DEBUG
        CallModel.noticeLog(toast)
        #endif
    }

    public func dismiss(_ id: UUID) {
        forget(id)
        toasts.removeAll { $0.id == id }
    }

    /// Takes down whatever was said under `key`, if anything still is.
    public func dismiss(key: String) {
        guard let toast = toasts.first(where: { $0.key == key }) else { return }
        dismiss(toast.id)
    }

    func act(on id: UUID) {
        let action = toastActions[id]
        dismiss(id)
        action?()
    }

    /// Holds every countdown while the corner is read, and lets them run on
    /// once it is not, each with what it had left.
    public func holdToasts(_ held: Bool) {
        guard held != toastsHeld else { return }
        toastsHeld = held
        let now = Self.now()
        for id in toastClocks.keys {
            if held {
                toastClocks[id]?.hold(now: now)
                toastTimers.removeValue(forKey: id)?.cancel()
            } else {
                toastClocks[id]?.run(now: now)
                runClock(id)
            }
        }
    }

    /// Registers what a toast's `attachment` draws. The closure is asked each
    /// time the card is drawn, so whatever it draws stays live.
    func attach(_ name: String, _ draw: @escaping @MainActor () -> AnyView) {
        attachments[name] = draw
    }

    private func runClock(_ id: UUID) {
        toastTimers.removeValue(forKey: id)?.cancel()
        guard let due = toastClocks[id]?.due(now: Self.now()) else { return }
        toastTimers[id] = Task { [weak self] in
            try? await Task.sleep(for: .seconds(due))
            guard !Task.isCancelled else { return }
            self?.dismiss(id)
        }
    }

    private func stopClock(_ id: UUID) {
        toastTimers.removeValue(forKey: id)?.cancel()
        toastClocks[id] = nil
    }

    private func forget(_ id: UUID) {
        stopClock(id)
        toastActions[id] = nil
    }

    private static func now() -> Double { ProcessInfo.processInfo.systemUptime }

    // MARK: The web's notices (notices.ts), for whatever raises them

    /// Something somebody pressed did not take (`actionFailed`): it fades,
    /// because pressing again is the whole of the remedy.
    public func actionFailed(_ reason: String) {
        announce(CallToast(reason, kind: .failure, lasts: NoticeTimes.atMost))
    }

    /// Something worked where working is not visible on its own (`actionDone`).
    public func actionDone(_ said: String) {
        announce(CallToast(said, lasts: NoticeTimes.atMost))
    }

    /// The uplink card for this participant's own share, while it stands
    /// (strain.ts): what the line carries, what the share needs, what fits.
    public private(set) var strainHint: StrainHint?

    /// This participant's own connection, as the server grades it.
    public private(set) var localQuality: LinkQuality = .good
    /// How the call is going from here, every two seconds (connection.ts):
    /// nothing until the first sample, which the badge reads as unmeasured.
    public private(set) var connection: ConnectionReading?
    /// Whether the badge's figures are open, kept for the call.
    public var connectionOpen = UserDefaults.standard.bool(forKey: "meet-live.signal-detail") {
        didSet { UserDefaults.standard.set(connectionOpen, forKey: "meet-live.signal-detail") }
    }

    public private(set) var microphoneOn = false
    public private(set) var cameraOn = false
    public private(set) var handUp = false

    /// The relay a move is taking this call to, between the server saying so
    /// and the room closing (web App.tsx ~560-627).
    public private(set) var moving: (relay: String, at: Date)?

    /// The call ended because the server moved it; the app joins again.
    public var onMoved: ((_ relay: String) -> Void)?

    /// Everybody else who wants to hear what the call says: the protocols
    /// layered over it (watching together, the song desk, hosting) each
    /// subscribe here and pick out what concerns them. Called on the main
    /// thread after the model itself has applied the event.
    private var listeners: [(CallEvent) -> Void] = []

    public func listen(_ listener: @escaping (CallEvent) -> Void) {
        listeners.append(listener)
    }

    /// The identities of everybody else here, for protocols that keep a
    /// roster (the theatre's and the desk's `roster`).
    public var identities: [String] { people.map(\.id) }

    /// Panels and stage pieces layered over the call, created once per call
    /// by whoever owns them, so the room view can find them.
    public var parts: [String: AnyObject] = [:]

    /// Whether this iPhone's screen is being shared through the broadcast
    /// extension right now.
    public private(set) var phoneSharing = false
    #if os(iOS)
    private var broadcast: BroadcastReceiver?
    #endif

    public let audio = CallAudio()
    @ObservationIgnored let lossless = LosslessPlayback()
    public let camera = CameraCapture()
    #if os(macOS)
    /// The share from this Mac. Kept with the call rather than in the view so
    /// that leaving, or being ended, always stops it: a capture dropped
    /// without being stopped stays counted by Control Center, which then
    /// declines to show the picker again (see `ScreenCapture.pick`).
    public let sharing = Sharing()
    #endif
    private var microphoneInput: AudioInput?
    private var cameraInput: VideoInput?
    private var bridge: Bridge?

    init(call: Call, deployment: Deployment, region: String?, sealed: Bool) {
        self.call = call
        self.room = call.roomName()
        self.localIdentity = call.localIdentity()
        self.localName = call.localName()
        self.deployment = deployment
        self.region = region
        self.sealed = sealed
        lossless.audio = audio
    }

    /// Joins the call that `options` describes.
    public static func join(_ options: CallOptions, deployment: Deployment, region: String?) async throws -> CallModel {
        let bridge = Bridge()
        let call = try await joinCall(options: options, observer: bridge)
        let model = CallModel(call: call, deployment: deployment, region: region, sealed: options.sealingWord != nil)
        model.bridge = bridge
        bridge.model = model
        // The people already here when this participant arrives are told
        // during the join itself, and nobody is told they joined: the web
        // listens only once its room is connected (notices.ts `watch`).
        bridge.flush()
        model.settled = true
        #if DEBUG
        model.driveChatForDebug()
        #endif
        call.setLosslessSink(sink: model.lossless)
        #if os(iOS)
        model.watchInterruptions()
        let receiver = BroadcastReceiver(call: call)
        receiver.onChange = { [weak model] on in model?.phoneSharing = on }
        receiver.start()
        model.broadcast = receiver
        #endif
        return model
    }

    // MARK: What the screens read

    public var surfaces: [Surface] {
        people.flatMap { person -> [Surface] in
            var out = [Surface(id: person.id + "/camera", person: person, kind: .camera,
                               sid: person.track(.camera).flatMap { $0.muted ? nil : $0.sid })]
            if let screen = person.track(.screenShare) {
                out.append(Surface(id: person.id + "/screen", person: person, kind: .screen, sid: screen.sid))
            }
            return out
        }
    }

    /// What the small window shows when the app is in the background: a
    /// shared screen first, then the one other person in a call of two, then
    /// whoever is speaking with a camera on.
    public var pictureInPictureSid: String? {
        if let screen = surfaces.first(where: { $0.kind == .screen && $0.sid != nil }) { return screen.sid }
        let cameras = surfaces.filter { $0.kind == .camera && $0.sid != nil }
        if cameras.count == 1 { return cameras[0].sid }
        return cameras.first { $0.person.speaking }?.sid ?? cameras.first?.sid
    }

    public func person(_ identity: String) -> Person? {
        people.first { $0.id == identity }
    }

    // MARK: What the person does

    public func setMicrophone(_ on: Bool) async throws {
        if on {
            if microphoneInput == nil {
                let input = try await call.publishAudio(kind: .voice(original: audio.originalSound))
                microphoneInput = input
            }
            try audio.startMicrophone(into: microphoneInput!)
            microphoneInput?.setMuted(muted: false)
        } else {
            microphoneInput?.setMuted(muted: true)
            audio.stopMicrophone()
        }
        microphoneOn = on
    }

    /// The web's "original sound" (sound.ts `applyOriginalSound`): the
    /// system's voice processing off and the voice republished at the higher
    /// rate, because nothing is taken out of it any more.
    public func setOriginalSound(_ original: Bool) async throws {
        guard original != audio.originalSound else { return }
        try audio.setOriginalSound(original)
        guard let old = microphoneInput else { return }
        let fresh = try await call.publishAudio(kind: .voice(original: original))
        microphoneInput = fresh
        try? await old.unpublish()
        if microphoneOn {
            try audio.startMicrophone(into: fresh)
        } else {
            fresh.setMuted(muted: true)
        }
    }

    /// Sends another camera instead, keeping the same track.
    public func useCamera(_ device: AVCaptureDevice) throws {
        guard let input = cameraInput else { return }
        try camera.start(device: device, into: input)
    }

    public func setCamera(_ on: Bool) async throws {
        if on {
            let input = try await call.publishVideo(kind: .camera, width: CameraCapture.width, height: CameraCapture.height)
            cameraInput = input
            try camera.start(into: input)
        } else {
            camera.stop()
            if let input = cameraInput {
                cameraInput = nil
                try? await input.unpublish()
            }
        }
        cameraOn = on
    }

    /// A message is on its way (useChat.ts `sending`): the box sends nothing
    /// more until it has gone or failed.
    public private(set) var sending = false

    /// Sends a message, and adds it to the call's own once the media server
    /// has it. Throws when it could not be sent, so that the box can put the
    /// words back: a send that failed silently looks exactly like one that
    /// worked (ChatPanel.tsx).
    public func say(_ text: String) async throws {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        sending = true
        defer { sending = false }
        #if DEBUG
        if rehearseFailedSends { try await Task.sleep(for: .milliseconds(400)); throw RehearsedFailure() }
        #endif
        try await call.sendChat(text: trimmed)
        messages.append(ChatLine(id: UUID().uuidString, identity: localIdentity, name: localName, text: trimmed, at: Date(), mine: true))
        if messages.count > 500 { messages.removeFirst(messages.count - 500) }
    }

    /// What is in the messages box, kept with the call rather than the panel,
    /// so that closing the panel half way through a sentence does not lose it.
    public var chatDraft = "" {
        didSet { if chatDraft != oldValue, !chatDraft.isEmpty { chatFailed = false } }
    }
    /// The last message did not leave. Its words are back in the box, and the
    /// box says so until the next is typed or sent.
    public private(set) var chatFailed = false

    /// Sends what is in the box (ChatPanel.tsx `send`): cleared at once and
    /// put back if it did not leave, because losing what somebody typed is
    /// worse than showing it twice. Nothing goes while a message is already
    /// on its way, or while the call is reconnecting.
    public func sendDraft() async {
        let body = chatDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty, !sending, state == .connected else { return }
        chatDraft = ""
        chatFailed = false
        do {
            try await say(body)
        } catch {
            // Whatever was typed since goes after it, rather than being
            // replaced by it.
            chatDraft = chatDraft.isEmpty ? body : body + "\n" + chatDraft
            chatFailed = true
        }
    }

    #if DEBUG
    /// TOMOSHIBI_DEBUG_CHAT's `fail on`: sends fail, so the box can be seen
    /// putting the words back.
    @ObservationIgnored var rehearseFailedSends = false
    struct RehearsedFailure: Error {}
    #endif

    private var watchingWritten = ""

    /// The `watching` attribute, written only when it changes (watching.ts).
    public func setWatching(_ sharer: String) async throws {
        guard sharer != watchingWritten else { return }
        try await call.setAttributes(attributes: ["watching": sharer])
        watchingWritten = sharer
    }

    /// The hand is an attribute, written only when it changes (hands.ts).
    public func setHand(_ up: Bool) async throws {
        guard up != handUp else { return }
        try await call.setAttributes(attributes: ["hand": up ? "up" : ""])
        handUp = up
    }

    public func send(topic: String, payload: Data, to destinations: [String] = [], reliable: Bool = true) async throws {
        try await call.sendData(topic: topic, payload: payload, destinations: destinations, reliable: reliable)
    }

    #if os(iOS)
    @ObservationIgnored private var interruptions: NSObjectProtocol?

    /// A telephone call, an alarm or Siri takes the audio session away; when
    /// it is handed back the engine is started again, with the microphone if
    /// it was on. Without this a call came back silent in both directions.
    private func watchInterruptions() {
        interruptions = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  AVAudioSession.InterruptionType(rawValue: raw) == .ended else { return }
            MainActor.assumeIsolated {
                guard let self else { return }
                try? self.audio.start()
                if self.microphoneOn, let input = self.microphoneInput {
                    try? self.audio.startMicrophone(into: input)
                }
            }
        }
    }
    #endif

    /// The listener's setting for one of a person's sounds (hearing.ts): the
    /// level on the Opus track's node and on any lossless stream standing in
    /// for it, and a block that the core applies at the server to both.
    public func hear(_ identity: String, sound: MediaSource, level: Float, blocked: Bool) async {
        if let sid = person(identity)?.tracks.values.first(where: { track in
            sound == .unknown ? (track.source == .unknown && track.name.hasPrefix("music")) : track.source == sound
        })?.sid {
            audio.setVolume(sid: sid, level)
        }
        guard sound == .screenShareAudio || sound == .unknown else { return }
        let music = sound == .unknown
        lossless.setLevel(identity, music: music, level)
        await call.setHearing(identity: identity, music: music, level: level, blocked: blocked)
    }

    /// What the parts layered over the call say before it goes, each awaited
    /// while the call can still carry it.
    @ObservationIgnored public var beforeLeaving: [String: @MainActor () async -> Void] = [:]

    public func leave() async {
        for last in beforeLeaving.values { await last() }
        beforeLeaving = [:]
        lossless.closeAll()
        #if os(iOS)
        broadcast?.stop()
        #else
        await sharing.stop()
        #endif
        audio.stop()
        camera.stop()
        await call.leave()
    }

    // MARK: Events

    fileprivate func apply(_ event: CallEvent) {
        switch event {
        case let .stateChanged(state):
            self.state = state
        case let .ended(reason):
            endedReason = reason
            audio.stop()
            camera.stop()
            #if os(macOS)
            Task { await sharing.stop() }
            #endif
            // The server deleting the room is `RoomDeleted` in the Rust SDK
            // (the protocol's ROOM_DELETED, 5, which the web's SDK reads as
            // its room-closed reason); waiting only for `RoomClosed` left the
            // app sitting in a call that had gone, never rejoining.
            let gone = ["RoomDeleted", "RoomClosed", "ParticipantRemoved"].contains { reason.contains($0) }
            if let moving, Date().timeIntervalSince(moving.at) < 15, gone {
                onMoved?(moving.relay)
            }
        case let .participantJoined(participant):
            let arriving = person(participant.identity) == nil
            upsert(participant)
            if arriving, settled {
                announce(CallToast(t("{name} joined", ["name": Self.named(participant.name, participant.identity)])))
            }
        case let .participantChanged(participant):
            upsert(participant)
        case let .participantLeft(identity):
            if let person = person(identity) {
                for sid in person.tracks.keys { audio.stopPlaying(sid: sid) }
                announce(CallToast(t("{name} left", ["name": Self.named(person.name, identity)])))
            }
            people.removeAll { $0.id == identity }
        case let .trackPublished(identity, track), let .trackSubscribed(identity, track):
            if case .trackPublished = event, settled, let person = person(identity), person.tracks[track.sid] == nil {
                announceShare(track, by: person)
            }
            update(identity) { $0.tracks[track.sid] = track }
            if case .trackSubscribed = event, track.kind == .audio {
                let stereo = track.source == .screenShareAudio || track.source == .unknown
                try? audio.play(sid: track.sid, channels: stereo ? 2 : 1, from: call)
            }
        case let .trackUnpublished(identity, sid):
            audio.stopPlaying(sid: sid)
            update(identity) { $0.tracks[sid] = nil }
        case let .trackUnsubscribed(identity, sid):
            audio.stopPlaying(sid: sid)
            update(identity) { $0.tracks[sid]?.subscribed = false }
        case let .trackMuted(identity, sid, muted):
            update(identity) { $0.tracks[sid]?.muted = muted }
        case let .localTrackPublished(track):
            localTracks[track.sid] = track
        case let .localTrackUnpublished(sid):
            localTracks[sid] = nil
        case let .speaking(identities):
            let now = Set(identities)
            for index in people.indices { people[index].speaking = now.contains(people[index].id) }
        case let .quality(identity, quality):
            if identity == localIdentity { localQuality = quality }
            update(identity) { $0.quality = quality }
        case let .chat(id, identity, name, text, timestamp):
            messages.append(ChatLine(id: id, identity: identity, name: name, text: text,
                                     at: Date(timeIntervalSince1970: TimeInterval(timestamp) / 1000), mine: false))
            // The web keeps the last 500 (chat.ts).
            if messages.count > 500 { messages.removeFirst(messages.count - 500) }
            if !chatOpen, let line = messages.last {
                unread = true
                arriving.append(line)
                Task { [weak self] in
                    try? await Task.sleep(for: Self.saidFor)
                    self?.arriving.removeAll { $0.id == line.id }
                }
            }
        case let .data(topic, identity, payload):
            // Server messages come with no sender, and only those are
            // believed (host.ts, App.tsx).
            if identity == nil, topic == "moving" || topic == "placing" {
                moving = (String(decoding: payload, as: UTF8.self), Date())
            }
        case let .attributesChanged(identity, attributes):
            if identity == localIdentity {
                handUp = attributes["hand"] == "up"
            } else {
                update(identity) { $0.attributes = attributes }
            }
        case let .sealed(identity, readable):
            update(identity) { $0.readable = readable }
        case let .strain(hint):
            strainHint = hint
        case let .connection(reading):
            connection = reading
        case .losslessGaveUp:
            // Said to the person sharing, because it is their upload that
            // could not keep up and they are the one who might do something
            // about it; it stays a while, with a close button, and the share
            // itself carries on (notices.ts `losslessGaveUp`).
            announce(CallToast(t("Lossless sound stopped"),
                               detail: t("Your connection could not keep up. Everyone still hears the sound in real time."),
                               lasts: NoticeTimes.atMost, closable: true))
        }
        for listener in listeners { listener(event) }
    }

    /// Whether the join has finished telling this model who was already
    /// here; until then nobody arriving is announced.
    @ObservationIgnored private var settled = false

    /// What a person is called in a notice: their name, or their identity
    /// when they gave none (notices.ts `named`).
    static func named(_ name: String, _ identity: String) -> String {
        name.isEmpty ? identity : name
    }

    /// Somebody else taking the stage with a share, or starting sound on its
    /// own, which has no picture to announce it: otherwise music simply
    /// starts, from nowhere anybody can see (notices.ts `onPublished`). Only a
    /// track that was not already there, so the shares running when this
    /// participant arrived are not news; the core tells those with the person
    /// they belong to.
    private func announceShare(_ track: TrackInfo, by person: Person) {
        let name = Self.named(person.name, person.id)
        // sound.ts `soundOnly`: the shared sound's channel, in either form
        // (lossless.ts `SHARED`), and not the song desk's music.
        let soundOnly = track.source == .screenShareAudio && (track.name == "listening" || track.name == "listening-lossless")
        guard track.source == .screenShare || soundOnly else { return }
        announce(CallToast(soundOnly ? t("{name} is sharing sound", ["name": name]) : t("{name} started sharing", ["name": name]),
                           kind: .signal, lasts: NoticeTimes.aMoment))
    }

    private func upsert(_ participant: ParticipantInfo) {
        var tracks: [String: TrackInfo] = [:]
        for track in participant.tracks { tracks[track.sid] = track }
        if let index = people.firstIndex(where: { $0.id == participant.identity }) {
            people[index].name = participant.name
            people[index].attributes = participant.attributes
            people[index].tracks.merge(tracks) { _, new in new }
        } else {
            people.append(Person(id: participant.identity, name: participant.name, attributes: participant.attributes, tracks: tracks))
        }
    }

    private func update(_ identity: String, _ change: (inout Person) -> Void) {
        guard let index = people.firstIndex(where: { $0.id == identity }) else { return }
        change(&people[index])
    }

    /// Carries the core's events to the main thread in order. Events that
    /// arrive before the model exists (the participants already in the room
    /// are announced during the join itself) wait for it.
    private final class Bridge: CallObserver, @unchecked Sendable {
        weak var model: CallModel?
        private var early: [CallEvent] = []
        private let lock = NSLock()

        func onEvent(event: CallEvent) {
            lock.lock()
            if model == nil {
                early.append(event)
                lock.unlock()
                return
            }
            lock.unlock()
            DispatchQueue.main.async { [weak self] in
                MainActor.assumeIsolated { self?.model?.apply(event) }
            }
        }

        @MainActor func flush() {
            lock.lock()
            let pending = early
            early.removeAll()
            lock.unlock()
            for event in pending { model?.apply(event) }
        }
    }
}
