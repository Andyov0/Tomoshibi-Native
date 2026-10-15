import Foundation
import Observation
import SwiftUI
import TomoshibiCore

/// Watching together, layered over a call (web/src/live/watch.ts,
/// components/room/WatchScreen.tsx, WatchPanel.tsx, WatchTab.tsx).
///
/// What the room view holds. The room view makes one of these every time its
/// own initialiser runs, which SwiftUI does on every redraw of the screen
/// around it, and keeps only the first; so this is a handle and nothing more,
/// and the show itself -- the theatre, its timer, the player -- is made once
/// per call and kept in the call's `parts`. Made here instead, every redraw
/// would have said hello to the room again and left a timer running.
@MainActor
@Observable
public final class WatchFeature {
    unowned let call: CallModel
    let engine: WatchEngine

    /// Whether a show is running and its picture belongs on the stage: the
    /// web lists the watched video as a picture whenever a show is believed,
    /// with or without a video in it (surface.ts `surfaces`), and its tile
    /// says nothing is playing until there is.
    public var onStage: Bool { engine.onStage }

    public init(call: CallModel) {
        self.call = call
        if let kept = call.parts[WatchEngine.part] as? WatchEngine {
            engine = kept
        } else {
            engine = WatchEngine(call: call)
            call.parts[WatchEngine.part] = engine
        }
    }
}

/// One call's show, as viewer and as holder: the protocol crate's
/// `WatchTheatre` wired to the call as its module header says, the videos'
/// descriptions asked of the deployment, and the player for whatever is on.
@MainActor
@Observable
final class WatchEngine {
    static let part = "watch"

    @ObservationIgnored unowned let call: CallModel
    @ObservationIgnored private let theatre: WatchTheatre

    /// The show as this participant knows it, and when it was heard (or for
    /// the holder, when this version of it was first read): what the timeline
    /// is reckoned from.
    private(set) var show: WatchShow?
    private(set) var heardAtMs: UInt64 = 0
    /// Whether there is a show at all, kept apart from `show` so that the
    /// room's arrangement, which reads only this, is not redrawn every time
    /// the holder says where it is.
    private(set) var onStage = false
    /// The show runs here.
    private(set) var mine = false
    /// Whether this person may read links and start a show: signed in, given
    /// watching together, on a deployment with a gateway (watch-api.ts
    /// `watchReady`). Asked once, as Room.tsx asks it.
    private(set) var ready = false
    /// What each ticket plays, asked of the server once and kept for the call.
    private(set) var infos: [String: WatchInfo] = [:]
    /// The media servers this person may browse, asked when the panel opens.
    private(set) var servers: [MediaServer] = []
    /// A refusal, said for as long as the web's notice stays (notices.ts
    /// `AT_MOST`) or until it is pressed away.
    private(set) var notice: String?
    /// The player for what is on now, one per video as the web keys its
    /// player by the video's key.
    private(set) var player: WatchPlayer?
    /// The panel is on screen, where a refusal is said rather than on the
    /// stage.
    var panelOpen = false

    /// Released from deinit, which runs outside the main actor's isolation;
    /// only ever touched on the main thread, where the call's model is let go.
    @ObservationIgnored nonisolated(unsafe) private var timer: Timer?
    @ObservationIgnored private var asking: [String: [CheckedContinuation<WatchInfo, Never>]] = [:]
    @ObservationIgnored private var sending: Task<Void, Never>?
    @ObservationIgnored private var noticeTask: Task<Void, Never>?
    @ObservationIgnored private var reconnecting = false
    @ObservationIgnored private var over = false

    init(call: CallModel) {
        self.call = call
        theatre = WatchTheatre(identity: call.localIdentity, name: call.localName, sealed: call.sealed)
        // Not offered at all in a sealed call: nothing is listened to, said
        // or asked, and the server is not even asked whether this person
        // could start a show (Room.tsx: "an offer there led to a panel that
        // could only say so").
        guard !call.sealed else { return }

        call.listen { [weak self] event in self?.hear(event) }
        let now = watchClock()
        // The people already here were announced during the join, before
        // anybody could listen, so the roster is read rather than heard.
        handle(theatre.roster(identities: call.identities, nowMs: now))
        handle(theatre.joined(nowMs: now))

        // Four times a second: the keeper's step (WatchScreen.tsx `KEEP_MS`),
        // and the theatre's tick, which it wants at least once a second. In
        // the common modes, so that dragging a slider does not stop the show.
        let timer = Timer(timeInterval: Double(WatchPlayer.keepMs) / 1000, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.beat() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer

        Task { [weak self] in
            guard let self else { return }
            if let signIn = WatchDebug.signIn {
                _ = try? await call.deployment.signIn(name: signIn.name, passphrase: signIn.passphrase)
            }
            ready = (try? await call.deployment.watchReady()) ?? false
        }
    }

    deinit {
        timer?.invalidate()
    }

    // MARK: The call

    private func hear(_ event: CallEvent) {
        guard !over else { return }
        let now = watchClock()
        switch event {
        case let .participantJoined(participant):
            handle(theatre.participantConnected(identity: participant.identity, nowMs: now))
        case let .participantLeft(identity):
            handle(theatre.participantDisconnected(identity: identity, nowMs: now))
        case let .stateChanged(state):
            switch state {
            case .reconnecting:
                reconnecting = true
            case .connected where reconnecting:
                // A reconnect replaces the roster without saying who left.
                reconnecting = false
                handle(theatre.roster(identities: call.identities, nowMs: now))
            case .disconnected:
                end()
            default:
                break
            }
        case let .data(topic, identity, payload):
            // Only from a participant: the server says nothing on this topic,
            // and anything arriving without a sender is not a show's word.
            guard topic == "watch", let identity else { return }
            let name = call.person(identity)?.name ?? ""
            handle(theatre.onMessage(sender: identity, senderName: name, payload: payload, nowMs: now))
        case .ended:
            end()
        default:
            break
        }
    }

    /// The call is over: the player stops at once rather than when the call's
    /// model is let go, which would leave a film playing after the person
    /// left the room.
    private func end() {
        guard !over else { return }
        over = true
        timer?.invalidate()
        timer = nil
        player?.stop()
        player = nil
        _ = theatre.close(nowMs: watchClock())
        show = nil
        onStage = false
    }

    private func beat() {
        guard !over else { return }
        let now = watchClock()
        handle(theatre.tick(nowMs: now))
        refresh(now)
        if let player, let show {
            player.keep(now: now, show: show, heardAtMs: heardAtMs)
        }
        for press in WatchDebug.presses() { debugPress(press) }
    }

    /// Reads the show again and follows it: a new video gets a player of its
    /// own, and a show that ended takes its player with it.
    private func refresh(_ now: UInt64) {
        let snapshot = theatre.state(nowMs: now)
        if snapshot.show != show || snapshot.heardAtMs != heardAtMs {
            show = snapshot.show
            heardAtMs = snapshot.heardAtMs
        }
        if snapshot.mine != mine { mine = snapshot.mine }
        if (show != nil) != onStage { onStage = show != nil }

        // A video that stopped being playable here (askAgain learned the
        // server will not relay it) takes its player with it too, and the
        // stage says so in its place.
        let video = show?.now
        let playable = video.flatMap { infos[$0.ticket]?.video }
        if let current = player, current.video.key != video?.key || playable == nil {
            current.stop()
            player = nil
        }
        guard let video else { return }
        if player == nil, case let .known(resolved) = info(video.ticket) {
            player = WatchPlayer(video: video, info: resolved, engine: self, mainland: mainland(region: call.region))
        }
    }

    // MARK: What the theatre says

    private func handle(_ out: WatchOutput, answering: Bool = true) {
        for packet in out.packets { send(packet) }
        for why in out.refusals { say(WatchWords.refusal(why)) }
        if answering {
            for ask in out.asks {
                Task { [weak self] in
                    guard let self else { return }
                    let answered = await self.answer(ask)
                    self.handle(answered)
                }
            }
        }
        if out.changed { refresh(watchClock()) }
    }

    /// Published in the order the theatre said them: one at a time, each
    /// after the last has gone, since a state overtaken by the word that the
    /// show closed would have everybody believe in it again.
    private func send(_ packet: WatchPacket) {
        let previous = sending
        sending = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            try? await self.call.send(topic: packet.topic, payload: packet.payload, to: packet.destinations, reliable: packet.reliable)
        }
    }

    /// Finds out what the theatre asked and gives it the answer.
    private func answer(_ ask: WatchAsk) async -> WatchOutput {
        switch ask {
        case let .resolve(request, text, forOther):
            let answer: WatchAnswer
            do {
                switch try await call.deployment.resolveVideo(pasted: text, forOther: forOther) {
                case let .playable(video):
                    remember(video)
                    answer = .found(ticket: video.ticket, duration: video.duration, live: video.live, grantor: video.grantor)
                case let .notPlayable(reason):
                    answer = .refused(why: WatchWords.code(reason))
                }
            } catch {
                // What the web's resolveVideo makes of anything thrown.
                answer = .refused(why: "failed")
            }
            return theatre.answer(request: request, answer: answer, nowMs: watchClock())
        case let .read(request, ticket):
            let answer: WatchAnswer
            switch await readTicket(ticket) {
            case let .known(video):
                answer = .found(ticket: video.ticket, duration: video.duration, live: video.live, grantor: video.grantor)
            case .unavailable:
                answer = .refused(why: "unavailable")
            case .failed:
                answer = .refused(why: "failed")
            }
            return theatre.answer(request: request, answer: answer, nowMs: watchClock())
        }
    }

    func say(_ text: String) {
        notice = text
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(10))
            guard !Task.isCancelled else { return }
            self?.notice = nil
        }
    }

    func dismissNotice() {
        noticeTask?.cancel()
        notice = nil
    }

    // MARK: What each ticket plays (watch-api.ts)

    /// What a ticket plays, if this app knows yet; asks the server once if
    /// not, and the answer redraws whoever asked.
    func info(_ ticket: String) -> WatchInfo? {
        if let known = infos[ticket] { return known }
        if asking[ticket] == nil {
            asking[ticket] = []
            Task { [weak self] in await self?.fetch(ticket) }
        }
        return nil
    }

    /// The same question, waited for (watch-api.ts `readTicket`).
    private func readTicket(_ ticket: String) async -> WatchInfo {
        if let known = info(ticket) { return known }
        return await withCheckedContinuation { waiting in
            asking[ticket, default: []].append(waiting)
        }
    }

    private func fetch(_ ticket: String) async {
        let found: WatchInfo
        do {
            switch try await call.deployment.videoInfo(ticket: ticket) {
            case let .known(video): found = .known(video)
            case .unavailable: found = .unavailable
            case .failed: found = .failed
            }
        } catch {
            found = .failed
        }
        infos[ticket] = found
        let waiting = asking.removeValue(forKey: ticket) ?? []
        for one in waiting { one.resume(returning: found) }
        refresh(watchClock())
    }

    /// What a ticket plays, asked again, for a player whose relay stopped
    /// partway through a video that plays nowhere else (watch-api.ts
    /// `askAgain`). Only "the server will not relay it" replaces what was
    /// known: a ticket that still reads as playable is the player's to try
    /// again, and one that could not be read this time is no reason to
    /// unlearn a video that was playing.
    func askAgain(_ ticket: String) async -> WatchInfo {
        let found: WatchInfo
        do {
            switch try await call.deployment.videoInfo(ticket: ticket) {
            case let .known(video): found = .known(video)
            case .unavailable: found = .unavailable
            case .failed: found = .failed
            }
        } catch {
            found = .failed
        }
        if found == .unavailable {
            infos[ticket] = .unavailable
            refresh(watchClock())
        }
        return found
    }

    /// What this person read themselves, so it is not asked for again.
    private func remember(_ video: ResolvedVideo) {
        infos[video.ticket] = .known(video)
    }

    // MARK: What the person does

    /// Adds what a link plays to the show; true if it was added, or sent to
    /// be (WatchTab.tsx `addVideo`).
    ///
    /// Read here by anybody who may read links, and the show is handed only
    /// the ticket; started here with it when nobody runs one. Somebody who may
    /// not hands the link itself to whoever runs the show, who reads it on
    /// their behalf. Read here rather than by the holder because of whose
    /// choice a video is: a film from a media server is relayed only for the
    /// person given that who chose it, and a link the holder reads for
    /// somebody else is said to be somebody else's and refused such a film.
    func add(_ pasted: String) async -> Bool {
        let text = pasted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !call.sealed, !over else { return false }
        let now = watchClock()

        if !ready {
            let out = theatre.addLink(text: text, nowMs: now)
            handle(out)
            return out.accepted
        }

        refresh(now)
        if show == nil {
            let started = theatre.start(nowMs: now)
            handle(started)
            guard started.accepted else { return false }
            return await own(theatre.addLink(text: text, nowMs: watchClock()))
        }
        if mine {
            return await own(theatre.addLink(text: text, nowMs: now))
        }

        let found: ResolvedLink
        do {
            found = try await call.deployment.resolveVideo(pasted: WatchWords.cut(text, 2000), forOther: false)
        } catch {
            say(WatchWords.refusal("failed"))
            return false
        }
        switch found {
        case let .notPlayable(reason):
            say(WatchWords.refusal(WatchWords.code(reason)))
            return false
        case let .playable(video):
            remember(video)
            // The link beside its ticket, for a holder whose page predates
            // tickets; never for a media server's film, which such a holder
            // would read with its own session and so as its own choice.
            let link: String? = video.play.kind == .relay ? nil : text
            // Where the show ended while the link was being read, this starts
            // the next one here with it (the theatre's add_ticket does).
            return await own(theatre.addTicket(ticket: video.ticket, link: link, nowMs: watchClock()))
        }
    }

    /// An output whose asks are this person's own adding: answered before
    /// saying whether it worked, so the field empties only for a video that
    /// went in.
    private func own(_ out: WatchOutput) async -> Bool {
        handle(out, answering: false)
        var added = out.accepted && out.refusals.isEmpty
        for ask in out.asks {
            let answered = await answer(ask)
            handle(answered)
            if !answered.refusals.isEmpty { added = false }
        }
        return added
    }

    /// Play or pause for everybody. Felt here at once: the player is told too,
    /// so the press does not wait for the show's answer (WatchScreen.tsx
    /// `toggle`).
    func toggle() {
        guard let show else { return }
        if show.playing {
            player?.pauseNow()
            handle(theatre.pause(nowMs: watchClock()))
        } else {
            player?.playNow()
            handle(theatre.play(nowMs: watchClock()))
        }
    }

    func setPlaying(_ playing: Bool) {
        guard let show, show.playing != playing else { return }
        toggle()
    }

    /// Moves everybody to `to`, and this player at once (`seekTo`).
    func seek(to: Double) {
        player?.seekNow(to)
        handle(theatre.seek(to: to, nowMs: watchClock()))
    }

    /// Past the video named, and only that one: two presses do not skip two.
    func skip(_ key: String) {
        handle(theatre.skip(key: key, nowMs: watchClock()))
    }

    func remove(_ key: String) {
        handle(theatre.remove(key: key, nowMs: watchClock()))
    }

    /// Stop running the show here, saying so last of all.
    func stop() {
        handle(theatre.close(nowMs: watchClock()))
    }

    func playerReady(_ key: String) {
        handle(theatre.playerReady(key: key, nowMs: watchClock()))
    }

    func playerEnded(_ key: String) {
        handle(theatre.playerEnded(key: key, nowMs: watchClock()))
    }

    /// The media servers, asked when the panel opens and only of somebody who
    /// could start a show: browsing is for choosing, and choosing is theirs
    /// (WatchPanel.tsx). The web reads every refusal here as none.
    func loadServers() {
        guard ready, !call.sealed else { return }
        Task { [weak self] in
            guard let self else { return }
            let found = (try? await call.deployment.mediaServers()) ?? []
            if found != servers { servers = found }
        }
    }

    // MARK: Driven by a test

    private func debugPress(_ press: String) {
        let parts = press.split(separator: " ", maxSplits: 1).map(String.init)
        let word = parts.first ?? ""
        let rest = parts.count > 1 ? parts[1] : ""
        WatchDebug.log("press \(press)")
        switch word {
        case "add": Task { WatchDebug.log("added \(await add(rest))") }
        case "play": setPlaying(true)
        case "pause": setPlaying(false)
        case "seek": if let to = Double(rest) { seek(to: to) }
        case "skip": if let key = show?.now?.key { skip(key) }
        case "remove": if let index = Int(rest), let queue = show?.queue, queue.indices.contains(index) { remove(queue[index].key) }
        case "stop": stop()
        case "pip": player?.togglePictureInPicture()
        default: break
        }
    }
}
