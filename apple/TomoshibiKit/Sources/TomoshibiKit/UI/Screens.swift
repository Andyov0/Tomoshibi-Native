import AVFoundation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// The whole app: one screen at a time, each arriving and leaving with the
/// same two curves.
public struct AppRoot: View {
    @Bindable var app: AppModel
    @State private var appearance = Appearance.shared
    @State private var localization = Localization.shared
    @Environment(\.scenePhase) private var scenePhase
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #endif

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
                if let call = app.call {
                    RoomView(model: call)
                        .transition(Theme.arrive)
                        .onChange(of: call.endedReason) {
                            guard let reason = call.endedReason else { return }
                            // A move the server announced comes back on its
                            // own (CallModel's `onMoved`); anything else is the
                            // end of this call, and says which end it was.
                            let gone = ["RoomDeleted", "RoomClosed", "ParticipantRemoved"].contains { reason.contains($0) }
                            if let moving = call.moving, Date().timeIntervalSince(moving.at) < 15, gone { return }
                            Task { await app.ended(because: reason) }
                        }
                } else if let to = app.movingTo {
                    MovingOverlay(to: to).transition(Theme.arrive)
                } else {
                    // Never nothing. A call screen with no call and no move in
                    // progress was a black window after a rejoin that failed;
                    // the join screen, with the reason on it, is where that
                    // person can do something.
                    PreJoinScreen(app: app).transition(Theme.arrive)
                }
            case .ended:
                EndedScreen(app: app).transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: app.screen)
        .animation(Theme.settle, value: app.account?.name)
        .animation(Theme.settle, value: app.call == nil)
        .preferredColorScheme(.dark)
        // The colour the person chose, for every control the system draws.
        .tint(appearance.accent.color)
        .task {
            await app.start()
            #if os(macOS) && DEBUG
            DebugWindow.size()
            if LobbyDebug.open == "settings" { openSettings() }
            #endif
        }
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

#if os(macOS) && DEBUG
/// TOMOSHIBI_DEBUG_WINDOW=<width>x<height> sizes the main window, for a test
/// to look at a screen at more than one size without a pointer.
enum DebugWindow {
    @MainActor
    static func size() {
        guard let size = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_WINDOW"]?.split(separator: "x").compactMap({ Double($0) }),
              size.count == 2,
              let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) else { return }
        window.setContentSize(NSSize(width: size[0], height: size[1]))
    }
}
#endif

/// A screen that is a single card of glass over the evening room.
private struct CardScreen<Content: View>: View {
    var width: CGFloat = 420
    @ViewBuilder var content: Content
    @State private var popovers = Popovers()

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
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
    }
}

/// The join screens on the phone: the picture of yourself filling the
/// screen, and a pane of glass at its foot that scrolls where it is taller
/// than the room left above the keyboard.
private struct PaneScreen<Content: View>: View {
    @Bindable var app: AppModel
    @ViewBuilder var content: Content
    @State private var popovers = Popovers()

    var body: some View {
        ZStack {
            CameraStage(app: app).ignoresSafeArea()
            GeometryReader { geometry in
                ScrollView {
                    VStack(spacing: 0) {
                        Spacer(minLength: 0)
                        VStack(spacing: 16) { content }
                            .padding(18)
                            .glassPane(cornerRadius: 34)
                            .padding(10)
                    }
                    .frame(minHeight: geometry.size.height)
                }
                .scrollIndicators(.never)
                .scrollDismissesKeyboard(.interactively)
            }
            .rise()
        }
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
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

/// A line under the room saying something about it, in the muted voice.
private struct Note: View {
    let text: String
    var danger = false

    var body: some View {
        Text(text)
            .font(.system(size: 12.5))
            .foregroundStyle(danger ? Theme.danger : Theme.muted)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity)
    }
}

// MARK: - Which deployment

struct ServerScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Mark(size: 56)
            Heading(title: t("Server"), detail: t("The server address"))
            GlassField(prompt: AppModel.home.replacingOccurrences(of: "https://", with: ""), text: $app.serverAddress,
                       symbol: "server.rack", height: 46) { Task { await app.connect() } }
                #if os(iOS)
                .keyboardType(.URL)
                #endif
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
    @State private var sealing: Bool

    init(app: AppModel) {
        self.app = app
        // A word already typed for this sitting is shown, and the switch on.
        _sealing = State(initialValue: !app.sealingWord.isEmpty)
    }

    var body: some View {
        #if os(macOS)
        CardScreen(width: 560) { content }
        #else
        PaneScreen(app: app) { content }
        #endif
    }

    @ViewBuilder
    private var content: some View {
        RoomHeading(app: app, room: app.room, door: door)
        #if os(macOS)
        CameraStage(app: app, cornerRadius: 20)
            .aspectRatio(16 / 9, contentMode: .fit)
            .overlay(alignment: .bottom) {
                DeviceSwitches(app: app, size: 38).padding(6).glassSurface(Capsule()).padding(.bottom, 12)
            }
        #else
        DeviceSwitches(app: app, size: 52)
        #endif
        if app.account == nil {
            VStack(spacing: 8) {
                // Somebody on an invitation has a name to choose and nothing
                // else to answer: the link is what lets them in, and a
                // passphrase field beside it reads as something they forgot.
                // A name the link was made for is shown and cannot be changed,
                // because the server signs that name in whatever is typed.
                IdentityBox(name: Binding(get: { app.invitedName ?? app.name }, set: { app.name = $0 }),
                            passphrase: $app.passphrase,
                            nameOnly: app.arrivedInvited,
                            fixed: app.invitedName != nil,
                            onSubmit: join)
                Note(text: identityLine)
                    .contentTransition(.opacity)
                if let window = linkWindow {
                    Note(text: window)
                }
            }
            .animation(Theme.settle, value: app.passphrase.isEmpty)
        }
        VStack(spacing: 0) {
            // Where the call is held, only for somebody who has not already
            // chosen it in the lobby, and only where there is a choice.
            if app.account == nil, app.offersRelayChoice {
                HStack(spacing: 12) {
                    Image(systemName: "server.rack").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted).frame(width: 18)
                    Text(t("Server")).font(.system(size: 14))
                    Spacer(minLength: 8)
                    ServerMenu(app: app, compact: true, id: "prejoin.server")
                }
                .padding(.horizontal, 14).frame(minHeight: 48)
                Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
            }
            SealControl(on: $sealing, word: $app.sealingWord)
                .padding(.horizontal, 14).padding(.vertical, 12)
        }
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        if let problem = app.problem {
            ProblemLine(app: app, text: problem)
        }
        Button(action: join) {
            Group {
                if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Join")) }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.glassCapsule(52, prominent: true))
        .disabled(app.busy || app.joiningName.isEmpty)
        .keyboardShortcut(.defaultAction)
        if app.canKnock {
            Button { Task { await app.knock() } } label: { Text(t("Ask to be let in")).frame(maxWidth: .infinity) }
                .buttonStyle(.glassCapsule(44))
                .disabled(app.joiningName.isEmpty)
                .transition(Theme.arrive)
        }
        BackButton { app.backToLobby() }
    }

    private func join() {
        guard !app.busy, !app.joiningName.isEmpty else { return }
        if !sealing { app.sealingWord = "" }
        Task { await app.join() }
    }

    /// PreJoin.tsx's one line under the identity, which changes rather than
    /// doubling: what a passphrase is for until there is one, and what it
    /// does once there is.
    private var identityLine: String {
        if let invited = app.invitedName { return t("You were invited to this room as {name}.", ["name": invited]) }
        if app.arrivedInvited { return t("You were invited to this room. Choose a name and go in.") }
        if !app.passphrase.isEmpty {
            return t("Only you can join as {name}", ["name": app.joiningName.isEmpty ? "?" : app.joiningName])
        }
        return t("Add a passphrase to keep your name, and to run the room you open.")
    }

    /// When the link stops working, where it does, in the reader's own clock:
    /// a guest who arrives early for a call at four should know the link will
    /// still be good at four. A link with no end says nothing.
    private var linkWindow: String? {
        guard app.arrivedInvited, let until = app.invitation?.until.flatMap(AppModel.instant) else { return nil }
        if let from = app.invitation?.from.flatMap(AppModel.instant) {
            return t("This link works from {from} until {until}.", ["from": LobbyModel.said(from), "until": LobbyModel.said(until)])
        }
        return t("This link works until {until}.", ["until": LobbyModel.said(until)])
    }

    /// RoomTitle.tsx: what this name is worth at this deployment's door, said
    /// before anybody presses Join. Not to somebody holding an invitation,
    /// whom it would tell to go and do what they just did, nor to somebody
    /// signed in, whose door the session already answers.
    private var door: String? {
        guard app.account == nil, !app.arrivedInvited, let info = app.info else { return nil }
        let scope = scopeOf(name: app.room)
        if !scope.isEmpty { return t("Only members of {scope} can join this room.", ["scope": scope]) }
        switch info.joinedBy {
        case .accounts: return t("Sign in to join a room here.")
        case .invited: return t("Rooms here are by invitation. Open the link you were sent, or sign in.")
        case .anyone: break
        }
        switch info.openedBy {
        case .admins: return t("Only administrators can start new rooms. Enter the name you were given.")
        case .signed: return t("Set a passphrase to start a new room. Anybody can join one that already exists.")
        default: return looksGenerated(name: app.room) ? nil : t("Anyone who guesses this name can join.")
        }
    }
}

/// The room, as the heading of the screen, in the machine's monospace, with
/// the way to copy its address where a plain address is a way in.
private struct RoomHeading: View {
    let app: AppModel
    let room: String
    var door: String?

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 8) {
                Text(room)
                    .font(.system(size: 20, design: .monospaced))
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                if let link {
                    CopyLinkButton(link: link, size: 30)
                }
            }
            if let door {
                Text(door).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
                    .transition(Theme.arrive)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// RoomTitle.tsx `useLinkWorks`: only where anybody may come in by the
    /// name, and never for a room held under a scope. Where the door asks
    /// for an invitation the copied address opens for nobody, and whoever
    /// sends it has invited a person to be turned away while believing they
    /// invited them.
    private var link: String? {
        guard app.info?.joinedBy == .anyone, scopeOf(name: room).isEmpty, !room.isEmpty,
              let base = app.deployment?.baseUrl() else { return nil }
        return (base.hasSuffix("/") ? base : base + "/") + "#/" + room
    }
}

/// Who somebody is, as one control (Identity.tsx): a name and the passphrase
/// that proves it are one answer, so they are one box with a hairline through
/// it. The key lights once there is a passphrase, and an eye offers to show it
/// once there is something to show.
private struct IdentityBox: View {
    @Binding var name: String
    @Binding var passphrase: String
    var nameOnly = false
    var fixed = false
    var onSubmit: () -> Void = {}
    @State private var shown = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: fixed ? "person.fill.checkmark" : "person").font(.system(size: 15))
                    .foregroundStyle(fixed ? Theme.tally : Theme.muted).frame(width: 18)
                if fixed {
                    Text(name).font(.system(size: 15)).foregroundStyle(Theme.muted)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                } else {
                    TextField("", text: $name, prompt: Text(t("Your name")).foregroundStyle(Theme.muted))
                        .textContentType(.username)
                        .onSubmit(onSubmit)
                }
            }
            .modifier(IdentityRow())
            if !nameOnly {
                Rectangle().fill(Color.white.opacity(0.07)).frame(height: 1)
                HStack(spacing: 12) {
                    Image(systemName: "key").font(.system(size: 15))
                        .foregroundStyle(passphrase.isEmpty ? Theme.muted : Theme.tally)
                        .frame(width: 18)
                    Group {
                        if shown {
                            TextField("", text: $passphrase, prompt: Text(t("Passphrase")).foregroundStyle(Theme.muted))
                        } else {
                            SecureField("", text: $passphrase, prompt: Text(t("Passphrase")).foregroundStyle(Theme.muted))
                        }
                    }
                    .textContentType(.password)
                    .onSubmit(onSubmit)
                    if !passphrase.isEmpty {
                        Button { shown.toggle() } label: {
                            Image(systemName: shown ? "eye.slash" : "eye").font(.system(size: 13)).foregroundStyle(Theme.muted)
                                .frame(width: 24, height: 24).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(shown ? t("Hide passphrase") : t("Show passphrase"))
                        .transition(Theme.arrive)
                    }
                }
                .modifier(IdentityRow())
                .transition(Theme.arrive)
            }
        }
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.10)))
        .animation(Theme.settle, value: passphrase.isEmpty)
        .animation(Theme.settle, value: nameOnly)
    }
}

private struct IdentityRow: ViewModifier {
    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .font(.system(size: 15))
            .foregroundStyle(Theme.foreground)
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            #endif
            .padding(.horizontal, 16)
            .frame(height: 50)
    }
}

/// Why the last attempt did not take, in red under the form; one that is about
/// why they are not in the meeting can be closed rather than fading.
private struct ProblemLine: View {
    let app: AppModel
    let text: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: "exclamationmark.circle.fill").font(.system(size: 12.5))
            Text(text).font(.system(size: 12.5)).fixedSize(horizontal: false, vertical: true)
            if app.problemStays {
                Button { app.problem = nil } label: {
                    Image(systemName: "xmark").font(.system(size: 9.5, weight: .bold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close"))
            }
        }
        .foregroundStyle(Theme.danger)
        .multilineTextAlignment(.leading)
        .padding(.horizontal, 12).padding(.vertical, 9)
        .frame(maxWidth: .infinity)
        .background(RoundedRectangle(cornerRadius: 13, style: .continuous).fill(Theme.danger.opacity(0.10)))
        .transition(Theme.arrive)
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

/// Waiting.tsx inside PreJoin.tsx: the ordinary join screen, because the
/// point of arriving early is to have the camera, the microphone and the name
/// settled before the meeting, with a button that waits. Nothing is asked of
/// the door until it is pressed, so nobody is joined with a name they were
/// still typing.
struct WaitingScreen: View {
    @Bindable var app: AppModel
    @State private var sealing: Bool

    init(app: AppModel) {
        self.app = app
        _sealing = State(initialValue: !app.sealingWord.isEmpty)
    }

    var body: some View {
        #if os(macOS)
        CardScreen(width: 560) { content }
        #else
        PaneScreen(app: app) { content }
        #endif
    }

    @ViewBuilder
    private var content: some View {
        Heading(title: app.room, mono: true)
        #if os(macOS)
        CameraStage(app: app, cornerRadius: 20)
            .aspectRatio(16 / 9, contentMode: .fit)
            .overlay(alignment: .bottom) {
                DeviceSwitches(app: app, size: 38).padding(6).glassSurface(Capsule()).padding(.bottom, 12)
            }
        #else
        DeviceSwitches(app: app, size: 52)
        #endif
        if app.account == nil {
            // A guest with no saved name gives one here; the button waits for
            // it (Waiting.tsx: "Nothing is asked until there is one").
            IdentityBox(name: $app.name, passphrase: $app.passphrase, nameOnly: true)
        }
        VStack(spacing: 0) {
            SealControl(on: $sealing, word: $app.sealingWord)
                .padding(.horizontal, 14).padding(.vertical, 12)
        }
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        // The host's button comes alive when the window opens, without
        // anybody reloading anything: a clock, every thirty seconds.
        TimelineView(.periodic(from: .now, by: 30)) { timeline in
            VStack(spacing: 12) { state(now: timeline.date) }
        }
        if let problem = app.problem {
            ProblemLine(app: app, text: problem)
        }
        BackButton { app.stopWaiting() }
    }

    @ViewBuilder
    private func state(now: Date) -> some View {
        let waiting = app.waiting
        switch waiting.gone {
        case .cancelled:
            Note(text: t("That invitation is no longer good. Ask for another.")).transition(Theme.arrive)
        case .ended:
            Note(text: t("That meeting has ended.")).transition(Theme.arrive)
        case .notComing:
            VStack(spacing: 10) {
                Note(text: t("The host has not started this meeting."))
                Button { app.askAgain() } label: { Text(t("Ask again")).frame(maxWidth: .infinity) }
                    .buttonStyle(.glassCapsule(44))
            }
            .transition(Theme.arrive)
        case nil:
            let tooEarly = app.arrangement?.mine == true && !app.arrangedReady && (app.arrangedOpens.map { now < $0 } ?? false)
            if let arrangement = app.arrangement, !app.arrangedReady {
                let when = AppModel.instant(arrangement.at).map(LobbyModel.said) ?? arrangement.at
                Note(text: arrangement.mine
                     ? tooEarly
                        ? t("Your meeting can start from {when}.", ["when": app.arrangedOpens.map(LobbyModel.said) ?? when])
                        : t("Your meeting is arranged for {when}. Joining starts it.", ["when": when])
                     : t("This meeting is arranged for {when}.", ["when": when]))
                    .transition(Theme.arrive)
            } else if app.arrangement == nil {
                if waiting.unreachable {
                    Note(text: t("Could not reach the meeting. Trying again.")).transition(Theme.arrive)
                } else {
                    ProgressView().controlSize(.small)
                }
            }
            Button { Task { await app.goArranged() } } label: {
                HStack(spacing: 8) {
                    if waiting.asking || app.busy {
                        ProgressView().controlSize(.small).tint(Theme.tallyForeground)
                    }
                    Text(waiting.asking ? t("Waiting for the host to start the meeting.")
                         : app.arrangedReady ? t("Join")
                         : app.arrangement?.mine == true ? t("Start the meeting")
                         : t("Wait for the host"))
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassCapsule(52, prominent: true))
            .disabled(app.joiningName.isEmpty || waiting.asking || app.busy || app.arrangement == nil || tooEarly)
            .keyboardShortcut(.defaultAction)
        }
    }
}

// MARK: - After

struct EndedScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Image(systemName: symbol).font(.system(size: 26, weight: .medium))
                .foregroundStyle(app.ending == .left ? Theme.muted : Theme.foreground)
                .frame(width: 64, height: 64)
                .glassSurface(Circle())
            Heading(title: title, detail: app.room.isEmpty ? nil : app.room)
            // Back in where that is something to do: after leaving, and after a
            // connection that dropped, which is one press back with the
            // devices already chosen (App.tsx). Not into a room the host
            // closed, or one somebody was put out of.
            if app.ending == .left || app.ending == .lost {
                Button { Task { await app.join() } } label: {
                    Group {
                        if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Join again")) }
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(48, prominent: true))
            }
            if let problem = app.problem {
                ProblemLine(app: app, text: problem)
            }
            BackButton { app.backToLobby() }
        }
    }

    /// App.tsx's three endings, and the one somebody chose.
    private var title: String {
        switch app.ending {
        case .left: t("You left the call.")
        case .closed: t("The host closed this room.")
        case .removed: t("You were removed from this room.")
        case .lost: t("The connection to this room was lost.")
        }
    }

    private var symbol: String {
        switch app.ending {
        case .left: "phone.down.fill"
        case .closed: "door.left.hand.closed"
        case .removed: "person.crop.circle.badge.xmark"
        case .lost: "wifi.exclamationmark"
        }
    }
}
