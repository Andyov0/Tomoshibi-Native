import AVFoundation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The whole app: one screen at a time, each arriving and leaving with the
/// same two curves.
public struct AppRoot: View {
    @Bindable var app: AppModel
    @State private var appearance = Appearance.shared
    @State private var localization = Localization.shared
    @Environment(\.scenePhase) private var scenePhase

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            switch app.screen {
            case .starting:
                StartingScreen().transition(Theme.arrive)
            case .server:
                ServerScreen(app: app).transition(Theme.arrive)
            case .lobby:
                LobbyView(app: app).transition(Theme.arrive)
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
        // The colour the person chose, for every control the system draws.
        .tint(appearance.accent.color)
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
        .environment(\.locale, localization.language.locale)
    }
}

/// A screen that is a single card of glass over the evening room.
private struct CardScreen<Content: View>: View {
    var width: CGFloat = 420
    @ViewBuilder var content: Content

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(spacing: 18) { content }
                    .padding(22)
                    .frame(maxWidth: width)
                    .glassPane(cornerRadius: 30)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 60)
                    .frame(maxWidth: .infinity, minHeight: 0)
            }
            .scrollIndicators(.never)
            .rise()
        }
        .overlay(alignment: .topTrailing) { LanguageMenu().padding(16) }
    }
}

/// The plainer screens' heading.
private struct Heading: View {
    let title: String
    var detail: String?
    var mono = false

    var body: some View {
        VStack(spacing: 6) {
            Text(title)
                .font(mono ? .system(size: 20, design: .monospaced) : .system(size: 24, weight: .bold))
                .multilineTextAlignment(.center)
                .lineLimit(2)
            if let detail {
                Text(detail).font(.system(size: 13.5)).foregroundStyle(Theme.muted)
                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity)
    }
}

/// A quiet way back.
private struct BackButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(t("Back"), systemImage: "chevron.left").font(.system(size: 13))
        }
        .buttonStyle(.glassCapsule(32))
    }
}

// MARK: - Which deployment

struct ServerScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Mark(size: 56)
            Heading(title: t("Server"), detail: t("The server address"))
            TextField("", text: $app.serverAddress,
                      prompt: Text(AppModel.home.replacingOccurrences(of: "https://", with: "")).foregroundStyle(Theme.muted))
                .textFieldStyle(.plain)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                #endif
                .onSubmit { Task { await app.connect() } }
                .padding(.horizontal, 16).frame(height: 46)
                .background(Capsule().fill(Color.black.opacity(0.22)))
                .overlay(Capsule().stroke(Color.white.opacity(0.12)))
            Button { Task { await app.connect() } } label: {
                Group {
                    if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Connect")) }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassCapsule(48, prominent: true))
            .disabled(app.busy || app.serverAddress.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .toast(app)
    }
}

// MARK: - Arriving through a link

/// The join screen for somebody who came through a link (PreJoin.tsx): the
/// picture of yourself, what you are invited to, your name, and in.
struct PreJoinScreen: View {
    @Bindable var app: AppModel
    @State private var sealing = false

    var body: some View {
        #if os(macOS)
        CardScreen(width: 560) { content }
            .toast(app)
        #else
        ZStack(alignment: .bottom) {
            CameraStage(app: app).ignoresSafeArea()
            VStack(spacing: 16) { content }
                .padding(18)
                .glassPane(cornerRadius: 34)
                .padding(10)
        }
        .toast(app)
        #endif
    }

    @ViewBuilder
    private var content: some View {
        Heading(title: app.room, detail: explanation, mono: true)
        #if os(macOS)
        CameraStage(app: app, cornerRadius: 20)
            .aspectRatio(16 / 9, contentMode: .fit)
            .overlay(alignment: .bottom) {
                DeviceSwitches(app: app, size: 38).padding(6).glassSurface(Capsule()).padding(.bottom, 12)
            }
        #else
        DeviceSwitches(app: app, size: 52)
        #endif
        VStack(spacing: 0) {
            if app.account == nil {
                row(t("Your name")) {
                    TextField("", text: $app.name).textFieldStyle(.plain).multilineTextAlignment(.trailing)
                }
                Divider().overlay(Color.white.opacity(0.06))
                row(t("Passphrase")) {
                    SecureField("", text: $app.passphrase).textFieldStyle(.plain).multilineTextAlignment(.trailing)
                }
                Divider().overlay(Color.white.opacity(0.06))
            }
            row(t("Server")) { ServerMenu(app: app, compact: true) }
            Divider().overlay(Color.white.opacity(0.06))
            row(t("Seal this call")) { Toggle("", isOn: $sealing).labelsHidden().toggleStyle(.switch) }
            if sealing {
                Divider().overlay(Color.white.opacity(0.06))
                row(t("Sealing word")) {
                    SecureField("", text: $app.sealingWord).textFieldStyle(.plain).multilineTextAlignment(.trailing)
                }
                .transition(Theme.arrive)
            }
        }
        .font(.system(size: 14))
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        if app.account == nil {
            Text(t("Add a passphrase to keep your name, and to run the room you open."))
                .font(.system(size: 12)).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
        }
        if let problem = app.problem {
            Text(problem).font(.system(size: 12.5)).foregroundStyle(Theme.danger).multilineTextAlignment(.center)
                .transition(Theme.arrive)
        }
        Button { Task { await app.join() } } label: {
            Group {
                if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Join")) }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.glassCapsule(52, prominent: true))
        .disabled(app.busy || (app.account == nil && app.name.trimmingCharacters(in: .whitespaces).isEmpty))
        .keyboardShortcut(.defaultAction)
        if app.canKnock {
            Button { Task { await app.knock() } } label: { Text(t("Ask to be let in")).frame(maxWidth: .infinity) }
                .buttonStyle(.glassCapsule(44))
                .transition(Theme.arrive)
        }
        BackButton { app.backToLobby() }
    }

    private func row<Control: View>(_ label: String, @ViewBuilder control: () -> Control) -> some View {
        HStack(spacing: 12) {
            Text(label).fixedSize()
            Spacer(minLength: 8)
            control()
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 46)
    }

    private var explanation: String? {
        guard let invitation = app.invitation else { return nil }
        if let name = invitation.name, !name.isEmpty {
            return t("You were invited to this room as {name}.", ["name": name])
        }
        return t("You were invited to this room. Choose a name and go in.")
    }
}

// MARK: - At the door

struct KnockingScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Heading(title: app.room, mono: true)
            HStack(spacing: 12) {
                ProgressView().controlSize(.small)
                Text(t("Waiting for somebody to let you in.")).font(.system(size: 14))
            }
            .padding(.vertical, 8)
            BackButton { app.stopKnocking() }
        }
    }
}

// MARK: - An arranged meeting

struct WaitingScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Heading(title: app.room, mono: true)
            if let arrangement = app.arrangement {
                let when = AppModel.instant(arrangement.at).map(LobbyModel.said) ?? arrangement.at
                if arrangement.mine {
                    Text(t("Your meeting is arranged for {when}. Joining starts it.", ["when": when]))
                        .font(.system(size: 14)).multilineTextAlignment(.center)
                    Button { Task { await app.startArranged() } } label: {
                        Group {
                            if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Start the meeting")) }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.glassCapsule(48, prominent: true))
                } else {
                    Text(t("This meeting is arranged for {when}.", ["when": when]))
                        .font(.system(size: 14)).multilineTextAlignment(.center)
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text(t("Waiting for the host to start the meeting.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    }
                }
            } else {
                ProgressView().controlSize(.small)
            }
            if let says = app.waitingSays {
                Text(says).font(.system(size: 12.5)).foregroundStyle(Theme.danger).transition(Theme.arrive)
            }
            BackButton { app.stopWaiting() }
        }
        .animation(Theme.settle, value: app.waitingSays)
        .animation(Theme.settle, value: app.arrangement?.mine)
    }
}

// MARK: - After

struct EndedScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Image(systemName: "phone.down.fill").font(.system(size: 28)).foregroundStyle(Theme.muted)
                .frame(width: 64, height: 64)
                .glassSurface(Circle())
            Heading(title: t("You left the call."))
            Button { Task { await app.join() } } label: {
                Group {
                    if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Join again")) }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassCapsule(48, prominent: true))
            BackButton { app.backToLobby() }
        }
    }
}
