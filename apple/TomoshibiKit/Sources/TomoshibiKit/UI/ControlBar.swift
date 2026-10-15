import SwiftUI
import TomoshibiCore

/// The island of controls at the foot of the call (components/room/ControlBar.tsx):
/// microphone, camera, share, hand and reactions, messages, leave.
/// Forty-four point buttons on a raised surface, the active ones in the one
/// signal colour, leaving in red and nothing else in red.
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
    @Binding var problem: String?
    let sharingActive: Bool
    let toggleShare: () -> Void
    let offers: Offers
    #if os(macOS)
    let sharing: Sharing
    #endif
    @State private var confirmLeave = false
    @State private var busy = false

    var body: some View {
        // The whole bar where it fits, and where it does not (a phone, a
        // narrowed window) the same bar with the panels behind one menu.
        // On a phone the whole bar was wider than the screen and both ends
        // hung off it, hanging up with them.
        ViewThatFits(in: .horizontal) {
            bar(compact: false)
            bar(compact: true)
        }
        .padding(.horizontal, 10)
        .disabled(busy)
    }

    private func bar(compact: Bool) -> some View {
        let size: CGFloat = compact ? 40 : 44
        return HStack(spacing: compact ? 4 : 6) {
            Control(symbol: model.microphoneOn ? "mic.fill" : "mic.slash.fill",
                    label: model.microphoneOn ? t("Mute microphone") : t("Unmute microphone"),
                    lit: false, warn: !model.microphoneOn, size: size) {
                act { try await model.setMicrophone(!model.microphoneOn) }
            }
            Control(symbol: model.cameraOn ? "video.fill" : "video.slash.fill",
                    label: model.cameraOn ? t("Turn camera off") : t("Turn camera on"),
                    lit: false, warn: !model.cameraOn, size: size) {
                act { try await model.setCamera(!model.cameraOn) }
            }
            #if os(macOS)
            ShareMenu(sharing: sharing, active: sharingActive, problem: $problem, call: model.call, sealed: model.sealed)
            #else
            // The system's own broadcast sheet: starting and stopping a share of
            // the whole phone is the system's to do, through the extension.
            ZStack {
                Control(symbol: "rectangle.on.rectangle", label: model.phoneSharing ? t("Stop sharing") : t("Share your screen"),
                        lit: model.phoneSharing, size: size) { BroadcastPicker.press() }
                BroadcastPicker().frame(width: 1, height: 1).opacity(0.02).allowsHitTesting(false)
            }
            #endif
            // The hand and, behind the narrow half beside it, the reactions
            // (HandButton.tsx).
            HandControl(model: model, act: act, panel: $panel)
            Control(symbol: "bubble.left.fill", label: panel == .messages ? t("Hide messages") : t("Show messages"),
                    lit: panel == .messages, size: size) {
                panel = panel == .messages ? nil : .messages
            }
            // The one dot that says something was said while the messages were
            // closed, in the signal colour (ControlBar.tsx ~383).
            .overlay(alignment: .topTrailing) {
                if model.unread && panel != .messages {
                    Circle().fill(Theme.tally).frame(width: 9, height: 9)
                        .overlay(Circle().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
                        .offset(x: -2, y: 2)
                        .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: model.unread)
            .animation(Theme.settle, value: offers)
            if compact {
                more(size: size)
            } else {
                Control(symbol: "speaker.wave.2.fill", label: panel == .sound ? t("Hide sound") : t("Show sound"),
                        lit: panel == .sound) {
                    panel = panel == .sound ? nil : .sound
                }
                if !model.sealed && (offers.watch || panel == .watch) {
                    Control(symbol: "play.rectangle.fill", label: panel == .watch ? t("Hide watch together") : t("Show watch together"),
                            lit: panel == .watch) {
                        panel = panel == .watch ? nil : .watch
                    }
                    // The show is playing while its panel is closed (ControlBar.tsx ~430).
                    .overlay(alignment: .topTrailing) {
                        if offers.watchPlaying && panel != .watch {
                            Circle().fill(Theme.tally).frame(width: 9, height: 9)
                                .overlay(Circle().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
                                .offset(x: -2, y: 2)
                                .transition(Theme.arrive)
                        }
                    }
                    .transition(Theme.arrive)
                }
                if offers.music || panel == .music {
                    Control(symbol: "music.note", label: panel == .music ? t("Hide music") : t("Show music"),
                            lit: panel == .music) {
                        panel = panel == .music ? nil : .music
                    }
                    .overlay(alignment: .topTrailing) {
                        if offers.musicPlaying && panel != .music {
                            Circle().fill(Theme.tally).frame(width: 9, height: 9)
                                .overlay(Circle().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
                                .offset(x: -2, y: 2)
                                .transition(Theme.arrive)
                        }
                    }
                    .transition(Theme.arrive)
                }
                if offers.host || panel == .host {
                    Control(symbol: "crown.fill", label: t("Running this room"), lit: panel == .host) {
                        panel = panel == .host ? nil : .host
                    }
                    // Counted as well as coloured (HostPanel.tsx ~400): somebody
                    // waiting outside is the one thing in the room that is lost if
                    // nobody notices.
                    .overlay(alignment: .topTrailing) {
                        if offers.knocking > 0 && panel != .host {
                            Text("\(offers.knocking)")
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
                Control(symbol: "gearshape.fill", label: t("Devices"), lit: panel == .devices) {
                    panel = panel == .devices ? nil : .devices
                }
            }
            if !compact {
                Divider().frame(height: 26).overlay(Theme.border)
            }
            Button {
                confirmLeave = true
            } label: {
                Image(systemName: "phone.down.fill")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Theme.dangerForeground)
                    .frame(width: compact ? 52 : 56, height: size)
                    .background(Capsule().fill(Theme.danger))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(t("Leave"))
            .help(t("Leave"))
            .confirmationDialog(t("Leave the call"), isPresented: $confirmLeave) {
                Button(t("Leave"), role: .destructive) { Task { await model.leave() } }
            }
        }
        .padding(compact ? 4 : 6)
        .background(Capsule().fill(Theme.surfaceHigh))
        .overlay(Capsule().strokeBorder(Theme.border))
        .shadow(color: .black.opacity(0.4), radius: 16, y: 6)
        .fixedSize()
    }

    /// The panels, behind one button on a narrow bar. It carries the signal
    /// dot whenever one of the buttons it stands for would: a show or the
    /// music playing, somebody at the door.
    private func more(size: CGFloat) -> some View {
        let behind: [RoomView.Panel] = [.sound, .watch, .music, .host, .devices]
        let open = panel.map { behind.contains($0) } ?? false
        let busy = (offers.watchPlaying && panel != .watch) || (offers.musicPlaying && panel != .music)
            || (offers.knocking > 0 && panel != .host)
        return Menu {
            item(t("Sound"), "speaker.wave.2.fill", .sound)
            if !model.sealed && (offers.watch || panel == .watch) {
                item(t("Watch together"), "play.rectangle.fill", .watch)
            }
            if offers.music || panel == .music {
                item(t("Music"), "music.note", .music)
            }
            if offers.host || panel == .host {
                item(offers.knocking > 0 ? "\(t("Running this room")) (\(offers.knocking))" : t("Running this room"),
                     "crown.fill", .host)
            }
            item(t("Devices"), "gearshape.fill", .devices)
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(open ? Theme.tallyForeground : Theme.foreground)
                .frame(width: size, height: size)
                .background(Circle().fill(open ? Theme.tally : Theme.surface))
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(t("More"))
        .help(t("More"))
        .overlay(alignment: .topTrailing) {
            if busy {
                Circle().fill(Theme.tally).frame(width: 9, height: 9)
                    .overlay(Circle().strokeBorder(Theme.surfaceHigh, lineWidth: 2))
                    .offset(x: -2, y: 2)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: busy)
        .animation(Theme.settle, value: open)
    }

    private func item(_ title: String, _ symbol: String, _ target: RoomView.Panel) -> some View {
        Button {
            panel = panel == target ? nil : target
        } label: {
            Label(title, systemImage: panel == target ? "checkmark" : symbol)
        }
    }

    /// A press that did not take is over, so it says so briefly and goes.
    private func act(_ work: @escaping () async throws -> Void) {
        busy = true
        Task {
            do {
                try await work()
            } catch {
                problem = t("That could not be done. Try again.")
                Task {
                    try? await Task.sleep(for: .seconds(4))
                    problem = nil
                }
            }
            busy = false
        }
    }

    private struct Control: View {
        let symbol: String
        let label: String
        var lit = false
        var warn = false
        var size: CGFloat = 44
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(lit ? Theme.tallyForeground : (warn ? Theme.muted : Theme.foreground))
                    .frame(width: size, height: size)
                    .background(Circle().fill(lit ? Theme.tally : Theme.surface))
                    .animation(Theme.settle, value: lit)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(label)
            .help(label)
        }
    }
}

/// The call's messages (components/room/ChatPanel.tsx): kept only for the
/// call, as the web says at the foot of the panel.
struct ChatPanel: View {
    @Bindable var model: CallModel
    let close: () -> Void
    @State private var draft = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("Messages")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close messages"))
            }
            .padding(12)
            Divider().overlay(Theme.border)
            ScrollViewReader { reader in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 10) {
                        ForEach(model.messages) { line in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(line.mine ? t("{name} (you)", ["name": line.name]) : line.name)
                                    .font(.system(size: 11, weight: .semibold))
                                    .foregroundStyle(Theme.muted)
                                Text(line.text)
                                    .font(.system(size: 13))
                                    .foregroundStyle(Theme.foreground)
                                    .textSelection(.enabled)
                            }
                            .id(line.id)
                            .transition(Theme.arrive)
                        }
                    }
                    .padding(12)
                }
                .onChange(of: model.messages.count) {
                    if let last = model.messages.last { withAnimation(Theme.settle) { reader.scrollTo(last.id, anchor: .bottom) } }
                }
            }
            Divider().overlay(Theme.border)
            HStack(spacing: 8) {
                TextField(t("Say something"), text: $draft)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .onSubmit(send)
                    .accessibilityLabel(t("Say something"))
                Button(action: send) {
                    Image(systemName: "arrow.up").font(.system(size: 12, weight: .bold))
                        .foregroundStyle(Theme.tallyForeground)
                        .frame(width: 26, height: 26)
                        .background(Circle().fill(draft.isEmpty ? Theme.border : Theme.tally))
                }
                .buttonStyle(.plain)
                .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                .accessibilityLabel(t("Send"))
            }
            .padding(10)
            Text(t("Messages disappear when the call ends."))
                .font(.system(size: 10.5))
                .foregroundStyle(Theme.muted)
                .padding(.bottom, 8)
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
    }

    private func send() {
        let text = draft
        draft = ""
        Task { try? await model.say(text) }
    }
}

#if os(macOS)
/// The share control on a Mac is a menu, as the web's is (ShareButton.tsx):
/// a screen, a window or an app with its sound; sound alone; the size and the
/// frame rate; and stopping.
struct ShareMenu: View {
    @Bindable var sharing: Sharing
    let active: Bool
    @Binding var problem: String?
    let call: Call
    let sealed: Bool

    var body: some View {
        Menu {
            if active {
                Button(sharing.listening ? t("Stop sharing sound") : t("Stop sharing")) { Task { await sharing.stop() } }
            } else {
                Button(t("Share your screen")) { run { try await sharing.start(on: call) } }
                Button(t("Share only sound")) { run { try await sharing.startSoundOnly(on: call, sealed: sealed) } }
                if !sealed {
                    Toggle(t("Lossless"), isOn: $sharing.lossless)
                }
                Divider()
                Picker(t("Picture"), selection: $sharing.quality) {
                    Text(t("Automatic")).tag(ScreenCapture.Quality.auto)
                    Text("1080p").tag(ScreenCapture.Quality.p1080)
                    Text("1440p").tag(ScreenCapture.Quality.p1440)
                    Text("4K").tag(ScreenCapture.Quality.p2160)
                }
                if sharing.quality != .auto {
                    Picker(t("Frames a second"), selection: $sharing.fps) {
                        ForEach(Sharing.rates(for: sharing.quality), id: \.self) { rate in
                            Text("\(rate) fps").tag(rate)
                        }
                    }
                }
            }
        } label: {
            Image(systemName: "rectangle.on.rectangle")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(active ? Theme.tallyForeground : Theme.foreground)
                .frame(width: 44, height: 44)
                .background(Circle().fill(active ? Theme.tally : Theme.surface))
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(active ? t("Stop sharing") : t("Share your screen"))
        .help(active ? t("Stop sharing") : t("Share your screen"))
    }

    private func run(_ work: @escaping () async throws -> Void) {
        Task {
            do { try await work() } catch { problem = t("That could not be done. Try again.") }
        }
    }
}
#endif
