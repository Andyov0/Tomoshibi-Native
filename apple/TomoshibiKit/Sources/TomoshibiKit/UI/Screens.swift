import AVFoundation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The whole app: one screen at a time, each arriving and leaving with the
/// same two curves the web uses.
public struct AppRoot: View {
    @Bindable var app: AppModel
    @Environment(\.scenePhase) private var scenePhase

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        ZStack {
            Theme.background.ignoresSafeArea()
            switch app.screen {
            case .starting:
                StartingScreen().transition(Theme.arrive)
            case .server:
                ServerScreen(app: app).transition(Theme.arrive)
            case .lobby:
                // Signed in or not is the whole difference between the two
                // screens a deployment that keeps its rooms shows.
                if let account = app.account {
                    HomeScreen(app: app, account: account).transition(Theme.arrive)
                } else {
                    SignInScreen(app: app).transition(Theme.arrive)
                }
            case .prejoin:
                PreJoinScreen(app: app).transition(Theme.arrive)
            case .knocking:
                KnockingScreen(app: app).transition(Theme.arrive)
            case .waiting:
                WaitingScreen(app: app).transition(Theme.arrive)
            case .call:
                if app.call == nil, let to = app.movingTo {
                    MovingOverlay(to: to).transition(Theme.arrive)
                }
                if let call = app.call {
                    RoomView(model: call)
                        .transition(Theme.arrive)
                        .onChange(of: call.endedReason) {
                            if call.endedReason != nil, call.moving == nil {
                                Task { await app.leave() }
                            }
                        }
                }
            case .ended:
                EndedScreen(app: app).transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: app.screen)
        .animation(Theme.settle, value: app.account?.name)
        .preferredColorScheme(.dark)
        .task { await app.start() }
        // The light in the corner, asked again every minute: a reassurance
        // rather than a monitor (Lobby.tsx `Fleet`).
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                if app.screen != .call { await app.refreshRelays() }
            }
        }
        // Readings taken while the app is in the background say nothing about
        // a freeze (the web skips a hidden tab); inactive is still on screen.
        .onChange(of: scenePhase) { app.call?.call.setBackground(background: scenePhase == .background) }
        .onOpenURL { url in Task { await app.open(url) } }
        .environment(\.locale, Localization.shared.language.locale)
    }
}

/// The plainer screens' heading: a title, and a line under it.
private struct Heading: View {
    let title: String
    var detail: String?
    var mono = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(mono ? .system(size: 20, design: .monospaced) : .system(size: 22, weight: .semibold))
                .tracking(mono ? 0 : -0.4)
                .foregroundStyle(Theme.foreground)
                .lineLimit(2)
            if let detail {
                Text(detail).font(.system(size: 13)).foregroundStyle(Theme.muted).lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A quiet way back, under a screen's main action.
private struct BackButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "chevron.left").font(.system(size: 10, weight: .semibold))
                Text(t("Back")).font(.system(size: 12.5))
            }
            .foregroundStyle(Theme.muted)
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Which deployment

struct ServerScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        LobbyFrame(app: app) {
            LanguageMenu()
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                Heading(title: t("Server"), detail: t("The server address")).rise()
                VStack(spacing: 12) {
                    LobbyInput(placeholder: AppModel.home.replacingOccurrences(of: "https://", with: ""),
                               text: $app.serverAddress) { Task { await app.connect() } }
                    Button { Task { await app.connect() } } label: {
                        HStack {
                            if app.busy {
                                ProgressView().controlSize(.small).tint(Theme.background)
                            } else {
                                Text(t("Connect"))
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(LobbyButtonStyle(tall: true))
                    .disabled(app.busy || app.serverAddress.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .rise(0.06)
            }
        }
    }
}

// MARK: - Before joining

/// PreJoin.tsx: the picture of yourself beside what you are about to join,
/// side by side where there is room and one above the other where there is
/// not.
struct PreJoinScreen: View {
    @Bindable var app: AppModel
    @State private var preview = CameraCapture()
    @State private var sealing = false

    var body: some View {
        LobbyFrame(app: app, width: 1040) {
            LanguageMenu()
        } content: {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .center, spacing: 32) {
                    stage.frame(minWidth: 480)
                    details.frame(width: 380)
                }
                VStack(spacing: 24) {
                    stage
                    details
                }
            }
            .padding(.top, 24)
        }
        .animation(Theme.settle, value: sealing)
        .animation(Theme.settle, value: app.problem)
        .onChange(of: app.startWithCamera) { showPreview(app.startWithCamera) }
        .onAppear { showPreview(app.startWithCamera) }
        .onDisappear { preview.session.stopRunning() }
    }

    /// The preview, with the two switches on it as the web puts them.
    private var stage: some View {
        ZStack(alignment: .bottom) {
            RoundedRectangle(cornerRadius: 12).fill(Theme.surface)
            if app.startWithCamera {
                SelfView(session: preview.session)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .transition(.opacity.animation(Theme.settle))
            } else {
                Image(systemName: "video.slash").font(.system(size: 26)).foregroundStyle(Theme.muted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .transition(.opacity.animation(Theme.settle))
            }
            HStack(spacing: 6) {
                toggle(on: app.startWithMicrophone, symbol: app.startWithMicrophone ? "mic.fill" : "mic.slash.fill",
                       label: app.startWithMicrophone ? t("Mute microphone") : t("Unmute microphone")) {
                    app.startWithMicrophone.toggle()
                }
                toggle(on: app.startWithCamera, symbol: app.startWithCamera ? "video.fill" : "video.slash.fill",
                       label: app.startWithCamera ? t("Turn camera off") : t("Turn camera on")) {
                    app.startWithCamera.toggle()
                }
            }
            .padding(5)
            .background(Capsule().fill(Theme.background.opacity(0.75)))
            .overlay(Capsule().strokeBorder(Theme.border))
            .padding(.bottom, 14)
        }
        .aspectRatio(16 / 9, contentMode: .fit)
        .animation(Theme.settle, value: app.startWithCamera)
        .rise()
    }

    /// Off is drawn in red, as the web draws a switched-off device here: this
    /// is the last moment to notice before everybody else does.
    private func toggle(on: Bool, symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 14, weight: .semibold))
                .foregroundStyle(on ? Theme.foreground : Theme.dangerForeground)
                .frame(width: 36, height: 36)
                .background(Circle().fill(on ? Theme.surfaceHigh : Theme.danger))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .help(label)
        .animation(Theme.settle, value: on)
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: 16) {
            Heading(title: app.room, detail: explanation, mono: true)
            if app.account == nil {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 0) {
                        TextField("", text: $app.name, prompt: Text(t("Your name")).foregroundStyle(Theme.muted))
                            .padding(.horizontal, 12)
                        Rectangle().fill(Theme.border).frame(width: 1)
                        HStack(spacing: 8) {
                            Image(systemName: "key").font(.system(size: 12))
                                .foregroundStyle(app.passphrase.isEmpty ? Theme.muted : Theme.tally)
                            SecureField("", text: $app.passphrase, prompt: Text(t("Passphrase")).foregroundStyle(Theme.muted))
                        }
                        .padding(.horizontal, 12)
                    }
                    .textFieldStyle(.plain)
                    .font(.system(size: 14))
                    .foregroundStyle(Theme.foreground)
                    .frame(height: 46)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Theme.surface))
                    .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.border))
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    #endif
                    Text(t("Add a passphrase to keep your name, and to run the room you open."))
                        .font(.system(size: 12)).foregroundStyle(Theme.muted)
                }
            }
            if sealing {
                LobbyInput(placeholder: t("Sealing word"), text: $app.sealingWord, secure: true).transition(Theme.arrive)
            } else {
                Button { sealing = true } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "lock").font(.system(size: 11))
                        Text(t("Seal this call")).font(.system(size: 12))
                    }
                    .foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
            }
            if let relays = app.relays, relays.relays.count > 1 {
                Picker(t("Server"), selection: $app.relayChoice) {
                    Text(t("Whichever answers fastest")).tag(String?.none)
                    ForEach(relays.relays.filter { !$0.maintenance }, id: \.name) { relay in
                        Text(say(relay.label ?? relay.name)).tag(String?.some(relay.name))
                    }
                }
                .font(.system(size: 13))
                .tint(Theme.foreground)
            }
            Button { Task { await join() } } label: {
                HStack {
                    if app.busy {
                        ProgressView().controlSize(.small).tint(Theme.background)
                    } else {
                        Text(t("Join"))
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(LobbyButtonStyle(tall: true))
            .disabled(app.busy || (app.account == nil && app.name.trimmingCharacters(in: .whitespaces).isEmpty))
            if app.canKnock {
                Button { Task { await app.knock() } } label: {
                    Text(t("Ask to be let in")).frame(maxWidth: .infinity)
                }
                .buttonStyle(LobbyButtonStyle(primary: false))
                .transition(Theme.arrive)
            }
            BackButton { app.backToLobby() }
        }
        .rise(0.08)
    }

    private var explanation: String? {
        if let invitation = app.invitation {
            if let name = invitation.name, !name.isEmpty {
                return t("You were invited to this room as {name}.", ["name": name])
            }
            return t("You were invited to this room. Choose a name and go in.")
        }
        return nil
    }

    private func showPreview(_ on: Bool) {
        let session = preview.session
        if on {
            if session.inputs.isEmpty, let device = AVCaptureDevice.default(for: .video), let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) {
                session.addInput(input)
            }
            DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
        } else {
            DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
        }
    }

    private func join() async {
        preview.session.stopRunning()
        await app.join()
    }
}

// MARK: - At the door

struct KnockingScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        LobbyFrame(app: app) {
            LanguageMenu()
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                Heading(title: app.room, mono: true).rise()
                Panel {
                    HStack(spacing: 12) {
                        ProgressView().controlSize(.small).tint(Theme.tally)
                        Text(t("Waiting for somebody to let you in.")).font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                    }
                }
                .rise(0.06)
                BackButton { app.stopKnocking() }.rise(0.12)
            }
        }
    }
}

// MARK: - An arranged meeting

struct WaitingScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        LobbyFrame(app: app) {
            LanguageMenu()
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                Heading(title: app.room, mono: true).rise()
                Panel {
                    if let arrangement = app.arrangement {
                        let when = AppModel.instant(arrangement.at).map { $0.formatted(date: .abbreviated, time: .shortened) } ?? arrangement.at
                        if arrangement.mine {
                            Text(t("Your meeting is arranged for {when}. Joining starts it.", ["when": when]))
                                .font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                            Button { Task { await app.startArranged() } } label: {
                                HStack {
                                    if app.busy {
                                        ProgressView().controlSize(.small).tint(Theme.background)
                                    } else {
                                        Text(t("Start the meeting"))
                                    }
                                }
                                .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(LobbyButtonStyle())
                        } else {
                            Text(t("This meeting is arranged for {when}.", ["when": when]))
                                .font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                            HStack(spacing: 10) {
                                ProgressView().controlSize(.small).tint(Theme.tally)
                                Text(t("Waiting for the host to start the meeting.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                            }
                        }
                    } else {
                        ProgressView().controlSize(.small).tint(Theme.muted).frame(maxWidth: .infinity)
                    }
                    if let says = app.waitingSays {
                        Text(says).font(.system(size: 12.5)).foregroundStyle(Theme.danger).transition(Theme.arrive)
                    }
                }
                .rise(0.06)
                BackButton { app.stopWaiting() }.rise(0.12)
            }
            .animation(Theme.settle, value: app.waitingSays)
            .animation(Theme.settle, value: app.arrangement?.mine)
        }
    }
}

// MARK: - After

struct EndedScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        LobbyFrame(app: app) {
            LanguageMenu()
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                Heading(title: t("You left the call.")).rise()
                Button { Task { await app.join() } } label: {
                    HStack {
                        if app.busy {
                            ProgressView().controlSize(.small).tint(Theme.background)
                        } else {
                            Text(t("Join again"))
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(LobbyButtonStyle(tall: true))
                .rise(0.06)
                BackButton { app.backToLobby() }.rise(0.12)
            }
        }
    }
}
