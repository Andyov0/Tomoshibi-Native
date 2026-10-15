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
/// The volume beside what is playing is the listener's own. For whoever plays
/// it, it is their own speakers alone, and what everybody else hears is
/// untouched.
struct MusicPanelView: View {
    let feature: MusicFeature
    let close: () -> Void

    var body: some View {
        let desk = feature.desk
        let tab = desk.libraries == nil ? MusicTab.desk : desk.tab
        VStack(spacing: 0) {
            HStack {
                Text(t("Music")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close music"))
                .help(t("Close music"))
            }
            .padding(.leading, 12).padding(.trailing, 8).padding(.vertical, 8)
            Divider().overlay(Theme.border)

            if desk.libraries != nil {
                Tabs(desk: desk)
                    .transition(Theme.arrive)
                Divider().overlay(Theme.border)
            }

            if let notice = desk.notice {
                Text(notice)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.danger)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .transition(Theme.arrive)
            }

            Group {
                switch tab {
                case .desk: DeskTab(desk: desk)
                case .search: SearchTab(desk: desk)
                case .playlist: PlaylistTab(desk: desk)
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .id(tab)
            .transition(.opacity.animation(Theme.settle))
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .animation(Theme.settle, value: tab)
        .animation(Theme.settle, value: desk.notice)
        .animation(Theme.settle, value: desk.libraries == nil)
    }
}

// MARK: - Tabs

private struct Tabs: View {
    @Bindable var desk: SongDesk

    var body: some View {
        HStack(spacing: 4) {
            tab(.desk, t("Song desk"))
            tab(.search, t("Search"))
            tab(.playlist, t("Playlist"))
        }
        .padding(6)
    }

    private func tab(_ which: MusicTab, _ label: String) -> some View {
        let chosen = desk.tab == which
        return Button { desk.tab = which } label: {
            Text(label)
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

// MARK: - The desk

private struct DeskTab: View {
    @Bindable var desk: SongDesk
    @State private var typed = ""

    var body: some View {
        let state = desk.state
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let state, let now = state.now {
                        NowPlaying(desk: desk, state: state, now: now)
                            .transition(Theme.arrive)
                    } else if let player = desk.player {
                        VolumeRow(name: t("{name} (music)", ["name": player.name]),
                                  volume: Binding(get: { desk.volume(of: player.id) }, set: { desk.setVolume(of: player.id, $0) }))
                            .padding(6)
                            .transition(Theme.arrive)
                        Divider().overlay(Theme.border)
                    } else {
                        Empty(desk: desk)
                            .transition(Theme.arrive)
                    }

                    if let next = state?.next {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small).scaleEffect(0.7).frame(width: 14, height: 14)
                            Text(t("Loading {title}", ["title": next.title]))
                                .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                                .lineLimit(1).truncationMode(.tail)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            // Out of the queue already, and still the holder's
                            // to take out: it was on the list a moment ago.
                            if desk.mine {
                                SmallIcon(symbol: "xmark", label: t("Take {title} off", ["title": next.title])) { desk.remove(next.key) }
                            }
                        }
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .transition(Theme.arrive)
                    }

                    if let queue = state?.queue, !queue.isEmpty {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(queue.enumerated()), id: \.element.key) { index, entry in
                                QueueRow(desk: desk, entry: entry, index: index)
                                    .transition(Theme.arrive)
                            }
                        }
                        .padding(6)
                    }
                }
                .animation(Theme.settle, value: state)
            }

            Divider().overlay(Theme.border)
            let canAsk = state != nil || desk.libraries != nil
            HStack(spacing: 6) {
                TextField(t("A song's name, or a playlist link"), text: $typed)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .padding(.horizontal, 10).padding(.vertical, 7)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
                    .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
                    .accessibilityLabel(t("Ask for a song"))
                    .onSubmit(ask)
                Button(action: ask) {
                    ZStack {
                        if desk.asking {
                            ProgressView().controlSize(.small).scaleEffect(0.7).transition(.opacity)
                        } else {
                            Image(systemName: "plus").font(.system(size: 12, weight: .semibold)).transition(.opacity)
                        }
                    }
                    .foregroundStyle(Theme.foreground)
                    .frame(width: 32, height: 32)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Ask for a song"))
                .help(t("Ask for a song"))
            }
            .disabled(!canAsk)
            .opacity(canAsk ? 1 : 0.5)
            .padding(8)
            .animation(Theme.settle, value: desk.asking)
        }
    }

    private func ask() {
        let text = String(typed.prefix(2000))
        // Kept while the last ask is still being looked up, as the web's box
        // refuses a second one then.
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, !desk.asking else { return }
        desk.ask(text)
        typed = ""
    }
}

private struct NowPlaying: View {
    @Bindable var desk: SongDesk
    let state: DeskState
    let now: DeskEntry

    var body: some View {
        let voted = state.votes.contains(desk.call.localIdentity)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Cover(url: now.cover, size: 44)
                VStack(alignment: .leading, spacing: 2) {
                    Text(now.title).font(.system(size: 13, weight: .medium)).foregroundStyle(Theme.foreground).lineLimit(1)
                    Text(now.artists.joined(separator: " / ")).font(.system(size: 11.5)).foregroundStyle(Theme.muted).lineLimit(1)
                    Text([state.paused ? t("Paused") : nil, t("Asked for by {name}", ["name": now.by]), state.quality]
                        .compactMap { $0 }.joined(separator: " · "))
                        .font(.system(size: 10.5)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            if desk.mine {
                HStack(spacing: 8) {
                    Text(t("In your speakers")).font(.system(size: 11)).foregroundStyle(Theme.muted).fixedSize()
                    // Stepless: a stepped slider on the Mac draws a tick for
                    // every step, twenty dots under a hairline. The web's
                    // range input moves in twentieths, which the setter keeps.
                    Slider(value: Binding(get: { desk.monitorVolume }, set: { desk.monitorVolume = ($0 * 20).rounded() / 20 }), in: 0...1)
                        .controlSize(.mini)
                        .tint(Theme.foreground)
                        .accessibilityLabel(t("Music volume in your speakers"))
                    Text("\(Int((desk.monitorVolume * 100).rounded()))%")
                        .font(.system(size: 11).monospacedDigit()).foregroundStyle(Theme.muted)
                        .frame(width: 36, alignment: .trailing)
                }
                .padding(.horizontal, 6)
                .transition(Theme.arrive)
            } else {
                VolumeRow(name: t("Music"), volume: Binding(get: { desk.volume(of: state.holder) },
                                                             set: { desk.setVolume(of: state.holder, $0) }))
                    .transition(Theme.arrive)
            }

            // This participant's own, like the volume above it: see duck.ts.
            Check(on: $desk.ducks, label: t("Lower the music while anybody talks"))
                .padding(.horizontal, 6)

            HStack(spacing: 6) {
                if desk.mine {
                    Pill(symbol: state.paused ? "play.fill" : "pause.fill", text: nil,
                         label: state.paused ? t("Play") : t("Pause")) { desk.pause(!state.paused) }
                    Pill(symbol: "forward.fill", text: t("Next song"), wide: true) { desk.skip() }
                    Pill(symbol: "stop.circle", text: t("Stop the desk"), ghost: true) { desk.close() }
                } else {
                    // Not while the next is on its way: the desk does not
                    // count those votes (jukebox.ts `Desk.vote`).
                    Pill(symbol: "forward.fill",
                         text: t("Vote to skip ({votes}/{needed})", ["votes": state.votes.count, "needed": state.needed]),
                         wide: true) { desk.vote() }
                        .disabled(voted || state.loading)
                        .opacity(voted || state.loading ? 0.5 : 1)
                }
            }
        }
        .padding(12)
        .overlay(alignment: .bottom) { Divider().overlay(Theme.border) }
        .animation(Theme.settle, value: desk.mine)
    }
}

private struct Empty: View {
    @Bindable var desk: SongDesk

    var body: some View {
        VStack(spacing: 10) {
            Text(desk.state != nil ? t("Nothing is playing. Ask for a song below.")
                 : desk.libraries != nil ? t("Search a song or paste a playlist link to start the song desk.")
                 : t("When somebody plays music in this call, it shows up here."))
                .font(.system(size: 12)).foregroundStyle(Theme.muted)
                .multilineTextAlignment(.center)
            // An idle desk still tells the room it is there every fifteen
            // seconds; whoever runs it can put it away.
            if desk.mine {
                Pill(symbol: "stop.circle", text: t("Stop the desk"), ghost: true) { desk.close() }
                    .transition(Theme.arrive)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 16).padding(.vertical, 24)
    }
}

private struct QueueRow: View {
    @Bindable var desk: SongDesk
    let entry: DeskEntry
    let index: Int

    var body: some View {
        HStack(spacing: 8) {
            Text("\(index + 1)")
                .font(.system(size: 11).monospacedDigit()).foregroundStyle(Theme.muted)
                .frame(width: 16, alignment: .trailing)
            VStack(alignment: .leading, spacing: 1) {
                Text(entry.title).font(.system(size: 12.5)).foregroundStyle(Theme.foreground).lineLimit(1)
                Text([entry.artists.joined(separator: " / "), t("Asked for by {name}", ["name": entry.by])]
                    .filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.system(size: 10.5)).foregroundStyle(Theme.muted).lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if desk.mine {
                if index > 0 {
                    SmallIcon(symbol: "arrow.up.to.line", label: t("Play {title} next", ["title": entry.title])) { desk.toTop(entry.key) }
                }
                SmallIcon(symbol: "xmark", label: t("Take {title} off", ["title": entry.title])) { desk.remove(entry.key) }
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
    }
}

// MARK: - Searching the library

private struct SearchTab: View {
    @Bindable var desk: SongDesk

    var body: some View {
        let libraries = desk.libraries ?? []
        let library = libraries.first { $0.id == desk.searchSource }
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                if libraries.count > 1 {
                    HStack(spacing: 4) {
                        ForEach(libraries, id: \.id) { one in
                            Choice(text: one.name, chosen: one.id == desk.searchSource, wide: true) { desk.chooseLibrary(one.id) }
                        }
                    }
                }
                if let library, !library.signedIn {
                    Text(t("Not signed in, so at most 320 kbps.")).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                        .transition(Theme.arrive)
                }
                HStack(spacing: 6) {
                    Text(t("Quality")).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                    Choice(text: t("Lossless"), chosen: desk.quality == .lossless) { desk.quality = .lossless }
                    Choice(text: t("Highest"), chosen: desk.quality == .best) { desk.quality = .best }
                }
                if desk.quality == .best {
                    Text(t("Hi-Res and masters are large, and take longer to start.")).font(.system(size: 11)).foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                        .transition(Theme.arrive)
                }
                HStack(spacing: 6) {
                    Box(prompt: t("Search songs"), text: $desk.searchQuery) { Task { await desk.searchLibrary() } }
                        .accessibilityLabel(t("Search songs"))
                    SquareButton(symbol: "magnifyingglass", busy: desk.searching, label: t("Search")) {
                        Task { await desk.searchLibrary() }
                    }
                }
            }
            .padding(8)
            .animation(Theme.settle, value: desk.quality)
            Divider().overlay(Theme.border)

            ScrollView {
                LazyVStack(spacing: 0) {
                    if let problem = desk.searchProblem {
                        Text(problem).font(.system(size: 12)).foregroundStyle(Theme.danger)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                            .transition(Theme.arrive)
                    }
                    if let results = desk.searchResults {
                        if results.isEmpty {
                            Hint(text: t("Nothing found.")).transition(Theme.arrive)
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
                        Hint(text: t("Search for a song to play into the call.")).transition(Theme.arrive)
                    }
                }
                .padding(6)
                .animation(Theme.settle, value: desk.searchResults)
                .animation(Theme.settle, value: desk.searchProblem)
            }
        }
    }
}

// MARK: - A playlist or song link

private struct PlaylistTab: View {
    @Bindable var desk: SongDesk

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Box(prompt: t("Paste a playlist or song link"), text: $desk.pasted) { Task { await desk.openLink() } }
                    .accessibilityLabel(t("Playlist link"))
                Button { Task { await desk.openLink() } } label: {
                    ZStack {
                        if desk.reading {
                            ProgressView().controlSize(.small).scaleEffect(0.7).transition(.opacity)
                        } else {
                            Text(t("Open")).font(.system(size: 12, weight: .medium)).transition(.opacity)
                        }
                    }
                    .foregroundStyle(Theme.foreground)
                    .padding(.horizontal, 12)
                    .frame(height: 32)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
                }
                .buttonStyle(.plain)
            }
            .padding(8)
            .animation(Theme.settle, value: desk.reading)
            Divider().overlay(Theme.border)

            ScrollView {
                LazyVStack(spacing: 0) {
                    if let problem = desk.linkProblem {
                        Text(problem).font(.system(size: 12)).foregroundStyle(Theme.danger)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                            .transition(Theme.arrive)
                    }
                    if let found = desk.linked {
                        HStack(spacing: 8) {
                            VStack(alignment: .leading, spacing: 1) {
                                Text(found.title).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Theme.foreground).lineLimit(1)
                                Text(t("{count} songs", ["count": found.tracks.count])).font(.system(size: 11)).foregroundStyle(Theme.muted)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Pill(symbol: nil, text: t("Add all")) {
                                desk.add(found.tracks.map { SongDesk.song($0, from: found.source) })
                                desk.linkAdded = Set(found.tracks.map(\.id))
                            }
                            .disabled(found.tracks.isEmpty)
                        }
                        .padding(.horizontal, 8).padding(.vertical, 6)
                        .transition(Theme.arrive)
                        ForEach(found.tracks, id: \.id) { track in
                            TrackRow(track: track, added: desk.linkAdded.contains(track.id)) {
                                desk.add([SongDesk.song(track, from: found.source)])
                                desk.linkAdded.insert(track.id)
                            }
                            .transition(Theme.arrive)
                        }
                    } else if desk.linkProblem == nil {
                        Hint(text: t("Paste a link shared from a music app.")).transition(Theme.arrive)
                    }
                }
                .padding(6)
                .animation(Theme.settle, value: desk.linked)
                .animation(Theme.settle, value: desk.linkProblem)
            }
        }
    }
}

// MARK: - Pieces

private struct TrackRow: View {
    let track: MusicTrack
    let added: Bool
    let add: () -> Void

    var body: some View {
        Button(action: add) {
            HStack(spacing: 10) {
                Cover(url: track.cover, size: 36)
                VStack(alignment: .leading, spacing: 1) {
                    Text(track.title).font(.system(size: 13)).foregroundStyle(Theme.foreground).lineLimit(1)
                    Text([track.artists.joined(separator: " / "), track.album].filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(.system(size: 11.5)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Text(added ? t("Added") : musicClock(seconds: track.duration))
                    .font(.system(size: 11).monospacedDigit()).foregroundStyle(Theme.muted)
                    .animation(Theme.settle, value: added)
            }
            .padding(.horizontal, 8).padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(t("Add {title}", ["title": track.title]))
    }
}

/// A cover where the library has one, and a quiet square where it has not.
private struct Cover: View {
    let url: String
    let size: CGFloat

    var body: some View {
        let placeholder = RoundedRectangle(cornerRadius: 4).fill(Theme.surfaceHigh)
            .overlay(Image(systemName: "music.note.list").font(.system(size: size * 0.32)).foregroundStyle(Theme.muted))
        Group {
            if let link = URL(string: url), url.hasPrefix("http") {
                AsyncImage(url: link, transaction: Transaction(animation: Theme.settle)) { phase in
                    if let image = phase.image {
                        image.resizable().scaledToFill().transition(.opacity)
                    } else {
                        placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: 4))
    }
}

/// A listener's own volume for a sound (SoundPanel.tsx `SoundRow`, its slider
/// and readout).
private struct VolumeRow: View {
    let name: String
    @Binding var volume: Float

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(name).font(.system(size: 12.5, weight: .medium)).foregroundStyle(volume == 0 ? Theme.muted : Theme.foreground).lineLimit(1)
            HStack(spacing: 8) {
                Image(systemName: volume == 0 ? "speaker.slash.fill" : "speaker.wave.2.fill")
                    .font(.system(size: 11)).foregroundStyle(Theme.muted).frame(width: 16)
                Slider(value: Binding(get: { volume }, set: { volume = ($0 * 20).rounded() / 20 }), in: 0...1)
                    .controlSize(.mini)
                    .tint(Theme.foreground)
                    .accessibilityLabel(t("{name}'s volume", ["name": name]))
                Text("\(Int((volume * 100).rounded()))%")
                    .font(.system(size: 10.5).monospacedDigit()).foregroundStyle(Theme.muted)
                    .frame(width: 36, alignment: .trailing)
            }
        }
        .padding(.horizontal, 6).padding(.vertical, 4)
    }
}

private struct Check: View {
    @Binding var on: Bool
    let label: String

    var body: some View {
        Button { on.toggle() } label: {
            HStack(spacing: 6) {
                Image(systemName: on ? "checkmark.square.fill" : "square")
                    .font(.system(size: 12))
                    .foregroundStyle(on ? Theme.foreground : Theme.muted)
                    .contentTransition(.symbolEffect(.replace))
                Text(label).font(.system(size: 11)).foregroundStyle(Theme.muted)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityValue(on ? "1" : "0")
        .animation(Theme.settle, value: on)
    }
}

/// A small button on the panel: secondary, or a ghost of one.
private struct Pill: View {
    let symbol: String?
    let text: String?
    var label: String?
    var wide = false
    var ghost = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol {
                    Image(systemName: symbol).font(.system(size: 11, weight: .semibold))
                        .contentTransition(.symbolEffect(.replace))
                }
                if let text { Text(text).font(.system(size: 12)).lineLimit(1) }
            }
            .foregroundStyle(ghost ? Theme.muted : Theme.foreground)
            .padding(.horizontal, 10)
            .frame(maxWidth: wide ? .infinity : nil)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: 6).fill(ghost ? Color.clear : Theme.surfaceHigh))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label ?? text ?? "")
        .help(label ?? text ?? "")
    }
}

private struct SmallIcon: View {
    let symbol: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.muted)
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .help(label)
    }
}

/// One of a set of choices, lit in the signal colour when chosen.
private struct Choice: View {
    let text: String
    let chosen: Bool
    var wide = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(text)
                .font(.system(size: 11.5))
                .foregroundStyle(chosen ? Theme.foreground : Theme.muted)
                .padding(.horizontal, 8).padding(.vertical, 3)
                .frame(maxWidth: wide ? .infinity : nil)
                .background(RoundedRectangle(cornerRadius: 6).fill(chosen ? Theme.tally.opacity(0.15) : .clear))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(chosen ? Theme.tally.opacity(0.5) : Theme.border))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(chosen ? .isSelected : [])
        .animation(Theme.settle, value: chosen)
    }
}

private struct Box: View {
    let prompt: String
    @Binding var text: String
    let submit: () -> Void

    var body: some View {
        TextField(prompt, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .foregroundStyle(Theme.foreground)
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
            .onSubmit(submit)
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            #endif
    }
}

private struct SquareButton: View {
    let symbol: String
    let busy: Bool
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                if busy {
                    ProgressView().controlSize(.small).scaleEffect(0.7).transition(.opacity)
                } else {
                    Image(systemName: symbol).font(.system(size: 12, weight: .semibold)).transition(.opacity)
                }
            }
            .foregroundStyle(Theme.foreground)
            .frame(width: 32, height: 32)
            .background(RoundedRectangle(cornerRadius: 6).fill(Theme.surfaceHigh))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .help(label)
        .animation(Theme.settle, value: busy)
    }
}

private struct Hint: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 12)).foregroundStyle(Theme.muted)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 12).padding(.vertical, 32)
    }
}
