import SwiftUI
import TomoshibiCore

/// The call.
///
/// Every picture is drawn once, keyed by its surface, and only its frame
/// changes as the arrangement does (see `Plan`): putting somebody on the stage
/// moves their picture there rather than building a new one, so nothing
/// resubscribes and nothing goes black. The arrangement follows the web's
/// Room.tsx: a shared screen takes the stage with everybody else in a strip
/// beneath it; two people are one filling the screen and one in the corner;
/// three or more are a grid of up to nine.
public struct RoomView: View {
    @Bindable var model: CallModel
    #if os(macOS)
    @State private var sharing = Sharing()
    #endif
    @State private var panel: Panel?
    @State private var pinned: String?
    @State private var problem: String?
    @State private var watch: WatchFeature
    @State private var music: MusicFeature
    @State private var host: HostFeature

    enum Panel { case messages, sound, watch, music, host, devices }

    public init(model: CallModel) {
        self.model = model
        _watch = State(initialValue: WatchFeature(call: model))
        _music = State(initialValue: MusicFeature(call: model))
        _host = State(initialValue: HostFeature(call: model))
    }

    public var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let plan = arrangement(in: size)
            ZStack(alignment: .topLeading) {
                Theme.background.ignoresSafeArea()
                if watch.onStage, let spot = plan[Self.watchStage] {
                    WatchStageView(feature: watch)
                        .clipShape(RoundedRectangle(cornerRadius: Theme.tileRadius))
                        .frame(width: spot.width, height: spot.height)
                        .offset(x: spot.minX, y: spot.minY)
                        .transition(Theme.arrive)
                }
                ForEach(pictures) { picture in
                    if let spot = plan[picture.id] {
                        tile(for: picture, compact: spot.height < 140)
                            .frame(width: spot.width, height: spot.height)
                            .offset(x: spot.minX, y: spot.minY)
                            .onTapGesture(count: 2) { pinned = pinned == picture.id ? nil : picture.id }
                            .transition(Theme.arrive)
                    }
                }
                .animation(Theme.settle, value: plan.mapValues { [$0.minX, $0.minY, $0.width, $0.height] })

                CallOverlayView(feature: host)

                if model.state == .reconnecting {
                    Notice(text: t("Reconnecting…"))
                        .frame(maxWidth: .infinity)
                        .padding(.top, 12)
                        .transition(Theme.arrive)
                }
            }
            .overlay(alignment: .bottom) {
                ControlBar(model: model, panel: $panel, problem: $problem, sharingActive: sharingActive, toggleShare: toggleShare)
                    .padding(.bottom, 20)
            }
            .overlay(alignment: .topTrailing) {
                if let panel {
                    // Never taller than the space above the controls: a panel
                    // that grows past it covers the buttons that close it.
                    ScrollView {
                    Group {
                        switch panel {
                        case .messages: ChatPanel(model: model, close: { self.panel = nil })
                        case .sound: SoundPanelView(feature: host, close: { self.panel = nil })
                        case .watch: WatchPanelView(feature: watch, close: { self.panel = nil })
                        case .music: MusicPanelView(feature: music, close: { self.panel = nil })
                        case .host: HostPanelView(feature: host, close: { self.panel = nil })
                        case .devices: DevicesPanel(model: model, close: { self.panel = nil })
                        }
                    }
                    }
                    .scrollIndicators(.hidden)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .frame(width: min(360, size.width - 32))
                    .frame(maxHeight: max(160, size.height - Plan.island - 24), alignment: .top)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(12)
                    .transition(Theme.slideIn)
                    .id(panel)
                }
            }
            .overlay(alignment: .bottomLeading) {
                if !model.arriving.isEmpty && panel != .messages {
                    SaidView(lines: model.arriving)
                        .frame(maxWidth: min(320, size.width - 32), alignment: .leading)
                        .padding(.leading, 12)
                        .padding(.bottom, Plan.island + 8)
                        .transition(Theme.slideUp)
                }
            }
            .overlay(alignment: .top) {
                if let problem {
                    Notice(text: problem, danger: true)
                        .padding(.top, 12)
                        .transition(Theme.arrive)
                        .onTapGesture { self.problem = nil }
                }
            }
            .animation(Theme.settle, value: panel)
            .animation(Theme.settle, value: model.arriving)
            .onChange(of: panel) { model.chatOpen = panel == .messages }
            // TOMOSHIBI_DEBUG_PANEL=messages|sound|watch|music|host|devices opens
            // that panel on arrival, so a script can look at it without a pointer.
            .onAppear {
                switch ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PANEL"] {
                case "messages": panel = .messages
                case "sound": panel = .sound
                case "watch": panel = .watch
                case "music": panel = .music
                case "host": panel = .host
                case "devices": panel = .devices
                default: break
                }
            }
            .onChange(of: stageOwner(in: size)) { _, owner in watching(owner) }
            .animation(Theme.settle, value: problem)
            .animation(Theme.settle, value: model.state)
        }
        .preferredColorScheme(.dark)
    }

    // MARK: Arrangement

    private struct Picture: Identifiable, Equatable {
        let id: String
        let surface: Surface?
    }

    private static let me = "me/camera"
    /// The shared video, when a show is running: it takes the stage the way a
    /// shared screen does (Room.tsx).
    private static let watchStage = "watch/stage"

    private var pictures: [Picture] {
        model.surfaces.map { Picture(id: $0.id, surface: $0) } + [Picture(id: Self.me, surface: nil)]
    }

    private func arrangement(in size: CGSize) -> [String: CGRect] {
        let ids = pictures.map(\.id) + (watch.onStage ? [Self.watchStage] : [])
        let screens = model.surfaces.filter { $0.kind == .screen }.map(\.id)
        let stage = pinned.flatMap { ids.contains($0) ? $0 : nil } ?? (watch.onStage ? Self.watchStage : nil) ?? screens.first
        if let stage {
            let rest = ids.filter { $0 != stage }
            return Plan.focus(size, stage: stage, strip: Array(rest.prefix(Plan.stripCapacity(size))))
        }
        if ids.count == Plan.aloneTogether, let other = ids.first(where: { $0 != Self.me }) {
            return Plan.close(size, filling: other, cards: [Self.me])
        }
        return Plan.grid(size, ids: Array(ids.prefix(Plan.tilesPerPage)))
    }

    @ViewBuilder
    private func tile(for picture: Picture, compact: Bool) -> some View {
        if let surface = picture.surface {
            Tile(surface: surface, call: model.call, speaking: surface.person.speaking, compact: compact)
        } else {
            ZStack(alignment: .bottomLeading) {
                RoundedRectangle(cornerRadius: Theme.tileRadius).fill(Theme.surface)
                if model.cameraOn {
                    SelfView(session: model.camera.session).transition(.opacity.animation(Theme.settle))
                } else {
                    Avatar(name: model.localName, identity: model.localIdentity)
                }
                Text(t("{name} (you)", ["name": model.localName]))
                    .font(.system(size: compact ? 10.5 : 12.5, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .padding(.horizontal, 6).padding(.vertical, 3)
                    .background(Capsule().fill(.black.opacity(0.55)))
                    .padding(compact ? 4 : 8)
            }
            .animation(Theme.settle, value: model.cameraOn)
        }
    }

    // MARK: Whose share is on my stage

    /// The sharer whose screen is on this participant's stage, or "" (the
    /// web's `watched`, Room.tsx ~460-463): written to the `watching`
    /// attribute so a sharer can see who is looking at their screen.
    private func stageOwner(in size: CGSize) -> String {
        let screens = model.surfaces.filter { $0.kind == .screen }
        let stage = pinned ?? (watch.onStage ? Self.watchStage : nil) ?? screens.first?.id
        return screens.first { $0.id == stage }?.person.id ?? ""
    }

    private func watching(_ owner: String) {
        Task { try? await model.setWatching(owner) }
    }

    // MARK: Sharing

    private var sharingActive: Bool {
        #if os(macOS)
        sharing.active
        #else
        false
        #endif
    }

    private func toggleShare() {
        #if os(macOS)
        Task {
            do {
                if sharing.active { await sharing.stop() } else { try await sharing.start(on: model.call) }
            } catch {
                problem = t("That could not be done. Try again.")
            }
        }
        #endif
    }
}

/// A line across the top that says something is happening or went wrong.
struct Notice: View {
    let text: String
    var danger = false

    var body: some View {
        Text(text)
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(danger ? Theme.dangerForeground : Theme.foreground)
            .padding(.horizontal, 14).padding(.vertical, 8)
            .background(Capsule().fill(danger ? Theme.danger : Theme.surfaceHigh))
            .overlay(Capsule().strokeBorder(Theme.border))
    }
}
