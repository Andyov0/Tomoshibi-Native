import AVFoundation
import SwiftUI
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
            case .server:
                ServerScreen(app: app).transition(Theme.arrive)
            case .lobby:
                LobbyScreen(app: app).transition(Theme.arrive)
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
        .preferredColorScheme(.dark)
        .task { await app.start() }
        // Readings taken while the app is in the background say nothing about
        // a freeze (the web skips a hidden tab); inactive is still on screen.
        .onChange(of: scenePhase) { app.call?.call.setBackground(background: scenePhase == .background) }
        .onOpenURL { url in Task { await app.open(url) } }
        .environment(\.locale, Localization.shared.language.locale)
    }
}

/// A column of fields on a card, the shape every screen before a call shares.
struct Card<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 14) { content }
            .padding(24)
            .frame(maxWidth: 420)
            .background(RoundedRectangle(cornerRadius: 14).fill(Theme.surface))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Theme.border))
            .padding(16)
    }
}

struct Field: View {
    let label: String
    @Binding var text: String
    var secure = false
    var prompt: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.system(size: 12, weight: .medium)).foregroundStyle(Theme.muted)
            Group {
                if secure {
                    SecureField(prompt ?? "", text: $text)
                } else {
                    TextField(prompt ?? "", text: $text)
                }
            }
            .textFieldStyle(.plain)
            .font(.system(size: 14))
            .foregroundStyle(Theme.foreground)
            .padding(.horizontal, 12).padding(.vertical, 9)
            .background(RoundedRectangle(cornerRadius: 8).fill(Theme.background))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.border))
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            #endif
            .accessibilityLabel(label)
        }
    }
}

struct PrimaryButton: View {
    let title: String
    var busy = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) }
                Text(title).font(.system(size: 14, weight: .semibold))
            }
            .foregroundStyle(Theme.tallyForeground)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 10)
            .background(RoundedRectangle(cornerRadius: 8).fill(Theme.tally))
        }
        .buttonStyle(.plain)
        .disabled(busy)
    }
}

struct Problem: View {
    let text: String?

    var body: some View {
        if let text {
            Text(text)
                .font(.system(size: 12.5))
                .foregroundStyle(Theme.danger)
                .transition(Theme.arrive)
        }
    }
}

// MARK: - Which deployment

struct ServerScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        Card {
            Text("Tomoshibi").font(.system(size: 22, weight: .semibold)).foregroundStyle(Theme.foreground)
            Field(label: t("Server"), text: $app.serverAddress, prompt: t("The server address"))
                .onSubmit { Task { await app.connect() } }
            Problem(text: app.problem)
            PrimaryButton(title: t("Connect"), busy: app.busy) { Task { await app.connect() } }
                .disabled(app.serverAddress.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .animation(Theme.settle, value: app.problem)
    }
}

// MARK: - Lobby

struct LobbyScreen: View {
    @Bindable var app: AppModel
    @State private var roomName = ""
    @State private var signInName = ""
    @State private var signInPassphrase = ""

    var body: some View {
        Card {
            HStack {
                Text(app.account.map { t("Hello, {name}", ["name": $0.name]) } ?? t("Join a meeting"))
                    .font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(app.deployment?.baseUrl().replacingOccurrences(of: "https://", with: "") ?? "") { app.forgetServer() }
                    .buttonStyle(.plain).font(.system(size: 11)).foregroundStyle(Theme.muted)
            }
            Field(label: t("Room name"), text: $roomName, prompt: t("Name it yourself, or leave it blank for one nobody has used."))
                .onSubmit(go)
            Problem(text: app.problem)
            PrimaryButton(title: roomName.isEmpty ? t("Start a meeting") : t("Join"), busy: app.busy, action: go)

            Divider().overlay(Theme.border).padding(.vertical, 4)
            if let account = app.account {
                HStack {
                    Text(account.name).foregroundStyle(Theme.foreground)
                    Spacer()
                    Button(t("Sign out")) { Task { await app.signOut() } }.buttonStyle(.plain).foregroundStyle(Theme.muted)
                }
                .font(.system(size: 13))
                .transition(Theme.arrive)
                // The account's own page and the management pages stay on the
                // web, where they already are; the app opens them there.
                HStack(spacing: 14) {
                    if let base = app.deployment?.baseUrl(), let url = URL(string: base + "/account") {
                        Link(t("User console"), destination: url)
                    }
                    if account.admin, let base = app.deployment?.baseUrl(), let url = URL(string: base + "/admin") {
                        Link(t("Admin console"), destination: url)
                    }
                }
                .font(.system(size: 12.5))
                .foregroundStyle(Theme.muted)
                .transition(Theme.arrive)
            } else {
                Field(label: t("Name"), text: $signInName)
                Field(label: t("Passphrase"), text: $signInPassphrase, secure: true)
                Button(t("Sign in")) { Task { await app.signIn(name: signInName, passphrase: signInPassphrase) } }
                    .buttonStyle(.plain)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: app.account?.name)
        .animation(Theme.settle, value: app.problem)
    }

    private func go() {
        let wanted = roomName.trimmingCharacters(in: .whitespaces)
        app.prepare(room: wanted.isEmpty ? generateRoomName() : wanted)
    }
}

// MARK: - Before joining

struct PreJoinScreen: View {
    @Bindable var app: AppModel
    @State private var preview = CameraCapture()
    @State private var sealing = false

    var body: some View {
        ScrollView {
            Card {
                Text(app.room).font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.foreground)
                if let invitation = app.invitation, let name = invitation.name, !name.isEmpty {
                    Text(t("You were invited to this room as {name}.", ["name": name]))
                        .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                }
                ZStack {
                    RoundedRectangle(cornerRadius: Theme.tileRadius).fill(Theme.background)
                    if app.startWithCamera {
                        SelfView(session: preview.session).transition(.opacity.animation(Theme.settle))
                    } else {
                        Avatar(name: app.name, identity: app.name)
                    }
                }
                .aspectRatio(16 / 9, contentMode: .fit)
                .animation(Theme.settle, value: app.startWithCamera)
                HStack(spacing: 8) {
                    Toggle(isOn: $app.startWithMicrophone) { Image(systemName: app.startWithMicrophone ? "mic.fill" : "mic.slash.fill") }
                        .toggleStyle(.button)
                        .accessibilityLabel(app.startWithMicrophone ? t("Mute microphone") : t("Unmute microphone"))
                    Toggle(isOn: $app.startWithCamera) { Image(systemName: app.startWithCamera ? "video.fill" : "video.slash.fill") }
                        .toggleStyle(.button)
                        .accessibilityLabel(app.startWithCamera ? t("Turn camera off") : t("Turn camera on"))
                }
                if app.account == nil {
                    Field(label: t("Your name"), text: $app.name)
                    Field(label: t("Passphrase"), text: $app.passphrase, secure: true)
                    Text(t("Add a passphrase to keep your name, and to run the room you open."))
                        .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                }
                if sealing {
                    Field(label: t("Sealing word"), text: $app.sealingWord, secure: true).transition(Theme.arrive)
                } else {
                    Button(t("Seal this call")) { sealing = true }
                        .buttonStyle(.plain).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                }
                if let relays = app.relays, !relays.relays.isEmpty {
                    Picker(t("Server"), selection: $app.relayChoice) {
                        Text(t("Whichever answers fastest")).tag(String?.none)
                        ForEach(relays.relays.filter { !$0.adminOnly && !$0.maintenance }, id: \.name) { relay in
                            Text(relay.label ?? relay.name).tag(String?.some(relay.name))
                        }
                    }
                    .font(.system(size: 13))
                }
                Problem(text: app.problem)
                PrimaryButton(title: t("Join"), busy: app.busy) { Task { await join() } }
                    .disabled(app.account == nil && app.name.trimmingCharacters(in: .whitespaces).isEmpty)
                if app.canKnock {
                    Button(t("Ask to be let in")) { Task { await app.knock() } }
                        .buttonStyle(.plain).font(.system(size: 13, weight: .medium)).foregroundStyle(Theme.foreground)
                        .transition(Theme.arrive)
                }
                Button(t("Back")) { app.backToLobby() }
                    .buttonStyle(.plain).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
            }
            .frame(maxWidth: .infinity)
        }
        .animation(Theme.settle, value: sealing)
        .animation(Theme.settle, value: app.problem)
        .onChange(of: app.startWithCamera) { showPreview(app.startWithCamera) }
        .onAppear { showPreview(app.startWithCamera) }
        .onDisappear { preview.session.stopRunning() }
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
        Card {
            Text(app.room).font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.foreground)
            HStack(spacing: 10) {
                ProgressView().controlSize(.small)
                Text(t("Waiting for somebody to let you in.")).font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
            }
            Button(t("Back")) { app.stopKnocking() }
                .buttonStyle(.plain).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
        }
    }
}

// MARK: - An arranged meeting

struct WaitingScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        Card {
            Text(app.room).font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.foreground)
            if let arrangement = app.arrangement {
                let when = AppModel.instant(arrangement.at).map { $0.formatted(date: .abbreviated, time: .shortened) } ?? arrangement.at
                if arrangement.mine {
                    Text(t("Your meeting is arranged for {when}. Joining starts it.", ["when": when]))
                        .font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                    PrimaryButton(title: t("Start the meeting"), busy: app.busy) { Task { await app.startArranged() } }
                } else {
                    Text(t("This meeting is arranged for {when}.", ["when": when]))
                        .font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text(t("Waiting for the host to start the meeting.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    }
                }
            } else {
                ProgressView().controlSize(.small)
            }
            Problem(text: app.waitingSays)
            Button(t("Back")) { app.stopWaiting() }
                .buttonStyle(.plain).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
        }
        .animation(Theme.settle, value: app.waitingSays)
        .animation(Theme.settle, value: app.arrangement?.mine)
    }
}

// MARK: - After

struct EndedScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        Card {
            Text(t("You left the call.")).font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.foreground)
            PrimaryButton(title: t("Join again"), busy: app.busy) { Task { await app.join() } }
            Button(t("Back")) { app.backToLobby() }
                .buttonStyle(.plain).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
        }
    }
}
