import Foundation
import Observation
import TomoshibiCore

/// One way of playing: AVPlayer for a file or a playlist, or YouTube's own
/// player in a web view (WatchScreen.tsx `Handle`).
@MainActor
protocol WatchBackend: AnyObject {
    /// It has the video and can say where it is.
    var ready: Bool { get }
    var time: Double { get }
    /// Not paused, as a video element's `!paused && !ended` reads: a player
    /// waiting for data it was told to play is playing.
    var playing: Bool { get }
    /// The rate it runs at when it plays.
    var rate: Double { get }
    /// It can run a little fast or slow; YouTube's cannot by small amounts.
    var nudges: Bool { get }
    var youtube: Bool { get }
    /// It has enough to go on playing: a video element's `readyState >= 3`.
    var flowing: Bool { get }
    func play()
    func pause()
    func seek(_ to: Double)
    func setRate(_ rate: Double)
    func setVolume(_ level: Double, muted: Bool)
    func stop()
}

/// The video everybody is watching, as this app plays it (WatchScreen.tsx
/// `Player`): one per video, as the web keys its player by the video.
///
/// Each viewer plays the video themselves and keeps to the show's timeline.
/// Where it plays from is decided by trying: the place the video is, and if
/// that fails -- an error, or a player that has still not begun `stallMs`
/// after it should have -- the same video relayed by the deployment. YouTube
/// is the one source whose addresses nobody else can use, so it plays through
/// YouTube's own player where YouTube can be reached and through the relay
/// otherwise. On a deployment that relays nothing there is no second place,
/// and a viewer whose connection cannot reach the video is told so.
@MainActor
@Observable
final class WatchPlayer {
    /// How often the player is held to the show (`KEEP_MS`).
    static let keepMs: UInt64 = 250
    /// A player that has not begun this long after it should have is given
    /// the relay: the same as the show's own wait for its holder. A longer
    /// one was measured on the web as twelve seconds of a spinner for a viewer
    /// whose connection to the video's own servers was slow (`STALL_MS`).
    static let stallMs: UInt64 = 8000
    /// How many times a relay-only video that stopped partway is started
    /// again by itself before the viewer is asked to (`RESUMES`).
    static let resumes = 2
    /// YouTube's errors for a video whose owner allows it on no other site.
    static let embeddingRefused: Set<Int> = [101, 150]
    /// Where this viewer's volume is kept: the web's own key, so the habit
    /// carries over (WatchScreen.tsx `VOLUME_KEY`).
    static let volumeKey = "meet-live.watch-volume"

    /// Why a video does not play for this viewer: their connection does not
    /// reach it, the site refused it (`refused`, `failed`), or it is a kind of
    /// video this app's player cannot open at all (`format`: WebM, Matroska,
    /// Ogg, a codec the system has no decoder for), which no connection and
    /// no relay cures.
    enum Lost: Equatable { case network, refused, failed, format }
    /// Where a relay-only video that had been playing stands, once its relay
    /// stopped.
    enum Halted: Equatable { case resuming, stopped }

    let video: WatchVideo
    let info: ResolvedVideo
    /// Weak: the stage and a resume's task can hold a player past the call
    /// whose engine made it, and an unowned reference to an engine already
    /// let go is a crash rather than a press that does nothing.
    @ObservationIgnored private weak var engine: WatchEngine?
    /// The deployment's address, for YouTube's player, read once while the
    /// call is certainly there.
    @ObservationIgnored private let base: String

    /// Playing the deployment's relayed copy rather than the original.
    private(set) var relayed: Bool
    /// YouTube's player was asked to play and did not: the platform wants a
    /// press inside the player itself.
    private(set) var blocked = false
    /// Not playing for this viewer, with no relay to fall back on.
    private(set) var unreachable: Lost?
    private(set) var halted: Halted?
    /// Nothing to hold to yet: a spinner.
    private(set) var waiting = true
    /// Where the clock under the video reads.
    private(set) var position: Double = 0
    private(set) var backend: WatchBackend?
    /// Picture in picture can be started, or is running.
    private(set) var pictureInPicturePossible = false
    private(set) var pictureInPictureActive = false
    var volume: Double {
        didSet { applyVolume(); remember() }
    }
    var muted = false {
        didSet { applyVolume() }
    }

    /// Played at all: a later stall is buffering, not a source that does not
    /// work, and is not given the relay. Measured on the web: before this,
    /// any seek more than twelve seconds in moved a working direct source
    /// onto the relay for good.
    @ObservationIgnored private var played = false
    /// For a video that starts on the relay: whether the copy from where it is
    /// has been tried.
    @ObservationIgnored private var triedDirect = false
    /// Whether the relay has been tried, for a video that starts where it is.
    @ObservationIgnored private var triedRelay = false
    /// Some place it was tried from answered with a file the player cannot
    /// open, as opposed to not answering: what is said once neither plays.
    @ObservationIgnored private var unopenable = false
    @ObservationIgnored private var resumed = 0
    /// A resume is being asked about; a second failure meanwhile is the same.
    @ObservationIgnored private var askingAgain = false
    /// When this source began, and when the stall was last looked at.
    @ObservationIgnored private var stall = WatchStall(started: 0, stallMs: WatchPlayer.stallMs)
    /// Said once, where there is no relay: a stall goes on being one.
    @ObservationIgnored private var gaveUp = false
    @ObservationIgnored private var keeper = WatchKeeper()
    @ObservationIgnored private var playingShow = false

    init(video: WatchVideo, info: ResolvedVideo, engine: WatchEngine, base: String, mainland: Bool) {
        self.video = video
        self.info = info
        self.engine = engine
        self.base = base
        let kept = Rehearsal.choices?.string(forKey: Self.volumeKey).flatMap(Double.init)
        volume = kept.map { $0.isFinite && $0 >= 0 && $0 <= 1 ? $0 : 1 } ?? 1
        // Straight to the relay, for a YouTube video and a viewer who came in
        // through a relay in mainland China, where the server relays it at
        // all (WatchScreen.tsx ~240): YouTube's script does not arrive from
        // there, and every such viewer sat at a black tile waiting for it to
        // fail before being given the relay they were always going to get.
        // Which relay a call came in through is the one honest thing an app
        // knows about where its viewer is. A ticket the server will not relay
        // has no proxy, and its viewer goes on trying YouTube.
        let straightToRelay = mainland && info.play.kind == .youtube && info.proxy != nil
        relayed = info.relay || straightToRelay
        build()
    }

    // MARK: Where it plays from

    private var relayOnly: Bool { info.play.kind == .relay || info.play.kind == .bangumi }

    var playsAsYoutube: Bool { info.play.kind == .youtube && !relayed }

    /// The relay, unless this plays from where the video is; a relay-only
    /// source has nowhere else to play from.
    private var source: (url: String, hls: Bool)? {
        if relayed || info.play.kind == .youtube || relayOnly {
            guard let proxy = info.proxy else { return nil }
            return (proxy.url, proxy.kind == .hls)
        }
        guard let url = info.play.url else { return nil }
        return (url, info.play.kind == .hls)
    }

    private func build() {
        backend?.stop()
        backend = nil
        pictureInPicturePossible = false
        pictureInPictureActive = false
        gaveUp = false
        if relayed { triedRelay = true }
        stall.restart(at: watchClock())
        keeper = WatchKeeper()
        if playsAsYoutube, let id = info.play.id {
            backend = YouTubeBackend(id: id, base: base, owner: self)
        } else if let source, let url = URL(string: source.url) {
            // Held to the size it is shown at where the relay is not the one
            // chosen for its quality: a film at four times the pixels the tile
            // has is the deployment's bandwidth spent on nothing anybody sees.
            // Not where the deployment chose the relay for its better
            // qualities (hls.js `capLevelToPlayerSize: !info.relay`).
            backend = AVBackend(url: url, hls: source.hls, capped: !info.relay, owner: self)
        }
        applyVolume()
        WatchDebug.log("source \(playsAsYoutube ? "youtube" : (relayed ? "relay" : "direct")) \(source?.url ?? info.play.id ?? "")")
    }

    /// Where the video is has failed this viewer: the relay, or where there
    /// is none, saying so. The site's answer stands: a stall after it is that
    /// answer, not the connection.
    private func giveUp(_ why: Lost = .network) {
        if info.proxy != nil {
            if !relayed {
                relayed = true
                build()
            }
        } else if let was = unreachable, was != .network {
            unreachable = was
        } else {
            unreachable = why
        }
    }

    /// A file or playlist that failed: given the other place once, and only
    /// while it has never played; said once neither plays.
    ///
    /// The web stops at the second failure and goes on showing its spinner,
    /// which in a browser is the rare case: a browser opens WebM, Matroska
    /// with VP9 and Ogg. AVPlayer opens none of them, and a video like that
    /// failed where it was, failed again on the relay, and then spun for ever
    /// with nothing said. So the second failure is the end: "this video does
    /// not reach you", with the sentence for why and the link to open it
    /// elsewhere, as the web says for a video nothing relays.
    private func failed(_ why: String, format: Bool = false) {
        guard !played else { return }
        if format { unopenable = true }
        let lost: Lost = unopenable ? .format : .network
        if !relayed {
            if gaveUp { return }
            WatchDebug.log("direct failed \(why)")
            if info.proxy != nil, !triedRelay {
                relayed = true
                build()
            } else {
                gaveUp = true
                unreachable = lost
            }
        } else if info.relay, !relayOnly, !triedDirect {
            // The relay was the first choice and failed: the lower quality
            // from where the video is, rather than nothing.
            WatchDebug.log("relay failed \(why), trying direct")
            triedDirect = true
            relayed = false
            build()
        } else {
            WatchDebug.log("relay failed \(why), nowhere left to play it from")
            gaveUp = true
            unreachable = lost
        }
    }

    // MARK: What the backends report

    /// It can play: the holder's timeline starts now, for everybody. Said
    /// here, with the backend it belongs to: a holder whose YouTube fell back
    /// to the relay has an AVPlayer only from then on, and an ending it never
    /// heard left the whole room at the last frame on the web.
    func backendPlayable(_ from: WatchBackend) {
        guard from === backend else { return }
        engine?.playerReady(video.key)
    }

    func backendBegan(_ from: WatchBackend) {
        guard from === backend else { return }
        played = true
        resumed = 0
        if blocked {
            blocked = false
            letYoutubeTakePresses()
        }
        if unreachable != nil { unreachable = nil }
        if halted != nil { halted = nil }
    }

    func backendEnded(_ from: WatchBackend) {
        guard from === backend else { return }
        engine?.playerEnded(video.key)
    }

    /// AVPlayer could not play it. Before it has played, a source that does
    /// not work; after, for a video that plays only through the relay, the
    /// relay stopped, and it is asked about and started again.
    func backendFailed(_ from: WatchBackend, _ why: String, format: Bool) {
        guard from === backend else { return }
        if played, relayOnly, let av = from as? AVBackend {
            Task { await resume(av) }
        } else {
            failed(why, format: format)
        }
    }

    /// YouTube's script never arrived, or its player never became ready.
    func youtubeUnreachable(_ from: WatchBackend) {
        guard from === backend else { return }
        WatchDebug.log("youtube unreachable")
        giveUp()
    }

    /// YouTube's player refused the video: the relay, or where there is none,
    /// which of the two it was.
    func youtubeRefused(_ from: WatchBackend, code: Int) {
        guard from === backend else { return }
        WatchDebug.log("youtube refused \(code)")
        giveUp(Self.embeddingRefused.contains(code) ? .refused : .failed)
    }

    func pictureInPicture(possible: Bool, active: Bool) {
        if pictureInPicturePossible != possible { pictureInPicturePossible = possible }
        if pictureInPictureActive != active { pictureInPictureActive = active }
    }

    /// Somebody pressed play or pause in the picture-in-picture window, which
    /// drives this player alone; it is the show's to decide, so it is asked
    /// of the show, as the controls on the stage ask it.
    func pressedInPictureInPicture(playing: Bool) {
        engine?.setPlaying(playing)
    }

    /// A relay-only video that stopped after it had played: the server is
    /// asked about the ticket again first, since the switch turned off or a
    /// grant withdrawn is no fault a retry cures; told so, the stage says the
    /// video cannot be played here and this player goes. Otherwise it is
    /// started again from where it stood, `resumes` times, and then the
    /// viewer is told it stopped and given a press.
    private func resume(_ av: AVBackend) async {
        guard !askingAgain, let engine else { return }
        askingAgain = true
        halted = .resuming
        let now = await engine.askAgain(video.ticket)
        askingAgain = false
        guard av === backend, now != .unavailable else { return }
        if resumed >= Self.resumes {
            halted = .stopped
            return
        }
        resumed += 1
        av.reload()
    }

    /// The press once it has stopped.
    func tryAgain() {
        resumed = 0
        halted = .resuming
        (backend as? AVBackend)?.reload()
    }

    // MARK: Held to the show

    /// One keep step (WatchScreen.tsx `keep`, the rules in the protocol
    /// crate's `WatchKeeper`).
    func keep(now: UInt64, show: WatchShow, heardAtMs: UInt64) {
        playingShow = show.playing
        checkStall(now)
        let want = watchExpected(show: show, heardAtMs: heardAtMs, nowMs: now)
        guard let backend else {
            if !waiting { waiting = true }
            if !video.live { setPosition(want) }
            return
        }
        // A refused player is not asked again.
        let refusedForGood = unreachable == .refused || unreachable == .failed || unreachable == .format
        let reading = WatchPlayerReading(
            ready: backend.ready && !refusedForGood, time: backend.time, playing: backend.playing,
            rate: backend.rate, nudges: backend.nudges, youtube: backend.youtube)
        let step = keeper.keep(player: reading, showPlaying: show.playing, expected: want, live: video.live, nowMs: now)
        if waiting != step.waiting { waiting = step.waiting }
        if step.play { backend.play() }
        if step.pause { backend.pause() }
        if let to = step.seek { backend.seek(to) }
        if let rate = step.rate { backend.setRate(rate) }
        if step.refused, !blocked {
            blocked = true
            letYoutubeTakePresses()
        }
        if let at = step.position { setPosition(at) }
        if WatchDebug.logging {
            WatchDebug.log(String(format: "keep t=%.3f playing=%d rate=%.2f want=%.3f show=%d ready=%d%@",
                                  backend.time, backend.playing ? 1 : 0, backend.rate, want, show.playing ? 1 : 0,
                                  reading.ready ? 1 : 0, step.seek.map { String(format: " seek=%.3f", $0) } ?? ""))
        }
    }

    private func setPosition(_ at: Double) {
        if abs(at - position) >= 0.01 { position = at }
    }

    /// Asked once a second, as the web's interval asks it.
    private func checkStall(_ now: UInt64) {
        guard let backend, stall.due(now) else { return }
        let late = stall.late(now)
        if backend.youtube {
            // The API arrived but the player never became ready: the relay. A
            // player that is ready and refused a press is the platform's
            // doing, and is waited on.
            if !played, playingShow, late, !backend.ready { youtubeUnreachable(backend) }
        } else {
            if played || (relayed && !info.relay) || (relayed && triedDirect) { return }
            if playingShow, !backend.flowing, late { failed("stalled") }
        }
    }

    // MARK: Pressed here

    /// At once here, so the press is felt; the show's answer brings everybody
    /// else.
    func playNow() { backend?.play() }
    func pauseNow() { backend?.pause() }

    /// The viewer moved the player: counted as a jump, so the keeper does not
    /// jump it back before the show has heard the seek.
    func seekNow(_ to: Double) {
        keeper.seeked(nowMs: watchClock())
        backend?.seek(to)
        setPosition(to)
    }

    func togglePictureInPicture() {
        (backend as? AVBackend)?.togglePictureInPicture()
    }

    /// A refused YouTube player is started by a press inside YouTube's own
    /// frame, so the frame takes presses while it is refused.
    var tapYoutube: Bool { blocked && playsAsYoutube }

    private func applyVolume() {
        backend?.setVolume(volume, muted: muted || WatchDebug.silenced)
        letYoutubeTakePresses()
    }

    /// YouTube's frame takes no presses -- the controls drive the show, and
    /// YouTube's own would drive only this player -- except while it is
    /// refused, when a press inside it is the only thing that starts it.
    private func letYoutubeTakePresses() {
        (backend as? YouTubeBackend)?.setTappable(tapYoutube)
    }

    private func remember() {
        // Kept only when it is not the default, as the web keeps it, and never
        // in a rehearsal, whose volume is no business of the owner's next film
        // (`Rehearsal.choices`).
        Rehearsal.choices?.set(volume == 1 ? nil : String(volume), forKey: Self.volumeKey)
    }

    func stop() {
        backend?.stop()
        backend = nil
    }
}
