import AVFoundation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

// The screens in front of a call, drawn as the platforms draw their own
// (the owner's design canvas, 2026-10-15): on the Mac a window with a sidebar
// of glass floating over the picture of yourself, the way FaceTime opens; on
// the phone a large-titled home with the picture in a card and a field at the
// foot to join by name. What they offer is the web's lobby (Lobby.tsx,
// Schedule.tsx, ServerPicker.tsx), and only somebody signed in, or holding a
// link they were sent, gets past the first of them.

// MARK: - Which of them

/// TOMOSHIBI_DEBUG_LOBBY, honoured only in a debug build, opens a test on one
/// part of these screens: `scheduled`, `servers`, `start`, `join` or
/// `settings`. A test cannot press the sidebar on a machine whose
/// accessibility answers with the application for every element.
enum LobbyDebug {
    static let open: String? = {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_LOBBY"]
        #else
        nil
        #endif
    }()

    static var sheet: MeetingSheet.Mode? {
        switch open {
        case "start": .start
        case "join": .join("")
        default: nil
        }
    }
}

/// Signed in or not is the whole difference.
struct LobbyView: View {
    @Bindable var app: AppModel

    var body: some View {
        Group {
            if let account = app.account {
                #if os(macOS)
                MacLobby(app: app, account: account)
                #else
                PhoneLobby(app: app, account: account)
                #endif
            } else {
                SignInView(app: app)
            }
        }
        .task(id: app.account?.name) {
            await app.lobby.refreshArrangements()
        }
    }
}

// MARK: - The mark and small pieces

/// The product's own mark, as the favicon and the app icon draw it.
struct Mark: View {
    var size: CGFloat = 20

    var body: some View {
        Canvas { context, area in
            let unit = area.width / 32
            context.fill(Path(roundedRect: CGRect(origin: .zero, size: area), cornerRadius: 8 * unit, style: .continuous),
                         with: .color(Mark.ground))
            let inner = unit * 0.86
            let offset = (area.width - 32 * inner) / 2
            for (x, y, live) in [(6.0, 8.0, false), (17, 8, false), (6, 17, false), (17, 17, true)] {
                let tile = CGRect(x: offset + x * inner, y: offset + y * inner, width: 9 * inner, height: 7 * inner)
                context.fill(Path(roundedRect: tile, cornerRadius: 2 * inner), with: .color(live ? Mark.live : Mark.idle))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    static let ground = Color(red: 0x0F / 255, green: 0x0E / 255, blue: 0x0D / 255)
    static let idle = Color(red: 0x55 / 255, green: 0x52 / 255, blue: 0x4E / 255)
    static let live = Color(red: 0xFF / 255, green: 0xA2 / 255, blue: 0x3B / 255)
}

/// The person, as a circle with their initial in it.
struct Initial: View {
    let name: String
    var size: CGFloat = 32

    var body: some View {
        Text(String(name.prefix(1)).uppercased())
            .font(.system(size: size * 0.42, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Circle().fill(Initial.colour(for: name)))
            .accessibilityHidden(true)
    }

    /// The same colour for the same name everywhere it appears.
    static func colour(for name: String) -> Color {
        let palette: [Color] = [
            Color(red: 0.369, green: 0.361, blue: 0.902), Color(red: 0.180, green: 0.545, blue: 0.604),
            Color(red: 0.604, green: 0.361, blue: 0.784), Color(red: 0.204, green: 0.471, blue: 0.965),
            Color(red: 0.776, green: 0.420, blue: 0.247), Color(red: 0.247, green: 0.596, blue: 0.388),
        ]
        let sum = name.unicodeScalars.reduce(0) { $0 &+ Int($1.value) }
        return palette[sum % palette.count]
    }
}

/// How much of the fleet is answering, in a light and two numbers
/// (Lobby.tsx `Fleet`).
struct FleetLine: View {
    @Bindable var app: AppModel

    var body: some View {
        if let relays = app.relays, relays.total > 0 {
            let all = relays.online == relays.total
            HStack(spacing: 6) {
                Circle().fill(relays.online == 0 ? Theme.danger : all ? Theme.good : Theme.tally).frame(width: 6, height: 6)
                Text("\(relays.online)/\(relays.total)").monospacedDigit()
                Text(t("Relays answering"))
            }
            .font(.system(size: 11))
            .foregroundStyle(Theme.muted)
        }
    }
}

/// Something that happened and is over, at the foot of the screen for a few
/// seconds (Notices.tsx).
struct ToastModifier: ViewModifier {
    @Bindable var app: AppModel

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            ZStack {
                if let text = app.problem {
                    Text(text)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 16).padding(.vertical, 11)
                        .glassSurface(Capsule())
                        .padding(.horizontal, 20).padding(.bottom, 92)
                        .onTapGesture { app.problem = nil }
                        .task(id: text) {
                            try? await Task.sleep(for: .seconds(5))
                            if app.problem == text { app.problem = nil }
                        }
                        .transition(Theme.slideUp)
                }
            }
            .animation(Theme.settle, value: app.problem)
        }
    }
}

extension View {
    func toast(_ app: AppModel) -> some View { modifier(ToastModifier(app: app)) }
}

/// The camera, live, where somebody has it on; the evening room otherwise.
struct CameraStage: View {
    @Bindable var app: AppModel
    var cornerRadius: CGFloat = 0
    @State private var preview = CameraCapture()

    var body: some View {
        ZStack {
            Backdrop()
            if app.startWithCamera {
                SelfView(session: preview.session)
                    .transition(.opacity.animation(Theme.settle))
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        .animation(Theme.settle, value: app.startWithCamera)
        .onChange(of: app.startWithCamera) { show(app.startWithCamera) }
        .onAppear { show(app.startWithCamera) }
        .onDisappear { preview.session.stopRunning() }
    }

    private func show(_ on: Bool) {
        let session = preview.session
        if on {
            if session.inputs.isEmpty, let device = AVCaptureDevice.default(for: .video),
               let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) {
                session.addInput(input)
            }
            DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
        } else {
            DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
        }
    }
}

/// The microphone and camera as they will be on joining, as glass switches.
/// Off is drawn in red: this is the last moment to notice before everybody
/// else does.
struct DeviceSwitches: View {
    @Bindable var app: AppModel
    var size: CGFloat = 44

    var body: some View {
        GlassGroup(spacing: 8) {
            HStack(spacing: 8) {
                Button { app.startWithMicrophone.toggle() } label: {
                    Image(systemName: app.startWithMicrophone ? "mic.fill" : "mic.slash.fill").font(.system(size: size * 0.4, weight: .semibold))
                }
                .buttonStyle(GlassButtonStyle(shape: .circle(size), destructive: !app.startWithMicrophone))
                .accessibilityLabel(app.startWithMicrophone ? t("Mute microphone") : t("Unmute microphone"))
                Button { app.startWithCamera.toggle() } label: {
                    Image(systemName: app.startWithCamera ? "video.fill" : "video.slash.fill").font(.system(size: size * 0.38, weight: .semibold))
                }
                .buttonStyle(GlassButtonStyle(shape: .circle(size), destructive: !app.startWithCamera))
                .accessibilityLabel(app.startWithCamera ? t("Turn camera off") : t("Turn camera on"))
            }
        }
        .animation(Theme.settle, value: app.startWithMicrophone)
        .animation(Theme.settle, value: app.startWithCamera)
    }
}

// MARK: - Signing in

/// Say who you are (Lobby.tsx `SignIn`), or open the link you were sent.
struct SignInView: View {
    @Bindable var app: AppModel
    @State private var name = ""
    @State private var passphrase = ""
    @State private var shown = false
    @State private var link = ""

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(spacing: 22) {
                    VStack(spacing: 12) {
                        Mark(size: 76)
                            .shadow(color: .black.opacity(0.45), radius: 18, y: 8)
                        Text(t("Sign in")).font(.system(size: 28, weight: .bold))
                        Text(t("Meetings here are started by the people who belong here. If somebody sent you a link, open that instead."))
                            .font(.system(size: 14)).foregroundStyle(Theme.muted)
                            .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
                        ServerLine(app: app)
                    }
                    .rise()

                    VStack(spacing: 14) {
                        credentials
                        Button(action: signIn) {
                            Group {
                                if app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Sign in")) }
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.glassCapsule(48, prominent: true))
                        .disabled(app.busy || name.isEmpty || passphrase.isEmpty)
                        #if os(macOS)
                        HStack(spacing: 10) {
                            Rectangle().fill(Theme.border).frame(height: 1)
                            Text(t("Were you sent a link?")).font(.system(size: 11)).foregroundStyle(Theme.muted).fixedSize()
                            Rectangle().fill(Theme.border).frame(height: 1)
                        }
                        HStack(spacing: 8) {
                            HStack(spacing: 8) {
                                Image(systemName: "link").font(.system(size: 13)).foregroundStyle(Theme.muted)
                                TextField("", text: $link, prompt: Text(t("Paste the link you were sent.")).foregroundStyle(Theme.muted))
                                    .textFieldStyle(.plain)
                                    .onSubmit(openLink)
                            }
                            .padding(.horizontal, 14).frame(height: 38)
                            .background(Capsule().fill(Color.black.opacity(0.22)))
                            .overlay(Capsule().stroke(Color.white.opacity(0.12)))
                            Button(t("Join"), action: openLink)
                                .buttonStyle(.glassCapsule(38))
                                .fixedSize()
                                .disabled(URL(string: link.trimmingCharacters(in: .whitespaces))?.host() == nil)
                        }
                        #endif
                    }
                    .padding(18)
                    .glassPane(cornerRadius: 30)
                    .rise(0.06)
                }
                .frame(maxWidth: 420)
                .padding(.horizontal, 16)
                .padding(.top, 72)
                .padding(.bottom, 140)
                .frame(maxWidth: .infinity)
            }
            .scrollIndicators(.never)
        }
        #if os(iOS)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                Text(t("Were you sent a link?")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                // The system's own paste control: it reads the clipboard only
                // when pressed, so nothing asks permission to look at it.
                PasteButton(payloadType: String.self) { strings in
                    guard let text = strings.first?.trimmingCharacters(in: .whitespacesAndNewlines),
                          let url = URL(string: text), url.host() != nil else { return }
                    Task { await app.open(url) }
                }
                .buttonBorderShape(.capsule)
                .tint(Theme.surfaceHigh)
            }
            .padding(.bottom, 8)
        }
        #endif
        .overlay(alignment: .topTrailing) {
            LanguageMenu().padding(16)
        }
        .toast(app)
    }

    /// One box holding two fields, as the web holds them: they are one answer.
    private var credentials: some View {
        VStack(spacing: 0) {
            row(symbol: "person", lit: false) {
                TextField("", text: $name, prompt: Text(t("Name")).foregroundStyle(Theme.muted))
                    .textContentType(.username)
            }
            Rectangle().fill(Color.white.opacity(0.07)).frame(height: 1)
            row(symbol: "key", lit: !passphrase.isEmpty) {
                HStack {
                    Group {
                        if shown {
                            TextField("", text: $passphrase, prompt: Text(t("Passphrase")).foregroundStyle(Theme.muted))
                        } else {
                            SecureField("", text: $passphrase, prompt: Text(t("Passphrase")).foregroundStyle(Theme.muted))
                        }
                    }
                    .textContentType(.password)
                    .onSubmit(signIn)
                    if !passphrase.isEmpty {
                        Button { shown.toggle() } label: {
                            Image(systemName: shown ? "eye.slash" : "eye").font(.system(size: 13)).foregroundStyle(Theme.muted)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(shown ? t("Hide passphrase") : t("Show passphrase"))
                        .transition(Theme.arrive)
                    }
                }
                .animation(Theme.settle, value: passphrase.isEmpty)
            }
        }
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.10)))
    }

    private func row<Field: View>(symbol: String, lit: Bool, @ViewBuilder field: () -> Field) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 15))
                .foregroundStyle(lit ? Theme.tally : Theme.muted)
                .frame(width: 18)
            field()
                .textFieldStyle(.plain)
                .font(.system(size: 15))
                .foregroundStyle(Theme.foreground)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                #endif
        }
        .padding(.horizontal, 16)
        .frame(height: 50)
    }

    private func signIn() {
        guard !name.isEmpty, !passphrase.isEmpty, !app.busy else { return }
        Task {
            await app.signIn(name: name, passphrase: passphrase)
            // A refused attempt is something that happened, not something
            // still true: the field is cleared for the next, as on the web.
            if app.account == nil { passphrase = "" }
        }
    }

    private func openLink() {
        guard let url = URL(string: link.trimmingCharacters(in: .whitespaces)), url.host() != nil else { return }
        Task { await app.open(url) }
    }
}

/// Which deployment this is, with the way to another.
struct ServerLine: View {
    @Bindable var app: AppModel

    var body: some View {
        if let base = app.deployment?.baseUrl() {
            HStack(spacing: 6) {
                Circle().fill(Theme.good).frame(width: 6, height: 6)
                Text(URL(string: base)?.host() ?? base)
                Button(t("Change server")) { app.forgetServer() }
                    .buttonStyle(.plain)
                    .foregroundStyle(Theme.tally)
            }
            .font(.system(size: 12))
            .foregroundStyle(Theme.muted)
        }
    }
}

/// The language, before a call as on every web page; following the system
/// until somebody chooses.
struct LanguageMenu: View {
    var body: some View {
        Menu {
            LanguageChoices()
        } label: {
            Image(systemName: "globe").font(.system(size: 14, weight: .medium))
        }
        .menuStyle(.button)
        .buttonStyle(.glassCircle(36))
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(t("Language"))
    }
}

/// The languages as menu items, the system's first.
struct LanguageChoices: View {
    @State private var localization = Localization.shared

    var body: some View {
        Button {
            localization.followSystem()
        } label: {
            if localization.followsSystem { Label(t("Same as the system"), systemImage: "checkmark") } else { Text(t("Same as the system")) }
        }
        Divider()
        ForEach(Language.allCases) { one in
            Button {
                localization.language = one
            } label: {
                if !localization.followsSystem && localization.language == one {
                    Label(one.name, systemImage: "checkmark")
                } else {
                    Text(one.name)
                }
            }
        }
    }
}

// MARK: - The Mac

#if os(macOS)
/// A window with a sidebar, as the Mac's own apps are: what to do and what is
/// coming in the sidebar, and the picture of yourself filling the rest.
struct MacLobby: View {
    @Bindable var app: AppModel
    let account: Account
    @State private var section: Section? = LobbyDebug.open == "scheduled" ? .scheduled : LobbyDebug.open == "servers" ? .servers : .home
    @State private var sheet: MeetingSheet.Mode? = LobbyDebug.sheet

    enum Section: Hashable { case home, scheduled, servers }

    var body: some View {
        NavigationSplitView {
            MacSidebar(app: app, account: account, section: $section, sheet: $sheet)
                .navigationSplitViewColumnWidth(min: 240, ideal: 284, max: 340)
        } detail: {
            Group {
                switch section ?? .home {
                case .home: HomeStage(app: app, account: account, sheet: $sheet)
                case .scheduled: ScheduleView(app: app, account: account)
                case .servers: ServersView(app: app)
                }
            }
            .transition(.opacity.animation(Theme.settle))
        }
        .sheet(item: $sheet) { mode in
            MeetingSheet(app: app, mode: mode)
        }
        .toast(app)
    }
}

private struct MacSidebar: View {
    @Bindable var app: AppModel
    let account: Account
    @Binding var section: MacLobby.Section?
    @Binding var sheet: MeetingSheet.Mode?

    var body: some View {
        List(selection: $section) {
            Section {
                item(t("Home"), "house").tag(MacLobby.Section.home)
                item(t("Scheduled meetings"), "calendar")
                    .badge(app.lobby.upcoming.count)
                    .tag(MacLobby.Section.scheduled)
                item(t("Server"), "server.rack").tag(MacLobby.Section.servers)
            }
            if !app.lobby.upcoming.isEmpty {
                Section(t("Upcoming")) {
                    ForEach(app.lobby.upcoming.prefix(5), id: \.id) { one in
                        Button {
                            if let url = app.lobby.link(one) { Task { await app.open(url) } }
                        } label: {
                            MeetingRow(one: one, app: app)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            if !app.lobby.recentHere.isEmpty {
                Section(t("Recent")) {
                    ForEach(app.lobby.recentHere.prefix(6)) { recent in
                        Button {
                            sheet = .join(recent.room)
                        } label: {
                            HStack(spacing: 10) {
                                Initial(name: recent.room, size: 26)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(recent.room).font(.system(size: 13, weight: .medium)).lineLimit(1)
                                    Text(recent.at.formatted(.relative(presentation: .named)))
                                        .font(.system(size: 11)).foregroundStyle(Theme.muted)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .tint(Theme.tally)
        .safeAreaInset(edge: .top) {
            HStack(spacing: 8) {
                Button { sheet = .start } label: {
                    Label(t("Start"), systemImage: "video.fill").frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(36, prominent: true))
                .help(t("Start a meeting"))
                Button { sheet = .join("") } label: {
                    Label(t("Join"), systemImage: "link").frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(36))
                .help(t("Join a meeting"))
            }
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, 12).padding(.top, 6).padding(.bottom, 4)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                FleetLine(app: app).padding(.horizontal, 6)
                AccountMenu(app: app, account: account)
            }
            .padding(10)
        }
    }
}

extension MacSidebar {
    /// A sidebar entry with its symbol drawn in the person's colour, as the
    /// system's own sidebars draw theirs.
    func item(_ title: String, _ symbol: String) -> some View {
        Label {
            Text(title)
        } icon: {
            Image(systemName: symbol).foregroundStyle(Theme.tally)
        }
    }
}

/// The account, the server and everything that is not a meeting.
struct AccountMenu: View {
    @Bindable var app: AppModel
    let account: Account
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        Menu {
            if let url = page("/account") { Link(t("User console"), destination: url) }
            if account.admin, let url = page("/admin") { Link(t("Admin console"), destination: url) }
            Divider()
            Button(t("Settings")) { openSettings() }
            Button(t("Change server")) { app.forgetServer() }
            Divider()
            Button(t("Sign out")) { Task { await app.signOut() } }
        } label: {
            HStack(spacing: 10) {
                Initial(name: account.name, size: 30)
                VStack(alignment: .leading, spacing: 1) {
                    Text(account.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                    Text(app.deployment.flatMap { URL(string: $0.baseUrl())?.host() } ?? "")
                        .font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                Spacer(minLength: 4)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.muted)
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .glassSurface(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func page(_ path: String) -> URL? {
        app.deployment.flatMap { URL(string: $0.baseUrl().trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path) }
    }
}

/// Home: the picture of yourself, a greeting, and the switches for how you
/// will arrive.
private struct HomeStage: View {
    @Bindable var app: AppModel
    let account: Account
    @Binding var sheet: MeetingSheet.Mode?

    var body: some View {
        ZStack(alignment: .topLeading) {
            CameraStage(app: app)
                .ignoresSafeArea()
                .backgroundExtended()
            VStack(alignment: .leading, spacing: 6) {
                Text(t("Hello, {name}", ["name": account.name])).font(.system(size: 34, weight: .bold))
                Text(t("Start one from the sidebar when you are ready.")).font(.system(size: 15)).foregroundStyle(Color.white.opacity(0.78))
            }
            .shadow(color: .black.opacity(0.45), radius: 16, y: 2)
            .padding(.leading, 36).padding(.top, 56)
            .rise()
        }
        .overlay(alignment: .bottom) {
            DeviceSwitches(app: app)
                .padding(8)
                .glassSurface(Capsule())
                .padding(.bottom, 28)
                .rise(0.08)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                ServerMenu(app: app)
            }
        }
    }
}

extension View {
    /// Content that carries on under a floating sidebar, mirrored and blurred,
    /// where the system can (macOS 26); nothing before it.
    @ViewBuilder
    func backgroundExtended() -> some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            backgroundExtensionEffect()
        } else {
            self
        }
    }
}
#endif

// MARK: - The phone

#if os(iOS)
/// A large-titled home: the picture of yourself in a card with the two things
/// to do on it, what is coming, the server, and a field at the foot to join
/// by the name you were given.
struct PhoneLobby: View {
    @Bindable var app: AppModel
    let account: Account
    @State private var sheet: MeetingSheet.Mode? = LobbyDebug.sheet
    @State private var settings = LobbyDebug.open == "settings"
    @State private var scheduling = LobbyDebug.open == "scheduled"
    @State private var typed = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    hero.rise()
                    if !app.lobby.upcoming.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text(t("Upcoming")).font(.system(size: 20, weight: .bold))
                                Spacer()
                                Button(t("All of them")) { scheduling = true }.foregroundStyle(Theme.tally)
                            }
                            .padding(.horizontal, 6)
                            VStack(spacing: 0) {
                                ForEach(app.lobby.upcoming.prefix(4), id: \.id) { one in
                                    Button {
                                        if let url = app.lobby.link(one) { Task { await app.open(url) } }
                                    } label: {
                                        MeetingRow(one: one, app: app).padding(.horizontal, 14).padding(.vertical, 11)
                                    }
                                    .buttonStyle(.plain)
                                    if one.id != app.lobby.upcoming.prefix(4).last?.id { Divider().padding(.leading, 62) }
                                }
                            }
                            .background(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(Theme.surface))
                        }
                        .rise(0.06)
                    }
                    if !app.lobby.recentHere.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(t("Recent")).font(.system(size: 20, weight: .bold)).padding(.horizontal, 6)
                            VStack(spacing: 0) {
                                ForEach(app.lobby.recentHere.prefix(4)) { recent in
                                    Button { sheet = .join(recent.room) } label: {
                                        HStack(spacing: 12) {
                                            Initial(name: recent.room, size: 36)
                                            VStack(alignment: .leading, spacing: 2) {
                                                Text(recent.room).font(.system(size: 16, weight: .semibold))
                                                Text(recent.at.formatted(.relative(presentation: .named))).font(.system(size: 13)).foregroundStyle(Theme.muted)
                                            }
                                            Spacer()
                                            Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted.opacity(0.6))
                                        }
                                        .padding(.horizontal, 14).padding(.vertical, 11)
                                        .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                    if recent.id != app.lobby.recentHere.prefix(4).last?.id { Divider().padding(.leading, 62) }
                                }
                            }
                            .background(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(Theme.surface))
                        }
                        .rise(0.1)
                    }
                    NavigationLink {
                        ServersView(app: app)
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: "server.rack").font(.system(size: 16)).foregroundStyle(Theme.muted)
                                .frame(width: 36, height: 36)
                                .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.white.opacity(0.08)))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(t("Server")).font(.system(size: 16, weight: .semibold))
                                FleetLine(app: app)
                            }
                            Spacer()
                            Text(app.relayChoice.map { app.lobby.label($0) } ?? t("Automatic")).font(.system(size: 14)).foregroundStyle(Theme.muted)
                            Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted.opacity(0.6))
                        }
                        .padding(14)
                        .background(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(Theme.surface))
                    }
                    .buttonStyle(.plain)
                    .rise(0.14)
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 24)
            }
            .scrollIndicators(.never)
            .background(Color.black.ignoresSafeArea())
            .navigationTitle("Tomoshibi")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { settings = true } label: { Initial(name: account.name, size: 30) }
                        .accessibilityLabel(t("Settings"))
                }
            }
            .safeAreaInset(edge: .bottom) { joinBar }
        }
        .fullScreenCover(item: $sheet) { mode in
            MeetingSheet(app: app, mode: mode)
        }
        .sheet(isPresented: $settings) {
            NavigationStack { SettingsView(app: app) }
        }
        .sheet(isPresented: $scheduling) {
            NavigationStack { ScheduleView(app: app, account: account) }
        }
        .toast(app)
    }

    private var hero: some View {
        ZStack {
            CameraStage(app: app, cornerRadius: 30)
        }
        .frame(height: 292)
        .overlay(alignment: .topTrailing) {
            DeviceSwitches(app: app, size: 40).padding(12)
        }
        .overlay(alignment: .bottom) {
            GlassGroup(spacing: 8) {
                HStack(spacing: 8) {
                    Button { sheet = .start } label: {
                        Label(t("Start a meeting"), systemImage: "video.fill").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.glassCapsule(52, prominent: true))
                    Button { scheduling = true } label: {
                        Label(t("Schedule a meeting"), systemImage: "calendar").labelStyle(.iconOnly)
                    }
                    .buttonStyle(.glassCircle(52))
                    .accessibilityLabel(t("Schedule a meeting"))
                }
            }
            .font(.system(size: 16, weight: .semibold))
            .padding(12)
        }
    }

    /// Joining by the name you were given, from the foot of the screen where a
    /// thumb already is.
    private var joinBar: some View {
        HStack(spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "link").foregroundStyle(Theme.muted)
                TextField("", text: $typed, prompt: Text(t("The name you were given.")).foregroundStyle(Theme.muted))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.join)
                    .onSubmit(joinTyped)
            }
            .font(.system(size: 16))
            .padding(.horizontal, 18).frame(height: 54)
            .glassSurface(Capsule())
            Button(action: joinTyped) {
                Image(systemName: "arrow.right").font(.system(size: 20, weight: .bold))
            }
            .buttonStyle(.glassCircle(54, prominent: true))
            .disabled(!validRoomName(name: normaliseRoomName(typed: typed)))
            .accessibilityLabel(t("Join"))
        }
        .padding(.horizontal, 16).padding(.bottom, 6)
    }

    private func joinTyped() {
        let room = normaliseRoomName(typed: typed)
        guard validRoomName(name: room) else { return }
        sheet = .join(room)
        typed = ""
    }
}
#endif

// MARK: - Starting or joining

/// Start a meeting or join one: the preview, the room, where it is held and
/// whether it is sealed, and then straight into the call. On the Mac a sheet
/// over the window; on the phone the whole screen, the picture behind a pane
/// of glass at the foot.
struct MeetingSheet: View {
    enum Mode: Identifiable, Hashable {
        case start
        case join(String)

        var id: String {
            switch self {
            case .start: "start"
            case let .join(room): "join:" + room
            }
        }
    }

    @Bindable var app: AppModel
    @State var mode: Mode
    @State private var room = ""
    @State private var sealing = false
    @State private var working = false
    @Environment(\.dismiss) private var dismiss

    init(app: AppModel, mode: Mode) {
        self.app = app
        _mode = State(initialValue: mode)
        if case let .join(name) = mode { _room = State(initialValue: name) }
    }

    private var starting: Bool { if case .start = mode { return true } else { return false } }

    var body: some View {
        #if os(macOS)
        VStack(spacing: 16) {
            header
            CameraStage(app: app, cornerRadius: 20)
                .aspectRatio(16 / 9, contentMode: .fit)
                .overlay(alignment: .bottom) {
                    DeviceSwitches(app: app, size: 38).padding(6).glassSurface(Capsule()).padding(.bottom, 12)
                }
            form
            footer
        }
        .padding(20)
        .frame(width: 560)
        .animation(Theme.settle, value: sealing)
        .animation(Theme.settle, value: app.problem)
        #else
        ZStack(alignment: .bottom) {
            CameraStage(app: app).ignoresSafeArea()
            VStack(spacing: 16) {
                header
                DeviceSwitches(app: app, size: 52)
                form
                footer
            }
            .padding(18)
            .glassPane(cornerRadius: 34)
            .padding(10)
        }
        .animation(Theme.settle, value: sealing)
        .animation(Theme.settle, value: app.problem)
        #endif
    }

    private var header: some View {
        HStack {
            Button { dismiss() } label: { Image(systemName: "xmark").font(.system(size: 13, weight: .bold)) }
                .buttonStyle(.glassCircle(32))
                .accessibilityLabel(t("Cancel"))
            Spacer()
            Picker("", selection: Binding(get: { starting }, set: { mode = $0 ? .start : .join(room) })) {
                Text(t("Start new meeting")).tag(true)
                Text(t("Join a meeting")).tag(false)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(maxWidth: 280)
            Spacer()
            Color.clear.frame(width: 32, height: 32)
        }
    }

    private var form: some View {
        VStack(spacing: 0) {
            row(t("Room name")) {
                TextField("", text: $room, prompt: Text(starting ? t("Name it yourself, or leave it blank for one nobody has used.") : t("The name you were given.")).foregroundStyle(Theme.muted))
                    .textFieldStyle(.plain)
                    .multilineTextAlignment(.trailing)
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    #endif
                    .onSubmit(go)
            }
            Divider().overlay(Color.white.opacity(0.06))
            row(t("Server")) { ServerMenu(app: app, compact: true) }
            Divider().overlay(Color.white.opacity(0.06))
            row(t("Seal this call")) {
                Toggle("", isOn: $sealing).labelsHidden().toggleStyle(.switch).tint(Theme.tally)
            }
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
    }

    private func row<Control: View>(_ label: String, @ViewBuilder control: () -> Control) -> some View {
        HStack(spacing: 12) {
            Text(label).foregroundStyle(Theme.foreground).fixedSize()
            Spacer(minLength: 8)
            control()
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 46)
    }

    private var footer: some View {
        VStack(spacing: 10) {
            if let problem = app.problem {
                Text(problem).font(.system(size: 12.5)).foregroundStyle(Theme.danger)
                    .multilineTextAlignment(.center).transition(Theme.arrive)
            }
            HStack(spacing: 10) {
                if app.canKnock {
                    Button(t("Ask to be let in")) { Task { await app.knock(); dismiss() } }
                        .buttonStyle(.glassCapsule(40))
                        .transition(Theme.arrive)
                }
                Spacer()
                #if os(macOS)
                Button(t("Cancel")) { dismiss() }.buttonStyle(.glassCapsule(36)).keyboardShortcut(.cancelAction)
                #endif
                Button(action: go) {
                    Group {
                        if working || app.busy { ProgressView().controlSize(.small).tint(Theme.tallyForeground) }
                        else { Text(starting ? t("Start") : t("Join")) }
                    }
                    #if os(iOS)
                    .frame(maxWidth: .infinity)
                    #endif
                }
                .buttonStyle(.glassCapsule(platformTall, prominent: true))
                .keyboardShortcut(.defaultAction)
                .disabled(working || (!starting && !validRoomName(name: normaliseRoomName(typed: room))))
            }
        }
    }

    private var platformTall: CGFloat {
        #if os(iOS)
        56
        #else
        36
        #endif
    }

    /// Asks whether a meeting is live under the name first, as the web does:
    /// one that exists cannot be started and one that does not cannot be
    /// joined. An unknown answer goes through; the join is the authority.
    private func go() {
        guard !working, let deployment = app.deployment else { return }
        let typed = normaliseRoomName(typed: room)
        let target = starting ? (validRoomName(name: typed) ? typed : generateRoomName()) : typed
        guard validRoomName(name: target) else { return }
        if !sealing { app.sealingWord = "" }
        working = true
        app.problem = nil
        Task {
            defer { working = false }
            let live = (try? await deployment.roomLive(room: target)) ?? nil
            if starting, live == true {
                app.problem = t("A meeting is already happening under that name.")
                return
            }
            if !starting, live == false {
                app.problem = t("No meeting is happening under that name.")
                return
            }
            await app.enter(room: target)
        }
    }
}

/// Where the call is held, as a menu: automatic or one relay, each with how
/// quickly it answered from here.
struct ServerMenu: View {
    @Bindable var app: AppModel
    var compact = false

    var body: some View {
        Menu {
            Button { app.relayChoice = nil } label: {
                choice(t("Automatic"), detail: app.lobby.fastest.map { "\(app.lobby.label($0.name)) · \(Int($0.millis)) ms" }, chosen: app.relayChoice == nil)
            }
            ForEach(app.lobby.grouped(), id: \.region) { group in
                Section(group.region) {
                    ForEach(group.relays, id: \.name) { relay in
                        Button { app.relayChoice = relay.name } label: {
                            choice(say(relay.label ?? relay.name), detail: millis(relay.name), chosen: app.relayChoice == relay.name)
                        }
                        .disabled(relay.maintenance)
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                if !compact { Image(systemName: "server.rack").font(.system(size: 12)) }
                Text(app.relayChoice.map { app.lobby.label($0) } ?? t("Automatic"))
                if let ms = current {
                    Text("\(Int(ms)) ms").monospacedDigit().foregroundStyle(latencyColour(ms))
                }
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .semibold)).foregroundStyle(Theme.muted)
            }
            .font(.system(size: 12.5))
        }
        .menuStyle(.button)
        .buttonStyle(.glassCapsule(compact ? 30 : 34))
        .menuIndicator(.hidden)
        .fixedSize()
        .task { if app.lobby.measured == nil { await app.lobby.measure() } }
    }

    private var current: Double? {
        if let name = app.relayChoice { return app.lobby.measured?[name] ?? nil }
        return app.lobby.fastest?.millis
    }

    private func millis(_ name: String) -> String? {
        guard let measured = app.lobby.measured else { return nil }
        guard let value = measured[name] ?? nil else { return t("Timeout") }
        return "\(Int(value)) ms"
    }

    @ViewBuilder
    private func choice(_ title: String, detail: String?, chosen: Bool) -> some View {
        let text = detail.map { "\(title)  ·  \($0)" } ?? title
        if chosen { Label(text, systemImage: "checkmark") } else { Text(text) }
    }
}

/// ServerPicker.tsx `Latency`: green to 150 ms, the signal colour to 400,
/// red beyond.
@MainActor
func latencyColour(_ ms: Double) -> Color {
    ms <= 150 ? Theme.good : ms <= 400 ? Theme.tally : Theme.danger
}

/// One arranged meeting as a row.
struct MeetingRow: View {
    let one: Arrangement
    @Bindable var app: AppModel

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: one.started ? "video.fill" : "calendar")
                .font(.system(size: 15))
                .foregroundStyle(one.started ? Theme.tally : Theme.muted)
                .frame(width: 34, height: 34)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(one.started ? Theme.tally.opacity(0.18) : Color.white.opacity(0.08)))
            VStack(alignment: .leading, spacing: 2) {
                Text(one.room).font(.system(size: 14, weight: .semibold)).lineLimit(1)
                Text(when).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
            }
            Spacer(minLength: 4)
            if one.started {
                Text(t("Under way")).font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Theme.tallyForeground)
                    .padding(.horizontal, 8).padding(.vertical, 3)
                    .background(Capsule().fill(Theme.tally))
            }
        }
        .contentShape(Rectangle())
    }

    private var when: String {
        let date = AppModel.instant(one.at).map(LobbyModel.said) ?? one.at
        if let relay = one.relay, !relay.isEmpty { return "\(date) · \(app.lobby.label(relay))" }
        return date
    }
}

// MARK: - Arranging

/// Schedule.tsx: which name, which day, what time, which machine, and the
/// meetings already arranged with their links.
struct ScheduleView: View {
    @Bindable var app: AppModel
    let account: Account
    @State private var planning = ""
    @State private var day = Date()
    @State private var hour = Calendar.current.component(.hour, from: ScheduleView.nextQuarter)
    @State private var minute = Calendar.current.component(.minute, from: ScheduleView.nextQuarter)
    @State private var arranging = false
    @State private var copied: String?
    @State private var now = Date()

    private static let ahead: TimeInterval = 60 * 24 * 60 * 60

    /// The next quarter hour from now (Schedule.tsx `nextQuarter`). Worked
    /// out on the clock rather than by setting fields: setting the seconds
    /// to zero searches forward for the next such moment, and the minute it
    /// landed on was one the minute list does not offer, which left the list
    /// blank.
    private static var nextQuarter: Date {
        let now = Date().timeIntervalSinceReferenceDate
        return Date(timeIntervalSinceReferenceDate: ((now / 900).rounded(.down) + 1) * 900)
    }

    private var canArrange: Bool { account.can?.contains("arrange") ?? true }
    private var at: Date? { Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: day) }
    private var problem: Bool {
        guard let at else { return true }
        return at < now || at > now.addingTimeInterval(Self.ahead)
    }

    var body: some View {
        ScrollView {
            #if os(macOS)
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(t("Scheduled meetings")).font(.system(size: 26, weight: .bold))
                    Text(t("Pick a time and hand out the link before it starts.")).foregroundStyle(Theme.muted)
                }
                HStack(alignment: .top, spacing: 20) {
                    form.frame(width: 360)
                    list.frame(maxWidth: .infinity)
                }
            }
            .padding(28)
            #else
            VStack(alignment: .leading, spacing: 20) {
                form
                list
            }
            .padding(16)
            #endif
        }
        .scrollIndicators(.never)
        #if os(iOS)
        .navigationTitle(t("Scheduled meetings"))
        .navigationBarTitleDisplayMode(.inline)
        .background(Color.black.ignoresSafeArea())
        #else
        .background(Backdrop().opacity(0.6))
        #endif
        .task { await app.lobby.refreshArrangements() }
        .animation(Theme.settle, value: app.lobby.arrangements.map(\.id))
    }

    @ViewBuilder
    private var form: some View {
        VStack(alignment: .leading, spacing: 14) {
            if !canArrange {
                Text(t("Arranging meetings has not been opened to you.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
            } else {
                TextField("", text: $planning, prompt: Text(t("Room name")).foregroundStyle(Theme.muted))
                    .textFieldStyle(.plain)
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    #endif
                    .padding(.horizontal, 14).frame(height: 38)
                    .background(Capsule().fill(Color.white.opacity(0.07)))
                    .overlay(Capsule().stroke(Color.white.opacity(0.12)))
                Text(t("Name it yourself, or leave it blank for one nobody has used.")).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                DatePicker("", selection: $day, in: Calendar.current.startOfDay(for: Date())...Date().addingTimeInterval(Self.ahead),
                           displayedComponents: .date)
                    .datePickerStyle(.graphical)
                    .labelsHidden()
                    .tint(Theme.tally)
                    .frame(maxWidth: .infinity)
                HStack(spacing: 8) {
                    Text(t("Time")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                    clock(t("Hour"), value: $hour, values: Array(0..<24))
                    Text(":")
                    clock(t("Minute"), value: $minute, values: [0, 15, 30, 45])
                    Spacer()
                    ServerMenu(app: app, compact: true)
                }
                HStack(spacing: 10) {
                    Text(verdict).font(.system(size: 12)).foregroundStyle(problem ? Theme.danger : Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 8)
                    Button(action: plan) {
                        if arranging { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Arrange")) }
                    }
                    .buttonStyle(.glassCapsule(36, prominent: true))
                    .disabled(arranging || problem)
                }
            }
        }
        .padding(18)
        .glassPane(cornerRadius: 24)
    }

    private func clock(_ label: String, value: Binding<Int>, values: [Int]) -> some View {
        Picker(label, selection: value) {
            ForEach(values, id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .fixedSize()
        .accessibilityLabel(label)
    }

    private var verdict: String {
        guard let at else { return t("Choose a day.") }
        if at < now { return t("That time has already gone.") }
        if at > now.addingTimeInterval(Self.ahead) { return t("That is more than sixty days away.") }
        return t("Arranged for {when}", ["when": LobbyModel.said(at)])
    }

    @ViewBuilder
    private var list: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch app.lobby.status {
            case .signedOut:
                Text(t("Your session has expired. Sign in again.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
            case .failed:
                HStack {
                    Text(t("Could not load your meetings.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    Spacer()
                    Button(t("Try again")) { Task { await app.lobby.refreshArrangements() } }.buttonStyle(.glassCapsule(30))
                }
            case .loading where app.lobby.arrangements.isEmpty:
                ProgressView().controlSize(.small).frame(maxWidth: .infinity)
            default:
                if app.lobby.arrangements.isEmpty {
                    Text(t("No meetings arranged yet.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                }
            }
            if !app.lobby.arrangements.isEmpty {
                VStack(spacing: 0) {
                    ForEach(app.lobby.arrangements, id: \.id) { one in
                        HStack(spacing: 6) {
                            Button {
                                if !one.ended, let url = app.lobby.link(one) { Task { await app.open(url) } }
                            } label: {
                                MeetingRow(one: one, app: app)
                            }
                            .buttonStyle(.plain)
                            .disabled(one.ended)
                            .opacity(one.ended ? 0.5 : 1)
                            small(copied == one.id ? "checkmark" : "doc.on.doc", copied == one.id ? t("Link copied") : t("Copy link")) { copy(one) }
                            #if os(macOS)
                            small("calendar.badge.plus", t("Add to calendar")) {
                                if let file = app.lobby.calendarFile(one) { NSWorkspace.shared.open(file) }
                            }
                            #else
                            if let file = app.lobby.calendarFile(one) {
                                ShareLink(item: file) {
                                    Image(systemName: "calendar.badge.plus").font(.system(size: 13))
                                }
                                .buttonStyle(.glassCircle(32))
                                .accessibilityLabel(t("Add to calendar"))
                            }
                            #endif
                            small("xmark", t("Cancel meeting"), destructive: true) {
                                Task { if let said = await app.lobby.cancel(one) { app.problem = said } }
                            }
                        }
                        .padding(.horizontal, 14).padding(.vertical, 10)
                        if one.id != app.lobby.arrangements.last?.id { Divider().overlay(Color.white.opacity(0.06)) }
                    }
                }
                .glassPane(cornerRadius: 22)
            }
        }
    }

    private func small(_ symbol: String, _ label: String, destructive: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 12, weight: .semibold))
                .foregroundStyle(destructive ? Theme.danger : Theme.foreground)
        }
        .buttonStyle(.glassCircle(32))
        .accessibilityLabel(label)
        .help(label)
    }

    private func copy(_ one: Arrangement) {
        guard let link = app.lobby.link(one)?.absoluteString else { return }
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(link, forType: .string)
        #else
        UIPasteboard.general.string = link
        #endif
        copied = one.id
        Task {
            try? await Task.sleep(for: .seconds(1.6))
            if copied == one.id { copied = nil }
        }
    }

    private func plan() {
        guard !arranging, let at else { return }
        now = Date()
        guard !problem else { return }
        let wanted = normaliseRoomName(typed: planning)
        let room = validRoomName(name: wanted) ? wanted : generateRoomName()
        arranging = true
        Task {
            defer { arranging = false }
            switch await app.lobby.arrange(room: room, at: at, relay: app.relayChoice) {
            case let .success(made):
                planning = ""
                copy(made)
            case let .failure(refused):
                app.problem = refused.sentence
            }
        }
    }
}

// MARK: - The relays

/// Every relay grouped by region with its round trip from here, and the
/// automatic choice above them (ServerPicker.tsx `Rows`).
struct ServersView: View {
    @Bindable var app: AppModel

    var body: some View {
        List {
            Section {
                row(title: t("Automatic"), detail: automaticDetail, symbol: "wand.and.stars", chosen: app.relayChoice == nil, ms: nil, off: false) {
                    app.relayChoice = nil
                }
            }
            ForEach(app.lobby.grouped(), id: \.region) { group in
                Section(group.region) {
                    ForEach(group.relays, id: \.name) { relay in
                        row(title: relay.adminOnly ? "\(say(relay.label ?? relay.name)) [Admin]" : say(relay.label ?? relay.name),
                            detail: relay.maintenance ? t("Not taking calls") : relay.fallback ? "[Fallback]" : nil,
                            symbol: nil, chosen: app.relayChoice == relay.name,
                            ms: app.lobby.measured.map { $0[relay.name] ?? nil }, off: relay.maintenance) {
                            app.relayChoice = relay.name
                        }
                    }
                }
            }
            Section {
                FleetLine(app: app)
            }
        }
        #if os(iOS)
        .listStyle(.insetGrouped)
        .navigationTitle(t("Server"))
        .navigationBarTitleDisplayMode(.inline)
        #else
        .listStyle(.inset)
        .scrollContentBackground(.hidden)
        .background(Backdrop().opacity(0.6))
        #endif
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { await app.lobby.measure() }
                } label: {
                    Label(t("Measure again"), systemImage: "arrow.clockwise")
                }
                .disabled(app.lobby.measuring)
            }
        }
        .task { if app.lobby.measured == nil { await app.lobby.measure() } }
        .animation(Theme.settle, value: app.relayChoice)
    }

    private var automaticDetail: String {
        if let fastest = app.lobby.fastest { return "\(t("Whichever answers fastest")) · \(app.lobby.label(fastest.name))" }
        return t("Whichever answers fastest")
    }

    /// `ms` is nil when nothing has been measured yet, .some(nil) for a relay
    /// that did not answer, and the round trip otherwise.
    private func row(title: String, detail: String?, symbol: String?, chosen: Bool, ms: Double??, off: Bool,
                     pick: @escaping () -> Void) -> some View {
        Button(action: pick) {
            HStack(spacing: 12) {
                if let symbol {
                    Image(systemName: symbol).foregroundStyle(Theme.tally).frame(width: 22)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).foregroundStyle(Theme.foreground)
                    if let detail { Text(detail).font(.system(size: 12)).foregroundStyle(Theme.muted) }
                }
                Spacer()
                if symbol == nil {
                    if ms == nil {
                        if app.lobby.measuring { ProgressView().controlSize(.mini) }
                    } else if let value = ms ?? nil {
                        Text("\(Int(value.rounded())) ms").monospacedDigit().font(.system(size: 13)).foregroundStyle(latencyColour(value))
                    } else if !off {
                        Text(t("Timeout")).font(.system(size: 12)).foregroundStyle(Theme.danger)
                    }
                }
                Image(systemName: "checkmark").font(.system(size: 13, weight: .bold)).foregroundStyle(Theme.tally).opacity(chosen ? 1 : 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(off)
        .opacity(off ? 0.45 : 1)
    }
}

// MARK: - While the deployment is first asked about

/// The ground and the mark, rather than a form asking for an address the app
/// already has.
struct StartingScreen: View {
    @State private var breathing = false

    var body: some View {
        ZStack {
            Backdrop()
            VStack(spacing: 18) {
                Mark(size: 56)
                    .opacity(breathing ? 1 : 0.55)
                    .animation(.easeInOut(duration: 1.1).repeatForever(), value: breathing)
                ProgressView().controlSize(.small).tint(Theme.muted)
            }
        }
        .onAppear { breathing = true }
    }
}

/// Everything on these screens arrives in an order: in from fourteen points
/// below, over 420 ms on the arriving curve.
private struct Rise: ViewModifier {
    let delay: Double
    @State private var shown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .offset(y: shown || reduceMotion ? 0 : 14)
            .onAppear {
                withAnimation(.timingCurve(0.2, 0.9, 0.3, 1, duration: reduceMotion ? 0.01 : 0.42).delay(delay)) {
                    shown = true
                }
            }
    }
}

extension View {
    func rise(_ delay: Double = 0) -> some View { modifier(Rise(delay: delay)) }
}
