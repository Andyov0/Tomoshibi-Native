import SwiftUI
import TomoshibiCore

/// Watching a video together: a link to paste, what is playing and coming
/// up, and the media servers where this person may browse them
/// (WatchPanel.tsx, WatchTab.tsx).
struct WatchPanelView: View {
    let feature: WatchFeature
    let close: () -> Void
    @State private var tab: Tab = WatchDebug.panelAtMedia ? .media : .queue

    enum Tab { case queue, media }

    var body: some View {
        let engine = feature.engine
        VStack(spacing: 0) {
            HStack {
                Text(t("Watch together")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close watch together"))
                .help(t("Close watch together"))
            }
            .padding(12)
            Divider().overlay(Theme.border)

            if engine.call.sealed {
                // The control bar offers no button in a sealed call; should
                // the panel be reached anyway, it says why there is nothing.
                Text(t("Not available in an encrypted call"))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(16)
            } else {
                // The tab is there only for somebody the server lists media
                // servers for, which is somebody given them.
                if !engine.servers.isEmpty {
                    HStack(spacing: 4) {
                        TabButton(title: t("Queue"), chosen: tab == .queue) { tab = .queue }
                        TabButton(title: t("Media servers"), chosen: tab == .media) { tab = .media }
                    }
                    .padding(6)
                    .transition(Theme.arrive)
                    Divider().overlay(Theme.border)
                }
                Group {
                    if tab == .media, !engine.servers.isEmpty {
                        MediaBrowserView(engine: engine, servers: engine.servers) { tab = .queue }
                    } else {
                        QueueView(engine: engine)
                    }
                }
                .frame(maxHeight: .infinity)
            }
        }
        .glassPane(cornerRadius: 22)
        .clipShape(RoundedRectangle(cornerRadius: 12))
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

private struct TabButton: View {
    let title: String
    let chosen: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12))
                .foregroundStyle(chosen ? Theme.foreground : Theme.muted)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 5)
                .background(RoundedRectangle(cornerRadius: 6).fill(chosen ? Theme.surfaceHigh : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(chosen ? .isSelected : [])
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
            HStack(spacing: 6) {
                TextField(t("A video link, or the text it was shared in"), text: $pasted)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .padding(.horizontal, 10).padding(.vertical, 7)
                    .background(RoundedRectangle(cornerRadius: 7).fill(Theme.surfaceHigh))
                    .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Theme.border))
                    .accessibilityLabel(t("Video link"))
                    .onSubmit(add)
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    #endif
                    .onChange(of: pasted) {
                        // As long as the web's field allows.
                        if pasted.utf16.count > 2000 { pasted = WatchWords.cut(pasted, 2000) }
                    }
                Button(action: add) {
                    ZStack {
                        if busy {
                            ProgressView().controlSize(.small).transition(Theme.arrive)
                        } else {
                            Image(systemName: "plus").font(.system(size: 12, weight: .bold)).transition(Theme.arrive)
                        }
                    }
                    .foregroundStyle(Theme.foreground)
                    .frame(width: 30, height: 30)
                    .background(RoundedRectangle(cornerRadius: 7).fill(Theme.surfaceHigh))
                    .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Theme.border))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Add a video"))
                .help(t("Add a video"))
            }
            .disabled(!canAdd)
            .opacity(canAdd ? 1 : 0.5)
            .padding(8)

            if let notice = engine.notice {
                Text(notice)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.danger)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 10).padding(.bottom, 8)
                    .onTapGesture { engine.dismissNotice() }
                    .transition(Theme.arrive)
            }
            Divider().overlay(Theme.border)

            ScrollView {
                LazyVStack(spacing: 2) {
                    if let show, let now = show.now {
                        VideoRow(engine: engine, video: now, playing: true, remove: nil)
                            .transition(Theme.arrive)
                        ForEach(show.queue, id: \.key) { video in
                            VideoRow(engine: engine, video: video, playing: false) { engine.remove(video.key) }
                                .transition(Theme.arrive)
                        }
                    } else {
                        VStack(spacing: 8) {
                            Image(systemName: "film").font(.system(size: 20))
                            Text(canAdd
                                 ? t("Paste a link and everybody watches it together, in step.")
                                 : t("When somebody signed in starts a video, it shows up here."))
                                .font(.system(size: 12))
                                .multilineTextAlignment(.center)
                        }
                        .foregroundStyle(Theme.muted)
                        .padding(.horizontal, 16).padding(.vertical, 32)
                        .transition(Theme.arrive)
                    }
                }
                .padding(6)
            }
            .frame(maxHeight: .infinity)

            if engine.mine {
                Divider().overlay(Theme.border)
                HStack {
                    Spacer()
                    Button(t("Stop watching together")) { engine.stop() }
                        .buttonStyle(.plain)
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.muted)
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
        HStack(spacing: 10) {
            Cover(url: known?.cover ?? "")
                .frame(width: 64, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(known?.title ?? (info == .unavailable
                                      ? t("That video cannot be played here.")
                                      : info == .failed ? t("The video could not be read. Try again.") : "…"))
                    .font(.system(size: 12.5))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(1)
                Text([playing ? t("Playing") : nil,
                      t("Added by {name}", ["name": video.by]),
                      video.duration > 0 ? musicClock(seconds: video.duration) : nil].compactMap { $0 }.joined(separator: " · "))
                    .font(.system(size: 10.5))
                    .foregroundStyle(Theme.muted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if let remove {
                Button(action: remove) {
                    Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.muted)
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Take {title} off", ["title": known?.title ?? ""]))
                .help(t("Take {title} off", ["title": known?.title ?? ""]))
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 7).fill(playing ? Theme.surfaceHigh.opacity(0.6) : .clear))
    }
}

/// A poster: one relayed by the deployment by its ticket, which anybody in
/// the call may fetch, or one on the open web.
private struct Cover: View {
    let url: String

    var body: some View {
        ZStack {
            Theme.surfaceHigh
            if let address = URL(string: url), !url.isEmpty {
                AsyncImage(url: address) { image in
                    image.resizable().aspectRatio(contentMode: .fill).transition(Theme.arrive)
                } placeholder: {
                    Color.clear
                }
            }
        }
    }
}
