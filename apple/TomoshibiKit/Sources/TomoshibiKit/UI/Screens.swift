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
                // The whole window, for a rehearsed fold to be drawn down its
                // middle (FoldDebug).
                .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame in
                    FoldDebug.window = frame.size
                    #if os(iOS) && DEBUG
                    // Again on a window that moved: the app moved to another
                    // display can be in a scene of its own.
                    DebugTraits.apply()
                    #endif
                }
            switch app.screen {
            case .starting:
                StartingScreen().transition(Theme.arrive)
            case .unreachable:
                UnreachableScreen(app: app).transition(Theme.arrive)
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
                            Task { await app.ended(because: reason, in: call) }
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
            // Over everything while an update is put in place.
            if let updates = app.updates { UpdateInstalling(updates: updates) }
        }
        .animation(Theme.settle, value: app.screen)
        .animation(Theme.settle, value: app.account?.name)
        .animation(Theme.settle, value: app.call == nil)
        .preferredColorScheme(.dark)
        // A page that fits stays put. The sign-in, the join screens and the
        // cards around a call are in scroll views so that a phone on its side
        // can still reach the foot of them, and left to the default every one
        // of them rubber-banded under a finger or a trackpad even when there
        // was nothing more to see, which read as the page being loose. Where
        // the content is taller than the window, it scrolls as before.
        .scrollBounceBehavior(.basedOnSize)
        // The colour the person chose, for every control the system draws.
        .tint(appearance.accent.color)
        .task {
            #if os(iOS) && DEBUG
            DebugTraits.apply()
            #endif
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
        // A `tomoshibi:` link, and a universal link to the deployment
        // (`https://meet.example.com/?invite=T#/room`, `?meeting=`), which the
        // system hands over once the domain's association file names the app
        // (the Associated Domains entitlement, project.yml). Both are opened
        // the one way, `open`, which follows an https link and the scheme
        // alike, so a universal link lands exactly where the same link under
        // `tomoshibi:` does.
        .onOpenURL { url in receive(url) }
        // Apple says SwiftUI hands a universal link to `onOpenURL` as a URL.
        // A browsing-web activity given to SwiftUI's own delegates, as the
        // system gives one (AppDebug `activity`), reached neither handler
        // without this one, on the iPhone and the Mac alike, and the link
        // was dropped; so the activity is taken here as well, and `receive`
        // keeps the two from opening one link twice.
        .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
            if let url = activity.webpageURL { receive(url) }
        }
        .environment(\.locale, localization.language.locale)
    }

    /// The last link handed over and when, so the same link arriving by both
    /// ways in at once is opened once: opened twice, an invitation was asked
    /// about twice and its screen drawn twice.
    @State private var received: (url: URL, at: Date)?

    private func receive(_ url: URL) {
        if let received, received.url == url, Date().timeIntervalSince(received.at) < 2 { return }
        received = (url, Date())
        Task { await app.open(url) }
    }
}

#if os(iOS) && DEBUG
/// TOMOSHIBI_DEBUG_COMPACT=1, honoured only when a test drives the app
/// (TOMOSHIBI_BASE set), makes the scene compact both ways, as the iPhone
/// Duo's outer display is on its side and when it is set down as a tent. The
/// Duo simulator turns only from Device Hub's own controls, and the resizable
/// display the app is moved to for the other shapes is regular at 678 x 466,
/// so this is how those screens are laid out at the outer display's size
/// without them. On the window rather than in SwiftUI's environment, so the
/// sheets and full-screen covers presented in it are compact too; on the
/// scene, the window worked its size class out from its size regardless.
enum DebugTraits {
    static func apply() {
        let environment = ProcessInfo.processInfo.environment
        guard environment["TOMOSHIBI_BASE"] != nil, environment["TOMOSHIBI_DEBUG_COMPACT"] != nil else { return }
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            for window in scene.windows {
                window.traitOverrides.horizontalSizeClass = .compact
                window.traitOverrides.verticalSizeClass = .compact
            }
        }
    }
}
#endif

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

/// A screen that is a single card of glass over the backdrop.
private struct CardScreen<Content: View>: View {
    var width: CGFloat = 420
    @ViewBuilder var content: Content
    @State private var popovers = Popovers()

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(spacing: 18) { content }
                    .contentLayer()
                    .padding(22)
                    .frame(maxWidth: width)
                    .glassPane(cornerRadius: 30)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 60)
                    .frame(maxWidth: .infinity, minHeight: 0)
            }
            .scrollIndicators(.never)
            .rise()
            // A card centred on a Duo held half open sat across its fold; it
            // keeps to the page it is read on, or above a fold across it.
            .keptToOneSide(.reading)
        }
        .overlay(alignment: .topTrailing) { LanguageMenu().padding(16) }
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
    }
}

/// The join screens on the phone: the picture of yourself filling the
/// screen, and a pane of glass at its foot that scrolls where it is taller
/// than the room left above the keyboard. The pane's heading (the room, the
/// microphone and the camera) and its form come apart, to stand side by side
/// on a short screen.
private struct PaneScreen<Heading: View, Content: View>: View {
    @Bindable var app: AppModel
    @ViewBuilder var heading: Heading
    @ViewBuilder var content: Content
    @State private var popovers = Popovers()
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.verticalSizeClass) private var heightClass
    /// Centred on a short screen too (a phone on its side, the Duo's outer
    /// display on its side or as a tent), where across the foot the pane was
    /// as wide as the screen and taller than it.
    private var wide: Bool { sizeClass == .regular || heightClass == .compact }
    /// A short screen, where the pane stacked was half as tall again as the
    /// screen, its Join out of sight below the fields: the heading stands
    /// beside the form there. One layout either way, so that turning the
    /// phone keeps the name being typed and the keyboard.
    private var short: Bool { heightClass == .compact }
    #else
    private let wide = true
    private let short = false
    #endif

    var body: some View {
        ZStack {
            CameraStage(app: app).ignoresSafeArea()
            GeometryReader { geometry in
                ScrollView {
                    // At the foot of a phone, where the thumb is; in the
                    // middle of a wide screen at a card's width, where across
                    // the foot of an opened Duo the pane was a bar the width
                    // of the screen. The same pane either way, so a fold
                    // keeps the name half typed into it.
                    VStack(spacing: 0) {
                        Spacer(minLength: 0)
                        let layout = short ? AnyLayout(HStackLayout(alignment: .center, spacing: 24))
                                           : AnyLayout(VStackLayout(spacing: 16))
                        layout {
                            VStack(spacing: 16) { heading }
                                .frame(maxWidth: short ? 260 : nil)
                            VStack(spacing: 16) { content }
                        }
                        .padding(18)
                        .concentricGlassShell(minimum: 34)
                        .frame(maxWidth: wide ? (short ? 720 : 520) : .infinity)
                        .padding(10)
                        .animation(Theme.settle, value: short)
                        if wide { Spacer(minLength: 0) }
                    }
                    // As wide as the screen, for the pane to stand in its
                    // middle: the scroll view puts narrower content at its
                    // leading edge.
                    .frame(maxWidth: .infinity, minHeight: geometry.size.height)
                }
                .scrollIndicators(.never)
                .scrollDismissesKeyboard(.interactively)
            }
            .rise()
            .keptToOneSide(.reading)
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

// MARK: - The deployment not answering

/// What opening on a deployment that will not answer shows: the one way on,
/// which is to ask again. It was an address field, which let the app be
/// pointed at any server at all; the app is for one (AppModel `home`).
struct UnreachableScreen: View {
    @Bindable var app: AppModel

    var body: some View {
        CardScreen {
            Mark(size: 56)
            // Without the address: which server this is is not something the
            // app says anywhere (AppModel `home`).
            Heading(title: t("Could not reach the server."))
            Button { Task { await app.connect() } } label: {
                Group {
                    if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Try again")) }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassCapsule(48, prominent: true))
            .disabled(app.busy)
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
        CardScreen(width: 560) {
            heading
            animated
        }
        #else
        PaneScreen(app: app) { heading } content: { animated }
        #endif
    }

    /// What you are invited to, and the picture and the sound you go in with.
    @ViewBuilder
    private var heading: some View {
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
    }

    /// The line saying why the last press did not take, and the knock it may
    /// offer, come and go with the curves they declare. With nothing here
    /// animating the change, the line faded in on its own transition but was
    /// gone between one frame and the next when closed, the form jumping up
    /// under it.
    private var animated: some View {
        Group { content }
            .animation(Theme.settle, value: app.problem)
            .animation(Theme.settle, value: app.problemStays)
            .animation(Theme.settle, value: app.canKnock)
    }

    @ViewBuilder
    private var content: some View {
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
        OptionsCard(app: app, sealing: $sealing, server: "prejoin.server")
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
                .disabled(app.busy || app.joiningName.isEmpty)
                .transition(Theme.arrive)
        }
        // Calls off a join that is out (AppModel `cancelJoin`).
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

/// Where the call is held, where it is offered, and whether it is sealed, as
/// one card under the identity: what the join screens ask before a call.
///
/// The server row is the lobby's own picker (`ServerMenu`), with the lobby's
/// "Automatic", its readings and its greyed relays out of service, for
/// somebody who reached this screen without passing it
/// (`AppModel.offersRelayChoiceAtTheDoor`): opened from an invitation or a
/// meeting's link, they had no way to choose, and a machine picked in the
/// lobby is the one it says. It comes and goes with the screens' curves:
/// the deployment's relays arrive after the screen does, and the row slid in
/// with nothing animating it, the card jumping taller under the fields.
private struct OptionsCard: View {
    @Bindable var app: AppModel
    @Binding var sealing: Bool
    let server: String

    var body: some View {
        VStack(spacing: 0) {
            if app.offersRelayChoiceAtTheDoor {
                VStack(spacing: 0) {
                    HStack(spacing: 12) {
                        Image(systemName: "server.rack").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted).frame(width: 18)
                        Text(t("Server")).font(.system(size: 14))
                        Spacer(minLength: 8)
                        ServerMenu(app: app, compact: true, id: server)
                    }
                    .padding(.horizontal, 14).frame(minHeight: 48)
                    Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
                }
                .transition(Theme.arrive)
            }
            SealControl(on: $sealing, word: $app.sealingWord)
                .padding(.horizontal, 14).padding(.vertical, 12)
        }
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        .animation(Theme.settle, value: app.offersRelayChoiceAtTheDoor)
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
        CardScreen(width: 560) {
            heading
            animated
        }
        #else
        PaneScreen(app: app) { heading } content: { animated }
        #endif
    }

    /// The meeting's room, and the picture and the sound you will go in with.
    @ViewBuilder
    private var heading: some View {
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
    }

    /// What the meeting is waiting for, said in turn as it changes, and the
    /// line about the last press: each leaves with its curve as well as
    /// arriving with it, rather than being gone between two frames.
    private var animated: some View {
        Group { content }
            .animation(Theme.settle, value: app.problem)
            .animation(Theme.settle, value: app.waiting)
            .animation(Theme.settle, value: app.arrangement == nil)
            .animation(Theme.settle, value: app.arrangedReady)
    }

    @ViewBuilder
    private var content: some View {
        if app.account == nil {
            // A guest with no saved name gives one here; the button waits for
            // it (Waiting.tsx: "Nothing is asked until there is one").
            IdentityBox(name: $app.name, passphrase: $app.passphrase, nameOnly: true)
        }
        OptionsCard(app: app, sealing: $sealing, server: "waiting.server")
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
                    ProgressView().controlSize(.small).transition(Theme.arrive)
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
                // Once: pressed twice it ran two joins as the same
                // participant, and the server put one of them out.
                Button { Task { await app.join() } } label: {
                    Group {
                        if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Join again")) }
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(48, prominent: true))
                .disabled(app.busy)
            }
            if let problem = app.problem {
                ProblemLine(app: app, text: problem)
            }
            BackButton { app.backToLobby() }
        }
        .animation(Theme.settle, value: app.problem)
        .animation(Theme.settle, value: app.problemStays)
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
