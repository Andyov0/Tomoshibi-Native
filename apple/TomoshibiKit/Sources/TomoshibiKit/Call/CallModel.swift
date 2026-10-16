import AVFoundation
import Foundation
import Observation
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

/// One thing said in the corner of a call.
public struct CallToast: Identifiable, Equatable, Sendable {
    public enum Kind: Sendable { case info, warning, failure }

    public let id = UUID()
    public var title: String
    public var detail: String?
    public var kind: Kind
    /// Seconds before it goes by itself, or nil to stay until dismissed.
    public var lasts: Double?
    /// The words on its button, when it has one.
    public var actionTitle: String?
    /// A second toast with the same key replaces the first: one "Reconnecting"
    /// rather than three.
    public var key: String?

    public init(_ title: String, detail: String? = nil, kind: Kind = .info, lasts: Double? = 4,
                actionTitle: String? = nil, key: String? = nil) {
        self.title = title
        self.detail = detail
        self.kind = kind
        self.lasts = lasts
        self.actionTitle = actionTitle
        self.key = key
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
    /// What the join answer called the machine this call was sent to, and
    /// the machine the meeting is actually on where that is another one (the
    /// web's `relay` and `carrying`, which the signal badge's detail names).
    public internal(set) var relay: String?
    public internal(set) var holding: String?

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

    /// Something that happened and is over, said for a moment over the call.
    public var notice: String?

    /// What the call says in the corner, newest last (the web's toasts,
    /// notices.ts): somebody arriving, a device that would not start, a song
    /// that could not be played. At most three are shown.
    public private(set) var toasts: [CallToast] = []
    @ObservationIgnored private var toastActions: [UUID: @MainActor () -> Void] = [:]

    /// Says `toast`; one with an action calls it when its button is pressed.
    /// A toast that lasts for ever is one somebody has to act on, as the web
    /// keeps a refused device until it is dismissed; anything else goes.
    public func announce(_ toast: CallToast, action: (@MainActor () -> Void)? = nil) {
        if let key = toast.key, let index = toasts.firstIndex(where: { $0.key == key }) {
            toastActions[toasts[index].id] = nil
            toasts.remove(at: index)
        }
        toasts.append(toast)
        if let action { toastActions[toast.id] = action }
        while toasts.count > 3 { dismiss(toasts[0].id) }
        if let lasts = toast.lasts {
            let id = toast.id
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(lasts))
                self?.dismiss(id)
            }
        }
    }

    public func dismiss(_ id: UUID) {
        toasts.removeAll { $0.id == id }
        toastActions[id] = nil
    }

    func act(on id: UUID) {
        let action = toastActions[id]
        dismiss(id)
        action?()
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
    @ObservationIgnored private var cameraStarting = false
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
        bridge.flush()
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

    /// Where this participant's device choices are kept: under the web's
    /// names for a real call, nowhere for a rehearsal (AppModel sets it).
    @ObservationIgnored public var memory: DeviceMemory = .standard {
        didSet {
            #if os(macOS)
            sharing.remembers = memory.keeps
            #endif
        }
    }

    /// The microphone on or off. A microphone that will not start says so
    /// itself (`deviceFailed`), wherever it was asked from: the bar, the
    /// menu's shortcut, the join. The bar used to say "That could not be
    /// done" for four seconds and the join said nothing at all.
    public func setMicrophone(_ on: Bool) async throws {
        if on {
            do {
                try await DeviceAccess.ensure(.microphone)
                if microphoneInput == nil {
                    let input = try await call.publishAudio(kind: .voice(original: audio.originalSound))
                    microphoneInput = input
                }
                try audio.startMicrophone(into: microphoneInput!)
                microphoneInput?.setMuted(muted: false)
            } catch {
                // Muted at the track, so that nobody sees a microphone on
                // that sends nothing.
                microphoneInput?.setMuted(muted: true)
                deviceFailed(.microphone, error)
                throw error
            }
        } else {
            microphoneInput?.setMuted(muted: true)
            audio.stopMicrophone()
        }
        microphoneOn = on
    }

    /// The web's "original sound" (sound.ts `applyOriginalSound`): the
    /// system's voice processing off and the voice republished at the higher
    /// rate, because nothing is taken out of it any more. Remembered once it
    /// took, as the web remembers it.
    public func setOriginalSound(_ original: Bool) async throws {
        guard original != audio.originalSound else { return }
        do {
            try audio.setOriginalSound(original)
            if let old = microphoneInput {
                let fresh = try await call.publishAudio(kind: .voice(original: original))
                microphoneInput = fresh
                try? await old.unpublish()
                if microphoneOn {
                    try audio.startMicrophone(into: fresh)
                } else {
                    fresh.setMuted(muted: true)
                }
            }
        } catch {
            deviceFailed(.microphone, error)
            throw error
        }
        memory.remember(originalSound: original)
    }

    /// Sends another camera instead, keeping the same track; with the camera
    /// off, the one it turns on with. Remembered once it took.
    public func useCamera(_ device: AVCaptureDevice) throws {
        if let input = cameraInput {
            do {
                try camera.start(device: device, into: input)
            } catch {
                deviceFailed(.camera, error)
                throw error
            }
        } else {
            camera.preferred = device.uniqueID
        }
        memory.remember(.camera, id: device.uniqueID)
    }

    /// Another microphone or speakers, remembered once they took. A pair of
    /// speakers that will not take says nothing, as on the web: the sound
    /// stays where it was, which is where it was going anyway.
    public func useAudio(_ device: AudioDevice, input: Bool) throws {
        do {
            try audio.use(device, input: input)
        } catch {
            if input { deviceFailed(.microphone, error) }
            throw error
        }
        memory.remember(input ? .microphone : .speakers, id: device.id)
    }

    /// The devices last picked, where they are still there, and the voice as
    /// it was last sent (preferences.ts `restoreDevices`, sound.ts
    /// `rememberedOriginal`). Before the microphone and camera start, so the
    /// first track is from the right device rather than the default for a
    /// moment and then a switch. It cannot fail the join: a device that is
    /// gone leaves the system's default, where everybody was before the
    /// choice was remembered at all.
    public func restoreDevices() {
        if let id = memory.restorable(.camera, present: CameraCapture.cameras().map(\.uniqueID)) {
            camera.preferred = id
        }
        let microphone = memory.restorable(.microphone, present: CallAudio.devices(input: true).map(\.id))
        #if os(macOS)
        let speakers = memory.restorable(.speakers, present: CallAudio.devices(input: false).map(\.id))
        #else
        let speakers: String? = nil
        #endif
        audio.prefer(input: microphone, output: speakers, original: memory.originalSound)
    }

    public func setCamera(_ on: Bool) async throws {
        if on {
            // Once: a second press (the bar, or the menu's shortcut) while the
            // first was still asking would publish a second camera.
            guard cameraInput == nil, !cameraStarting else { return }
            cameraStarting = true
            defer { cameraStarting = false }
            do {
                try await DeviceAccess.ensure(.camera)
                let input = try await call.publishVideo(kind: .camera, width: CameraCapture.width, height: CameraCapture.height)
                do {
                    try camera.start(into: input)
                } catch {
                    // A camera published and never fed is a black tile for
                    // everybody else.
                    try? await input.unpublish()
                    throw error
                }
                cameraInput = input
            } catch {
                deviceFailed(.camera, error)
                throw error
            }
        } else {
            camera.stop()
            if let input = cameraInput {
                cameraInput = nil
                try? await input.unpublish()
            }
        }
        cameraOn = on
    }

    public func say(_ text: String) async throws {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        try await call.sendChat(text: trimmed)
        messages.append(ChatLine(id: UUID().uuidString, identity: localIdentity, name: localName, text: trimmed, at: Date(), mine: true))
    }

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
            upsert(participant)
        case let .participantChanged(participant):
            upsert(participant)
        case let .participantLeft(identity):
            if let person = person(identity) {
                for sid in person.tracks.keys { audio.stopPlaying(sid: sid) }
            }
            people.removeAll { $0.id == identity }
        case let .trackPublished(identity, track), let .trackSubscribed(identity, track):
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
            notice = t("Your connection could not keep up. Everyone still hears the sound in real time.")
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(8))
                self?.notice = nil
            }
        }
        for listener in listeners { listener(event) }
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
