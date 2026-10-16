import AVFoundation
import os
import SwiftUI
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// The island of controls at the foot of the call (components/room/ControlBar.tsx):
/// microphone, camera, the screen and its sound, hand and reactions,
/// messages, the panels, leave. Forty-four point buttons on a raised surface,
/// the active ones in the one signal colour, leaving in red and nothing else
/// in red.
/// What the features layered over the call say about their buttons, read in
/// the room view so the bar does not hold the features themselves.
struct Offers: Equatable {
    /// Room.tsx ~295: to whoever can start a show, to everybody once there is
    /// one.
    var watch = false
    var watchPlaying = false
    var music = false
    var musicPlaying = false
    /// HostPanel.tsx ~387: only for whoever runs the room.
    var host = false
    /// People at the door, counted on the button.
    var knocking = 0
}

struct ControlBar: View {
    @Bindable var model: CallModel
    @Binding var panel: RoomView.Panel?
    /// The room view's line across the top, and its share switch: kept so
    /// the room view's call stands, but no longer used here. A press that did
    /// not take is said as a toast now, and the share controls read the
    /// sharing itself.
    @Binding var problem: String?
    let sharingActive: Bool
    let toggleShare: () -> Void
    let offers: Offers
    #if os(macOS)
    let sharing: Sharing
    #endif
    /// Along the foot of the call, or down its side for the "At the side"
    /// placement (Appearance.controlsPlacement, controls.ts): the buttons in a
    /// column, and their popovers opening beside them instead of above.
    var axis: Axis = .horizontal
    @Environment(\.popovers) private var shared
    @State private var own = Popovers()
    /// A microphone or camera press still being carried out (ControlBar.tsx
    /// `guard`): another toggle press meanwhile is ignored. Only ignored:
    /// the bar used to be disabled for it, Leave included, so somebody could
    /// not leave while the system's permission prompt was open or a publish
    /// was slow.
    @State private var busy = false
    /// The hand's own press, so that a pending microphone does not hold up
    /// a raised hand, nor the hand a microphone.
    @State private var raising = false
    #if DEBUG
    /// TOMOSHIBI_DEBUG_PRESS's `offer:<watch|music|host|knock>`: what the
    /// features offer, changed mid-call as a song starting or somebody
    /// knocking changes it, which a rehearsal cannot otherwise bring about.
    @State private var rehearsedOffers: Offers?
    #endif

    /// What the features offer, or what a rehearsal says they do.
    private var offered: Offers {
        #if DEBUG
        rehearsedOffers ?? offers
        #else
        offers
        #endif
    }

    /// The room's popovers, one open at a time across the whole screen; the
    /// bar's own where it is drawn outside a room (a preview).
    private var popovers: Popovers { shared ?? own }

    private var layoutAxis: Axis { ControlDebug.axis ?? axis }

    var body: some View {
        Group {
            // The whole bar where it fits, and where it does not (a phone, a
            // narrowed window) the same bar with the panels behind one
            // button. On a phone the whole bar was wider than the screen and
            // both ends hung off it, hanging up with them.
            if layoutAxis == .horizontal {
                ViewThatFits(in: .horizontal) {
                    bar(compact: false)
                    bar(compact: true)
                }
                .padding(.horizontal, 10)
            } else {
                ViewThatFits(in: .vertical) {
                    bar(compact: false)
                    bar(compact: true)
                }
                .padding(.vertical, 10)
            }
        }
        #if os(macOS)
        .modifier(EscapeClosesPopovers(popovers: popovers))
        #endif
        .onAppear {
            ControlDebug.arrive(model, popovers: popovers) { which in
                switch which {
                case "mic": device { try await model.setMicrophone(!model.microphoneOn) }
                case "camera": device { try await model.setCamera(!model.cameraOn) }
                case "hand": act { try await model.setHand(!model.handUp) }
                #if DEBUG
                case "offer:watch": rehearsedOffers = { var o = offered; o.watch.toggle(); return o }()
                case "offer:music": rehearsedOffers = { var o = offered; o.music.toggle(); return o }()
                case "offer:host": rehearsedOffers = { var o = offered; o.host.toggle(); return o }()
                case "offer:knock": rehearsedOffers = { var o = offered; o.knocking = o.knocking > 0 ? 0 : 2; return o }()
                #endif
                default: break
                }
            }
        }
    }

    private func bar(compact: Bool) -> some View {
        let size: CGFloat = compact ? 40 : 44
        let spacing: CGFloat = compact ? 4 : 6
        let axis = layoutAxis
        let stack = axis == .horizontal ? AnyLayout(HStackLayout(spacing: spacing)) : AnyLayout(VStackLayout(spacing: spacing))
        return stack {
            BarButton(symbol: model.microphoneOn ? "mic.fill" : "mic.slash.fill",
                      label: model.microphoneOn ? t("Mute microphone") : t("Unmute microphone"),
                      quiet: !model.microphoneOn, size: size) {
                device { try await model.setMicrophone(!model.microphoneOn) }
            }
            BarButton(symbol: model.cameraOn ? "video.fill" : "video.slash.fill",
                      label: model.cameraOn ? t("Turn camera off") : t("Turn camera on"),
                      quiet: !model.cameraOn, size: size) {
                device { try await model.setCamera(!model.cameraOn) }
            }
            #if os(macOS)
            ScreenShareControl(sharing: sharing, model: model, popovers: popovers, axis: axis, size: size)
            SoundShareControl(sharing: sharing, model: model, popovers: popovers, axis: axis, size: size)
            #else
            // The system's own broadcast sheet: starting and stopping a share of
            // the whole phone is the system's to do, through the extension.
            ZStack {
                BarButton(symbol: "rectangle.on.rectangle", label: model.phoneSharing ? t("Stop sharing") : t("Share your screen"),
                          lit: model.phoneSharing, size: size) { BroadcastPicker.press() }
                BroadcastPicker().frame(width: 1, height: 1).opacity(0.02).allowsHitTesting(false)
            }
            #endif
            // The hand and, behind the narrow half beside it, the reactions
            // (HandButton.tsx).
            HandControl(model: model, act: act, panel: $panel, axis: axis)
            BarButton(symbol: "bubble.left.fill", label: panel == .messages ? t("Hide messages") : t("Show messages"),
                      lit: panel == .messages, size: size) {
                panel = panel == .messages ? nil : .messages
            }
            // The one dot that says something was said while the messages were
            // closed, in the signal colour (ControlBar.tsx ~383).
            .overlay(alignment: .topTrailing) {
                if model.unread && panel != .messages { BarDot() }
            }
            .animation(Theme.settle, value: model.unread)
            if compact {
                MoreControl(model: model, panel: $panel, offers: offered, popovers: popovers, axis: axis, size: size)
            } else {
                panels(size: size)
            }
            if !compact {
                Rectangle().fill(Theme.border)
                    .frame(width: axis == .horizontal ? 1 : 26, height: axis == .horizontal ? 26 : 1)
                    .padding(axis == .horizontal ? .horizontal : .vertical, 2)
            }
            LeaveControl(model: model, host: offered.host || ControlDebug.hosting, popovers: popovers, axis: axis, compact: compact, size: size)
        }
        .padding(compact ? 5 : 7)
        // Behind the buttons, not around them: the popovers hang outside the
        // capsule, and inside the glass an iPhone would not let them be
        // touched (Glass.swift `glassBacking`).
        .glassBacking(Capsule())
        .fixedSize()
        // On the whole bar rather than on one button: the buttons that come
        // and go with the features (watch, music, running the room, the
        // count of people at the door) are the bar's own children, and an
        // animation on the messages button beside them left them to pop in
        // and out while the capsule jumped to its new width.
        .animation(Theme.settle, value: offered)
    }

    @ViewBuilder
    private func panels(size: CGFloat) -> some View {
        BarButton(symbol: "speaker.wave.2.fill", label: panel == .sound ? t("Hide sound") : t("Show sound"),
                  lit: panel == .sound, size: size) {
            panel = panel == .sound ? nil : .sound
        }
        if !model.sealed && (offered.watch || panel == .watch) {
            BarButton(symbol: "play.rectangle.fill", label: panel == .watch ? t("Hide watch together") : t("Show watch together"),
                      lit: panel == .watch, size: size) {
                panel = panel == .watch ? nil : .watch
            }
            // The show is playing while its panel is closed (ControlBar.tsx ~430).
            .overlay(alignment: .topTrailing) {
                if offered.watchPlaying && panel != .watch { BarDot() }
            }
            .transition(Theme.arrive)
        }
        if offered.music || panel == .music {
            BarButton(symbol: "music.note", label: panel == .music ? t("Hide music") : t("Show music"),
                      lit: panel == .music, size: size) {
                panel = panel == .music ? nil : .music
            }
            .overlay(alignment: .topTrailing) {
                if offered.musicPlaying && panel != .music { BarDot() }
            }
            .transition(Theme.arrive)
        }
        if offered.host || panel == .host {
            BarButton(symbol: "crown.fill", label: t("Running this room"), lit: panel == .host, size: size) {
                panel = panel == .host ? nil : .host
            }
            // Counted as well as coloured (HostPanel.tsx ~400): somebody
            // waiting outside is the one thing in the room that is lost if
            // nobody notices.
            .overlay(alignment: .topTrailing) {
                if offered.knocking > 0 && panel != .host {
                    Text("\(offered.knocking)")
                        .font(.system(size: 10, weight: .bold).monospacedDigit())
                        .foregroundStyle(Theme.tallyForeground)
                        .padding(.horizontal, 4).frame(minWidth: 16, minHeight: 16)
                        .background(Capsule().fill(Theme.tally))
                        .overlay(Capsule().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
                        .offset(x: 2, y: -2)
                        .transition(Theme.arrive)
                }
            }
            .transition(Theme.arrive)
        }
        BarButton(symbol: "gearshape.fill", label: t("Devices"), lit: panel == .devices, size: size) {
            panel = panel == .devices ? nil : .devices
        }
    }

    /// A press that did not take, for the hand: said briefly, and gone. A
    /// second press while the first is still being written is ignored.
    private func act(_ work: @escaping () async throws -> Void) {
        guard !raising else { return }
        raising = true
        Task {
            do {
                try await work()
            } catch {
                model.announce(CallToast(t("That could not be done. Try again."), kind: .failure, lasts: 4))
            }
            raising = false
        }
    }

    /// The microphone or the camera. A device that would not start says so
    /// itself (CallModel `deviceFailed`); the bar only waits for it, because
    /// each press may ask the system for a device, and a second press while
    /// that question is open left the button and the device disagreeing
    /// about what was on (ControlBar.tsx `guard`). The press is ignored, as
    /// the web's guard ignores it; nothing else on the bar waits.
    private func device(_ work: @escaping () async throws -> Void) {
        guard !busy else { return }
        busy = true
        Task {
            try? await work()
            busy = false
        }
    }
}

// MARK: - Sharing

#if os(macOS)
/// The screen share's button and its popover (ShareButton.tsx): the size and
/// the frame rate first, because they are what somebody came to change, and
/// the press that starts the share last, nearest the button; while sharing,
/// the same settings, applied to the running share at once, and stopping.
private struct ScreenShareControl: View {
    @Bindable var sharing: Sharing
    let model: CallModel
    let popovers: Popovers
    let axis: Axis
    let size: CGFloat

    static let id = "share.screen"

    var body: some View {
        BarButton(symbol: sharing.active ? "rectangle.inset.filled.on.rectangle" : "rectangle.on.rectangle",
                  label: sharing.active ? t("Screen sharing settings") : t("Share your screen"),
                  lit: sharing.active, pressed: popovers.open == Self.id, size: size) {
            popovers.toggle(Self.id)
        }
        .barPopover(Self.id, in: popovers, axis: axis, width: 376) { panel }
    }

    private var panel: some View {
        let shown = sharing.shown
        return MenuCard {
            MenuHeading(title: t("Picture"))
            SegmentedChoice(options: [(ScreenCapture.Quality.auto, t("Automatic")), (.p1080, t("1080p")),
                                      (.p1440, t("1440p")), (.p2160, t("4K"))],
                            selection: Binding(get: { sharing.shown.quality }, set: { quality in
                                change { try await sharing.choose(quality: quality) }
                            }))
                .padding(.horizontal, 4)
            Text(Self.describes(shown.quality))
                .font(.system(size: 11.5))
                .foregroundStyle(Theme.muted)
                .padding(.horizontal, 10)
                .padding(.top, 4)
                .contentTransition(.opacity)
                .animation(Theme.settle, value: shown.quality)
            // Absent for automatic, which chooses the rate as well: a control
            // that did nothing would be a control somebody set and believed.
            if shown.quality != .auto {
                VStack(alignment: .leading, spacing: 2) {
                    MenuHeading(title: t("Frames a second"))
                    SegmentedChoice(options: Sharing.rates(for: shown.quality).map { ($0, "\($0)") },
                                    selection: Binding(get: { sharing.shown.fps }, set: { rate in
                                        change { try await sharing.choose(quality: sharing.shown.quality, fps: rate) }
                                    }))
                        .padding(.horizontal, 4)
                }
                .transition(Theme.arrive)
            }
            MenuRule()
            if sharing.active {
                MenuItem(title: t("Stop sharing"), symbol: "stop.circle", busy: sharing.screenBusy) {
                    popovers.close()
                    Task { await sharing.stopScreen() }
                }
                .transition(Theme.arrive)
            } else {
                MenuItem(title: t("Share your screen"), symbol: "rectangle.on.rectangle",
                         detail: sharing.listening ? t("Without its sound, while sound is shared on its own") : nil,
                         role: .prominent, busy: sharing.screenBusy) {
                    popovers.close()
                    start { try await sharing.start(on: model.call) }
                }
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: shown.quality == .auto)
        .animation(Theme.settle, value: sharing.active)
        // Not disabled while busy: a change waits its turn and a stop always
        // goes through (Sharing), and the press that releases a picker the
        // system never showed has to be taken while that picker is out.
    }

    /// What somebody is choosing between rather than pixels (ShareButton.tsx
    /// `QUALITY_LABELS`): automatic is the only setting that will quietly
    /// send less, and every other one is a promise to send what was asked.
    static func describes(_ quality: ScreenCapture.Quality) -> String {
        switch quality {
        case .auto: t("Adjusts to the connection")
        case .p1080: t("Up to 240 frames a second")
        case .p1440: t("Up to 120 frames a second")
        case .p2160: t("Up to 60 frames a second")
        }
    }

    /// A setting changed mid-share that did not take: the share goes on as
    /// it was, and says so.
    private func change(_ work: @escaping () async throws -> Void) {
        Task {
            do { try await work() } catch {
                model.announce(CallToast(t("That could not be done. Try again."), kind: .failure, lasts: 4))
            }
        }
    }

    /// A share that would not start. A cancelled picker is an answer, not a
    /// failure, and says nothing (ControlBar.tsx: it was once announced as a
    /// camera nobody could use).
    private func start(_ work: @escaping () async throws -> Void) {
        change(work)
    }
}

/// Sound on its own, with its own button beside the screen's (the web keeps
/// it in the share menu): to listen to something together, which is not the
/// same errand as showing a screen, and can run beside one.
private struct SoundShareControl: View {
    @Bindable var sharing: Sharing
    let model: CallModel
    let popovers: Popovers
    let axis: Axis
    let size: CGFloat

    static let id = "share.sound"

    /// Lossless is data, which the SDK does not seal and which would cross
    /// the relay as plain samples (sound.ts `losslessUnavailable`).
    private var sealed: Bool { model.sealed || ControlDebug.sealedLook }

    var body: some View {
        BarButton(symbol: sharing.listening ? "waveform.circle.fill" : "waveform",
                  label: sharing.listening ? t("Sharing sound") : t("Share only sound"),
                  lit: sharing.listening, pressed: popovers.open == Self.id, size: size) {
            popovers.toggle(Self.id)
        }
        .barPopover(Self.id, in: popovers, axis: axis, width: 300) { panel }
    }

    private var panel: some View {
        MenuCard {
            if sharing.listening {
                // Which of the two is going out, because the switch that chose
                // it is not shown while it is: it takes effect when the sharing
                // starts, and a setting shown mid-share would look as if
                // changing it changed something (ShareButton.tsx).
                MenuItem(title: t("Stop sharing sound"), symbol: "stop.circle",
                         detail: sharing.sendingLossless ? t("Lossless") : t("Real time"), busy: sharing.soundBusy) {
                    popovers.close()
                    Task { await sharing.stopSound() }
                }
                .transition(Theme.arrive)
            } else {
                MenuItem(title: t("Share only sound"), symbol: "waveform",
                         detail: sharing.active && sharing.withSound
                            ? t("Your shared screen goes quiet while this plays")
                            : t("Pick the app that is playing"),
                         role: .prominent, busy: sharing.soundBusy) {
                    popovers.close()
                    Task {
                        do { try await sharing.startSoundOnly(on: model.call, sealed: model.sealed) } catch {
                            model.announce(CallToast(t("That could not be done. Try again."), kind: .failure, lasts: 4))
                        }
                    }
                }
                .transition(Theme.arrive)
                MenuRule()
                // Beneath what it qualifies and decided before the picker
                // opens. Off where it cannot work, with the reason in place of
                // the description, so nobody turns it on and gets real time
                // without being told.
                MenuSwitch(title: t("Lossless"), symbol: "hifispeaker",
                           detail: sealed ? t("Not available in an encrypted call") : t("Every sample as it is played, about a second behind"),
                           isOn: sealed ? .constant(false) : $sharing.lossless)
                    .disabled(sealed)
                    // Its words dimmed with its switch: bright words beside a
                    // switch that will not move read as a fault.
                    .opacity(sealed ? 0.7 : 1)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: sharing.listening)
    }
}
#endif

// MARK: - Leaving

/// Leaving, and for whoever runs the room ending it (Leaving.tsx): one
/// popover above the red button rather than a dialog in the middle of the
/// window, so the answer is where the question was asked. Asked with the
/// consequence rather than with "are you sure", and closed by Escape or a
/// press anywhere else, because a question that can only be dismissed by
/// choosing something is one somebody chooses something in by accident.
private struct LeaveControl: View {
    let model: CallModel
    let host: Bool
    let popovers: Popovers
    let axis: Axis
    let compact: Bool
    let size: CGFloat
    @State private var ending = false
    @State private var hovering = false
    #if DEBUG
    @Environment(\.isEnabled) private var enabled
    #endif

    static let id = "leave"

    var body: some View {
        let open = popovers.open == Self.id
        Button {
            popovers.toggle(Self.id)
        } label: {
            Image(systemName: "phone.down.fill")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Theme.dangerForeground)
                .frame(width: axis == .horizontal ? (compact ? 52 : 56) : size,
                       height: axis == .horizontal ? size : (compact ? 48 : 52))
                .background(Capsule().fill(Theme.danger.opacity(open || hovering ? 0.86 : 1)))
                .overlay(Capsule().stroke(Color.white.opacity(open ? 0.4 : 0.25), lineWidth: 1))
                .contentShape(Capsule())
        }
        .buttonStyle(BarPress())
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityLabel(t("Leave"))
        .accessibilityAddTraits(open ? .isSelected : [])
        .help(t("Leave"))
        .barPopover(Self.id, in: popovers, axis: axis, place: .end, width: 272) { panel }
        #if DEBUG
        // Whether Leave can be pressed, said each time that changes, for a
        // rehearsal to read while the microphone is starting.
        .onChange(of: enabled, initial: true) { _, now in
            if ControlDebug.on { ControlDebug.log.notice("leave \(now ? "can" : "cannot", privacy: .public) be pressed") }
        }
        #endif
    }

    private var panel: some View {
        MenuCard {
            MenuItem(title: t("Leave the call"), symbol: "rectangle.portrait.and.arrow.right",
                     detail: t("The meeting carries on without you.")) {
                popovers.close()
                Task { await model.leave() }
            }
            if host {
                MenuItem(title: t("End for everybody"), symbol: "door.left.hand.closed",
                         detail: t("Everybody is disconnected and links stop working."),
                         role: .destructive, busy: ending, action: end)
                    .transition(Theme.arrive)
            }
        }
        .disabled(ending)
        .animation(Theme.settle, value: host)
    }

    private func end() {
        ending = true
        Task {
            do {
                try await Self.endForEverybody(model)
                popovers.close()
            } catch {
                ending = false
                model.announce(CallToast(t("That could not be done. Try again."), kind: .failure, lasts: 4))
            }
        }
    }

    /// The same request the host panel's "Close the room" makes
    /// (HostFeature.dissolve), made here so that a failure can be told apart
    /// from a success: the panel's own call says a failure in its notices and
    /// returns either way.
    static func endForEverybody(_ model: CallModel) async throws {
        try await model.deployment.dissolve(room: model.room, bearer: model.call.token())
        // The media server disconnects everybody, this app among them, and
        // that is what should carry the host out, to the screen everybody
        // else lands on. Waited for rather than relied on (Leaving.tsx): if
        // the disconnect does not arrive, whoever ended the meeting is left
        // sitting in a room that no longer exists.
        try? await Task.sleep(for: .milliseconds(1200))
        await model.leave()
    }
}

// MARK: - More

/// The panels, behind one button on a narrow bar. It carries the signal dot
/// whenever one of the buttons it stands for would: a show or the music
/// playing, somebody at the door.
private struct MoreControl: View {
    let model: CallModel
    @Binding var panel: RoomView.Panel?
    let offers: Offers
    let popovers: Popovers
    let axis: Axis
    let size: CGFloat

    static let id = "more"

    var body: some View {
        let behind: [RoomView.Panel] = [.sound, .watch, .music, .host, .devices]
        let open = panel.map { behind.contains($0) } ?? false
        let busy = (offers.watchPlaying && panel != .watch) || (offers.musicPlaying && panel != .music)
            || (offers.knocking > 0 && panel != .host)
        BarButton(symbol: "ellipsis", label: t("More"), lit: open, pressed: popovers.open == Self.id, size: size) {
            popovers.toggle(Self.id)
        }
        .overlay(alignment: .topTrailing) {
            if busy { BarDot() }
        }
        .animation(Theme.settle, value: busy)
        .barPopover(Self.id, in: popovers, axis: axis, place: .end, width: 240) {
            MenuCard {
                item(t("Sound"), "speaker.wave.2.fill", .sound)
                if !model.sealed && (offers.watch || panel == .watch) {
                    item(t("Watch together"), "play.rectangle.fill", .watch, dot: offers.watchPlaying)
                }
                if offers.music || panel == .music {
                    item(t("Music"), "music.note", .music, dot: offers.musicPlaying)
                }
                if offers.host || panel == .host {
                    item(offers.knocking > 0 ? "\(t("Running this room")) (\(offers.knocking))" : t("Running this room"),
                         "crown.fill", .host, dot: offers.knocking > 0)
                }
                MenuRule()
                item(t("Devices"), "gearshape.fill", .devices)
            }
        }
    }

    private func item(_ title: String, _ symbol: String, _ target: RoomView.Panel, dot: Bool = false) -> some View {
        MenuItem(title: title, symbol: symbol, checked: panel == target) {
            popovers.close()
            panel = panel == target ? nil : target
        }
        .overlay(alignment: .leading) {
            if dot && panel != target {
                Circle().fill(Theme.tally).frame(width: 6, height: 6).offset(x: 24, y: -7)
                    .transition(Theme.arrive)
            }
        }
    }
}

// MARK: - Rehearsal switches

/// Switches a test sets to look at the controls in states a script cannot
/// reach, honoured only in a debug build and only in a rehearsal (the app
/// driven by TOMOSHIBI_BASE, AppModel.rehearsal):
///
/// - TOMOSHIBI_DEBUG_SHARING=screen|sound|both[,lossless]: the bar as it is
///   while sharing, with nothing captured (the system's picker cannot be
///   answered by a script).
/// - TOMOSHIBI_DEBUG_SEALED=1: the sound popover as a sealed call shows it.
/// - TOMOSHIBI_DEBUG_HOSTING=1: "End for everybody" offered, as to the host.
/// - TOMOSHIBI_DEBUG_AXIS=vertical: the bar laid out down the side.
/// - TOMOSHIBI_DEBUG_LEAVE=end:<seconds>: "End for everybody", pressed that
///   long after arriving.
/// - TOMOSHIBI_DEBUG_DEVICE_FAILED=camera|microphone[:refused]: the toast a
///   device that would not start leaves.
/// - TOMOSHIBI_DEBUG_SHARE_SIZE=<shorter side>[@<rate>]: the share menu on
///   that size and rate.
/// - TOMOSHIBI_DEBUG_WINDOW=<width>x<height>: the window at that size, for
///   the narrow bar.
/// - TOMOSHIBI_DEBUG_SLOW=<seconds>[,<seconds>]: the microphone and the
///   camera wait that long to start, as they wait on the system's permission
///   prompt (`askedAsIfPrompted`), and the voice's publish the second figure
///   longer, as it waits on a distant server.
/// - TOMOSHIBI_DEBUG_PICKER=never|cancel[:<seconds>]|pick[:<seconds>]: the
///   system's share picker replaced by a stand-in that never answers (as a
///   picker the system declined to show), is cancelled, or picks a pretend
///   screen after that long; the pretend share publishes its tracks and
///   captures nothing, its capture taking TOMOSHIBI_DEBUG_CAPTURE seconds
///   (0.3 unless told) to start or change.
/// - TOMOSHIBI_DEBUG_PRESS=<seconds>:<what>[:<argument>];...: presses, each
///   that many seconds after the one before: `mic`, `camera` and `hand`
///   through the bar's own guards; `micon` straight to the call, as the join
///   and the menu's shortcut ask; `share`, `sound`, `stopshare`, `stopsound`
///   and `size:<shorter side>[@<rate>]` as the share menus press them;
///   `stop` as the share's tile and leaving stop everything; `systemstop` as
///   the menu bar's Stop; `strain:<shorter side>@<rate>` as the uplink card's
///   Switch; `leave`; `audiochange` (the hardware changing under the engine)
///   and `audiofail:<n>` (the next n starts of the engine fail); on a phone
///   `interrupt` and `resume` (a telephone call beginning and ending);
///   `failed:camera|microphone`; `offer:watch|music|host|knock` (a feature
///   offering its button, or somebody knocking, as the bar is told it);
///   `signal` (the badge's figures opened or
///   closed) and `lossless` (the sound share's switch), which a rehearsal
///   must not write down; and `state`, which logs what the call is doing
///   under the subsystem's `rehearsal` category.
enum ControlDebug {
    static let environment = ProcessInfo.processInfo.environment
    static let log = Logger(subsystem: "app.tomoshibi", category: "rehearsal")

    static var on: Bool {
        #if DEBUG
        environment["TOMOSHIBI_BASE"] != nil
        #else
        false
        #endif
    }

    static var axis: Axis? { on && environment["TOMOSHIBI_DEBUG_AXIS"] == "vertical" ? .vertical : nil }
    static var sealedLook: Bool { on && environment["TOMOSHIBI_DEBUG_SEALED"] != nil }
    static var hosting: Bool { on && environment["TOMOSHIBI_DEBUG_HOSTING"] != nil }

    #if DEBUG
    static var slowDevices: Double? { slow.first }
    private static var slow: [Double] {
        on ? (environment["TOMOSHIBI_DEBUG_SLOW"] ?? "").split(separator: ",").compactMap { Double($0) } : []
    }

    /// TOMOSHIBI_DEBUG_SLOW's second figure: a publish's round trip to a
    /// server further away than the test's own, which answers in a few
    /// milliseconds and so never leaves the window a real one leaves open.
    static func publishing() async throws {
        guard slow.count > 1 else { return }
        try await Task.sleep(for: .seconds(slow[1]))
    }
    @MainActor private static var prompt: ContinuousClock.Instant?

    /// TOMOSHIBI_DEBUG_SLOW: waits as a start of the microphone or the camera
    /// waits on the system's permission prompt. The first press opens it, and
    /// every press made while it is open is let go at the same moment, when
    /// it is answered: which is how two starts both came to find no voice
    /// track and published one each.
    @MainActor
    static func askedAsIfPrompted() async throws {
        guard let seconds = slowDevices else { return }
        let now = ContinuousClock.now
        let answered: ContinuousClock.Instant
        if let open = prompt, open > now {
            answered = open
        } else {
            answered = now + .seconds(seconds)
            prompt = answered
        }
        try await Task.sleep(until: answered, clock: .continuous)
    }

    enum PickerAnswer: CustomStringConvertible {
        case never, cancel(Double), pick(Double)

        var description: String {
            switch self {
            case .never: "never answers"
            case let .cancel(after): "cancelled after \(after) s"
            case let .pick(after): "picks after \(after) s"
            }
        }
    }

    static var picker: PickerAnswer? {
        guard on, let text = environment["TOMOSHIBI_DEBUG_PICKER"] else { return nil }
        let parts = text.split(separator: ":")
        let after = parts.count > 1 ? Double(parts[1]) ?? 1 : 1
        switch parts.first {
        case "never": return .never
        case "cancel": return .cancel(after)
        case "pick": return .pick(after)
        default: return nil
        }
    }

    /// A pretend share's capture starting or changing.
    static func pretendCapture() async throws {
        let seconds = environment["TOMOSHIBI_DEBUG_CAPTURE"].flatMap(Double.init) ?? 0.3
        try await Task.sleep(for: .seconds(seconds))
    }
    #endif

    /// Arrives in the call: what the switches above set up, and the presses,
    /// once per call however often the bar appears. `press` presses the bar's
    /// own microphone, camera or hand.
    @MainActor
    static func arrive(_ model: CallModel, popovers: Popovers, press: @escaping @MainActor (String) -> Void = { _ in }) {
        guard on else { return }
        #if DEBUG
        if model.parts["rehearsal.arrived"] == nil {
            model.parts["rehearsal.arrived"] = NSNull()
            if let steps = environment["TOMOSHIBI_DEBUG_PRESS"] {
                Task { await run(steps, model, popovers: popovers, press: press) }
            }
        }
        #if os(macOS)
        // TOMOSHIBI_DEBUG_SHARE_SIZE=<shorter side>[@<rate>]: the share menu
        // on that size, not written down (Sharing.remembers is off in a
        // rehearsal).
        if let chosen = environment["TOMOSHIBI_DEBUG_SHARE_SIZE"]?.split(separator: "@").compactMap({ Int($0) }),
           let size = chosen.first, let quality = ScreenCapture.Quality(rawValue: size) {
            model.sharing.quality = quality
            if chosen.count > 1 { model.sharing.fps = chosen[1] }
        }
        if let sharing = environment["TOMOSHIBI_DEBUG_SHARING"] {
            model.sharing.pretend(screen: sharing.hasPrefix("screen") || sharing.hasPrefix("both"),
                                  sound: sharing.hasPrefix("sound") || sharing.hasPrefix("both"),
                                  lossless: sharing.contains("lossless"))
        }
        if let size = environment["TOMOSHIBI_DEBUG_WINDOW"]?.split(separator: "x").compactMap({ Double($0) }), size.count == 2 {
            DispatchQueue.main.async {
                guard let window = NSApp.windows.first(where: { $0.isVisible && $0.canBecomeMain }) else { return }
                // Not saved as the window's size: the frame is kept under the
                // bundle's own name, and a test's narrow window would be the
                // size the owner's app next opened at.
                window.setFrameAutosaveName("")
                window.setContentSize(NSSize(width: size[0], height: size[1]))
            }
        }
        #endif
        // TOMOSHIBI_DEBUG_LEAVE=end:<seconds>: ends the meeting for everybody
        // that long after arriving, through the popover's own path.
        if let leave = environment["TOMOSHIBI_DEBUG_LEAVE"], leave.hasPrefix("end:"), let after = Double(leave.dropFirst(4)) {
            Task {
                try? await Task.sleep(for: .seconds(after))
                do { try await LeaveControl.endForEverybody(model) } catch {
                    Logger(subsystem: "app.tomoshibi", category: "controls").error("ending failed: \(String(describing: error), privacy: .public)")
                }
            }
        }
        if let failed = environment["TOMOSHIBI_DEBUG_DEVICE_FAILED"] {
            let kind: DeviceKind = failed.hasPrefix("camera") ? .camera : .microphone
            model.deviceFailed(kind, failed.hasSuffix(":refused") ? DeviceRefused(kind: kind) : CocoaError(.featureUnsupported))
        }
        #endif
    }

    #if DEBUG
    @MainActor
    private static func run(_ steps: String, _ model: CallModel, popovers: Popovers, press: @escaping @MainActor (String) -> Void) async {
        for step in steps.split(separator: ";") {
            let parts = step.split(separator: ":", maxSplits: 2).map(String.init)
            guard parts.count >= 2, let after = Double(parts[0]) else { continue }
            let argument = parts.count > 2 ? parts[2] : ""
            try? await Task.sleep(for: .seconds(after))
            log.notice("press \(parts[1], privacy: .public) \(argument, privacy: .public)")
            switch parts[1] {
            case "mic", "camera", "hand":
                press(parts[1])
            case "offer":
                press("offer:" + argument)
            case "micon":
                Task { try? await model.setMicrophone(true) }
            case "leave":
                Task { await model.leave() }
            case "failed":
                let kind: DeviceKind = argument == "camera" ? .camera : .microphone
                model.deviceFailed(kind, CocoaError(.featureUnsupported))
            case "signal":
                model.connectionOpen.toggle()
            case "audiochange":
                model.audio.rehearseHardwareChange()
            case "audiofail":
                model.audio.failingStarts = Int(argument) ?? 1
            #if os(iOS)
            case "popover":
                // One of the bar's popovers opened, as its button opens it.
                popovers.show(argument)
            case "interrupt":
                // As the system does it: the engine stopped, then the notice.
                model.audio.engine.stop()
                NotificationCenter.default.post(name: AVAudioSession.interruptionNotification, object: AVAudioSession.sharedInstance(),
                                                userInfo: [AVAudioSessionInterruptionTypeKey: AVAudioSession.InterruptionType.began.rawValue])
            case "resume":
                NotificationCenter.default.post(name: AVAudioSession.interruptionNotification, object: AVAudioSession.sharedInstance(),
                                                userInfo: [AVAudioSessionInterruptionTypeKey: AVAudioSession.InterruptionType.ended.rawValue])
            #endif
            #if os(macOS)
            case "share":
                Task {
                    do { try await model.sharing.start(on: model.call) } catch {
                        model.announce(CallToast(t("That could not be done. Try again."), kind: .failure, lasts: 4))
                    }
                }
            case "sound":
                Task { try? await model.sharing.startSoundOnly(on: model.call, sealed: model.sealed) }
            case "stopshare":
                Task { await model.sharing.stopScreen() }
            case "stopsound":
                Task { await model.sharing.stopSound() }
            case "stop":
                Task { await model.sharing.stop() }
            case "systemstop":
                model.sharing.rehearseSystemStop()
            case "lossless":
                model.sharing.lossless.toggle()
            case "size":
                let chosen = argument.split(separator: "@").compactMap { Int($0) }
                if let size = chosen.first, let quality = ScreenCapture.Quality(rawValue: size) {
                    Task {
                        do { try await model.sharing.choose(quality: quality, fps: chosen.count > 1 ? chosen[1] : nil) } catch {
                            log.notice("the size change failed: \(String(describing: error), privacy: .public)")
                        }
                    }
                }
            case "strain":
                let chosen = argument.split(separator: "@").compactMap { Int($0) }
                if chosen.count == 2, let quality = ScreenCapture.Quality(rawValue: chosen[0]) {
                    Task {
                        do { try await model.sharing.restart(quality: quality, fps: chosen[1], on: model.call) } catch {
                            log.notice("the uplink card's switch did not take: \(String(describing: error), privacy: .public)")
                        }
                    }
                }
            #endif
            case "state":
                state(model)
            default:
                log.error("no such press: \(parts[1], privacy: .public)")
            }
        }
    }

    /// What the call is doing, in one line of the log.
    @MainActor
    static func state(_ model: CallModel) {
        let tracks = model.localTracks.values
            .map { "\($0.source)/\($0.name) \($0.sid)\($0.muted ? " muted" : "")" }
            .sorted().joined(separator: ", ")
        var line = "microphone \(model.microphoneOn ? "on" : "off"), camera \(model.cameraOn ? "on" : "off"); \(model.audio.rehearsalState); tracks [\(tracks)]; notices [\(model.toasts.compactMap(\.key).joined(separator: ", "))]"
        #if os(macOS)
        line += "; \(model.sharing.rehearsalState)"
        #endif
        log.notice("state: \(line, privacy: .public)")
    }
    #endif
}
