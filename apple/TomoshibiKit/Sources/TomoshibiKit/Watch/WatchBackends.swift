import AVFoundation
import AVKit
import Foundation
import TomoshibiBasics
import WebKit

/// A file or an HLS playlist, from where it is or relayed: AVPlayer, which
/// plays HLS itself on both platforms, as the browser's own HLS does on iOS.
@MainActor
final class AVBackend: NSObject, WatchBackend {
    let player = AVPlayer()
    /// Kept here rather than in a view, so the picture survives the stage
    /// being redrawn and picture in picture has one layer to take.
    let layer = AVPlayerLayer()
    private let url: URL
    private let hls: Bool
    private let capped: Bool
    private weak var owner: WatchPlayer?
    private var item: AVPlayerItem?
    private var itemWatch: [NSKeyValueObservation] = []
    private var itemTokens: [NSObjectProtocol] = []
    private var playerWatch: [NSKeyValueObservation] = []
    private var pip: AVPictureInPictureController?
    private var pipWatch: [NSKeyValueObservation] = []
    /// What this app last told the player, to tell a press in the picture in
    /// picture window from its own doing.
    private var asked: Bool?
    private var largest: CGSize = .zero
    /// The viewer's own ceiling, read when the video is opened, as the web
    /// reads it (WatchScreen.tsx: "read when the video is opened, so a change
    /// on the account page applies from the next").
    private let ceiling = WatchCeiling.stored()

    init(url: URL, hls: Bool, capped: Bool, owner: WatchPlayer) {
        self.url = url
        self.hls = hls
        self.capped = capped
        self.owner = owner
        super.init()
        player.automaticallyWaitsToMinimizeStalling = true
        player.actionAtItemEnd = .pause
        layer.player = player
        layer.videoGravity = .resizeAspect
        layer.backgroundColor = CGColor(gray: 0, alpha: 1)
        playerWatch.append(player.observe(\.timeControlStatus, options: [.new]) { [weak self] player, _ in
            let playing = player.timeControlStatus == .playing
            DispatchQueue.main.async { MainActor.assumeIsolated { if playing, let self { self.owner?.backendBegan(self) } } }
        })
        playerWatch.append(player.observe(\.rate, options: [.new]) { [weak self] player, _ in
            let rate = player.rate
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.rateChanged(rate) } }
        })
        load(at: nil)
    }

    private func load(at time: CMTime?) {
        for token in itemTokens { NotificationCenter.default.removeObserver(token) }
        itemTokens.removeAll()
        itemWatch.removeAll()

        // Said to be a playlist, whatever its address looks like: the relay's
        // is `/api/watch/media?t=...`, with nothing in it that names HLS.
        var options: [String: Any] = [:]
        if hls { options[AVURLAssetOverrideMIMETypeKey] = "application/vnd.apple.mpegurl" }
        let asset = AVURLAsset(url: url, options: options)
        let item = AVPlayerItem(asset: asset)
        if capped, largest != .zero { item.preferredMaximumResolution = largest }
        self.item = item
        if hls, ceiling != .auto {
            Task { [weak self] in await self?.hold(item, to: asset) }
        }

        itemWatch.append(item.observe(\.status, options: [.new]) { [weak self] item, _ in
            let status = item.status
            let error = item.error.map { String(describing: $0) } ?? ""
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.item === item else { return }
                    if status == .readyToPlay { self.owner?.backendPlayable(self) }
                    if status == .failed { self.owner?.backendFailed(self, "error \(error)", format: Self.unopenable(item.error)) }
                }
            }
        })
        let center = NotificationCenter.default
        itemTokens.append(center.addObserver(forName: AVPlayerItem.didPlayToEndTimeNotification, object: item, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { if let self { self.owner?.backendEnded(self) } }
        })
        itemTokens.append(center.addObserver(forName: AVPlayerItem.failedToPlayToEndTimeNotification, object: item, queue: .main) { [weak self] note in
            let failure = note.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
            let error = failure.map { String(describing: $0) } ?? ""
            let format = Self.unopenable(failure)
            MainActor.assumeIsolated { if let self { self.owner?.backendFailed(self, "stopped \(error)", format: format) } }
        })

        player.replaceCurrentItem(with: item)
        if let time {
            player.seek(to: time, toleranceBefore: .zero, toleranceAfter: .zero)
            if asked == true { player.play() }
        }
    }

    /// The errors AVPlayer gives for a file it cannot open or decode, rather
    /// than one it could not fetch: WebM, Matroska and Ogg answer
    /// "Cannot Open" (-11828, under -12847, measured on macOS 27), and a codec
    /// the system has no decoder for answers one of the others. Looked for
    /// down the chain of underlying errors, where AVFoundation puts the
    /// specific one.
    nonisolated static func unopenable(_ error: Error?) -> Bool {
        let codes: Set<Int> = [AVError.Code.fileFormatNotRecognized, .fileFailedToParse, .failedToParse, .decoderNotFound,
                               .decodeFailed, .formatUnsupported, .operationNotSupportedForAsset, .incompatibleAsset].map(\.rawValue)
            .reduce(into: []) { $0.insert($1) }
        var next = error as NSError?
        while let one = next {
            if one.domain == AVFoundationErrorDomain, codes.contains(one.code) { return true }
            next = one.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return false
    }

    /// Holds a playlist to the viewer's ceiling (WatchScreen.tsx, `capping`
    /// on hls.js's `autoLevelCapping`): the highest rung the ceiling allows,
    /// ranked by bitrate as hls.js ranks its levels, becomes the most this
    /// item may fetch. A peak bitrate rather than a resolution, because that
    /// is what an index into a bitrate-ordered ladder is, and because the
    /// resolution is already the size cap's to set (`shown(pixels:)`), which
    /// this must only ever lower: AVPlayer honours both, so the lower wins.
    private func hold(_ item: AVPlayerItem, to asset: AVURLAsset) async {
        guard let variants = try? await asset.load(.variants), !variants.isEmpty, self.item === item else { return }
        func rate(_ variant: AVAssetVariant) -> Double { variant.peakBitRate ?? variant.averageBitRate ?? 0 }
        let ranked = variants.sorted { rate($0) < rate($1) }
        let levels = ranked.map { variant -> WatchCeiling.Level in
            let size = variant.videoAttributes?.presentationSize ?? .zero
            return WatchCeiling.Level(width: Double(size.width), height: Double(size.height))
        }
        guard let cap = ceiling.cap(levels), rate(ranked[cap]) > 0 else { return }
        // Half way to the rung above rather than the allowed rung's own
        // bitrate: AVPlayer keeps a margin under the limit it is given, and
        // handed exactly the 720p rung's bitrate it played 360p for the
        // whole of a three-minute film on a loopback line (measured). The
        // rung above is never allowed this way, which is the point.
        let allowed = rate(ranked[cap])
        let above = ranked.dropFirst(cap + 1).map(rate).first { $0 > allowed } ?? allowed * 2
        item.preferredPeakBitRate = (allowed + above) / 2
        WatchDebug.log("ceiling \(ceiling.rawValue): at most \(Int(item.preferredPeakBitRate)) b/s, up to \(ranked[cap].videoAttributes?.presentationSize ?? .zero)")
    }

    /// Loads the same address again from where it stood: what hls.js's
    /// `startLoad` does for a relay that stopped.
    func reload() {
        load(at: player.currentTime())
    }

    /// The size the picture is shown at, in pixels, for a player held to it.
    func shown(pixels: CGSize) {
        guard capped, pixels.width > 0, pixels.height > 0 else { return }
        // Only ever grown: shrinking the ceiling while a larger rendition is
        // buffered throws away what was fetched, and the stage and the strip
        // trade places whenever somebody pins a picture.
        let grown = CGSize(width: max(largest.width, pixels.width), height: max(largest.height, pixels.height))
        guard grown != largest else { return }
        largest = grown
        item?.preferredMaximumResolution = grown
    }

    var ready: Bool { item?.status == .readyToPlay }
    var time: Double {
        let seconds = player.currentTime().seconds
        return seconds.isFinite ? seconds : 0
    }
    var playing: Bool { player.timeControlStatus != .paused }
    var rate: Double { Double(player.defaultRate) }
    var nudges: Bool { true }
    var youtube: Bool { false }
    var flowing: Bool {
        guard let item, item.status == .readyToPlay else { return false }
        return item.isPlaybackLikelyToKeepUp || item.isPlaybackBufferFull
    }

    func play() {
        asked = true
        player.play()
    }

    func pause() {
        asked = false
        player.pause()
    }

    /// Exactly where asked, as a video element's `currentTime` is set: a seek
    /// that lands up to a keyframe away would be a gap the keeper then spends
    /// seconds closing.
    func seek(_ to: Double) {
        player.seek(to: CMTime(seconds: max(0, to), preferredTimescale: 600), toleranceBefore: .zero, toleranceAfter: .zero)
    }

    func setRate(_ rate: Double) {
        let wanted = Float(rate)
        if player.defaultRate != wanted { player.defaultRate = wanted }
        // Setting `rate` on a paused player would start it.
        if player.timeControlStatus != .paused, player.rate != wanted { player.rate = wanted }
    }

    func setVolume(_ level: Double, muted: Bool) {
        player.volume = Float(level)
        player.isMuted = muted
    }

    func stop() {
        pip?.stopPictureInPicture()
        pipWatch.removeAll()
        pip = nil
        playerWatch.removeAll()
        itemWatch.removeAll()
        for token in itemTokens { NotificationCenter.default.removeObserver(token) }
        itemTokens.removeAll()
        player.pause()
        player.replaceCurrentItem(with: nil)
        layer.player = nil
    }

    // MARK: Picture in picture

    /// Made once the layer is on screen, which picture in picture needs.
    func layerShown() {
        guard pip == nil, layer.player != nil, AVPictureInPictureController.isPictureInPictureSupported() else { return }
        let made: AVPictureInPictureController? = AVPictureInPictureController(playerLayer: layer)
        guard let made else { return }
        // No skipping in that window: a seek there would move this player
        // alone, and the keeper would move it straight back.
        made.requiresLinearPlayback = true
        pip = made
        pipWatch.append(made.observe(\.isPictureInPicturePossible, options: [.initial, .new]) { [weak self] _, _ in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.reportPictureInPicture() } }
        })
        pipWatch.append(made.observe(\.isPictureInPictureActive, options: [.new]) { [weak self] _, _ in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.reportPictureInPicture() } }
        })
    }

    private func reportPictureInPicture() {
        guard let pip else { return }
        owner?.pictureInPicture(possible: pip.isPictureInPicturePossible, active: pip.isPictureInPictureActive)
        WatchDebug.log("pip possible=\(pip.isPictureInPicturePossible) active=\(pip.isPictureInPictureActive)")
    }

    func togglePictureInPicture() {
        guard let pip else { return }
        if pip.isPictureInPictureActive {
            pip.stopPictureInPicture()
        } else {
            pip.startPictureInPicture()
        }
    }

    /// The picture-in-picture window's own play and pause drive this player
    /// alone; a change nobody here asked for, while that window is up, was
    /// pressed there, and is passed to the show.
    private func rateChanged(_ rate: Float) {
        guard let pip, pip.isPictureInPictureActive, let asked else { return }
        if rate == 0, asked {
            // The end of the video stops the player too, and is no press.
            if let duration = item?.duration.seconds, duration.isFinite, time >= duration - 0.5 { return }
            self.asked = false
            owner?.pressedInPictureInPicture(playing: false)
        } else if rate != 0, !asked {
            self.asked = true
            owner?.pressedInPictureInPicture(playing: true)
        }
    }
}

/// YouTube's own embedded player, where YouTube can be reached (youtube.ts):
/// its media addresses are bound to whoever resolved them, so nobody can be
/// handed one to play, and somebody who can reach YouTube plays it through
/// YouTube's player instead, driven through its IFrame API.
///
/// The page is given the deployment's address as its own, so the player is
/// embedded from the same origin the web client embeds it from, and YouTube
/// sees one site asking whichever client is in front of it.
@MainActor
final class YouTubeBackend: NSObject, WatchBackend, WKNavigationDelegate, WKUIDelegate {
    /// Long enough for a slow connection; short enough that a blocked one is
    /// not a long wait at a black tile (youtube.ts `LOAD_MS`).
    static let loadMs = 6000
    /// YT.PlayerState, by the numbers the API sends.
    static let ended = 0, playingState = 1, buffering = 3

    let web: WKWebView
    private weak var owner: WatchPlayer?
    private(set) var ready = false
    private var reported = 0.0
    private var reportedAt = 0.0
    private var state = -1
    private var loaded = false

    init(id: String, base: String, owner: WatchPlayer) {
        self.owner = owner
        let configuration = WKWebViewConfiguration()
        // Played when asked, as the web's page is once somebody has pressed
        // anything on it; the asking is the show's, not a press here.
        configuration.mediaTypesRequiringUserActionForPlayback = []
        #if os(iOS)
        configuration.allowsInlineMediaPlayback = true
        #endif
        let relay = MessageRelay()
        configuration.userContentController.add(relay, name: "watch")
        web = WKWebView(frame: .zero, configuration: configuration)
        super.init()
        relay.target = self
        web.navigationDelegate = self
        web.uiDelegate = self
        #if os(iOS)
        web.isOpaque = false
        web.backgroundColor = .black
        web.scrollView.isScrollEnabled = false
        #endif
        let origin = URL(string: base).flatMap { url -> String? in
            guard let scheme = url.scheme, let host = url.host else { return nil }
            return url.port.map { "\(scheme)://\(host):\($0)" } ?? "\(scheme)://\(host)"
        } ?? ""
        let api = WatchDebug.youtubeAPI ?? "https://www.youtube.com/iframe_api"
        web.loadHTMLString(Self.page(id: id, origin: origin, api: api), baseURL: URL(string: base))
    }

    var time: Double {
        // The page says where it is ten times a second; between two words a
        // playing player has moved on by however long it has been.
        guard state == Self.playingState else { return reported }
        return reported + ProcessInfo.processInfo.systemUptime - reportedAt
    }
    var playing: Bool { state == Self.playingState || state == Self.buffering }
    var rate: Double { 1 }
    var nudges: Bool { false }
    var youtube: Bool { true }
    var flowing: Bool { state == Self.playingState }

    func play() { run("watch.play()") }
    func pause() { run("watch.pause()") }
    func seek(_ to: Double) { run("watch.seek(\(max(0, to)))") }
    func setRate(_ rate: Double) {}
    func setVolume(_ level: Double, muted: Bool) { run("watch.volume(\(level), \(muted))") }
    func setTappable(_ on: Bool) { run("watch.tappable(\(on))") }

    func stop() {
        run("watch.destroy()")
        web.configuration.userContentController.removeScriptMessageHandler(forName: "watch")
        web.stopLoading()
        web.removeFromSuperview()
    }

    private func run(_ script: String) {
        web.evaluateJavaScript("window.watch && \(script)", completionHandler: nil)
    }

    fileprivate func heard(_ body: Any) {
        guard let message = body as? [String: Any], let kind = message["t"] as? String else { return }
        switch kind {
        case "ready":
            ready = true
            owner?.backendPlayable(self)
        case "tick", "state":
            if let at = message["time"] as? Double {
                reported = at
                reportedAt = ProcessInfo.processInfo.systemUptime
            }
            let was = state
            state = (message["state"] as? Int) ?? state
            if kind == "state" || state != was {
                if state == Self.playingState { owner?.backendBegan(self) }
                if state == Self.ended, was != Self.ended { owner?.backendEnded(self) }
            }
        case "error":
            owner?.youtubeRefused(self, code: (message["code"] as? Int) ?? 0)
        case "unreachable":
            owner?.youtubeUnreachable(self)
        default:
            break
        }
    }

    // The page and the player YouTube puts in it; nothing else is opened in
    // the view: a press on YouTube's name in the frame would otherwise take
    // the stage to YouTube's site.
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void) {
        let frame = action.targetFrame
        if frame != nil, frame?.isMainFrame == false {
            decisionHandler(.allow)
        } else if !loaded {
            loaded = true
            decisionHandler(.allow)
        } else {
            decisionHandler(.cancel)
        }
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction,
                 windowFeatures: WKWindowFeatures) -> WKWebView? {
        nil
    }

    private static func page(id: String, origin: String, api: String) -> String {
        // Every value as a JSON string, so nothing in one can end the script.
        func quoted(_ text: String) -> String {
            let data = (try? JSONSerialization.data(withJSONObject: [text])) ?? Data("[\"\"]".utf8)
            return String(decoding: data, as: UTF8.self).dropFirst().dropLast().description
        }
        return """
        <!doctype html><html><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width,initial-scale=1">
        <style>
        html,body{margin:0;width:100%;height:100%;background:#000;overflow:hidden}
        #player,iframe,video{position:absolute;inset:0;width:100%;height:100%;border:0}
        body.locked iframe,body.locked video{pointer-events:none}
        </style></head><body class="locked"><div id="player"></div><script>
        (() => {
          const say = (message) => window.webkit.messageHandlers.watch.postMessage(message);
          let player, ready = false, gaveUp = false, sound = null;
          const hear = () => {
            if (!player || !ready || !sound) return;
            player.setVolume(Math.round(sound[0] * 100));
            sound[1] ? player.mute() : player.unMute();
          };
          // The API's script either arrives or it does not; from where it does
          // not, the connection usually hangs rather than fails.
          const fail = () => { if (!gaveUp && !ready) { gaveUp = true; say({ t: "unreachable" }); } };
          const timer = setTimeout(fail, \(loadMs));
          window.onYouTubeIframeAPIReady = () => {
            clearTimeout(timer);
            if (gaveUp) return;
            player = new YT.Player("player", {
              videoId: \(quoted(id)), width: "100%", height: "100%",
              playerVars: { controls: 0, disablekb: 1, playsinline: 1, rel: 0, modestbranding: 1, iv_load_policy: 3, origin: \(quoted(origin)) },
              events: {
                onReady: () => { ready = true; hear(); say({ t: "ready" }); },
                onStateChange: (event) => say({ t: "state", state: event.data, time: player.getCurrentTime() }),
                onError: (event) => say({ t: "error", code: event.data }),
              },
            });
          };
          const script = document.createElement("script");
          script.src = \(quoted(api));
          script.async = true;
          script.onerror = () => { clearTimeout(timer); fail(); };
          document.head.append(script);
          setInterval(() => { if (ready) say({ t: "tick", time: player.getCurrentTime(), state: player.getPlayerState() }); }, 100);
          window.watch = {
            play: () => player && player.playVideo(),
            pause: () => player && player.pauseVideo(),
            seek: (to) => player && player.seekTo(to, true),
            // Kept until the player is ready to be told: asked before then,
            // a volume would be lost and the video start at full sound.
            volume: (level, muted) => { sound = [level, muted]; hear(); },
            tappable: (on) => document.body.classList.toggle("locked", !on),
            destroy: () => { ready = false; player && player.destroy(); },
          };
        })();
        </script></body></html>
        """
    }
}

/// Holds the backend weakly: a content controller keeps its handlers for as
/// long as the web view lives, and the web view lives as long as the backend.
private final class MessageRelay: NSObject, WKScriptMessageHandler {
    weak var target: YouTubeBackend?

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        let body = message.body
        MainActor.assumeIsolated { target?.heard(body) }
    }
}
