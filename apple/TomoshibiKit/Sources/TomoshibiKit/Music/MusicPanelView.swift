import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The music panel: the song desk, and the library to choose from
/// (components/room/MusicPanel.tsx).
///
/// Three tabs: the desk, which everybody has; and search and playlists, for
/// whoever can reach the library. Choosing a song adds it to the desk, and
/// starts the desk here if nobody runs one; somebody who cannot reach the
/// library asks the desk instead, by a song's name or a link as it was shared.
///
/// The volume beside what is playing is the listener's own. For somebody
/// listening it is the setting the sound panel keeps for that person's music,
/// so muting here stops it at the media server and the lossless stream is not
/// even asked for; for whoever plays it, it is their own speakers alone, and
/// what everybody else hears is untouched.
///
/// Drawn with the app's own pieces (Kit.swift, Kit+Media.swift): the web's
/// tabs, choices and checkbox are a sliding segmented choice and a switch
/// here, its range inputs the app's slider, and what is playing a card lit
/// by its own cover.
struct MusicPanelView: View {
    let feature: MusicFeature
    let close: () -> Void

    /// About as tall as the web's panel (`h-[min(34rem,...)]`): the room view
    /// lets a panel be as tall as it asks and scrolls it where the window is
    /// shorter, so the panel asks for a height of its own rather than one
    /// its lists would collapse to.
    static let height: CGFloat = 548

    var body: some View {
        let desk = feature.desk
        let tab = desk.libraries == nil ? MusicTab.desk : desk.tab
        VStack(spacing: 0) {
            MediaPanelHeader(title: t("Music"), symbol: "music.note", closeLabel: t("Close music"), close: close)

            if desk.libraries != nil {
                SegmentedChoice(options: [(MusicTab.desk, t("Song desk")), (.search, t("Search")), (.playlist, t("Playlist"))],
                                selection: Binding(get: { desk.tab }, set: { desk.tab = $0 }))
                    .padding(.horizontal, 12)
                    .padding(.bottom, 10)
                    .transition(Theme.arrive)
            }

            if let notice = desk.notice {
                MediaProblem(text: notice)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
                    .transition(Theme.arrive)
            }

            MediaRule()

            ZStack(alignment: .top) {
                switch tab {
                case .desk: DeskTab(desk: desk).transition(.opacity.animation(Theme.settle))
                case .search: SearchTab(desk: desk).transition(.opacity.animation(Theme.settle))
                case .playlist: PlaylistTab(desk: desk).transition(.opacity.animation(Theme.settle))
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .frame(minHeight: 320, idealHeight: Self.height, maxHeight: Self.height)
        .mediaPanelGlass()
        .animation(Theme.settle, value: tab)
        .animation(Theme.settle, value: desk.notice)
        .animation(Theme.settle, value: desk.libraries == nil)
        .onAppear { desk.panelOpen = true }
        .onDisappear { desk.panelOpen = false }
    }
}

// MARK: - The desk

private struct DeskTab: View {
    @Bindable var desk: SongDesk
    @State private var typed = ""

    var body: some View {
        let state = desk.state
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if let state, let now = state.now {
                        NowPlaying(desk: desk, state: state, now: now)
                            .transition(Theme.arrive)
                    } else if let player = desk.player {
                        // Music arriving from somebody whose desk is not heard
                        // here -- in a sealed call desks say nothing -- still
                        // has a volume.
                        HearingRow(name: t("{name} (music)", ["name": player.name.isEmpty ? player.id : player.name]),
                                   setting: desk.hearing(of: player.id),
                                   setVolume: { desk.setVolume(of: player.id, $0) },
                                   setBlocked: { desk.setBlocked(of: player.id, $0) })
                            .padding(12)
                            .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Color.white.opacity(0.05)))
                            .transition(Theme.arrive)
                    } else {
                        Idle(desk: desk)
                            .transition(Theme.arrive)
                    }

                    if let next = state?.next {
                        HStack(spacing: 10) {
                            MediaSpinner(size: 13)
                            Text(t("Loading {title}", ["title": next.title]))
                                .font(.system(size: 12))
                                .foregroundStyle(Theme.muted)
                                .lineLimit(1)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            // Out of the queue already, and still the holder's
                            // to take out: it was on the list a moment ago.
                            if desk.mine {
                                MediaIconButton(symbol: "xmark", label: t("Take {title} off", ["title": next.title]), size: 24) {
                                    desk.remove(next.key)
                                }
                            }
                        }
                        .padding(.horizontal, 10)
                        .transition(Theme.arrive)
                    }

                    if let queue = state?.queue, !queue.isEmpty {
                        VStack(alignment: .leading, spacing: 2) {
                            MenuHeading(title: t("Queue"))
                            ForEach(Array(queue.enumerated()), id: \.element.key) { index, entry in
                                QueueRow(desk: desk, entry: entry, index: index)
                                    .transition(Theme.arrive)
                            }
                        }
                        .transition(Theme.arrive)
                    }
                }
                .padding(12)
                .animation(Theme.settle, value: state)
                .animation(Theme.settle, value: desk.player?.id)
            }
            .scrollIndicators(.hidden)

            MediaRule()
            let canAsk = state != nil || desk.libraries != nil
            HStack(spacing: 8) {
                MediaField(prompt: t("A song's name, or a playlist link"), text: $typed, symbol: "music.note.list", limit: 2000, submit: ask)
                    .accessibilityLabel(t("Ask for a song"))
                ZStack {
                    if desk.asking {
                        MediaSpinner(size: 16).transition(Theme.arrive)
                    } else {
                        MediaIconButton(symbol: "plus", label: t("Ask for a song"), size: 34,
                                        prominent: !typed.trimmingCharacters(in: .whitespaces).isEmpty, action: ask)
                            .transition(Theme.arrive)
                    }
                }
                .frame(width: 34, height: 34)
            }
            .disabled(!canAsk)
            .opacity(canAsk ? 1 : 0.5)
            .padding(12)
            .animation(Theme.settle, value: desk.asking)
            .animation(Theme.settle, value: typed.isEmpty)
        }
    }

    private func ask() {
        // Kept while the last ask is still being looked up, as the web's box
        // refuses a second one then.
        guard !typed.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, !desk.asking else { return }
        desk.ask(typed)
        typed = ""
    }
}

/// What is playing, on a card lit from behind by its own cover.
private struct NowPlaying: View {
    @Bindable var desk: SongDesk
    let state: DeskState
    let now: DeskEntry

    var body: some View {
        let voted = state.votes.contains(desk.call.localIdentity)
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center, spacing: 12) {
                MediaArtwork(url: now.cover, corner: 10)
                    .frame(width: 66, height: 66)
                    .shadow(color: .black.opacity(0.4), radius: 10, y: 5)
                VStack(alignment: .leading, spacing: 3) {
                    Text(now.title)
                        .font(.system(size: 14.5, weight: .semibold))
                        .foregroundStyle(Theme.foreground)
                        .lineLimit(2)
                    if !now.artists.isEmpty {
                        Text(now.artists.joined(separator: " / "))
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.foreground.opacity(0.72))
                            .lineLimit(1)
                    }
                    Text(t("Asked for by {name}", ["name": now.by]))
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.muted)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            if state.paused || state.quality != nil {
                HStack(spacing: 6) {
                    if state.paused {
                        MediaBadge(text: t("Paused"), symbol: "pause.fill").transition(Theme.arrive)
                    }
                    if let quality = state.quality {
                        MediaBadge(text: quality, symbol: "waveform", lit: true)
                            .transition(Theme.arrive)
                            .id(quality)
                    }
                }
                .transition(Theme.arrive)
            }

            if desk.mine {
                Position(desk: desk)
                    .transition(Theme.arrive)
                HStack(spacing: 8) {
                    Image(systemName: desk.monitorVolume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Theme.muted)
                        .frame(width: 18)
                        .contentTransition(.symbolEffect(.replace))
                    Text(t("In your speakers"))
                        .font(.system(size: 11.5))
                        .foregroundStyle(Theme.muted)
                        .fixedSize()
                    MediaSlider(value: Binding(get: { Double(desk.monitorVolume) }, set: { desk.monitorVolume = Float($0) }), step: 0.05)
                        .accessibilityLabel(t("Music volume in your speakers"))
                    Text("\(Int((desk.monitorVolume * 100).rounded()))%")
                        .font(.system(size: 11, weight: .medium).monospacedDigit())
                        .foregroundStyle(Theme.muted)
                        .frame(width: 38, alignment: .trailing)
                }
                .transition(Theme.arrive)
            } else {
                HearingRow(name: t("Music"), setting: desk.hearing(of: state.holder),
                           setVolume: { desk.setVolume(of: state.holder, $0) },
                           setBlocked: { desk.setBlocked(of: state.holder, $0) })
                    .transition(Theme.arrive)
            }

            // This participant's own, like the volume above it: see duck.ts.
            HStack(spacing: 8) {
                Image(systemName: "person.wave.2.fill")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Theme.muted)
                    .frame(width: 18)
                Text(t("Lower the music while anybody talks"))
                    .font(.system(size: 11.5))
                    .foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 6)
                // Unlabelled, as MenuSwitch's is: the app's switch draws
                // whatever label it is given beside it, and the words are
                // already there.
                Toggle("", isOn: $desk.ducks)
                    .toggleStyle(.glassSwitch)
                    .scaleEffect(0.85)
                    .accessibilityLabel(t("Lower the music while anybody talks"))
            }

            HStack(spacing: 8) {
                if desk.mine {
                    MediaIconButton(symbol: state.paused ? "play.fill" : "pause.fill",
                                    label: state.paused ? t("Play") : t("Pause"), size: 34, prominent: true) {
                        desk.pause(!state.paused)
                    }
                    Button { desk.skip() } label: {
                        Label(t("Next song"), systemImage: "forward.fill").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.media(height: 34))
                    // A round button with its words on hover and for
                    // VoiceOver: written out, they took the room the next
                    // song's button needs and were cut short.
                    MediaIconButton(symbol: "stop.circle", label: t("Stop the desk"), size: 34) { desk.close() }
                } else {
                    // Not while the next is on its way: the desk does not
                    // count those votes (jukebox.ts `Desk.vote`).
                    Button { desk.vote() } label: {
                        Label(t("Vote to skip ({votes}/{needed})", ["votes": state.votes.count, "needed": state.needed]),
                              systemImage: voted ? "checkmark" : "forward.fill")
                            .frame(maxWidth: .infinity)
                            .contentTransition(.numericText())
                    }
                    .buttonStyle(.media(height: 34))
                    .disabled(voted || state.loading)
                }
            }
        }
        .padding(14)
        .background {
            ZStack {
                MediaArtwork(url: now.cover, corner: 0)
                    .scaleEffect(1.6)
                    .blur(radius: 36)
                    .opacity(0.55)
                LinearGradient(colors: [Color.black.opacity(0.15), Color.black.opacity(0.45)], startPoint: .top, endPoint: .bottom)
            }
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.09)))
        .animation(Theme.settle, value: desk.mine)
        .animation(Theme.settle, value: state.paused)
        // The badge saying what the file is arrives once the song is
        // decoded, and changes with the next song: it popped in, alone of
        // everything on this card.
        .animation(Theme.settle, value: state.quality)
    }
}

/// Where the song playing from here is: only the holder knows, and the web
/// shows nobody, but a holder with a queue wants to see how long is left.
private struct Position: View {
    let desk: SongDesk

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.5)) { _ in
            let progress = desk.progress.flatMap { $0.of > 0 ? $0 : nil }
            // Arrives when the song begins, and goes with it, animated: read
            // afresh on every tick, it appeared and disappeared in one frame.
            VStack(spacing: 0) {
                if let progress {
                    VStack(spacing: 4) {
                        MediaProgress(fraction: progress.at / progress.of)
                        HStack {
                            Text(musicClock(seconds: progress.at))
                            Spacer()
                            Text(musicClock(seconds: progress.of))
                        }
                        .font(.system(size: 10, weight: .medium).monospacedDigit())
                        .foregroundStyle(Theme.muted)
                    }
                    .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: progress != nil)
        }
    }
}

/// Nothing playing: what to do about it, and for whoever runs an idle desk,
/// the way to put it away (it still tells the room it is there every
/// fifteen seconds).
private struct Idle: View {
    @Bindable var desk: SongDesk

    var body: some View {
        VStack(spacing: 4) {
            MediaEmpty(symbol: "music.note.list",
                       text: desk.state != nil ? t("Nothing is playing. Ask for a song below.")
                           : desk.libraries != nil ? t("Search a song or paste a playlist link to start the song desk.")
                           : t("When somebody plays music in this call, it shows up here."))
            if desk.mine {
                Button { desk.close() } label: { Label(t("Stop the desk"), systemImage: "stop.circle") }
                    .buttonStyle(.media(quiet: true))
                    .transition(Theme.arrive)
            }
        }
        .frame(maxWidth: .infinity)
        .animation(Theme.settle, value: desk.mine)
    }
}

private struct QueueRow: View {
    @Bindable var desk: SongDesk
    let entry: DeskEntry
    let index: Int

    var body: some View {
        MediaRow { hovering in
            HStack(spacing: 10) {
                Text("\(index + 1)")
                    .font(.system(size: 11, weight: .medium).monospacedDigit())
                    .foregroundStyle(Theme.muted)
                    .frame(width: 16, alignment: .trailing)
                MediaArtwork(url: entry.cover, corner: 6)
                    .frame(width: 34, height: 34)
                VStack(alignment: .leading, spacing: 1) {
                    Text(entry.title).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Theme.foreground).lineLimit(1)
                    Text([entry.artists.joined(separator: " / "), t("Asked for by {name}", ["name": entry.by])]
                        .filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(.system(size: 10.5)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                // The holder's, and only under the pointer on a Mac, where a
                // row of buttons on every song is a list that cannot be read;
                // always on a touch screen, which has no pointer to bring
                // them.
                if desk.mine, hovering || touchScreen {
                    HStack(spacing: 2) {
                        if index > 0 {
                            MediaIconButton(symbol: "arrow.up.to.line", label: t("Play {title} next", ["title": entry.title]), size: 24) {
                                desk.toTop(entry.key)
                            }
                        }
                        MediaIconButton(symbol: "xmark", label: t("Take {title} off", ["title": entry.title]), size: 24) {
                            desk.remove(entry.key)
                        }
                    }
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

// MARK: - Searching the library

private struct SearchTab: View {
    @Bindable var desk: SongDesk

    var body: some View {
        let libraries = desk.libraries ?? []
        let library = libraries.first { $0.id == desk.searchSource }
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                if libraries.count > 1 {
                    SegmentedChoice(options: libraries.map { ($0.id, $0.name) },
                                    selection: Binding(get: { desk.searchSource }, set: { desk.chooseLibrary($0) }), height: 28)
                }
                HStack(spacing: 10) {
                    Text(t("Quality")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                    SegmentedChoice(options: [(MusicQuality.lossless, t("Lossless")), (.best, t("Highest"))],
                                    selection: $desk.quality, height: 28)
                }
                if library.map({ !$0.signedIn }) == true || desk.quality == .best {
                    VStack(alignment: .leading, spacing: 4) {
                        if let library, !library.signedIn {
                            Label(t("Not signed in, so at most 320 kbps."), systemImage: "info.circle")
                                .transition(Theme.arrive)
                        }
                        if desk.quality == .best {
                            Label(t("Hi-Res and masters are large, and take longer to start."), systemImage: "info.circle")
                                .fixedSize(horizontal: false, vertical: true)
                                .transition(Theme.arrive)
                        }
                    }
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.muted)
                    .transition(Theme.arrive)
                }
                HStack(spacing: 8) {
                    MediaField(prompt: t("Search songs"), text: $desk.searchQuery, symbol: "magnifyingglass", limit: 100) {
                        Task { await desk.searchLibrary() }
                    }
                    .accessibilityLabel(t("Search songs"))
                    ZStack {
                        if desk.searching {
                            MediaSpinner(size: 16).transition(Theme.arrive)
                        } else {
                            MediaIconButton(symbol: "arrow.right", label: t("Search"), size: 34,
                                            prominent: !desk.searchQuery.trimmingCharacters(in: .whitespaces).isEmpty) {
                                Task { await desk.searchLibrary() }
                            }
                            .transition(Theme.arrive)
                        }
                    }
                    .frame(width: 34, height: 34)
                }
            }
            .padding(12)
            .animation(Theme.settle, value: desk.quality)
            .animation(Theme.settle, value: desk.searching)
            MediaRule()

            ScrollView {
                LazyVStack(spacing: 2) {
                    if let problem = desk.searchProblem {
                        MediaProblem(text: problem).padding(.bottom, 6).transition(Theme.arrive)
                    }
                    if let results = desk.searchResults {
                        if results.isEmpty {
                            MediaEmpty(symbol: "magnifyingglass", text: t("Nothing found.")).transition(Theme.arrive)
                        } else {
                            ForEach(results, id: \.id) { track in
                                TrackRow(track: track, added: desk.searchAdded.contains(track.id)) {
                                    desk.add([SongDesk.song(track, from: desk.searchSource)])
                                    desk.searchAdded.insert(track.id)
                                }
                                .transition(Theme.arrive)
                            }
                        }
                    } else {
                        MediaEmpty(symbol: "music.magnifyingglass", text: t("Search for a song to play into the call."))
                            .transition(Theme.arrive)
                    }
                }
                .padding(8)
                .animation(Theme.settle, value: desk.searchResults)
                .animation(Theme.settle, value: desk.searchProblem)
            }
            .scrollIndicators(.hidden)
        }
    }
}

// MARK: - A playlist or song link

private struct PlaylistTab: View {
    @Bindable var desk: SongDesk

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                MediaField(prompt: t("Paste a playlist or song link"), text: $desk.pasted, symbol: "link", limit: 2000, url: true) {
                    Task { await desk.openLink() }
                }
                .accessibilityLabel(t("Playlist link"))
                Button { Task { await desk.openLink() } } label: {
                    ZStack {
                        Text(t("Open")).opacity(desk.reading ? 0 : 1)
                        if desk.reading { MediaSpinner(size: 13, tint: Theme.tallyForeground).transition(Theme.arrive) }
                    }
                }
                .buttonStyle(.media(prominent: true, height: 34))
            }
            .padding(12)
            .animation(Theme.settle, value: desk.reading)
            MediaRule()

            ScrollView {
                LazyVStack(spacing: 2) {
                    if let problem = desk.linkProblem {
                        MediaProblem(text: problem).padding(.bottom, 6).transition(Theme.arrive)
                    }
                    if let found = desk.linked {
                        HStack(spacing: 10) {
                            MediaArtwork(url: found.tracks.first?.cover ?? "", symbol: "music.note.list", corner: 8)
                                .frame(width: 44, height: 44)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(found.title).font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.foreground).lineLimit(2)
                                Text(t("{count} songs", ["count": found.tracks.count])).font(.system(size: 11)).foregroundStyle(Theme.muted)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Button(t("Add all")) {
                                desk.add(found.tracks.map { SongDesk.song($0, from: found.source) })
                                desk.linkAdded = Set(found.tracks.map(\.id))
                            }
                            .buttonStyle(.media(prominent: true))
                            .disabled(found.tracks.isEmpty)
                        }
                        .padding(10)
                        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.05)))
                        .padding(.bottom, 6)
                        .transition(Theme.arrive)
                        ForEach(found.tracks, id: \.id) { track in
                            TrackRow(track: track, added: desk.linkAdded.contains(track.id)) {
                                desk.add([SongDesk.song(track, from: found.source)])
                                desk.linkAdded.insert(track.id)
                            }
                            .transition(Theme.arrive)
                        }
                    } else if desk.linkProblem == nil {
                        MediaEmpty(symbol: "link", text: t("Paste a link shared from a music app.")).transition(Theme.arrive)
                    }
                }
                .padding(8)
                .animation(Theme.settle, value: desk.linked)
                .animation(Theme.settle, value: desk.linkProblem)
            }
            .scrollIndicators(.hidden)
        }
    }
}

// MARK: - Pieces

/// A song from the library: pressing it adds it to the desk.
private struct TrackRow: View {
    let track: TomoshibiCore.MusicTrack
    let added: Bool
    let add: () -> Void

    var body: some View {
        Button(action: add) {
            MediaRow { hovering in
                HStack(spacing: 10) {
                    MediaArtwork(url: track.cover, corner: 7)
                        .frame(width: 40, height: 40)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(track.title).font(.system(size: 13, weight: .medium)).foregroundStyle(Theme.foreground).lineLimit(1)
                        Text([track.artists.joined(separator: " / "), track.album].filter { !$0.isEmpty }.joined(separator: " · "))
                            .font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    ZStack(alignment: .trailing) {
                        if added {
                            MediaBadge(text: t("Added"), symbol: "checkmark", lit: true).transition(Theme.arrive)
                        } else if hovering {
                            Image(systemName: "plus.circle.fill")
                                .font(.system(size: 17))
                                .foregroundStyle(Theme.tally)
                                .transition(Theme.arrive)
                        } else {
                            Text(musicClock(seconds: track.duration))
                                .font(.system(size: 11, weight: .medium).monospacedDigit())
                                .foregroundStyle(Theme.muted)
                                .transition(.opacity.animation(Theme.settle))
                        }
                    }
                    .frame(minWidth: 40, alignment: .trailing)
                    .animation(Theme.settle, value: added)
                    .animation(Theme.settle, value: hovering)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(t("Add {title}", ["title": track.title]))
    }
}
