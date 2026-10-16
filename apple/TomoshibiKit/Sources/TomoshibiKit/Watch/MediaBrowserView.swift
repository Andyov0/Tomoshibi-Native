import SwiftUI
import TomoshibiCore

/// The deployment's media servers, browsed: their libraries, what is in them,
/// and a search, for somebody given them choosing what everybody will watch
/// (MediaBrowser.tsx).
///
/// Choosing something hands its link to the show the way a pasted one is, so
/// it is read here under this person's grant even in somebody else's show: a
/// film from a media server is relayed for everybody on the chooser's grant,
/// and a link the holder read for them would be refused as somebody else's.
struct MediaBrowserView: View {
    let engine: WatchEngine
    let servers: [MediaServer]
    /// Something was added: back to the queue, where it now is.
    let chose: () -> Void

    @State private var server = ""
    /// Where in the server: a trail of folders, the first being the server's
    /// libraries.
    @State private var trail: [(id: String, name: String)] = []
    @State private var items: [MediaItem]?
    @State private var loading = false
    @State private var failed = false
    @State private var words = ""
    @State private var searched: String?
    @State private var adding: String?

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                if servers.count > 1 {
                    SegmentedChoice(options: servers.map { ($0.key, $0.name) },
                                    selection: Binding(get: { server }, set: { chosen in
                                        server = chosen
                                        trail = []
                                        searched = nil
                                    }), height: 28)
                }
                HStack(spacing: 8) {
                    MediaField(prompt: t("Search films and series"), text: $words, symbol: "magnifyingglass", limit: 100, submit: search)
                        .accessibilityLabel(t("Search films and series"))
                    MediaIconButton(symbol: "arrow.right", label: t("Search"), size: 34,
                                    prominent: !words.trimmingCharacters(in: .whitespaces).isEmpty, action: search)
                }
                if !trail.isEmpty || searched != nil {
                    Button(action: back) {
                        Label(searched.map { t("Results for {words}", ["words": $0]) } ?? trail.last?.name ?? "", systemImage: "chevron.left")
                            .lineLimit(1)
                    }
                    .buttonStyle(.media(quiet: true, height: 26))
                    .transition(Theme.arrive)
                }
            }
            .padding(12)
            MediaRule()

            ZStack(alignment: .top) {
                if loading {
                    MediaSpinner(size: 20).padding(.vertical, 40).transition(Theme.arrive)
                } else if failed {
                    MediaEmpty(symbol: "externaldrive.badge.exclamationmark", text: t("The media server did not answer. Try again."))
                        .transition(Theme.arrive)
                } else if let items, items.isEmpty {
                    MediaEmpty(symbol: "magnifyingglass", text: t("Nothing found.")).transition(Theme.arrive)
                } else {
                    ScrollView {
                        LazyVStack(spacing: 2) {
                            ForEach(items ?? [], id: \.id) { item in
                                ItemRow(engine: engine, item: item, adding: adding == item.id) { open(item) }
                            }
                        }
                        .padding(8)
                    }
                    .scrollIndicators(.hidden)
                    .transition(.opacity.animation(Theme.settle))
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .animation(Theme.settle, value: loading)
        .animation(Theme.settle, value: failed)
        .animation(Theme.settle, value: trail.count)
        .animation(Theme.settle, value: searched)
        .onAppear { if server.isEmpty { server = servers.first?.key ?? "" } }
        .task(id: "\(server)/\(trail.last?.id ?? "")/\(searched == nil)") { await browse() }
    }

    private func browse() async {
        guard !server.isEmpty, searched == nil else { return }
        loading = true
        failed = false
        let found = try? await engine.deployment.browseMedia(server: server, parent: trail.last?.id)
        guard !Task.isCancelled else { return }
        items = found
        failed = found == nil
        loading = false
    }

    private func search() {
        let asked = words.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty else {
            searched = nil
            return
        }
        loading = true
        failed = false
        Task {
            let found = try? await engine.deployment.searchMedia(server: server, words: asked)
            items = found
            failed = found == nil
            searched = asked
            loading = false
        }
    }

    private func open(_ item: MediaItem) {
        if item.playable {
            adding = item.id
            Task {
                let added = await engine.add(watchMediaLink(server: server, item: item.id))
                adding = nil
                if added { chose() }
            }
        } else if item.folder {
            searched = nil
            trail.append((item.id, item.name))
        }
    }

    private func back() {
        if searched != nil {
            searched = nil
            words = ""
        } else if !trail.isEmpty {
            trail.removeLast()
        }
    }
}

private struct ItemRow: View {
    let engine: WatchEngine
    let item: MediaItem
    let adding: Bool
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            MediaRow { hovering in
                HStack(spacing: 10) {
                    ZStack {
                        MediaArtwork(url: "", symbol: item.folder ? "folder.fill" : "play.fill", corner: 6)
                        if !item.image.isEmpty {
                            Poster(engine: engine, url: item.image)
                        }
                    }
                    .frame(width: 34, height: 50)
                    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item.name).font(.system(size: 13, weight: .medium)).foregroundStyle(Theme.foreground).lineLimit(1)
                        Text([item.year > 0 ? String(item.year) : nil, item.duration > 0 ? musicClock(seconds: item.duration) : nil]
                            .compactMap { $0 }.joined(separator: " · "))
                            .font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    ZStack {
                        if adding {
                            MediaSpinner(size: 14).transition(Theme.arrive)
                        } else if hovering, item.playable || item.folder {
                            Image(systemName: item.folder ? "chevron.right" : "play.circle.fill")
                                .font(.system(size: item.folder ? 12 : 17, weight: .semibold))
                                .foregroundStyle(item.folder ? Theme.muted : Theme.tally)
                                .transition(Theme.arrive)
                        }
                    }
                    .animation(Theme.settle, value: hovering)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(!item.playable && !item.folder)
        .opacity(!item.playable && !item.folder ? 0.5 : 1)
        .accessibilityLabel(item.playable ? t("Watch {title}", ["title": item.name]) : item.name)
        .animation(Theme.settle, value: adding)
    }
}

/// A media server's poster, which the deployment fetches with its account
/// and hands only to somebody whose session may browse: so it is asked for
/// with this app's session cookie, which an image view on its own would not
/// send.
private struct Poster: View {
    let engine: WatchEngine
    let url: String
    @State private var image: Image?

    var body: some View {
        ZStack {
            if let image {
                image.resizable().aspectRatio(contentMode: .fill).transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: image != nil)
        .task(id: url) {
            guard let address = URL(string: url) else { return }
            var request = URLRequest(url: address)
            if let cookie = engine.deployment.cookieHeader() { request.setValue(cookie, forHTTPHeaderField: "Cookie") }
            request.httpShouldHandleCookies = false
            guard let (data, response) = try? await URLSession.shared.data(for: request),
                  (response as? HTTPURLResponse)?.statusCode == 200 else { return }
            #if os(macOS)
            if let picture = NSImage(data: data) { image = Image(nsImage: picture) }
            #else
            if let picture = UIImage(data: data) { image = Image(uiImage: picture) }
            #endif
        }
    }
}
