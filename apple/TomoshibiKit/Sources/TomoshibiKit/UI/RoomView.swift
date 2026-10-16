import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The call.
///
/// Every picture is drawn once, keyed by its id, and only its frame changes
/// as the arrangement does (see `Plan`): putting somebody on the stage moves
/// their picture there rather than building a new one, so nothing
/// resubscribes and nothing goes black. The arrangement follows the web's
/// Room.tsx: whatever is pinned takes the stage with everybody else in a strip
/// beneath it, a shared screen pinning itself; two people are one filling the
/// screen and one in the corner; three or more are a grid of up to nine, your
/// own picture on every page.
public struct RoomView: View {
    @Bindable var model: CallModel
    @State private var panel: Panel?
    /// The control bar's popovers (Kit.swift): one open at a time.
    @State private var popovers = Popovers()
    /// The alone-in-the-room capsule, once closed, stays closed for the call.
    @State private var aloneDismissed = false
    /// Which picture is on the stage (usePin.ts).
    @State private var pin = StagePin()
    /// Which page of the grid or the strip is showing (usePagination.ts):
    /// pictures on other pages are not drawn at all, so they cost nothing.
    @State private var page = 0
    /// The stage given the whole screen (the web's "Fill the screen").
    @State private var filled = false
    @State private var problem: String?
    @State private var watch: WatchFeature
    @State private var music: MusicFeature
    @State private var host: HostFeature
    @State private var record: StageRecord
    @State private var idle = IdleWatch()
    @State private var presses = Presses()
    /// The pointer is on the controls, which do not step aside from under it
    /// (ControlBar.tsx `near`).
    @State private var nearControls = false
    /// How tall the pile of notices in the corner is, for lifting the card
    /// above it clear of the filmstrip.
    @State private var toastsHeight: CGFloat = 0
    #if os(macOS)
    @State private var window = WindowFill()
    #else
    /// The phone's safe area, kept for the filled stage to stay clear of.
    @State private var deviceInsets = EdgeInsets()
    #endif

    enum Panel { case messages, sound, watch, music, host, devices }

    public init(model: CallModel) {
        self.model = model
        _watch = State(initialValue: WatchFeature(call: model))
        _music = State(initialValue: MusicFeature(call: model))
        _host = State(initialValue: HostFeature(call: model))
        _record = State(initialValue: StageRecord.of(model))
    }

    public var body: some View {
        GeometryReader { geometry in
            let whole = geometry.size
            let placement = StageDebug.placement ?? Appearance.shared.controlsPlacement
            // Down the side only where the window is tall enough to hold a
            // column (useRoomFor.ts); a shape that does not fit is not the
            // thing somebody chose.
            let side = placement == .side && whole.height >= Plan.sideNeeds
            // A lane for the controls, and the reason nothing else has to know
            // about them: the room is narrower by it, so the pictures, the
            // panels and the cards on the right all keep out of it (Room.tsx
            // `aside`). Not while the stage fills the screen, which is the
            // whole screen.
            let lane = side && !filled ? Plan.sideLane : 0
            let size = CGSize(width: max(0, whole.width - lane), height: whole.height)
            let island = side ? Plan.gap : Plan.island
            let pictures = self.pictures
            let room = Plan.room(size, pictures: pictures.map(\.id), mine: Self.me, stage: pin.pinned,
                                 fullscreen: filled, page: page, island: island)
            // What the room keeps clear of while it reaches under the
            // system's furniture, which is only while it fills a phone: the
            // insets as they were before it reached past them, since while it
            // ignores the safe area, the safe area it ignores reads as nothing.
            #if os(iOS)
            let inset = filled ? deviceInsets : EdgeInsets()
            #else
            let inset = EdgeInsets()
            #endif
            // The controls may step aside when the setting says so, and
            // whenever the stage fills the screen, and come back on any sign
            // of somebody: the same rule for both, because fullscreen used to
            // hide them outright and nothing brought them back
            // (ControlBar.tsx `hiding`).
            let hides = placement == .idle || filled
            let shown = !hides || !idle.idle || nearControls || popovers.open != nil

            content(pictures, room: room, size: size, island: island, shown: shown, hides: hides, inset: inset)
                .frame(width: size.width, height: size.height)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .overlay(alignment: .bottom) {
                    if !side {
                        controls(.horizontal, shown: shown)
                            .padding(.bottom, 20 + inset.bottom)
                            .offset(y: shown ? 0 : 110)
                            .transition(Theme.slideUp)
                    }
                }
                .overlay(alignment: .trailing) {
                    if side {
                        controls(.vertical, shown: shown)
                            .padding(.trailing, 12 + inset.trailing)
                            .offset(x: shown ? 0 : 110)
                            .transition(Theme.slideIn)
                    }
                }
                // Over everything, the controls included: drawn under them, the
                // bar stayed pressable through "you will be back in a moment".
                .overlay {
                    if let to = host.movingTo {
                        MovingOverlay(to: to).transition(Theme.arrive)
                    }
                }
                .animation(Theme.settle, value: host.movingTo)
                .animation(shown ? Theme.settle : Theme.leave, value: shown)
                .animation(Theme.settle, value: side)
                .animation(Theme.settle, value: filled)
                .onChange(of: PinInputs(pictures: pictures.map(\.id), shares: shares), initial: true) { _, now in
                    withAnimation(Theme.settle) { pin.update(pictures: now.pictures, shares: now.shares) }
                }
                // Taking the last picture off the stage leaves nothing to fill
                // the screen with, and a screen still filled by nothing is one
                // somebody has to find the way out of: on a phone, with the
                // controls hidden, there was none (Room.tsx `exit`).
                .onChange(of: pin.pinned) { _, now in
                    if now == nil && filled { unfill() }
                }
                .onChange(of: stageOwner, initial: true) { _, owner in watching(owner) }
                .task(id: hides) {
                    if hides { await idle.run() }
                }
                #if os(iOS)
                .onChange(of: geometry.safeAreaInsets, initial: true) { _, now in
                    if !filled { deviceInsets = now }
                }
                #endif
                .task { await rehearse() }
        }
        #if os(iOS)
        // Filled on a phone means the glass edge to edge, under the clock and
        // the home indicator, which are hidden while it is. Reaching past the
        // safe area by hand, from inside it, covered the top and left a band
        // of the room showing at the foot.
        .ignoresSafeArea(.container, edges: filled ? .all : [])
        #endif
        .background(Theme.background.ignoresSafeArea())
        // Anybody there brings the controls back: the pointer moving over
        // anything in the room, a press anywhere, a key.
        .onContinuousHover { phase in
            if case .active = phase { idle.poke() }
        }
        #if os(macOS)
        .background(WindowReader { window.attach($0) })
        .onAppear { idle.listen() }
        // The green button, the View menu or the system's shortcut leave the
        // full screen without passing through here; the stage follows the
        // window out rather than staying filled inside it.
        .onChange(of: window.exits) {
            if filled { withAnimation(Theme.settle) { filled = false } }
        }
        .onDisappear {
            idle.stopListening()
            window.leave()
        }
        #else
        .simultaneousGesture(TapGesture().onEnded { idle.poke() })
        .statusBarHidden(filled)
        .persistentSystemOverlays(filled ? .hidden : .automatic)
        #endif
        .background {
            // Escape leaves the filled stage, as it leaves a browser's full
            // screen; an open popover takes it first.
            if filled {
                Button("") {
                    if popovers.open != nil { popovers.close() } else { unfill() }
                }
                .keyboardShortcut(.escape, modifiers: [])
                .opacity(0)
                .frame(width: 0, height: 0)
                .accessibilityHidden(true)
            }
        }
        .environment(\.popovers, popovers)
        .preferredColorScheme(.dark)
    }

    // MARK: The room

    @ViewBuilder
    private func content(_ pictures: [Picture], room: Plan.Room, size: CGSize, island: CGFloat, shown: Bool, hides: Bool, inset: EdgeInsets) -> some View {
        let suspects = self.suspects
        let narrow = size.width < Self.panelWidth(size) + 24 + 172 + 22
        // The list of who is watching moves beside an open panel rather than
        // going behind it, and on a screen with no room beside one it shrinks
        // to the eye and the count in the row at the top, which the panel
        // then starts below.
        let watchersInRow = mySharePicture && panel != nil && narrow
        ZStack(alignment: .topLeading) {
            // The space between the pictures: a press there is the second
            // half of a double press that moved its picture away from under
            // the pointer, and its menu carries the room's link (RoomMenu.tsx).
            Theme.background
                .contentShape(Rectangle())
                .onTapGesture { pressed(nil) }
                .roomMenu(record.linkWorks ? record.link : nil)
                .zIndex(-3)
            #if os(iOS)
            // Under everything, the size of the screen: only a layer in a
            // window can be taken into the small window. Under the room's own
            // ground as well, which is opaque: drawn over it, the picture it
            // carries showed through every gap between the tiles, a second
            // copy of a shared screen edging out from behind the stage.
            PictureInPictureHost(call: model.call, sid: model.pictureInPictureSid)
                .frame(width: size.width, height: size.height)
                .allowsHitTesting(false)
                .zIndex(-4)
            #endif
            ForEach(pictures) { picture in
                if let spot = room.spots[picture.id] {
                    tile(picture, spot: spot, room: room, suspects: suspects, shown: shown, hides: hides, inset: inset)
                        .frame(width: spot.width, height: spot.height)
                        .offset(x: spot.minX, y: spot.minY)
                        // Above the room and under everything laid over it, and
                        // the larger picture under the smaller: in a call of
                        // two the card in the corner sits over the person
                        // filling the screen.
                        .zIndex(-1 - Double(spot.width * spot.height) / 1e8)
                        .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: room.spots)

            CallOverlayView(feature: host)

            pager(room, size: size, island: island)

            if !filled || shown {
                HStack(alignment: .top, spacing: 8) {
                    RoomTitle(model: model)
                    // Only once connected, as the web draws it: a reading of a
                    // connection that is not there yet is a reading of nothing.
                    if Appearance.shared.connectionBadge != .hidden && model.state == .connected {
                        SignalBadge(reading: model.connection ?? Self.unmeasured(model.localQuality),
                                    detailed: Appearance.shared.connectionBadge == .detailed,
                                    open: StageDebug.signalOpen ? .constant(true) : $model.connectionOpen,
                                    relay: model.relay,
                                    holding: model.holding)
                            .transition(Theme.arrive)
                    }
                    if watchersInRow {
                        WatchersCard(model: model, compact: true).transition(Theme.arrive)
                    }
                }
                .padding(.leading, 14 + inset.leading)
                .padding(.top, 12 + inset.top)
                .transition(.opacity.animation(Theme.settle))
                .animation(Theme.settle, value: Appearance.shared.connectionBadge)
                .animation(Theme.settle, value: model.state)
            }

            if model.people.isEmpty && pin.pinned == nil && !aloneDismissed && !filled
                && model.state == .connected && record.joining != nil {
                EmptyRoomCard(model: model, linkWorks: record.linkWorks, host: host.standing.yours, link: record.link,
                              recheck: { [host] in await host.refreshStanding() }) {
                    withAnimation(Theme.settle) { aloneDismissed = true }
                }
                .padding(.horizontal, 16)
                .frame(width: size.width, height: max(0, size.height - island - 2), alignment: .bottom)
                .transition(Theme.arrive)
            }

            if let notice = model.notice {
                Notice(text: notice)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 48)
                    .transition(Theme.arrive)
            }

            if model.state != .connected {
                Notice(text: model.state == .reconnecting ? t("Reconnecting…") : t("Connecting…"))
                    .frame(maxWidth: .infinity)
                    .padding(.top, 12)
                    .transition(Theme.arrive)
            }
        }
        // A click outside an open popover closes it; drawn under the
        // controls, which is where the popovers hang from.
        .overlay { PopoverCatcher(popovers: popovers) }
        .overlay(alignment: .topTrailing) {
            // Who has our share open, to the right of the pictures and below
            // the stage's corner (Room.tsx: top-16 right-3), beside a panel
            // when one is open.
            if mySharePicture && !watchersInRow && (!filled || shown) {
                WatchersCard(model: model)
                    .padding(.top, panel == nil ? 58 : 12)
                    .padding(.trailing, panel == nil ? 14 : Self.panelWidth(size) + 24 + 8)
                    .transition(Theme.arrive)
            }
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
                .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                .frame(width: Self.panelWidth(size))
                .frame(maxHeight: max(160, size.height - Plan.island - 24 - (watchersInRow ? 38 : 0)), alignment: .top)
                .fixedSize(horizontal: false, vertical: true)
                .padding(12)
                .padding(.top, (watchersInRow ? 38 : 0) + inset.top)
                .transition(Theme.slideIn)
                .id(panel)
            }
        }
        .overlay(alignment: .bottomLeading) {
            // Always there, so each card leaves with its own animation.
            SaidView(model: model)
                .frame(maxWidth: min(320, size.width - 32), alignment: .leading)
                .padding(.leading, 12)
                .padding(.bottom, Plan.island + 8)
        }
        // The corner for what the call says: the uplink card above the
        // toasts, one column so they cannot land on each other, and drawn
        // after the panels so a tall panel never covers them. Beside an open
        // panel where the window leaves room for both; over it where it does
        // not (a phone, a narrow window), since a toast is brief and a panel
        // is not.
        .overlay(alignment: .bottomTrailing) {
            let beside = panel != nil && size.width - Self.panelWidth(size) - 48 >= 280
            // Where the filmstrip begins, where there is one: the strain card
            // ends above it rather than above the controls, under which the
            // strip starts (Room.tsx `aboveStrip`, plan.ts `clearOf`).
            let aboveStrip = pin.pinned != nil && !filled ? Plan.clear(of: room.spots, ids: room.shown, container: size) : nil
            let lift = Plan.lift(toClear: aboveStrip, ends: Plan.island + 8 + toastsHeight + 10)
            VStack(alignment: .trailing, spacing: 10) {
                #if os(macOS)
                if let hint = model.strainHint ?? StageDebug.strain, sharing.active {
                    StrainCard(hint: hint, model: model, sharing: sharing)
                        .padding(.bottom, lift)
                        .transition(Theme.slideUp)
                }
                #endif
                ToastStack(model: model)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { toastsHeight = $0 }
            }
            .animation(Theme.settle, value: lift)
            .padding(.trailing, beside ? Self.panelWidth(size) + 24 + 8 : 16)
            .padding(.bottom, Plan.island + 8)
            .animation(Theme.settle, value: beside)
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
        .animation(Theme.settle, value: model.people.isEmpty)
        .animation(Theme.settle, value: model.strainHint)
        .animation(Theme.settle, value: model.notice)
        .animation(Theme.settle, value: problem)
        .animation(Theme.settle, value: model.state)
        .animation(Theme.settle, value: mySharePicture)
        .animation(Theme.settle, value: watchersInRow)
        .animation(Theme.settle, value: record.joining)
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
            // TOMOSHIBI_DEBUG_POPOVER=<id> opens one of the bar's popovers.
            if let id = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_POPOVER"] { popovers.show(id) }
        }
    }

    static func panelWidth(_ size: CGSize) -> CGFloat {
        min(360, size.width - 32)
    }

    // MARK: Arrangement

    private struct Picture: Identifiable {
        enum Role {
            case watch, me, myShare
            case remote(Surface)
        }
        let id: String
        let role: Role
    }

    private struct PinInputs: Equatable {
        let pictures: [String]
        let shares: [String]
    }

    private static let me = "me/camera"
    private static let myShare = "me/screen"
    /// The shared video, when a show is running: it takes the stage the way a
    /// shared screen does (Room.tsx).
    private static let watchStage = "watch/stage"

    /// Every picture, in the order the web lists them (surface.ts
    /// `surfaces`): the video watched together first, then each person's
    /// camera and screen together, this participant's own among them.
    ///
    /// Once there are more people than a page holds, whoever spoke last comes
    /// forward (useSpeakingOrder.ts): somebody talking from the second page is
    /// somebody nobody can see. Below that the order holds still, because a
    /// grid that rearranges itself whenever somebody speaks is worse than one
    /// that does not.
    private var pictures: [Picture] {
        var out: [Picture] = []
        if watch.onStage { out.append(Picture(id: Self.watchStage, role: .watch)) }
        out.append(Picture(id: Self.me, role: .me))
        if mySharePicture { out.append(Picture(id: Self.myShare, role: .myShare)) }
        let crowded = model.people.count + 1 > Plan.tilesPerPage
        let order = crowded ? record.speaking.order(model.identities) : model.identities
        let rank = Dictionary(order.enumerated().map { ($1, $0) }, uniquingKeysWith: { first, _ in first })
        let surfaces = model.surfaces.enumerated().sorted { a, b in
            let (left, right) = (rank[a.element.person.id] ?? 0, rank[b.element.person.id] ?? 0)
            return left != right ? left < right : a.offset < b.offset
        }
        out += surfaces.map { Picture(id: $0.element.id, role: .remote($0.element)) }
        return out
    }

    /// What takes the stage by itself when it starts (usePin.ts): somebody's
    /// shared screen, and the video watched together.
    ///
    /// Not this participant's own share, which the web pins because it draws
    /// the screen being sent. There is no preview of the capture here, and a
    /// stage given to a card saying so would push everybody the sharer is
    /// talking to into the strip for nothing; it stays a card among them,
    /// one press from the stage.
    private var shares: [String] {
        (watch.onStage ? [Self.watchStage] : []) + model.surfaces.filter { $0.kind == .screen }.map(\.id)
    }

    /// Whoever is wearing a name somebody else in the room proved, worked out
    /// here rather than in a tile, because a tile can only see itself
    /// (Room.tsx ~402-406).
    private var suspects: Set<String> {
        let roster = [NamedIdentity(name: model.localName, identity: model.localIdentity)]
            + model.people.map { NamedIdentity(name: $0.name, identity: $0.id) }
        return Set(roster.filter { impersonating(participant: $0, everybody: roster) }.map(\.identity))
    }

    /// Before the first sample, the server's own verdict is all there is.
    static func unmeasured(_ quality: LinkQuality) -> ConnectionReading {
        let grade: ConnectionGrade = switch quality {
        case .excellent, .good: .good
        case .poor: .poor
        case .lost: .lost
        }
        return ConnectionReading(grade: grade, rttMs: nil, lossPercent: nil, upKbps: nil, downKbps: nil, jitterMs: nil,
                                 share: nil, ownAddress: nil, holding: nil, measured: false)
    }

    private var offers: Offers {
        Offers(watch: watch.offered, watchPlaying: watch.playing,
               music: music.available, musicPlaying: music.playing,
               host: host.standing.yours, knocking: host.waiting.count)
    }

    // MARK: Pressing

    /// A press on a picture, or on the space between them (nil).
    ///
    /// One press puts the picture on the stage or takes it off, at once; a
    /// second within the system's double-click interval fills the screen with
    /// it (Tile.tsx `onSelect`, `onExpand`). The second press counts for the
    /// picture the first one was on wherever it lands, because the first one
    /// moved that picture: from the stage back to the grid, the pointer is
    /// over somebody else by the time the second arrives, and it pinned them.
    private func pressed(_ id: String?) {
        let now = Date()
        if let last = presses.last, now.timeIntervalSince(last.at) < Presses.interval {
            presses.last = nil
            expand(last.id)
            return
        }
        guard let id else {
            presses.last = nil
            return
        }
        presses.last = (id, now)
        select(id)
    }

    private func select(_ id: String) {
        // While the stage fills the screen a press only brings the controls
        // back. On the web it took the picture off the stage, and so out of
        // the full screen with it, which no video player does with a click.
        guard !filled else {
            idle.poke()
            return
        }
        withAnimation(Theme.settle) { pin.toggle(id) }
    }

    private func expand(_ id: String) {
        if filled {
            unfill()
            return
        }
        withAnimation(Theme.settle) { pin.pin(id) }
        fill()
    }

    /// Fills the screen with the stage. On the Mac that is the window's own
    /// full screen, the nearest thing to the browser's: the stage alone, edge
    /// to edge, in a space of its own, and the controls out of the way until
    /// the pointer moves. On a phone it is the whole glass.
    private func fill() {
        guard pin.pinned != nil else { return }
        withAnimation(Theme.settle) { filled = true }
        idle.poke()
        #if os(macOS)
        if !StageDebug.fillsWindowOnly { window.enter() }
        #endif
    }

    private func unfill() {
        withAnimation(Theme.settle) { filled = false }
        #if os(macOS)
        window.leave()
        #endif
    }

    // MARK: Pictures

    @ViewBuilder
    private func tile(_ picture: Picture, spot: CGRect, room: Plan.Room, suspects: Set<String>, shown: Bool, hides: Bool, inset: EdgeInsets) -> some View {
        let onStage = picture.id == pin.pinned
        let compact = spot.height < 140
        // The stage's own controls: under the pointer on a Mac, and out of
        // sight with the bar while it is; on a touch screen whenever the bar
        // is, since there is no pointer to hover.
        let reveal: (Bool) -> Bool = { hovering in
            #if os(macOS)
            onStage && (hovering || StageDebug.hover) && (!hides || shown)
            #else
            onStage && shown
            #endif
        }
        let radius = filled && onStage ? 0 : Theme.tileRadius
        let ground = filled && onStage ? Color.black : Theme.surface
        // And above the controls while they are out, where the bar is wide
        // enough to reach the name at the foot of a filled stage, as it is
        // on a phone: the name slides up with the bar and back down after it.
        let lift = filled && onStage && shown && room.spots[picture.id].map { $0.width < 900 } == true ? Plan.island - Plan.gap : 0
        let edge = filled && onStage
            ? EdgeInsets(top: inset.top, leading: inset.leading, bottom: inset.bottom + lift, trailing: inset.trailing)
            : EdgeInsets()
        // The card in the corner of a call of two.
        let floating = pin.pinned == nil && room.spots.count == Plan.aloneTogether && picture.id == Self.me
        // The top left of a picture there is under the room's name and the
        // signal, so a hand raised there was drawn behind them; it goes
        // beside the name instead.
        let cornered = furniture.intersects(CGRect(x: spot.minX, y: spot.minY, width: 44, height: 44))
        switch picture.role {
        case .watch:
            WatchStageView(feature: watch, openPanel: { panel = .watch })
                .clipShape(RoundedRectangle(cornerRadius: radius))
                .contentShape(Rectangle())
                .onTapGesture { pressed(picture.id) }
                .overlay(alignment: .topTrailing) {
                    if reveal(true) {
                        StageControls(other: nil, filled: filled, switchTo: {}, fill: { filled ? unfill() : fill() })
                            .padding(12)
                            .padding(edge)
                            .transition(Theme.arrive)
                    }
                }
        case .me:
            let speaking = record.meSpeaking && model.microphoneOn
            Steady(value: speaking) { held in
                TileFrame(marks: TileMarks(label: t("{name} (you)", ["name": model.localName]),
                                           proven: proven(model.localIdentity),
                                           unverified: suspects.contains(model.localIdentity),
                                           speaking: held, muted: !model.microphoneOn, hand: model.handUp, handBesideName: cornered),
                          compact: compact, selected: onStage, expandable: !onStage, radius: radius, floating: floating, ground: ground, edge: edge,
                          press: { pressed(picture.id) }, select: { select(picture.id) }) {
                    SelfPicture(model: model, compact: compact)
                } overlay: { hovering in
                    stageControls(for: picture, revealed: reveal(hovering), edge: edge)
                }
            }
            .contextMenu { pictureMenu(picture, onStage: onStage, label: model.localName, person: nil, sound: .voice, identity: model.localIdentity) }
        case .myShare:
            TileFrame(marks: TileMarks(label: t("{name} (you)", ["name": t("{name} (screen)", ["name": model.localName])])),
                      compact: compact, selected: onStage, expandable: !onStage, radius: radius, ground: ground, edge: edge,
                      press: { pressed(picture.id) }, select: { select(picture.id) }) {
                MyShareFace(compact: compact, stop: stopSharing)
            } overlay: { hovering in
                stageControls(for: picture, revealed: reveal(hovering), edge: edge)
            }
            .contextMenu { pictureMenu(picture, onStage: onStage, label: t("{name} (screen)", ["name": model.localName]), person: nil, sound: .screen, identity: model.localIdentity) }
        case let .remote(surface):
            let person = surface.person
            let name = person.name.isEmpty ? person.id : person.name
            let label = surface.kind == .screen ? t("{name} (screen)", ["name": name]) : name
            let sound: Sound = surface.kind == .screen ? .screen : .voice
            let quiet = host.setting(person, sound).silenced
            if surface.kind == .screen && !onStage {
                TileFrame(marks: TileMarks(label: name, silenced: quiet),
                          compact: compact, selected: false, radius: radius,
                          press: { pressed(picture.id) }, select: { select(picture.id) }) {
                    ShareCardFace(name: name, compact: compact)
                } overlay: { _ in EmptyView() }
                .contextMenu { pictureMenu(picture, onStage: onStage, label: label, person: person, sound: sound, identity: person.id) }
            } else {
                let camera = surface.kind == .camera
                Steady(value: camera && person.speaking) { held in
                    TileFrame(marks: TileMarks(label: label,
                                               proven: camera && proven(person.id),
                                               unverified: camera && suspects.contains(person.id),
                                               speaking: held,
                                               muted: camera && (person.track(.microphone).map(\.muted) ?? true),
                                               silenced: quiet,
                                               hand: person.handUp,
                                               handBesideName: cornered),
                              compact: compact, selected: onStage, expandable: !onStage, radius: radius, ground: ground, edge: edge,
                              press: { pressed(picture.id) }, select: { select(picture.id) }) {
                        SurfacePicture(surface: surface, call: model.call, compact: compact)
                    } overlay: { hovering in
                        stageControls(for: picture, revealed: reveal(hovering), edge: edge)
                    }
                }
                .contextMenu { pictureMenu(picture, onStage: onStage, label: label, person: person, sound: sound, identity: person.id) }
            }
        }
    }

    @ViewBuilder
    private func stageControls(for picture: Picture, revealed: Bool, edge: EdgeInsets) -> some View {
        if revealed {
            let other = counterpart(of: picture)
            StageControls(other: other?.kind, filled: filled,
                          switchTo: { if let other { withAnimation(Theme.settle) { pin.pin(other.id) } } },
                          fill: { filled ? unfill() : fill() })
                .padding(12)
                .padding(edge)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                .transition(Theme.arrive)
        }
    }

    /// The same person's other picture, which the switch on the stage offers
    /// (Room.tsx `counterpart`): never the video watched together, which
    /// belongs to whoever runs it but is not another picture of them.
    private func counterpart(of picture: Picture) -> (id: String, kind: Surface.Kind)? {
        switch picture.role {
        case .watch: return nil
        case .me: return mySharePicture ? (Self.myShare, .screen) : nil
        case .myShare: return (Self.me, .camera)
        case let .remote(surface):
            guard let other = model.surfaces.first(where: { $0.person.id == surface.person.id && $0.id != surface.id }) else { return nil }
            return (other.id, other.kind)
        }
    }

    /// Where the room's name and the signal sit over the pictures, with the
    /// signal's figures when they are open.
    private var furniture: CGRect {
        let open = Appearance.shared.connectionBadge == .detailed && model.connectionOpen
        return CGRect(x: 0, y: 0, width: open ? 380 : 340, height: open ? 330 : 50)
    }

    private func proven(_ identity: String) -> Bool {
        signatureOf(identity: identity)?.proven == true
    }

    /// What can be done to one picture, on the gesture that has always meant
    /// that (PictureMenu.tsx). Everything here is reachable another way, and
    /// that is the point: a double press nobody finds is a feature nobody
    /// has, and a menu says the names out loud. Each item names the picture,
    /// since a menu opened on a small tile in a busy grid has to say which one
    /// it caught.
    @ViewBuilder
    private func pictureMenu(_ picture: Picture, onStage: Bool, label: String, person: Person?, sound: Sound, identity: String) -> some View {
        Button {
            withAnimation(Theme.settle) { pin.toggle(picture.id) }
        } label: {
            Label(onStage ? t("Show everybody") : t("Show {name} larger", ["name": label]),
                  systemImage: onStage ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
        }
        // Only for the picture already on the stage, which is what fills the
        // screen: anywhere else it would have to be put there first, and the
        // press would do two things under one name.
        if onStage {
            Button {
                filled ? unfill() : fill()
            } label: {
                Label(filled ? t("Leave fullscreen") : t("Fill the screen"),
                      systemImage: filled ? "arrow.down.right.and.arrow.up.left.rectangle" : "arrow.up.left.and.arrow.down.right.rectangle")
            }
        }
        // Nobody hears themselves, so nothing here for our own pictures.
        if let person {
            Divider()
            let setting = host.setting(person, sound)
            Button {
                host.setBlocked(person, sound, !setting.blocked)
            } label: {
                Label(setting.blocked ? t("Unmute {name}", ["name": label]) : t("Mute {name}", ["name": label]),
                      systemImage: setting.blocked ? "speaker.wave.2.fill" : "speaker.slash.fill")
            }
            Button {
                panel = .sound
            } label: {
                Label(t("Sound settings"), systemImage: "slider.horizontal.3")
            }
        }
        // Only a signature somebody earned: an issued one is fresh each visit,
        // and a copy of it is true for this call and misleading afterwards.
        if let signature = signatureOf(identity: identity), signature.proven {
            Divider()
            Button {
                Pasteboard.copy(signature.trip)
            } label: {
                Label(t("Copy signature"), systemImage: "signature")
            }
        }
        // The room's own, at the foot of every picture's: two people are one
        // picture filling the frame and another in its corner, so the space
        // around the pictures, where the room's menu lives, is not there in
        // the commonest call anybody holds.
        if record.linkWorks {
            Divider()
            Button {
                Pasteboard.copy(record.link)
            } label: {
                Label(t("Copy link"), systemImage: "link")
            }
        }
    }

    // MARK: Pages

    @ViewBuilder
    private func pager(_ room: Plan.Room, size: CGSize, island: CGFloat) -> some View {
        if room.pages > 1 && !filled {
            // Beside the strip when there is a stage, where the pictures being
            // paged are; either side of the grid otherwise.
            let strip = pin.pinned != nil ? room.shown.compactMap { room.spots[$0]?.midY }.first : nil
            HStack {
                PageArrow(symbol: "chevron.left", label: t("Previous page")) {
                    withAnimation(Theme.settle) { page = (room.page - 1 + room.pages) % room.pages }
                }
                Spacer()
                PageArrow(symbol: "chevron.right", label: t("Next page")) {
                    withAnimation(Theme.settle) { page = (room.page + 1) % room.pages }
                }
            }
            .padding(.horizontal, 10)
            .frame(width: size.width)
            .position(x: size.width / 2, y: strip ?? (size.height - island) / 2)
            .transition(Theme.arrive)

            PageIndicator(page: room.page, pages: room.pages)
                .frame(width: size.width)
                // Below the row at the top where the two would meet.
                .padding(.top, size.width < 560 ? 52 : 15)
                .transition(Theme.arrive)
        }
    }

    // MARK: The controls

    private func controls(_ axis: Axis, shown: Bool) -> some View {
        Group {
            #if os(macOS)
            ControlBar(model: model, panel: $panel, problem: $problem, sharingActive: sharingActive, toggleShare: toggleShare, offers: offers, sharing: sharing, axis: axis)
            #else
            ControlBar(model: model, panel: $panel, problem: $problem, sharingActive: sharingActive, toggleShare: toggleShare, offers: offers, axis: axis)
            #endif
        }
        .onHover { nearControls = $0 }
        .opacity(shown ? 1 : 0)
        .allowsHitTesting(shown)
        .accessibilityHidden(!shown)
    }

    // MARK: Whose share is on my stage

    /// The sharer whose screen is on this participant's stage, or "" (the
    /// web's `watched`, Room.tsx ~460-463): written to the `watching`
    /// attribute so a sharer can see who is looking at their screen. Only a
    /// share actually on the stage counts; one sitting in the strip as a card
    /// is a thing nobody has opened.
    private var stageOwner: String {
        model.surfaces.first { $0.kind == .screen && $0.id == pin.pinned }?.person.id ?? ""
    }

    private func watching(_ owner: String) {
        Task { try? await model.setWatching(owner) }
    }

    // MARK: Sharing

    #if os(macOS)
    private var sharing: Sharing { model.sharing }
    #endif

    private var sharingActive: Bool {
        #if os(macOS)
        sharing.active
        #else
        false
        #endif
    }

    /// This participant is sharing a screen: the web lists a screen surface
    /// whenever the screen's track is there (surface.ts ~91), whatever else
    /// is being shared beside it. `active` is the screen alone and
    /// `listening` sound shared on its own, and the two run at once; read as
    /// "a screen and not sound as well", starting "Share only sound" during
    /// a screen share took away the share's card and its Stop, the list of
    /// who was watching and the stage's switch to it.
    private var mySharePicture: Bool {
        #if os(macOS)
        sharing.active || StageDebug.myShare
        #else
        model.phoneSharing || StageDebug.myShare
        #endif
    }

    /// The share card's Stop, which is the screen's: sound shared on its own
    /// goes on until its own control stops it, as the web's menu stops only
    /// the screen (ShareButton.tsx "Stop sharing", room.ts `stop`).
    private var stopSharing: (() -> Void)? {
        #if os(macOS)
        return { Task { await sharing.stopScreen() } }
        #else
        // A phone's broadcast is the system's to stop, from its own sheet.
        return nil
        #endif
    }

    private func toggleShare() {
        #if os(macOS)
        Task {
            do {
                if sharing.active { await sharing.stopScreen() } else { try await sharing.start(on: model.call) }
            } catch {
                model.actionFailed(t("That could not be done. Try again."))
            }
        }
        #endif
    }

    // MARK: Rehearsal

    /// Runs `TOMOSHIBI_DEBUG_STAGE` (see StageDebug) against this room.
    private func rehearse() async {
        #if os(macOS)
        if let size = StageDebug.window, let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) {
            window.setContentSize(size)
        }
        #endif
        if let names = StageDebug.relay {
            model.relay = names.first
            model.holding = names.count > 1 ? names[1] : nil
        }
        for step in StageDebug.steps {
            try? await Task.sleep(for: .seconds(step.at))
            switch step.action {
            case "fill":
                if pin.pinned == nil, let first = pictures.first(where: { $0.id != Self.me })?.id { pin.pin(first) }
                fill()
            case "unfill": unfill()
            case "grid": withAnimation(Theme.settle) { pin.pin(nil) }
            case "press": pressed(StageDebug.find(step.argument, in: pictures.map(\.id), people: model.people) ?? Self.me)
            case "double":
                let id = StageDebug.find(step.argument, in: pictures.map(\.id), people: model.people) ?? Self.me
                pressed(id)
                try? await Task.sleep(for: .milliseconds(120))
                pressed(id)
            case "pin": withAnimation(Theme.settle) { pin.pin(StageDebug.find(step.argument, in: pictures.map(\.id), people: model.people)) }
            case "page": withAnimation(Theme.settle) { page += 1 }
            case "panel": panel = step.argument == "sound" ? .sound : step.argument == "messages" ? .messages : step.argument == "devices" ? .devices : nil
            case "mute":
                // A guest's setting is never written down, so a rehearsal that
                // mutes only guests leaves nothing behind in the app's settings.
                if let person = model.people.first(where: { $0.name == step.argument }),
                   signatureOf(identity: person.id)?.proven != true {
                    host.setBlocked(person, .voice, true)
                }
            case "hand": try? await model.setHand(true)
            case "leave": await model.leave()
            #if os(macOS)
            case "wiggle", "escape":
                // The pointer moving, or Escape, as the system delivers them:
                // through the application, where the room listens.
                if let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) {
                    let at = NSPoint(x: window.frame.width / 2, y: window.frame.height / 2)
                    let event = step.action == "wiggle"
                        ? NSEvent.mouseEvent(with: .mouseMoved, location: at, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                             windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 0, pressure: 0)
                        : NSEvent.keyEvent(with: .keyDown, location: at, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                           windowNumber: window.windowNumber, context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}",
                                           isARepeat: false, keyCode: 53)
                    if let event { NSApp.postEvent(event, atStart: false) }
                }
            #endif
            case "share":
                #if os(macOS)
                try? await sharing.start(on: model.call)
                #endif
            default: break
            }
        }
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
            .glassSurface(Capsule(), tint: danger ? Theme.danger : nil)
    }
}

extension View {
    /// The room's own menu on the space between the pictures (RoomMenu.tsx):
    /// one thing, the link, because everything else is a press away on a bar
    /// that is always on screen. Nothing at all where a plain link opens for
    /// nobody, since an offer to copy the address reads as an offer to invite
    /// somebody, and under an invitation-only door it is the opposite.
    @ViewBuilder
    func roomMenu(_ link: String?) -> some View {
        if let link {
            contextMenu {
                Button {
                    Pasteboard.copy(link)
                } label: {
                    Label(t("Copy link"), systemImage: "link")
                }
            }
        } else {
            self
        }
    }
}

/// Debug switches for driving the stage without a pointer, honoured only in
/// a debug build driven by a test (TOMOSHIBI_BASE set, as AppModel's
/// rehearsal), so none of them can reach somebody's own app.
///
/// - `TOMOSHIBI_DEBUG_CONTROLS=always|idle|side` places the controls for this
///   run without writing the person's setting.
/// - `TOMOSHIBI_DEBUG_FILL=window` fills the window rather than the screen,
///   so pictures can be taken without moving the display to a new space.
/// - `TOMOSHIBI_DEBUG_HOVER=1` draws every picture as if the pointer were on
///   it.
/// - `TOMOSHIBI_DEBUG_MYSHARE=1`, `TOMOSHIBI_DEBUG_WATCHERS=Bob,Carol`,
///   `TOMOSHIBI_DEBUG_JOINING=invited` and `TOMOSHIBI_DEBUG_RELAY=a|b` stand
///   in for a share of this participant's own, who is watching it, the
///   deployment's door and the join answer's machine names;
///   `TOMOSHIBI_DEBUG_SIGNAL=open` opens the signal's figures,
///   `TOMOSHIBI_DEBUG_STRAIN=1` raises the uplink card and
///   `TOMOSHIBI_DEBUG_WINDOW=WxH` sizes the window.
/// - `TOMOSHIBI_DEBUG_STAGE=4:press:Alice;6:double:Alice;9:unfill` presses,
///   double presses or pins a picture (by a person's name, `me`, `myshare`
///   or `screen:<name>`), and fills, unfills, shows the grid, turns the page,
///   opens a panel, starts a share, raises this participant's hand, mutes a
///   guest for this participant or leaves the call as the Leave menu does,
///   each that many seconds after the one before.
enum StageDebug {
    struct Step {
        let at: Double
        let action: String
        let argument: String
    }

    static var environment: [String: String] {
        #if DEBUG
        let all = ProcessInfo.processInfo.environment
        return all["TOMOSHIBI_BASE"] == nil ? [:] : all
        #else
        return [:]
        #endif
    }

    static let placement: ControlsPlacement? = environment["TOMOSHIBI_DEBUG_CONTROLS"].flatMap(ControlsPlacement.init(rawValue:))
    /// Draws the room as if this participant were sharing a picture, for the
    /// share's own card and the list of who is watching, which a script
    /// cannot otherwise reach: the system's picker wants a person.
    static let myShare = environment["TOMOSHIBI_DEBUG_MYSHARE"] != nil
    /// Names to list as watching that share.
    static let watchers: [String] = (environment["TOMOSHIBI_DEBUG_WATCHERS"] ?? "").split(separator: ",").map(String.init)
    /// What the deployment is taken to say about who may enter: `invited`
    /// shows what somebody sees under an invitation-only door.
    static let joining: Joining? = environment["TOMOSHIBI_DEBUG_JOINING"].flatMap {
        switch $0 {
        case "anyone": .anyone
        case "invited": .invited
        case "accounts": .accounts
        default: nil
        }
    }
    /// `WxH`: the window's size, for pictures of a narrow one.
    static let window: CGSize? = environment["TOMOSHIBI_DEBUG_WINDOW"].flatMap { text in
        let parts = text.split(separator: "x").compactMap { Double($0) }
        return parts.count == 2 ? CGSize(width: parts[0], height: parts[1]) : nil
    }
    /// Opens the signal's figures for this run without writing the
    /// person's remembered choice.
    static let signalOpen = environment["TOMOSHIBI_DEBUG_SIGNAL"] == "open"
    /// `relay|holding`: the names the join answer would have given.
    static let relay: [String]? = environment["TOMOSHIBI_DEBUG_RELAY"].map { $0.split(separator: "|").map(String.init) }
    static let fillsWindowOnly = environment["TOMOSHIBI_DEBUG_FILL"] == "window"
    static let hover = environment["TOMOSHIBI_DEBUG_HOVER"] != nil
    /// `TOMOSHIBI_DEBUG_STRAIN=1`: the uplink card as the core raises it for
    /// a line that cannot carry the share, for a picture of where it sits;
    /// with `TOMOSHIBI_DEBUG_SHARING=screen` for a share to raise it over.
    static let strain: StrainHint? = environment["TOMOSHIBI_DEBUG_STRAIN"] == nil ? nil
        : StrainHint(have: 2_400_000, need: 7_800_000, to: ShareSetting(quality: .p1080, frameRate: 30))

    static let steps: [Step] = (environment["TOMOSHIBI_DEBUG_STAGE"] ?? "").split(separator: ";").compactMap { text in
        let parts = text.split(separator: ":", maxSplits: 2).map(String.init)
        guard parts.count >= 2, let at = Double(parts[0]) else { return nil }
        return Step(at: at, action: parts[1], argument: parts.count > 2 ? parts[2] : "")
    }

    static func find(_ name: String, in ids: [String], people: [Person]) -> String? {
        if name == "me" { return "me/camera" }
        if name == "myshare" { return "me/screen" }
        if name == "watch" { return "watch/stage" }
        let screen = name.hasPrefix("screen:")
        let wanted = screen ? String(name.dropFirst(7)) : name
        let suffix = screen ? "/screen" : "/camera"
        let named = people.filter { $0.name == wanted } + people.filter { $0.name != wanted && $0.name.contains(wanted) }
        return named.map { $0.id + suffix }.first { ids.contains($0) }
    }
}
