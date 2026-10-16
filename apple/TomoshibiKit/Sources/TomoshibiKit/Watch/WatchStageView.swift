import SwiftUI
import TomoshibiCore

/// The video everybody is watching, drawn on the stage (WatchScreen.tsx).
///
/// The controls drive the show, not this player: pressing pause asks the show
/// to pause, and everybody pauses. This player also does it at once, so the
/// press is felt without waiting for the show's answer to come back.
///
/// In the strip, when somebody has pinned another picture, it is a tile like
/// the others: the picture goes on playing, labelled, with no controls.
struct WatchStageView: View {
    let feature: WatchFeature
    /// Opens the watch panel, from the empty stage and from the controls.
    var openPanel: () -> Void = {}

    var body: some View {
        let engine = feature.engine
        GeometryReader { geometry in
            let compact = geometry.size.height < 200
            ZStack {
                Color.black
                stage(engine, compact: compact)
                if WatchDebug.panel, !compact {
                    HStack {
                        Spacer()
                        WatchPanelView(feature: feature, close: {})
                            .frame(width: min(360, geometry.size.width - 32))
                            .padding(12)
                    }
                    .transition(Theme.slideIn)
                }
            }
            .overlay(alignment: .top) {
                // A refusal, said here while the panel that would say it is
                // not on screen: the web's notice for it is the room's own
                // (`actionFailed`), which this feature cannot reach.
                if let notice = engine.notice, !engine.panelOpen, !compact {
                    Notice(text: notice, danger: true)
                        .padding(.top, 12)
                        .padding(.horizontal, 16)
                        .onTapGesture { engine.dismissNotice() }
                        .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: engine.notice)
            // Whether a refusal said now would be seen here; otherwise it is
            // a call toast (WatchEngine.say).
            .onChange(of: compact, initial: true) { engine.stageShowing = !compact }
        }
        .onDisappear { engine.stageShowing = false }
    }

    @ViewBuilder
    private func stage(_ engine: WatchEngine, compact: Bool) -> some View {
        if let video = engine.show?.now {
            let info = engine.info(video.ticket)
            Group {
                switch info {
                case .known:
                    if let player = engine.player, player.video.key == video.key {
                        PlayerStage(engine: engine, player: player, compact: compact, openPanel: openPanel)
                            .id(video.key)
                    } else {
                        Spinner()
                    }
                case .unavailable, .failed:
                    VStack(spacing: 12) {
                        // Not "try again" for a video that plays only
                        // through a relay the server will not run for it: no
                        // retry plays it.
                        Text(info == .unavailable ? t("That video cannot be played here.") : t("The video could not be read. Try again."))
                            .font(.system(size: 13))
                            .foregroundStyle(.white.opacity(0.7))
                            .multilineTextAlignment(.center)
                        if !compact {
                            Button { engine.skip(video.key) } label: { Label(t("Next video"), systemImage: "forward.end.fill") }
                                .buttonStyle(StageButtonStyle())
                        }
                    }
                    .padding(16)
                case nil:
                    Spinner()
                }
            }
            .transition(Theme.arrive)
        } else {
            VStack(spacing: 12) {
                Image(systemName: "film")
                    .font(.system(size: compact ? 18 : 28))
                if !compact {
                    Text(t("Nothing is playing. Paste a video link to watch together."))
                        .font(.system(size: 13))
                        .multilineTextAlignment(.center)
                    Button(action: openPanel) { Label(t("Add a video"), systemImage: "plus") }
                        .buttonStyle(StageButtonStyle(prominent: true))
                }
            }
            .foregroundStyle(Theme.muted)
            .padding(16)
            .transition(Theme.arrive)
        }
        if compact {
            VStack {
                Spacer()
                HStack {
                    Text(t("Watching together"))
                        .font(.system(size: 10.5, weight: .medium))
                        .foregroundStyle(Theme.foreground)
                        .padding(.horizontal, 6).padding(.vertical, 3)
                        .background(Capsule().fill(.black.opacity(0.55)))
                        .padding(4)
                    Spacer()
                }
            }
            .transition(Theme.arrive)
        }
    }
}

private struct Spinner: View {
    var body: some View {
        MediaSpinner(size: 26, tint: .white.opacity(0.85))
    }
}

/// A player and everything said over it.
private struct PlayerStage: View {
    let engine: WatchEngine
    @Bindable var player: WatchPlayer
    let compact: Bool
    let openPanel: () -> Void
    @State private var hovering = false
    @State private var touched = false
    @State private var touchTask: Task<Void, Never>?
    @State private var scrubbing: Double?

    var body: some View {
        let playing = engine.show?.playing ?? false
        let controls = !compact && (hovering || touched || !playing || scrubbing != nil)
        ZStack {
            WatchSurface(player: player)
                .allowsHitTesting(player.tapYoutube)

            if player.waiting, player.unreachable == nil, player.halted != .stopped {
                Spinner().allowsHitTesting(false).transition(Theme.arrive)
            }

            if let lost = player.unreachable {
                Unreachable(lost: lost, link: player.info.link)
                    .transition(Theme.arrive)
            }

            if player.halted == .stopped {
                ZStack {
                    Color.black.opacity(0.85)
                    VStack(spacing: 12) {
                        Image(systemName: "icloud.slash").font(.system(size: 22)).foregroundStyle(.white.opacity(0.7))
                        Text(t("The video stopped coming through.")).font(.system(size: 13)).foregroundStyle(.white)
                        Button { player.tryAgain() } label: { Label(t("Try again"), systemImage: "arrow.clockwise") }
                            .buttonStyle(StageButtonStyle(prominent: true))
                    }
                    .padding(16)
                }
                .transition(Theme.arrive)
            }

            VStack {
                if player.halted == .resuming {
                    Notice(text: t("Reconnecting…")).transition(Theme.arrive)
                }
                if player.tapYoutube, player.unreachable == nil {
                    Label(t("Tap the video to join"), systemImage: "play.fill")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 16).padding(.vertical, 9)
                        .glassSurface(Capsule())
                        .allowsHitTesting(false)
                        .transition(Theme.arrive)
                }
                Spacer()
            }
            .padding(.top, 12)

            if !compact {
                VStack {
                    Spacer()
                    if controls {
                        Controls(engine: engine, player: player, playing: playing, scrubbing: $scrubbing, openPanel: openPanel)
                            .transition(Theme.slideUp)
                    }
                }
            }
        }
        .contentShape(Rectangle())
        #if os(macOS)
        .onHover { hovering = $0 }
        #else
        // No pointer to hover with: a touch shows the controls for a while,
        // as a video player on a phone does.
        .simultaneousGesture(TapGesture().onEnded {
            touched = true
            touchTask?.cancel()
            touchTask = Task {
                try? await Task.sleep(for: .seconds(3))
                if !Task.isCancelled { touched = false }
            }
        })
        #endif
        .animation(Theme.settle, value: controls)
        .animation(Theme.settle, value: player.waiting)
        .animation(Theme.settle, value: player.unreachable)
        .animation(Theme.settle, value: player.halted)
        .animation(Theme.settle, value: player.tapYoutube)
    }
}

/// Where the video does not reach this viewer, with no relay to fall back on.
private struct Unreachable: View {
    let lost: WatchPlayer.Lost
    let link: String

    var body: some View {
        let site = WatchWords.site(link)
        ZStack {
            Color.black.opacity(0.85)
            VStack(spacing: 8) {
                Image(systemName: lost == .format ? "film.stack" : "icloud.slash")
                    .font(.system(size: 20, weight: .medium))
                    .foregroundStyle(.white.opacity(0.75))
                    .frame(width: 48, height: 48)
                    .background(Circle().fill(.white.opacity(0.08)))
                    .padding(.bottom, 4)
                Text(t("This video does not reach you")).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                // Not "everybody else goes on watching" for what the site
                // refused: what it refuses here, it refuses everybody.
                Text(sentence(site))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.white.opacity(0.6))
                    .multilineTextAlignment(.center)
                if let url = URL(string: link), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                    Link(destination: url) {
                        Label(t("Open the link"), systemImage: "arrow.up.right.square")
                    }
                    .buttonStyle(StageButtonStyle())
                    .padding(.top, 4)
                }
            }
            .frame(maxWidth: 288)
            .padding(16)
        }
    }

    private func sentence(_ site: String) -> String {
        switch lost {
        // A kind of video the player here cannot open, from where it is and
        // through the relay alike: the browsers in the call can, so the show
        // goes on for them.
        case .format: t("This app cannot play this kind of video. Everybody else goes on watching.")
        case .refused where !site.isEmpty: t("It plays only on {site} itself.", ["site": site])
        case .failed where !site.isEmpty: t("{site} would not play it here.", ["site": site])
        default:
            site.isEmpty
                ? t("Your connection cannot reach where it is. Everybody else goes on watching.")
                : t("Your connection cannot reach {site}. Everybody else goes on watching.", ["site": site])
        }
    }
}

/// The controls under the video: its title, where it is, and the presses
/// that drive the show.
private struct Controls: View {
    let engine: WatchEngine
    @Bindable var player: WatchPlayer
    let playing: Bool
    @Binding var scrubbing: Double?
    let openPanel: () -> Void

    var body: some View {
        let video = player.video
        let length = video.duration > 0 ? video.duration : player.info.duration
        let shown = scrubbing ?? player.position
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(player.info.title)
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)
                Text([t("Added by {name}", ["name": video.by]), player.relayed ? t("Relayed") : nil].compactMap { $0 }.joined(separator: " · "))
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.6))
                    .lineLimit(1)
                    .layoutPriority(1)
            }
            .foregroundStyle(.white)

            if length > 0, !video.live {
                MediaSlider(
                    value: Binding(get: { min(shown, length) }, set: { scrubbing = $0 }),
                    range: 0...length,
                    tint: Theme.tally,
                    editing: { editing in
                        if !editing, let to = scrubbing {
                            engine.seek(to: to)
                            scrubbing = nil
                        }
                    }
                )
                .accessibilityLabel(t("Position in the video"))
            }

            HStack(spacing: 4) {
                StageControl(symbol: playing ? "pause.fill" : "play.fill",
                             label: playing ? t("Pause for everybody") : t("Play for everybody")) {
                    engine.toggle()
                }
                StageControl(symbol: "forward.end.fill", label: t("Next video")) {
                    engine.skip(video.key)
                }
                // The exact position too, for whoever is checking two players
                // against each other, as the web carries it in data-position.
                Text(video.live ? t("Live") : length > 0 ? "\(musicClock(seconds: shown)) / \(musicClock(seconds: length))" : musicClock(seconds: shown))
                    .font(.system(size: 11).monospacedDigit())
                    .foregroundStyle(.white.opacity(0.8))
                    .padding(.horizontal, 4)
                    .accessibilityValue(String(format: "%.2f", player.position))
                Spacer(minLength: 0)
                StageControl(symbol: player.muted || player.volume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill",
                             label: player.muted ? t("Unmute the video") : t("Mute the video")) {
                    player.muted.toggle()
                }
                MediaSlider(value: Binding(get: { player.muted ? 0 : player.volume }, set: { level in
                    player.muted = false
                    player.volume = level
                }), step: 0.05, tint: .white)
                .frame(width: 84)
                .accessibilityLabel(t("Video volume"))
                if player.pictureInPicturePossible || player.pictureInPictureActive {
                    StageControl(symbol: player.pictureInPictureActive ? "pip.exit" : "pip.enter",
                                 label: t("Picture in picture"), lit: player.pictureInPictureActive) {
                        player.togglePictureInPicture()
                    }
                    .transition(Theme.arrive)
                }
                StageControl(symbol: "list.bullet.rectangle", label: t("Videos"), lit: false, action: openPanel)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 32)
        .padding(.bottom, 8)
        .background(LinearGradient(colors: [.clear, .black.opacity(0.8)], startPoint: .top, endPoint: .bottom))
        .animation(Theme.settle, value: player.pictureInPicturePossible)
    }
}

private struct StageControl: View {
    let symbol: String
    let label: String
    var lit = false
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(lit ? Theme.tallyForeground : .white)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: 34, height: 34)
                .background(Circle().fill(lit ? Theme.tally : .white.opacity(hovering ? 0.16 : 0.001)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityLabel(label)
        .help(label)
    }
}

/// The few buttons said over the video: glass capsules, the chosen colour
/// for the one that is the way on.
struct StageButtonStyle: ButtonStyle {
    var prominent = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12.5, weight: .semibold))
            .foregroundStyle(prominent ? Theme.tallyForeground : .white)
            .padding(.horizontal, 14)
            .frame(minHeight: 32)
            .glassSurface(Capsule(), tint: prominent ? Theme.tally : nil, interactive: true)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(Theme.settle, value: configuration.isPressed)
    }
}
