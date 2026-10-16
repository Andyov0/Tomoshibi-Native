import Foundation
import Observation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The song desk, layered over a call (web/src/live/jukebox.ts, music.ts,
/// sound.ts, duck.ts, components/room/MusicPanel.tsx).
///
/// The room view makes one of these whenever it is built, which SwiftUI may
/// do more than once for the same call; what does the work is the call's one
/// `SongDesk`, found in the call's parts, so a second feature is a second
/// window onto the same desk rather than a second desk saying hello to the
/// room.
@MainActor
@Observable
public final class MusicFeature {
    /// Held, not borrowed, for the reason HostFeature gives: the view keeps
    /// this object past the call it was made for.
    let call: CallModel
    let desk: SongDesk

    public init(call: CallModel) {
        self.call = call
        self.desk = SongDesk.of(call)
    }

    /// Whether the control bar offers the panel (Room.tsx ~280): to somebody
    /// who can reach the library, before there is any music; to everybody
    /// once there is a desk or music to turn down or ask for; and while the
    /// panel is open, so the button that opened it is there to close it after
    /// the music has stopped.
    ///
    /// A sealed call is no exception. The desk there plays for its holder
    /// alone, saying nothing to the room (the desk's state and the asks are
    /// data, which the call does not seal); the music itself is a sealed
    /// track like any other, and everybody else gets a volume for it. The
    /// panel used to be withheld in a sealed call altogether, which the web
    /// never did.
    public var available: Bool {
        desk.libraries != nil || desk.state != nil || desk.playingHere || desk.panelOpen
            || call.people.contains { $0.musicTrack != nil }
    }

    /// Whether music is playing from anybody, for a control that wants to
    /// light up (Room.tsx `playing`).
    public var playing: Bool { desk.state?.now != nil || desk.playingHere || desk.player != nil }
}

/// Which part of the panel is showing (MusicPanel.tsx `Tab`).
enum MusicTab: Hashable {
    case desk, search, playlist
}

/// One call's song desk: the protocol crate's `JukeboxDesk` fed from the call
/// and its answers carried out, the music it plays, and the music it hears
/// turned down under speech.
///
/// The desk itself is the protocol crate's (core/protocol/src/jukebox): this
/// tells it what the call says and does what it asks — fetch and decode a
/// song, publish it, pause it, stop it, search the library, read a link —
/// answering with what came of each. Everything it says to the room goes out
/// on the `jukebox` topic exactly as the web writes it.
///
/// Every answer to the desk is given from a task of its own, never from inside
/// the handling of another of its outputs: what one call asks must all be
/// done, in order, before anything the next one says.
///
/// Switches for driving it without a pointer are in MusicDebug.swift.
@MainActor
@Observable
final class SongDesk {
    // MARK: What the panel draws

    private(set) var state: DeskState?
    private(set) var mine = false
    /// The libraries this participant can reach; nil where there is none or
    /// they are not signed in, which is most people (Room.tsx `library`).
    private(set) var libraries: [MusicLibrary]?
    /// Something that did not work, said for a few seconds (the web's
    /// `actionFailed`).
    private(set) var notice: String?
    /// An ask is being looked up in the library here.
    private(set) var asking = false
    /// The music published from here, playing or paused or between songs.
    private(set) var playingHere = false
    /// The panel is on screen, where something that did not work is said in
    /// it; while it is closed, the same words are a call toast, as the web
    /// says every one of them (notices.ts `actionFailed`). Set by the panel.
    var panelOpen = false

    /// The library's quality, this participant's own (preferences.ts
    /// `meet-live.music-quality`): lossless unless the highest was chosen.
    var quality: MusicQuality {
        didSet { UserDefaults.standard.set(quality == .best ? "best" : nil, forKey: Self.qualityKey) }
    }

    /// Whether the music is turned down while anybody talks (duck.ts
    /// `meet-live.music-duck`, on unless turned off).
    var ducks: Bool {
        didSet {
            UserDefaults.standard.set(ducks ? nil : "off", forKey: Self.duckKey)
            duck.enabled = ducks
            applyDuck(Self.clock())
        }
    }

    /// How loud the music playing from here is in this machine's own
    /// speakers (sound.ts `monitorVolume`, `meet-live.music-monitor`). Only
    /// there: what is sent is the decoded file whatever this says.
    var monitorVolume: Float {
        didSet {
            UserDefaults.standard.set(monitorVolume == 1 ? nil : String(monitorVolume), forKey: Self.monitorKey)
            call.audio.setVolume(sid: MusicDuck.local, monitorVolume)
        }
    }

    // What the panel was showing, kept for the call: a playlist somebody had
    // opened was gone the moment they looked at the desk otherwise
    // (MusicPanel.tsx `useRemembered`).
    var tab: MusicTab = .desk
    var searchSource = ""
    var searchQuery = ""
    private(set) var searching = false
    private(set) var searchResults: [MusicTrack]?
    private(set) var searchProblem: String?
    var searchAdded: Set<String> = []
    var pasted = ""
    private(set) var reading = false
    private(set) var linked: MusicLink?
    private(set) var linkProblem: String?
    var linkAdded: Set<String> = []

    // MARK: Machinery

    unowned let call: CallModel
    @ObservationIgnored private let desk: JukeboxDesk
    @ObservationIgnored private var outbox: AsyncStream<DeskPacket>.Continuation?
    @ObservationIgnored private var loads: [UInt64: Task<Void, Never>] = [:]
    @ObservationIgnored private var prepared: [UInt64: DecodedSong] = [:]
    @ObservationIgnored private var output: MusicOutput?
    @ObservationIgnored private var opening: Task<MusicOutput, Error>?
    /// Bumped by every stop, so that a track still being published when the
    /// music was stopped finds out and takes itself down (sound.ts
    /// `generations`).
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var duck: MusicDuck
    @ObservationIgnored private var meters: [String: (identity: String, level: SoundLevel)] = [:]
    @ObservationIgnored private var microphone: MicrophoneMeter?
    @ObservationIgnored private var ducking: Task<Void, Never>?
    @ObservationIgnored private var lastDuck = 0.0
    @ObservationIgnored private var wasFading = false
    @ObservationIgnored private var asked: UInt64?
    @ObservationIgnored private var noticeTask: Task<Void, Never>?
    @ObservationIgnored private var over = false
    /// What was last handed to the call for each person's music, so the book
    /// is only put back on the call where it changed.
    @ObservationIgnored private var heard: [String: HearingSetting] = [:]
    @ObservationIgnored let debug = MusicDebug()

    static let qualityKey = "meet-live.music-quality"
    static let duckKey = "meet-live.music-duck"
    static let monitorKey = "meet-live.music-monitor"
    /// Shared with the share menu's switch (Sharing.swift): the web keeps one
    /// choice for both, sound shared on its own and the desk's music
    /// (sound.ts `rememberedLossless`, on unless turned off).
    static let losslessKey = "meet-live.lossless"
    private static let part = "music"

    /// Whether the music goes out with a lossless stream beside it, as the
    /// web's `playDecoded(room, decoded, rememberedLossless())` decides: where
    /// it is wanted, and never in a sealed call, whose data is not sealed and
    /// would carry the file's samples across the relay in the clear. Any
    /// participant here can send it, which in the web only Chromium can.
    var losslessOffered: Bool {
        soundLosslessOffered(wanted: UserDefaults.standard.string(forKey: Self.losslessKey) != "off",
                             encrypted: call.sealed, able: true)
    }

    /// The call's desk, made the first time anybody asks for it.
    static func of(_ call: CallModel) -> SongDesk {
        if let desk = call.parts[part] as? SongDesk { return desk }
        let desk = SongDesk(call: call)
        call.parts[part] = desk
        desk.begin()
        return desk
    }

    private init(call: CallModel) {
        self.call = call
        desk = JukeboxDesk(identity: call.localIdentity, name: call.localName, sealed: call.sealed)
        let defaults = UserDefaults.standard
        let ducks = defaults.string(forKey: Self.duckKey) != "off"
        self.ducks = ducks
        duck = MusicDuck(enabled: ducks)
        quality = defaults.string(forKey: Self.qualityKey) == "best" ? .best : .lossless
        // A volume nothing was kept for is full, and so is one that is not a
        // volume: sound.ts once read "nothing kept" as nought and played the
        // music to everybody but its own speakers.
        if let kept = defaults.string(forKey: Self.monitorKey).flatMap(Float.init), kept >= 0, kept <= 1 {
            monitorVolume = kept
        } else {
            monitorVolume = 1
        }
    }

    private func begin() {
        let (stream, continuation) = AsyncStream<DeskPacket>.makeStream()
        outbox = continuation
        // One sender, so the room hears the desk's words in the order it
        // said them.
        Task { [weak self] in
            for await packet in stream {
                guard let call = self?.call else { return }
                try? await call.send(topic: packet.topic, payload: packet.payload, to: packet.destinations, reliable: packet.reliable)
            }
        }
        call.listen { [weak self] event in self?.on(event) }
        debug.prepareEngine(call.audio)

        // Whoever is here already was announced while the call was being
        // joined, before this existed: the roster, their microphones and any
        // music they are playing are taken up now.
        let now = Self.now()
        apply(desk.roster(identities: call.identities, nowMs: now))
        apply(desk.joined(nowMs: now))
        for person in call.people {
            for track in person.tracks.values { subscribed(person.id, track) }
            if person.musicTrack != nil { arrived(person.id) }
        }
        refresh()
        followTheBook()

        // The holder's word every fifteen seconds, and a desk heard of that
        // has gone quiet: the desk wants the time at least once a second.
        Task { [weak self] in
            while true {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !self.over else { return }
                self.apply(self.desk.tick(nowMs: Self.now()))
            }
        }
        Task { [weak self] in await self?.findLibraries() }
    }

    /// The libraries, asked once, as Room.tsx asks: a deployment without one,
    /// or somebody not signed in, is no library and no tabs to search it.
    /// Asked in a sealed call too, where the desk plays for its holder alone.
    private func findLibraries() async {
        await debug.signIn(call.deployment)
        let found: [MusicLibrary]
        if let files = debug.fileLibraries {
            found = files
        } else {
            found = (try? await call.deployment.musicSources()) ?? []
        }
        libraries = found.isEmpty ? nil : found
        if searchSource.isEmpty {
            searchSource = (found.first { $0.signedIn } ?? found.first)?.id ?? ""
        }
        debug.log("libraries: \(found.map(\.id))")
        await debug.drive(self)
    }

    // MARK: The call

    private func on(_ event: CallEvent) {
        guard !over else { return }
        let now = Self.now()
        if debug.logging {
            switch event {
            case .trackPublished, .trackSubscribed, .trackUnpublished, .trackUnsubscribed, .participantJoined:
                debug.log("event \(String(describing: event).prefix(160))")
            default: break
            }
        }
        switch event {
        case let .participantJoined(participant):
            apply(desk.participantConnected(identity: participant.identity, nowMs: now))
        case let .participantLeft(identity):
            apply(desk.participantDisconnected(identity: identity, nowMs: now))
            meters = meters.filter { $0.value.identity != identity }
            duck.left(identity)
        case let .stateChanged(state):
            // A reconnect replaces the roster without saying who left.
            if state == .connected { apply(desk.roster(identities: call.identities, nowMs: now)) }
        case let .data(topic, identity, payload):
            // Only from somebody in the room: the server's own messages are
            // not the desk's (jukebox.ts `onData`, `!participant`).
            guard topic == "jukebox", let identity else { return }
            debug.log("heard \(String(decoding: payload.prefix(60), as: UTF8.self)) from \(identity)")
            apply(desk.onMessage(sender: identity, senderName: call.person(identity)?.name ?? "", payload: payload, nowMs: now))
        case let .trackPublished(identity, track):
            if Self.isMusic(track) { arrived(identity) }
        case let .trackSubscribed(identity, track):
            subscribed(identity, track)
        case let .trackUnpublished(identity, sid), let .trackUnsubscribed(identity, sid):
            meters[sid] = nil
            if call.person(identity)?.musicTrack == nil {
                duck.left(identity)
                heard[identity] = nil
            }
        case .ended:
            end()
        default:
            break
        }
    }

    /// A remote sound began to play here: a microphone is listened to for the
    /// duck, and music is faded in, turned down under speech and set to the
    /// listener's volume. The call model has already started it playing on a
    /// node of its own, which is what the meter listens to.
    private func subscribed(_ identity: String, _ track: TrackInfo) {
        debug.log("track \(identity) \(track.source) \(track.name) \(track.kind) playing \(call.audio.volume(sid: track.sid) != nil)")
        guard track.kind == .audio, call.audio.volume(sid: track.sid) != nil else { return }
        if track.source == .microphone, let level = call.audio.meter(sid: track.sid) {
            meters[track.sid] = (identity, level)
        }
        if Self.isMusic(track) {
            arrived(identity)
            // The player was made at full volume: the listener's setting goes
            // back on it, and on the core, which decides from it whether to
            // ask for the lossless stream at all.
            heard[identity] = nil
            hear(identity)
            debug.feedLossless(call, from: identity)
            let factor = Float(duck.factor(identity, now: Self.clock()))
            call.audio.setDuck(sid: track.sid, factor)
            call.lossless.setDuck(identity, factor)
        }
    }

    /// The song desk's music, in either form (sound.ts `isMusic`, lossless.ts
    /// `MUSIC`): its own source, so that it and a screen's sound never take
    /// each other's place.
    static func isMusic(_ track: TrackInfo) -> Bool {
        track.kind == .audio && track.source == .unknown && (track.name == "music" || track.name == "music-lossless")
    }

    /// Music arriving from somebody whose desk is not heard here, which still
    /// has a volume (MusicPanel.tsx `player`).
    var player: Person? {
        state == nil ? call.people.first { $0.musicTrack != nil } : nil
    }

    private func end() {
        over = true
        for task in loads.values { task.cancel() }
        loads = [:]
        prepared = [:]
        ducking?.cancel()
        if let microphone { call.audio.unmeter(microphone) }
        microphone = nil
        output?.stop()
        output = nil
        outbox?.finish()
    }

    // MARK: The desk's words and wishes

    private func apply(_ out: DeskOutput) {
        for packet in out.packets { outbox?.yield(packet) }
        for action in out.actions { perform(action) }
        for entry in out.skipped {
            say(t("Skipped {title}: the library's account cannot play it.", ["title": entry.title]))
        }
        if out.changed { refresh() }
    }

    private func refresh() {
        let snapshot = desk.state(nowMs: Self.now())
        if (state == nil) != (snapshot.state == nil) {
            debug.log(snapshot.state.map { "desk heard from \($0.holder), mine \(snapshot.mine)" } ?? "no desk")
        }
        if state != snapshot.state { state = snapshot.state }
        if mine != snapshot.mine { mine = snapshot.mine }
    }

    private func perform(_ action: DeskAction) {
        debug.log("action: \(action)")
        switch action {
        case let .prepare(request, entry):
            prepare(request, entry)
        case let .abandon(request):
            loads.removeValue(forKey: request)?.cancel()
            prepared[request] = nil
        case let .play(request, entry):
            play(request, entry)
        case .stop:
            stopMusic()
        case let .pause(paused):
            output?.pause(paused)
        case let .search(request, query, mine):
            search(request, query, mine)
        case let .readLink(request, text, mine):
            readLink(request, text, mine)
        }
    }

    /// Describe, fetch and decode, off the main thread; the desk is told what
    /// came of it unless it abandoned the request meanwhile.
    private func prepare(_ request: UInt64, _ entry: DeskEntry) {
        let quality = quality
        let deployment = call.deployment
        // The file's own samples are kept only where a lossless stream will
        // carry them: the track already out says, or else whether the next
        // one would be offered.
        let keepOriginal = output?.lossless ?? losslessOffered
        let files = debug.files
        loads[request] = Task { [weak self] in
            let outcome: DeskPrepared
            var song: DecodedSong?
            do {
                let loaded = if let files, entry.source == MusicDebug.filesSource {
                    try await MusicDebug.load(entry.id, in: files, keepOriginal: keepOriginal)
                } else {
                    try await MusicLoader.load(source: entry.source, id: entry.id, quality: quality,
                                               keepOriginal: keepOriginal, from: deployment)
                }
                switch loaded {
                case let .ready(decoded):
                    song = decoded
                    outcome = .ready(quality: decoded.quality)
                case .unavailable:
                    outcome = .unavailable
                }
            } catch {
                outcome = .failed
            }
            guard let self, !Task.isCancelled, self.loads.removeValue(forKey: request) != nil else { return }
            if let song { self.prepared[request] = song }
            self.debug.log("prepared \(request) \(entry.title): \(outcome)")
            self.apply(self.desk.prepared(request: request, outcome: outcome, nowMs: Self.now()))
        }
    }

    private func play(_ request: UInt64, _ entry: DeskEntry) {
        let song = prepared.removeValue(forKey: request)
        let started = generation
        Task { [weak self] in
            guard let self else { return }
            var ok = false
            if let song, let output = try? await self.openOutput(), started == self.generation, !self.over {
                output.play(song, key: entry.key)
                ok = true
            }
            guard !self.over else { return }
            self.apply(self.desk.played(request: request, ok: ok, nowMs: Self.now()))
        }
    }

    /// The music's own track, published the first time a song plays and kept
    /// across the songs that follow until the music stops.
    private func openOutput() async throws -> MusicOutput {
        if let output { return output }
        if let opening { return try await opening.value }
        let started = generation
        let task = Task<MusicOutput, Error> { [weak self] in
            guard let call = self?.call, let offered = self?.losslessOffered else { throw CancellationError() }
            let input = try await call.call.publishAudio(kind: .libraryMusic(losslessOffered: offered))
            // Stopped while it was being published: taken down again rather
            // than left playing silence into the room (sound.ts, "a stop that
            // arrives in between used to be outrun").
            guard let self, started == self.generation, !self.over else {
                try? await input.unpublish()
                throw CancellationError()
            }
            var ring: PCMRing?
            do {
                ring = try call.audio.playLocally(key: MusicDuck.local, channels: 2)
            } catch {
                // The music still goes out; only this machine's own speakers
                // go without it.
                self.debug.log("no monitor: \(error)")
            }
            let output = MusicOutput(input: input, lossless: offered, monitor: ring) { [weak self] key in
                MainActor.assumeIsolated { self?.songEnded(key) }
            }
            self.debug.log("published music, lossless offered \(offered)")
            if self.debug.meterMonitor, let level = call.audio.meter(sid: MusicDuck.local) {
                self.meters[MusicDuck.local] = (call.localIdentity, level)
            }
            self.arrived(MusicDuck.local)
            call.audio.setVolume(sid: MusicDuck.local, self.monitorVolume)
            call.audio.setDuck(sid: MusicDuck.local, Float(self.duck.factor(MusicDuck.local, now: Self.clock())))
            output.start()
            self.output = output
            self.playingHere = true
            return output
        }
        opening = task
        defer { if opening == task { opening = nil } }
        return try await task.value
    }

    private func stopMusic() {
        generation += 1
        opening = nil
        if let output {
            output.stop()
            let input = output.input
            Task { try? await input.unpublish() }
        }
        output = nil
        playingHere = false
        call.audio.stopPlaying(sid: MusicDuck.local)
        meters[MusicDuck.local] = nil
        duck.left(MusicDuck.local)
    }

    private func songEnded(_ key: String) {
        guard !over else { return }
        debug.log("ended \(key)")
        apply(desk.trackEnded(key: key, nowMs: Self.now()))
    }

    /// The top hit for a keyword, from the library the desk searches
    /// (`desk_library`: the first signed in, or else the first).
    private func search(_ request: UInt64, _ query: String, _ mine: Bool) {
        let library = deskLibrary(libraries: (libraries ?? []).map { DeskLibrary(id: $0.id, signedIn: $0.signedIn) })
        Task { [weak self] in
            guard let self else { return }
            var songs: [DeskSong] = []
            var failed = false
            if let library {
                do {
                    songs = try await self.searched(library, query).map { Self.song($0, from: library) }
                } catch {
                    failed = true
                }
            }
            guard !self.over else { return }
            let out = self.desk.found(request: request, songs: songs, nowMs: Self.now())
            self.apply(out)
            guard mine else { return }
            self.answered(request, failed ? t("The library did not answer. Try again.") : out.accepted ? nil : t("Nothing found."))
        }
    }

    private func readLink(_ request: UInt64, _ text: String, _ mine: Bool) {
        Task { [weak self] in
            guard let self else { return }
            var songs: [DeskSong] = []
            var failed = false
            do {
                if let link = try await self.call.deployment.musicLink(pasted: text) {
                    songs = link.tracks.map { Self.song($0, from: link.source) }
                }
            } catch {
                failed = true
            }
            guard !self.over else { return }
            let out = self.desk.found(request: request, songs: songs, nowMs: Self.now())
            self.apply(out)
            guard mine else { return }
            self.answered(request, failed ? t("The library did not answer. Try again.") : out.accepted ? nil : t("That link names no playlist or song."))
        }
    }

    private func answered(_ request: UInt64, _ problem: String?) {
        if asked == request {
            asked = nil
            asking = false
        }
        if let problem { say(problem) }
    }

    static func song(_ track: MusicTrack, from source: String) -> DeskSong {
        DeskSong(source: source, id: track.id, title: track.title, artists: track.artists,
                 album: track.album, cover: track.cover, duration: track.duration)
    }

    // MARK: What the person does

    /// Where nobody runs a desk and this participant can reach a library, it
    /// starts here first (MusicPanel.tsx `add`, `ask`).
    private func startIfNobodyRunsOne() {
        if state == nil, libraries != nil { apply(desk.start(nowMs: Self.now())) }
    }

    /// Adds songs chosen from the library: to the desk here, or sent to
    /// whoever runs the one heard.
    func add(_ songs: [DeskSong]) {
        startIfNobodyRunsOne()
        let out = desk.addSongs(songs: songs, nowMs: Self.now())
        apply(out)
        if !out.accepted { say(t("Nobody is running the song desk.")) }
    }

    /// Asks for a song by what was typed: a song's name, or a playlist or song
    /// link (MusicPanel.tsx `ask`).
    func ask(_ typed: String) {
        let text = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !asking else { return }
        startIfNobodyRunsOne()
        let out = desk.ask(text: text, nowMs: Self.now())
        // The search or link this set off, where the desk is here: the box
        // stays busy until it is answered, as the web's awaits it.
        for action in out.actions {
            switch action {
            case let .search(request, _, true), let .readLink(request, _, true):
                asked = request
                asking = true
            default:
                break
            }
        }
        apply(out)
        if !out.accepted { say(mine ? t("Nothing found.") : t("Nobody is running the song desk.")) }
    }

    func vote() { apply(desk.vote(nowMs: Self.now())) }
    func skip() { apply(desk.skip(nowMs: Self.now())) }
    func pause(_ paused: Bool) { apply(desk.pause(paused: paused, nowMs: Self.now())) }
    func remove(_ key: String) { apply(desk.remove(key: key, nowMs: Self.now())) }
    func toTop(_ key: String) { apply(desk.toTop(key: key, nowMs: Self.now())) }
    func close() { apply(desk.close(nowMs: Self.now())) }

    /// A library's answer to a search: the deployment's, or the files a test
    /// stands in for one with.
    private func searched(_ library: String, _ query: String) async throws -> [MusicTrack] {
        if let files = debug.files, library == MusicDebug.filesSource { return MusicDebug.search(query, in: files) }
        return try await call.deployment.musicSearch(source: library, query: query, page: 1)
    }

    // MARK: How loud somebody's music is here

    /// The listener's own setting for somebody's music: the one the sound
    /// panel keeps for that person's music, so a change in either is the
    /// other's too (MusicPanel.tsx uses the sound panel's own row for it).
    func hearing(of identity: String) -> HearingSetting {
        Hearing.shared.setting(identity, .music)
    }

    func setVolume(of identity: String, _ volume: Double) {
        var setting = hearing(of: identity)
        setting.volume = volume
        Hearing.shared.set(identity, .music, setting)
        hear(identity)
    }

    /// Muting is a block at the media server, not a volume of nought: a muted
    /// song is not downloaded at all, and its lossless stream not even asked
    /// for (MusicPanel.tsx, hearing.ts `setBlocked`).
    func setBlocked(of identity: String, _ blocked: Bool) {
        var setting = hearing(of: identity)
        setting.blocked = blocked
        Hearing.shared.set(identity, .music, setting)
        hear(identity)
    }

    /// Puts the book's setting for this person's music on the call, where it
    /// changed: the Opus track's node, the lossless stream standing in for
    /// it, and the core, which blocks both at the server and decides whether
    /// the lossless stream is asked for. Through `CallModel.hear`, as the
    /// web's sound panel row goes through hearing.ts and Audible.tsx; before
    /// this, the panel's volume moved only the Opus node, so a lossless
    /// stream played on at full level and a muted song went on downloading.
    private func hear(_ identity: String) {
        guard call.person(identity)?.musicTrack != nil else { return }
        let setting = hearing(of: identity)
        guard heard[identity] != setting else { return }
        heard[identity] = setting
        // Silenced, not only blocked, is what keeps the lossless stream from
        // being asked for: hearing.ts `silenced` counts a volume of nought,
        // which the core's own record does not, and a stream asked for and
        // played at nothing is upload the sender spends on nobody.
        let silenced = setting.silenced
        debug.log("hearing \(identity): volume \(setting.volume), blocked \(setting.blocked)")
        Task { await call.hear(identity, sound: .unknown, level: Float(setting.volume), blocked: silenced) }
    }

    /// Follows the book wherever it is written: the sound panel's music row
    /// writes the same setting, and the lossless stream and the core must
    /// hear of it as they do from here.
    private func followTheBook() {
        guard !over else { return }
        // The book alone is watched, through any one of its settings: the
        // people in the call change many times a second as they speak.
        withObservationTracking {
            _ = Hearing.shared.setting("", .music)
        } onChange: { [weak self] in
            Task { @MainActor [weak self] in
                guard let self, !self.over else { return }
                for person in self.call.people where person.musicTrack != nil { self.hear(person.id) }
                self.followTheBook()
            }
        }
    }

    /// Searches the library chosen in the panel's search tab.
    func searchLibrary() async {
        let asked = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty, !searching else { return }
        searching = true
        searchProblem = nil
        do {
            searchResults = try await searched(searchSource, asked)
        } catch {
            searchProblem = t("The library did not answer. Try again.")
        }
        searching = false
    }

    func chooseLibrary(_ id: String) {
        searchSource = id
        searchResults = nil
    }

    /// Reads the link pasted in the panel's playlist tab.
    func openLink() async {
        guard !pasted.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, !reading else { return }
        reading = true
        linkProblem = nil
        do {
            let found = try await call.deployment.musicLink(pasted: pasted)
            if found == nil { linkProblem = t("That link names no playlist or song.") }
            linked = found
            linkAdded = []
        } catch {
            linkProblem = t("The library did not answer. Try again.")
        }
        reading = false
    }

    /// Says something that did not work: in the panel while it is open, and
    /// as a call toast while it is closed, where the words would otherwise
    /// never be seen (a song skipped while nobody had the panel open).
    private func say(_ text: String) {
        guard panelOpen else {
            call.announce(CallToast(text, kind: .failure, lasts: 10))
            return
        }
        notice = text
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            self?.notice = nil
        }
    }

    // MARK: Turning the music down under speech

    /// A music stream has started from somebody, or from here: faded in, and
    /// the microphones listened to while there is music to turn down.
    private func arrived(_ key: String) {
        duck.arrived(key, now: Self.clock())
        guard ducking == nil, !over else { return }
        lastDuck = Self.clock()
        ducking = Task { [weak self] in
            while true {
                try? await Task.sleep(for: .milliseconds(Int(MusicDuck.tickMs)))
                guard let self, !self.over, !Task.isCancelled else { return }
                if !self.duckTick() {
                    self.ducking = nil
                    return
                }
            }
        }
    }

    /// One reading of every microphone that is on, this participant's
    /// included, and the music moved (duck.ts `tick`). False once there is
    /// no music and nothing still moving, when the clock stops.
    private func duckTick() -> Bool {
        let now = Self.clock()
        let music = output != nil || opening != nil || call.people.contains { $0.musicTrack != nil }

        if music, call.microphoneOn || debug.meterMicrophone {
            if microphone == nil { microphone = call.audio.meterMicrophone() }
        } else if let microphone {
            call.audio.unmeter(microphone)
            self.microphone = nil
        }

        var levels: [Double] = []
        for (sid, meter) in meters where call.person(meter.identity)?.tracks[sid]?.muted != true {
            levels.append(meter.level.rms)
        }
        if let microphone { levels.append(microphone.level.rms) }

        let before = duck.level
        // Once more on the first tick after a fade-in ends: the duck says it
        // moved only while a stream is inside its fade, and a tick that came
        // late -- the main thread busy, as it is while a call opens -- left
        // the music, Opus and lossless alike, at whatever the last tick
        // inside the fade had made it. Measured at 0.56 for good, a stream
        // that faded in for 840 ms and then stopped.
        let fadedIn = wasFading && !duck.fading(at: now)
        if duck.tick(levels: levels, now: now, elapsed: now - lastDuck) || fadedIn { applyDuck(now) }
        wasFading = duck.fading(at: now)
        lastDuck = now
        if duck.level != before || debug.every(25) {
            let chunks = meters.values.map(\.level.lastChunk) + (microphone.map { [$0.level.lastChunk] } ?? [])
            debug.log(String(format: "duck %.3f, %d microphones, loudest %.4f, chunks %@", duck.level, levels.count, levels.max() ?? 0, "\(chunks)"))
            debug.logLossless(call)
            if let output { debug.log(String(format: "music clock woke at worst %.1f ms late", output.takeWorstLate())) }
        }
        return music || !duck.settled(at: now)
    }

    private func applyDuck(_ now: Double) {
        for person in call.people {
            if let sid = person.musicTrack?.sid {
                let factor = Float(duck.factor(person.id, now: now))
                call.audio.setDuck(sid: sid, factor)
                // And the lossless stream standing in for it, which is played
                // at the same level (lossless-receiver.ts `levelOf`).
                call.lossless.setDuck(person.id, factor)
            }
        }
        if output != nil {
            call.audio.setDuck(sid: MusicDuck.local, Float(duck.factor(MusicDuck.local, now: now)))
        }
    }

    /// How far the music is turned down now, for the log and the panel.
    var duckLevel: Double { duck.level }

    /// Where the song playing from here is and how long it is, for the
    /// holder's own panel; nobody else knows, since the desk does not say.
    var progress: (at: Double, of: Double)? { output?.progress }

    // MARK: Clocks

    /// The wall clock the desk's keys and staleness are written in, as
    /// `Date.now()` is in jukebox.ts.
    static func now() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }

    /// A clock that never jumps, for the duck, in milliseconds.
    static func clock() -> Double {
        Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000
    }
}
