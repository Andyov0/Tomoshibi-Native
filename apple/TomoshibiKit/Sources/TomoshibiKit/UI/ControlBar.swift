import SwiftUI
import TomoshibiCore

/// The island of controls at the foot of the call (components/room/ControlBar.tsx):
/// microphone, camera, share, hand, messages, leave. Forty-four point buttons
/// on a raised surface, the active ones in the one signal colour, leaving in
/// red and nothing else in red.
struct ControlBar: View {
    @Bindable var model: CallModel
    @Binding var panel: RoomView.Panel?
    @Binding var problem: String?
    let sharingActive: Bool
    let toggleShare: () -> Void
    @State private var confirmLeave = false
    @State private var busy = false

    var body: some View {
        HStack(spacing: 6) {
            Control(symbol: model.microphoneOn ? "mic.fill" : "mic.slash.fill",
                    label: model.microphoneOn ? t("Mute microphone") : t("Unmute microphone"),
                    lit: false, warn: !model.microphoneOn) {
                act { try await model.setMicrophone(!model.microphoneOn) }
            }
            Control(symbol: model.cameraOn ? "video.fill" : "video.slash.fill",
                    label: model.cameraOn ? t("Turn camera off") : t("Turn camera on"),
                    lit: false, warn: !model.cameraOn) {
                act { try await model.setCamera(!model.cameraOn) }
            }
            #if os(macOS)
            Control(symbol: "rectangle.on.rectangle", label: sharingActive ? t("Stop sharing") : t("Share your screen"),
                    lit: sharingActive) { toggleShare() }
            #else
            // The system's own broadcast sheet: starting and stopping a share of
            // the whole phone is the system's to do, through the extension.
            ZStack {
                Control(symbol: "rectangle.on.rectangle", label: model.phoneSharing ? t("Stop sharing") : t("Share your screen"),
                        lit: model.phoneSharing) { BroadcastPicker.press() }
                BroadcastPicker().frame(width: 1, height: 1).opacity(0.02).allowsHitTesting(false)
            }
            #endif
            Control(symbol: "hand.raised.fill", label: model.handUp ? t("Lower your hand") : t("Raise your hand"),
                    lit: model.handUp) {
                act { try await model.setHand(!model.handUp) }
            }
            Control(symbol: "bubble.left.fill", label: panel == .messages ? t("Hide messages") : t("Show messages"),
                    lit: panel == .messages) {
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
            Control(symbol: "speaker.wave.2.fill", label: panel == .sound ? t("Hide sound") : t("Show sound"),
                    lit: panel == .sound) {
                panel = panel == .sound ? nil : .sound
            }
            if !model.sealed {
                Control(symbol: "play.rectangle.fill", label: panel == .watch ? t("Hide watch together") : t("Show watch together"),
                        lit: panel == .watch) {
                    panel = panel == .watch ? nil : .watch
                }
                Control(symbol: "music.note", label: panel == .music ? t("Hide music") : t("Show music"),
                        lit: panel == .music) {
                    panel = panel == .music ? nil : .music
                }
            }
            Control(symbol: "person.2.badge.gearshape.fill", label: t("Running this room"), lit: panel == .host) {
                panel = panel == .host ? nil : .host
            }
            Control(symbol: "gearshape.fill", label: t("Devices"), lit: panel == .devices) {
                panel = panel == .devices ? nil : .devices
            }
            Divider().frame(height: 26).overlay(Theme.border)
            Button {
                confirmLeave = true
            } label: {
                Image(systemName: "phone.down.fill")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Theme.dangerForeground)
                    .frame(width: 56, height: 44)
                    .background(Capsule().fill(Theme.danger))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(t("Leave"))
            .help(t("Leave"))
            .confirmationDialog(t("Leave the call"), isPresented: $confirmLeave) {
                Button(t("Leave"), role: .destructive) { Task { await model.leave() } }
            }
        }
        .padding(6)
        .background(Capsule().fill(Theme.surfaceHigh))
        .overlay(Capsule().strokeBorder(Theme.border))
        .shadow(color: .black.opacity(0.4), radius: 16, y: 6)
        .disabled(busy)
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
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(lit ? Theme.tallyForeground : (warn ? Theme.muted : Theme.foreground))
                    .frame(width: 44, height: 44)
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
