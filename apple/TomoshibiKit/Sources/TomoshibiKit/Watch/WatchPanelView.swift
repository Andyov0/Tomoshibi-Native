import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// Watching a video together: a link to paste, what is playing and coming
/// up, and the media servers where this person may browse them
/// (WatchPanel.tsx, WatchTab.tsx).
///
/// Drawn with the app's own pieces, as the music panel is (Kit.swift,
/// Kit+Media.swift): the web's tab strip is a sliding segmented choice, its
/// field a capsule that lights while it is typed in, and each video a row
/// with its poster.
struct WatchPanelView: View {
    let feature: WatchFeature
    let close: () -> Void
    @State private var tab: Tab = WatchDebug.panelAtMedia ? .media : .queue

    enum Tab: Hashable { case queue, media }

    /// As tall as the music panel, for the reason it gives.
    static let height: CGFloat = MusicPanelView.height

    var body: some View {
        let engine = feature.engine
        VStack(spacing: 0) {
            MediaPanelHeader(title: t("Watch together"), symbol: "play.rectangle.fill",
                             closeLabel: t("Close watch together"), close: close)

            if engine.sealed {
                // The control bar offers no button in a sealed call; should
                // the panel be reached anyway, it says why there is nothing.
                MediaEmpty(symbol: "lock.fill", text: t("Not available in an encrypted call"))
                    .frame(maxHeight: .infinity)
            } else {
                // The tab is there only for somebody the server lists media
                // servers for, which is somebody given them.
                if !engine.servers.isEmpty {
                    SegmentedChoice(options: [(Tab.queue, t("Queue")), (.media, t("Media servers"))], selection: $tab)
                        .padding(.horizontal, 12)
                        .padding(.bottom, 10)
                        .transition(Theme.arrive)
                }
                MediaRule()
                ZStack(alignment: .top) {
                    if tab == .media, !engine.servers.isEmpty {
                        MediaBrowserView(engine: engine, servers: engine.servers) { tab = .queue }
                            .transition(.opacity.animation(Theme.settle))
                    } else {
                        QueueView(engine: engine)
                            .transition(.opacity.animation(Theme.settle))
                    }
                }
                .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        .frame(minHeight: 320, idealHeight: Self.height, maxHeight: Self.height)
        .mediaPanelGlass()
        .animation(Theme.settle, value: engine.servers.isEmpty)
        .animation(Theme.settle, value: tab)
        .onAppear {
            engine.panelOpen = true
            engine.loadServers()
        }
        .onDisappear { engine.panelOpen = false }
        .onChange(of: engine.ready) { engine.loadServers() }
    }
}

/// The box a link is pasted into, and the show's videos.
private struct QueueView: View {
    let engine: WatchEngine
    @State private var pasted = ""
    @State private var busy = false

    var body: some View {
        let show = engine.show
        // Pasting starts the show here when nobody runs one and this person
        // may read links; anybody may add to a show that is running.
        let canAdd = show != nil || engine.ready
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                MediaField(prompt: t("A video link, or the text it was shared in"), text: $pasted, symbol: "link",
                           limit: 2000, url: true, submit: add)
                    .accessibilityLabel(t("Video link"))
                ZStack {
                    if busy {
                        MediaSpinner(size: 16).transition(Theme.arrive)
                    } else {
                        MediaIconButton(symbol: "plus", label: t("Add a video"), size: 34,
                                        prominent: !pasted.trimmingCharacters(in: .whitespaces).isEmpty, action: add)
                            .transition(Theme.arrive)
                    }
                }
                .frame(width: 34, height: 34)
            }
            .disabled(!canAdd)
            .opacity(canAdd ? 1 : 0.5)
            .padding(12)

            if let notice = engine.notice {
                MediaProblem(text: notice)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 10)
                    .onTapGesture { engine.dismissNotice() }
                    .transition(Theme.arrive)
            }
            MediaRule()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    if let show, let now = show.now {
                        VideoRow(engine: engine, video: now, playing: true, remove: nil)
                            .transition(Theme.arrive)
                        if !show.queue.isEmpty {
                            MenuHeading(title: t("Queue")).padding(.top, 6)
                        }
                        ForEach(show.queue, id: \.key) { video in
                            VideoRow(engine: engine, video: video, playing: false) { engine.remove(video.key) }
                                .transition(Theme.arrive)
                        }
                    } else {
                        MediaEmpty(symbol: "film",
                                   text: canAdd
                                       ? t("Paste a link and everybody watches it together, in step.")
                                       : t("When somebody signed in starts a video, it shows up here."))
                            .transition(Theme.arrive)
                    }
                }
                .padding(8)
            }
            .scrollIndicators(.hidden)
            .frame(maxHeight: .infinity)

            if engine.mine {
                MediaRule()
                HStack {
                    Spacer()
                    Button { engine.stop() } label: { Label(t("Stop watching together"), systemImage: "stop.circle") }
                        .buttonStyle(.media(quiet: true))
                }
                .padding(10)
                .transition(Theme.slideUp)
            }
        }
        .animation(Theme.settle, value: engine.notice)
        .animation(Theme.settle, value: engine.mine)
        .animation(Theme.settle, value: show?.now?.key)
        .animation(Theme.settle, value: show?.queue.map(\.key))
        .animation(Theme.settle, value: busy)
        .animation(Theme.settle, value: pasted.isEmpty)
    }

    private func add() {
        let text = pasted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !busy else { return }
        busy = true
        Task {
            if await engine.add(text) { pasted = "" }
            busy = false
        }
    }
}

/// One video in the show. Its title and picture come from the server by the
/// ticket, never from the call.
private struct VideoRow: View {
    let engine: WatchEngine
    let video: WatchVideo
    let playing: Bool
    let remove: (() -> Void)?

    var body: some View {
        let info = engine.info(video.ticket)
        let known = info?.video
        MediaRow(lit: playing) { hovering in
            HStack(spacing: 10) {
                ZStack(alignment: .bottomLeading) {
                    MediaArtwork(url: known?.cover ?? "", symbol: "film", corner: 6)
                    if playing {
                        Image(systemName: "play.fill")
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(Theme.tallyForeground)
                            .frame(width: 16, height: 16)
                            .background(Circle().fill(Theme.tally))
                            .padding(4)
                    }
                }
                .frame(width: 72, height: 40)
                VStack(alignment: .leading, spacing: 2) {
                    Text(known?.title ?? (info == .unavailable
                                          ? t("That video cannot be played here.")
                                          : info == .failed ? t("The video could not be read. Try again.") : "…"))
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(Theme.foreground)
                        .lineLimit(1)
                    Text([playing ? t("Playing") : nil,
                          t("Added by {name}", ["name": video.by]),
                          video.duration > 0 ? musicClock(seconds: video.duration) : nil].compactMap { $0 }.joined(separator: " · "))
                        .font(.system(size: 10.5))
                        .foregroundStyle(playing ? Theme.tallyInk : Theme.muted)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if let remove, hovering || touchScreen {
                    MediaIconButton(symbol: "xmark", label: t("Take {title} off", ["title": known?.title ?? ""]), size: 24, action: remove)
                        .transition(.opacity.animation(Theme.settle))
                }
            }
        }
    }
}

#if os(iOS)
private let touchScreen = true
#else
private let touchScreen = false
#endif
