import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

// The screens in front of a call (web/src/routes/Lobby.tsx), drawn as the web
// draws them: one column on an empty ground with a single warm glow behind it,
// the name of the thing and how much of its fleet is answering in the corner,
// and everything arriving in an order. The app only ever shows the version of
// these screens a deployment that does not let strangers open rooms shows:
// somebody signs in, or arrives through a link they were sent, and a room
// name typed by somebody nobody knows is not a way in.

// MARK: - The ground

/// Lobby.tsx `Frame`: the header in its own row, the column under it, the
/// glow behind both.
struct LobbyFrame<Corner: View, Content: View>: View {
    @Bindable var app: AppModel
    /// The column's widest: the web's `max-w-sm`, unless a screen needs more.
    var width: CGFloat = 384
    @ViewBuilder var corner: Corner
    @ViewBuilder var content: Content
    @State private var narrow = false

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .top) {
                Theme.background.ignoresSafeArea()
                Glow(size: geometry.size)
                ScrollView {
                    VStack(spacing: 0) {
                        header
                        content
                            .frame(maxWidth: width)
                            .padding(.horizontal, 20)
                            // Lifted off dead centre, and only a little: a page
                            // that can scroll should not begin with a screenful
                            // of nothing.
                            .padding(.top, geometry.size.width >= 640 ? 32 : 16)
                            .padding(.bottom, 64)
                            .frame(maxWidth: .infinity)
                    }
                }
                .scrollIndicators(.never)
            }
            .onAppear { narrow = geometry.size.width < 640 }
            .onChange(of: geometry.size.width) { narrow = geometry.size.width < 640 }
        }
        .overlay(alignment: .bottom) { Toast(text: app.problem) { app.problem = nil } }
    }

    private var header: some View {
        HStack(spacing: 8) {
            HStack(spacing: 8) {
                Mark(size: 20)
                Text("Tomoshibi").font(.system(size: 13, weight: .semibold)).tracking(-0.2)
                    .foregroundStyle(Theme.foreground)
                // Beside the name rather than under it: it is a property of this
                // deployment, not a heading of its own.
                Rectangle().fill(Theme.border).frame(width: 1, height: 14).padding(.leading, 4)
                Fleet(app: app)
                ServerName(app: app)
            }
            .layoutPriority(1)
            Spacer(minLength: 8)
            HStack(spacing: 8) { corner }
        }
        .environment(\.narrowLobby, narrow)
        .padding(.horizontal, 20)
        .padding(.vertical, 20)
    }
}

/// Whether the header is on a phone's width, where the web's corner drops
/// its words and keeps its symbols (`hidden sm:inline`, below 640 points).
private struct NarrowLobby: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var narrowLobby: Bool {
        get { self[NarrowLobby.self] }
        set { self[NarrowLobby.self] = newValue }
    }
}

/// One glow, high and behind everything, in the signal colour at a fraction of
/// its strength: it gives the screen a top and a bottom, so a single card
/// reads as placed rather than stranded (Lobby.tsx, `radial-gradient(60% 100%
/// at 50% 0%, tally, transparent 70%)` at seven per cent).
private struct Glow: View {
    let size: CGSize

    var body: some View {
        let height = max(size.height * 0.45, 1)
        // A circle that has faded out by seven tenths of the glow's height,
        // stretched sideways into the web's ellipse: drawn as an ellipse with
        // a gradient inside it, the shape's own edge showed as a line across
        // the screen wherever the gradient had not yet reached nothing.
        Rectangle()
            .fill(RadialGradient(colors: [Theme.tally, Theme.tally.opacity(0)], center: .top,
                                 startRadius: 0, endRadius: height * 0.7))
            .scaleEffect(x: size.width * 0.6 / height, y: 1, anchor: .top)
            .frame(width: size.width, height: height, alignment: .top)
            .clipped()
            .opacity(0.07)
            .allowsHitTesting(false)
            .ignoresSafeArea()
    }
}

/// The product's own mark, as the favicon draws it: a room of pictures with
/// one of them live, in the ground, a neutral from the warm ramp, and the one
/// hue the interface spends.
struct Mark: View {
    var size: CGFloat = 20

    var body: some View {
        Canvas { context, area in
            let unit = area.width / 32
            context.fill(Path(roundedRect: CGRect(origin: .zero, size: area), cornerRadius: 8 * unit),
                         with: .color(Mark.ground))
            for (x, y, live) in [(6.0, 8.0, false), (17, 8, false), (6, 17, false), (17, 17, true)] {
                let tile = CGRect(x: x * unit, y: y * unit, width: 9 * unit, height: 7 * unit)
                context.fill(Path(roundedRect: tile, cornerRadius: 2 * unit),
                             with: .color(live ? Mark.live : Mark.idle))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    static let ground = Color(red: 0x0F / 255, green: 0x0E / 255, blue: 0x0D / 255)
    static let idle = Color(red: 0x55 / 255, green: 0x52 / 255, blue: 0x4E / 255)
    static let live = Color(red: 0xFF / 255, green: 0xA2 / 255, blue: 0x3B / 255)
}

/// How much of the fleet is answering (Lobby.tsx `Fleet`): both numbers,
/// because either alone says nothing, and a light read before them.
private struct Fleet: View {
    @Bindable var app: AppModel

    var body: some View {
        if let relays = app.relays, relays.total > 0 {
            let all = relays.online == relays.total
            let none = relays.online == 0
            HStack(spacing: 6) {
                Circle().fill(none ? Theme.danger : all ? Theme.good : Theme.tally).frame(width: 6, height: 6)
                Text("\(relays.online)/\(relays.total)")
                    .font(.system(size: 11).monospacedDigit())
                    .foregroundStyle(Theme.muted)
            }
            .help(t("Relays answering"))
            .transition(Theme.arrive)
        }
    }
}

/// Which deployment this is, and the way to another one: the web is always
/// on the deployment it was loaded from, and an app has to say.
private struct ServerName: View {
    @Bindable var app: AppModel

    var body: some View {
        if let base = app.deployment?.baseUrl() {
            Button { app.forgetServer() } label: {
                Text(URL(string: base)?.host() ?? base)
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.muted.opacity(0.8))
                    .lineLimit(1)
            }
            .buttonStyle(.plain)
            .help(t("Server"))
        }
    }
}

/// The language, chosen anywhere before a call as on every web page.
struct LanguageMenu: View {
    @State private var language = Localization.shared.language
    @Environment(\.narrowLobby) private var narrow

    var body: some View {
        Menu {
            ForEach(Language.allCases) { one in
                Button {
                    language = one
                    Localization.shared.language = one
                } label: {
                    if one == language { Label(one.name, systemImage: "checkmark") } else { Text(one.name) }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "translate").font(.system(size: 13))
                if !narrow { Text(language.name).font(.system(size: 12.5, weight: .medium)) }
            }
            .foregroundStyle(Theme.foreground)
            .padding(.horizontal, 8).padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(t("Language"))
    }
}

/// Something that happened and is over, at the foot of the screen for a few
/// seconds (the web's notices, Notices.tsx).
private struct Toast: View {
    let text: String?
    let dismiss: () -> Void

    var body: some View {
        ZStack {
            if let text {
                Text(text)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Theme.surfaceHigh))
                    .overlay(alignment: .leading) {
                        UnevenRoundedRectangle(topLeadingRadius: 10, bottomLeadingRadius: 10)
                            .fill(Theme.danger).frame(width: 3)
                    }
                    .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.border))
                    .shadow(color: .black.opacity(0.45), radius: 18, y: 8)
                    .padding(.horizontal, 20).padding(.bottom, 28)
                    .frame(maxWidth: 440)
                    .onTapGesture(perform: dismiss)
                    .task(id: text) {
                        try? await Task.sleep(for: .seconds(5))
                        dismiss()
                    }
                    .transition(Theme.slideUp)
            }
        }
        .animation(Theme.settle, value: text)
    }
}

// MARK: - Pieces the cards are made of

/// Everything on these screens arrives in an order (`animate-rise` with its
/// delays): in from fourteen points below, over 420 ms on the arriving curve.
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

/// The web's `rounded-xl border bg-surface p-4`.
struct Panel<Content: View>: View {
    var padding: CGFloat = 16
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) { content }
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
    }
}

/// A card's heading: the square with its symbol, the title, and a line under it.
private struct CardHeading<Trailing: View>: View {
    let symbol: String
    var lit = false
    let title: String
    let detail: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(lit ? Theme.tally : Theme.muted)
                .frame(width: 40, height: 40)
                .background(RoundedRectangle(cornerRadius: 8).fill(lit ? Theme.tally.opacity(0.15) : Theme.surfaceHigh))
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .medium)).foregroundStyle(Theme.foreground)
                Text(detail).font(.system(size: 12)).foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            trailing
        }
    }
}

extension CardHeading where Trailing == EmptyView {
    init(symbol: String, lit: Bool = false, title: String, detail: String) {
        self.init(symbol: symbol, lit: lit, title: title, detail: detail) { EmptyView() }
    }
}

/// A field as the lobby draws one: forty points, `bg-surface-hi`, a border
/// that brightens while it is typed in.
struct LobbyInput: View {
    let placeholder: String
    @Binding var text: String
    var secure = false
    var onSubmit: () -> Void = {}
    @FocusState private var focused: Bool

    var body: some View {
        Group {
            if secure {
                SecureField("", text: $text, prompt: Text(placeholder).foregroundStyle(Theme.muted))
            } else {
                TextField("", text: $text, prompt: Text(placeholder).foregroundStyle(Theme.muted))
            }
        }
        .textFieldStyle(.plain)
        .font(.system(size: 14))
        .foregroundStyle(Theme.foreground)
        .focused($focused)
        .onSubmit(onSubmit)
        .padding(.horizontal, 12)
        .frame(height: 40)
        .background(RoundedRectangle(cornerRadius: 8).fill(Theme.surfaceHigh))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(focused ? Theme.foreground.opacity(0.4) : Theme.border))
        .animation(Theme.settle, value: focused)
        #if os(iOS)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        #endif
        .accessibilityLabel(placeholder)
    }
}

/// The web's two button looks: primary (light on dark) and secondary.
struct LobbyButtonStyle: ButtonStyle {
    var primary = true
    var tall = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: tall ? 15 : 14, weight: .medium))
            .foregroundStyle(primary ? Theme.background : Theme.foreground)
            .padding(.horizontal, 16)
            .frame(height: tall ? 48 : 40)
            .background(RoundedRectangle(cornerRadius: 8)
                .fill(primary ? Theme.foreground : Theme.surfaceHigh)
                .opacity(configuration.isPressed ? 0.85 : 1))
            .opacity(enabled ? 1 : 0.5)
            .contentShape(Rectangle())
            .animation(Theme.settle, value: enabled)
    }
}

/// A small spinner in a button's own colour.
private struct Busy: View {
    var dark = true
    var body: some View {
        ProgressView().controlSize(.small).tint(dark ? Theme.background : Theme.foreground)
    }
}

// MARK: - Signing in

/// Say who you are (Lobby.tsx `SignIn`), and, because an app cannot be sent
/// to a page by a link the way a browser can, a place to open the link
/// somebody sent.
struct SignInScreen: View {
    @Bindable var app: AppModel
    @State private var name = ""
    @State private var passphrase = ""
    @State private var shown = false
    @State private var link = ""

    var body: some View {
        LobbyFrame(app: app) {
            LanguageMenu()
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(t("Sign in")).font(.system(size: 22, weight: .semibold)).tracking(-0.4)
                        .foregroundStyle(Theme.foreground)
                    Text(t("Meetings here are started by the people who belong here. If somebody sent you a link, open that instead."))
                        .font(.system(size: 13)).foregroundStyle(Theme.muted).lineSpacing(3)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .rise()

                credentials.rise(0.06)

                Button(action: signIn) {
                    HStack { if app.busy { Busy() } else { Text(t("Sign in")) } }.frame(maxWidth: .infinity)
                }
                .buttonStyle(LobbyButtonStyle(tall: true))
                .disabled(app.busy || name.isEmpty || passphrase.isEmpty)
                .rise(0.12)

                Panel {
                    CardHeading(symbol: "link", title: t("Open the link"), detail: t("Paste the link you were sent."))
                    HStack(spacing: 8) {
                        LobbyInput(placeholder: "https://", text: $link, onSubmit: openLink)
                        Button(t("Join"), action: openLink)
                            .buttonStyle(LobbyButtonStyle(primary: false))
                            .disabled(URL(string: link.trimmingCharacters(in: .whitespaces))?.host() == nil)
                    }
                }
                .rise(0.18)
            }
        }
    }

    /// One box holding two fields, as the web holds them: they are one answer.
    private var credentials: some View {
        VStack(spacing: 0) {
            row(symbol: "person", lit: false) {
                TextField("", text: $name, prompt: Text(t("Name")).foregroundStyle(Theme.muted))
                    .textContentType(.username)
            }
            Rectangle().fill(Theme.border).frame(height: 1)
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
                    // Only once there is something to look at: an eye beside an
                    // empty field offers to reveal nothing.
                    if !passphrase.isEmpty {
                        Button { shown.toggle() } label: {
                            Image(systemName: shown ? "eye.slash" : "eye").font(.system(size: 12)).foregroundStyle(Theme.muted)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(shown ? t("Hide passphrase") : t("Show passphrase"))
                        .transition(Theme.arrive)
                    }
                }
                .animation(Theme.settle, value: passphrase.isEmpty)
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
    }

    private func row<Field: View>(symbol: String, lit: Bool, @ViewBuilder field: () -> Field) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 14))
                .foregroundStyle(lit ? Theme.tally : Theme.muted)
                .frame(width: 16)
                .animation(Theme.settle, value: lit)
            field()
                .textFieldStyle(.plain)
                .font(.system(size: 14))
                .foregroundStyle(Theme.foreground)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                #endif
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
    }

    private func signIn() {
        guard !name.isEmpty, !passphrase.isEmpty, !app.busy else { return }
        Task {
            await app.signIn(name: name, passphrase: passphrase)
            // A refused attempt is something that happened, not something
            // still true: the field is cleared for the next one, as the web
            // clears it.
            if app.account == nil { passphrase = "" }
        }
    }

    private func openLink() {
        guard let url = URL(string: link.trimmingCharacters(in: .whitespaces)), url.host() != nil else { return }
        Task { await app.open(url) }
    }
}

// MARK: - Signed in

/// Say what you want (Lobby.tsx `Lobby`): start a meeting, join one, arrange
/// one, and where it is held.
struct HomeScreen: View {
    @Bindable var app: AppModel
    let account: Account
    @State private var naming = ""
    @State private var typed = ""
    @State private var busy: String?
    @State private var scheduling = false

    var body: some View {
        LobbyFrame(app: app) {
            Corner(app: app, account: account)
        } content: {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(t("Hello, {name}", ["name": account.name]))
                        .font(.system(size: 22, weight: .semibold)).tracking(-0.4).foregroundStyle(Theme.foreground)
                    Text(t("What would you like to do?")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                }
                .rise()

                // Naming the new meeting, with a generated name as the answer
                // to leaving it blank rather than as the only answer.
                Panel {
                    CardHeading(symbol: "plus", lit: true, title: t("Start a meeting"),
                                detail: t("Name it yourself, or leave it blank for one nobody has used."))
                    HStack(spacing: 8) {
                        LobbyInput(placeholder: t("Room name"), text: $naming, onSubmit: start)
                        // Never disabled on the field being empty: blank is an
                        // answer here.
                        Button(action: start) {
                            HStack(spacing: 6) {
                                if busy == "start" { Busy() } else { Text(t("Start")) }
                                Image(systemName: "arrow.right").font(.system(size: 13, weight: .semibold))
                            }
                        }
                        .buttonStyle(LobbyButtonStyle())
                        .disabled(busy != nil)
                    }
                }
                .rise(0.06)

                Panel {
                    CardHeading(symbol: "eye", title: t("Join a meeting"), detail: t("The name you were given."))
                    HStack(spacing: 8) {
                        LobbyInput(placeholder: t("Room name"), text: $typed, onSubmit: join)
                        Button(action: join) {
                            if busy == "join" { Busy(dark: false) } else { Text(t("Join")) }
                        }
                        .buttonStyle(LobbyButtonStyle(primary: false))
                        .disabled(!validRoomName(name: normaliseRoomName(typed: typed)) || busy != nil)
                    }
                }
                .rise(0.12)

                ScheduleCard(app: app, account: account, open: $scheduling)
                    .rise(0.18)

                // Under both choices rather than inside either: it is the same
                // decision whichever of them somebody is making.
                if (app.relays?.relays.count ?? 0) > 1 && !scheduling {
                    ServerList(app: app)
                        .rise(0.24)
                        .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: scheduling)
        }
    }

    private func start() {
        let wanted = normaliseRoomName(typed: naming)
        go("start", validRoomName(name: wanted) ? wanted : generateRoomName())
    }

    private func join() {
        let wanted = normaliseRoomName(typed: typed)
        guard validRoomName(name: wanted) else { return }
        go("join", wanted)
    }

    /// Takes somebody to a name once it is clear the name is the right kind:
    /// a meeting that already exists cannot be started and one that does not
    /// cannot be joined. An unknown answer goes through, because the check is
    /// a courtesy and the join is the authority.
    private func go(_ kind: String, _ room: String) {
        guard busy == nil, let deployment = app.deployment else { return }
        busy = kind
        Task {
            defer { busy = nil }
            let live = (try? await deployment.roomLive(room: room)) ?? nil
            if kind == "start", live == true {
                app.problem = t("A meeting is already happening under that name.")
                return
            }
            if kind == "join", live == false {
                app.problem = t("No meeting is happening under that name.")
                return
            }
            app.prepare(room: room)
        }
    }
}

/// The corner of a page somebody is signed in on (Lobby.tsx `Corner`).
private struct Corner: View {
    @Bindable var app: AppModel
    let account: Account
    @Environment(\.narrowLobby) private var narrow

    var body: some View {
        if account.admin, let url = page("/admin") {
            Link(destination: url) {
                HStack(spacing: 6) {
                    Image(systemName: "shield").font(.system(size: 12)).foregroundStyle(Theme.tally)
                    if !narrow { Text(t("Admin console")).font(.system(size: 12)) }
                }
                .foregroundStyle(Theme.foreground)
                .padding(.horizontal, 10).frame(height: 30)
                .background(RoundedRectangle(cornerRadius: 6).fill(Theme.tally.opacity(0.1)))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.tally.opacity(0.3)))
            }
            .buttonStyle(.plain)
            .help(t("Admin console"))
        }
        if let url = page("/account") {
            Link(destination: url) {
                HStack(spacing: 6) {
                    Image(systemName: "person").font(.system(size: 12))
                    // Cut rather than framed: a frame of the longest width
                    // pushed a short name into the middle of a wide pill.
                    if !narrow {
                        Text(account.name.count > 18 ? String(account.name.prefix(17)) + "…" : account.name)
                            .font(.system(size: 12)).lineLimit(1)
                    }
                }
                .foregroundStyle(Theme.muted)
                .padding(.horizontal, 10).frame(height: 30)
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
            }
            .buttonStyle(.plain)
            .help(t("User console"))
        }
        Button { Task { await app.signOut() } } label: {
            Image(systemName: "rectangle.portrait.and.arrow.right").font(.system(size: 12))
                .foregroundStyle(Theme.muted)
                .frame(width: 30, height: 30)
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.border))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(t("Sign out"))
        .help(t("Sign out"))
        LanguageMenu()
    }

    private func page(_ path: String) -> URL? {
        app.deployment.flatMap { URL(string: $0.baseUrl().trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path) }
    }
}

// MARK: - Where the call is held

/// The relays and how quickly each answers, to choose one by hand
/// (ServerPicker.tsx `ServerList` and `Rows`): measured once on arrival, again
/// on request, and settled into order once the measurements land, so a row
/// does not move out from under a finger already reaching for it.
struct ServerList: View {
    @Bindable var app: AppModel
    @State private var measured: [String: Double?]?
    @State private var measuring = false
    @State private var order: [String] = []

    var body: some View {
        let relays = app.relays?.relays ?? []
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(t("Server")).font(.system(size: 11)).foregroundStyle(Theme.muted)
                Spacer()
                Button(action: measure) {
                    HStack(spacing: 4) {
                        Image(systemName: "arrow.clockwise").font(.system(size: 10, weight: .semibold))
                            .rotationEffect(.degrees(measuring ? 360 : 0))
                            .animation(measuring ? .linear(duration: 0.9).repeatForever(autoreverses: false) : Theme.settle, value: measuring)
                        Text(t("Measure again")).font(.system(size: 11))
                    }
                    .foregroundStyle(Theme.muted)
                    .padding(.horizontal, 6).padding(.vertical, 3)
                }
                .buttonStyle(.plain)
                .disabled(measuring)
            }
            .padding(.horizontal, 4)

            VStack(spacing: 0) {
                option(label: t("Automatic"), describes: t("Whichever answers fastest"), symbol: "wand.and.stars",
                       chosen: app.relayChoice == nil, disabled: false, latency: nil, known: true) {
                    app.relayChoice = nil
                }
                ForEach(Array(groups(relays).enumerated()), id: \.offset) { _, group in
                    if !group.region.isEmpty {
                        Text(say(group.region))
                            .font(.system(size: 10.5, weight: .medium))
                            .textCase(.uppercase)
                            .tracking(0.4)
                            .foregroundStyle(Theme.muted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 12).padding(.top, 10).padding(.bottom, 4)
                    }
                    ForEach(group.relays, id: \.name) { relay in
                        option(label: relay.adminOnly ? "\(say(relay.label ?? relay.name)) [Admin]" : say(relay.label ?? relay.name),
                               describes: relay.maintenance ? t("Not taking calls") : relay.fallback ? "[Fallback]" : nil,
                               symbol: nil,
                               chosen: app.relayChoice == relay.name,
                               disabled: relay.maintenance,
                               latency: measured?[relay.name] ?? nil,
                               known: measured != nil) {
                            app.relayChoice = relay.name
                        }
                    }
                }
            }
            .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
            .clipShape(RoundedRectangle(cornerRadius: 12))
        }
        .task { if measured == nil { measure() } }
    }

    /// Relays under their region, in the order the measurements settled on.
    private func groups(_ relays: [Relay]) -> [(region: String, relays: [Relay])] {
        let ranked = order.isEmpty ? relays : relays.sorted { (order.firstIndex(of: $0.name) ?? .max) < (order.firstIndex(of: $1.name) ?? .max) }
        var out: [(region: String, relays: [Relay])] = []
        for relay in ranked {
            let region = (relay.region ?? "").split(separator: "/").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }.joined(separator: " / ")
            if let index = out.firstIndex(where: { $0.region == region }) {
                out[index].relays.append(relay)
            } else {
                out.append((region, [relay]))
            }
        }
        return out
    }

    private func measure() {
        guard !measuring, let relays = app.relays?.relays, !relays.isEmpty else { return }
        measuring = true
        Task {
            let timings = await relayTimings(relays: relays)
            var found: [String: Double?] = [:]
            for timing in timings { found[timing.name] = timing.millis }
            measured = found
            if order.isEmpty {
                order = timings.sorted { ($0.millis ?? .infinity) < ($1.millis ?? .infinity) }.map(\.name)
            }
            measuring = false
        }
    }

    private func option(label: String, describes: String?, symbol: String?, chosen: Bool, disabled: Bool,
                        latency: Double?, known: Bool, pick: @escaping () -> Void) -> some View {
        Button(action: pick) {
            HStack(spacing: 10) {
                Image(systemName: "checkmark").font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Theme.tally).opacity(chosen ? 1 : 0)
                if let symbol {
                    Image(systemName: symbol).font(.system(size: 12)).foregroundStyle(Theme.muted)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(label).font(.system(size: 14)).foregroundStyle(Theme.foreground).lineLimit(1)
                    if let describes {
                        Text(describes).font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                }
                Spacer(minLength: 8)
                if symbol == nil {
                    if !known {
                        ProgressView().controlSize(.mini)
                    } else if let latency {
                        Text("\(Int(latency.rounded())) ms")
                            .font(.system(size: 11.5).monospacedDigit())
                            .foregroundStyle(latency <= 150 ? Theme.good : latency <= 400 ? Theme.tally : Theme.danger)
                            .transition(Theme.arrive)
                    } else {
                        Text(t("Timeout")).font(.system(size: 11)).foregroundStyle(Theme.danger)
                    }
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 11)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(chosen ? Theme.foreground.opacity(0.08) : .clear)
            // A bar down the edge as well as the tick: the tick says which is
            // chosen to somebody reading, the bar to somebody glancing.
            .overlay(alignment: .leading) {
                UnevenRoundedRectangle(bottomTrailingRadius: 2, topTrailingRadius: 2)
                    .fill(Theme.tally).frame(width: 3).padding(.vertical, 4).opacity(chosen ? 1 : 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.45 : 1)
        .animation(Theme.settle, value: chosen)
    }
}

// MARK: - Arranging a meeting

/// Schedule.tsx: a card that opens. Closed it is one line like the two above
/// it; open it is the whole question (which name, which day, what time, which
/// machine) with the meetings already arranged under it.
private struct ScheduleCard: View {
    @Bindable var app: AppModel
    let account: Account
    @Binding var open: Bool
    @State private var planning = ""
    @State private var day: Date?
    @State private var hour = Calendar.current.component(.hour, from: ScheduleCard.nextQuarter)
    @State private var minute = Calendar.current.component(.minute, from: ScheduleCard.nextQuarter)
    @State private var arranging = false
    @State private var list: [Arrangement] = []
    @State private var status = "loading"
    @State private var copied: String?
    @State private var now = Date()

    /// How far ahead the server lets a meeting be arranged.
    private static let ahead: TimeInterval = 60 * 24 * 60 * 60

    private static var nextQuarter: Date {
        let calendar = Calendar.current
        let now = Date()
        let minutes = calendar.component(.minute, from: now)
        let up = (minutes / 15 + 1) * 15
        return calendar.date(byAdding: .minute, value: up - minutes, to: calendar.date(bySetting: .second, value: 0, of: now) ?? now) ?? now
    }

    private var canArrange: Bool { account.can?.contains("arrange") ?? true }

    private var at: Date? {
        guard let day else { return nil }
        return Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: day)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { open.toggle() } label: {
                CardHeading(symbol: "calendar.badge.clock", title: t("Schedule a meeting"),
                            detail: t("Pick a time and hand out the link before it starts.")) {
                    Image(systemName: "chevron.down").font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(Theme.muted)
                        .rotationEffect(.degrees(open ? 180 : 0))
                }
                .padding(16)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if open {
                form
                    .padding(.horizontal, 16).padding(.bottom, 16)
                    .transition(Theme.arrive)
            }
        }
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
        .animation(Theme.settle, value: open)
        .task { await refresh() }
        .onChange(of: open) { if open { now = Date(); Task { await refresh() } } }
    }

    @ViewBuilder
    private var form: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !canArrange {
                Text(t("Arranging meetings has not been opened to you."))
                    .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    label(t("Room name"))
                    LobbyInput(placeholder: t("Room name"), text: $planning)
                    Text(t("Name it yourself, or leave it blank for one nobody has used."))
                        .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                }
                VStack(alignment: .leading, spacing: 6) {
                    label(t("Day"))
                    DatePicker("", selection: Binding(get: { day ?? Date() }, set: { day = $0 }),
                               in: Calendar.current.startOfDay(for: Date())...Date().addingTimeInterval(Self.ahead),
                               displayedComponents: .date)
                        .datePickerStyle(.graphical)
                        .labelsHidden()
                        .tint(Theme.tally)
                        .frame(maxWidth: .infinity)
                        .padding(8)
                        .background(RoundedRectangle(cornerRadius: 8).fill(Theme.surfaceHigh))
                }
                VStack(alignment: .leading, spacing: 6) {
                    label(t("Time"))
                    HStack(spacing: 8) {
                        clock(t("Hour"), value: $hour, values: Array(0..<24))
                        clock(t("Minute"), value: $minute, values: [0, 15, 30, 45])
                    }
                }
                if (app.relays?.relays.count ?? 0) > 1 {
                    ServerList(app: app)
                }
                HStack(spacing: 10) {
                    Text(verdict).font(.system(size: 12)).foregroundStyle(problem ? Theme.danger : Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 8)
                    Button(action: plan) {
                        if arranging { Busy() } else { Text(t("Arrange")) }
                    }
                    .buttonStyle(LobbyButtonStyle())
                    .disabled(arranging || at == nil || problem)
                }
            }

            switch status {
            case "signed out":
                HStack {
                    Text(t("Your session has expired. Sign in again.")).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    Spacer()
                    Button(t("Sign in")) { Task { await app.signOut() } }.buttonStyle(LobbyButtonStyle(primary: false))
                }
            case "failed":
                HStack {
                    Text(t("Could not load your meetings.")).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    Spacer()
                    Button(t("Try again")) { Task { await refresh() } }.buttonStyle(LobbyButtonStyle(primary: false))
                }
            case "loading" where list.isEmpty:
                ProgressView().controlSize(.small).frame(maxWidth: .infinity)
            default:
                if canArrange && list.isEmpty {
                    Text(t("No meetings arranged yet.")).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                }
            }

            if !list.isEmpty {
                VStack(spacing: 0) {
                    ForEach(list, id: \.id) { one in
                        meetingRow(one)
                        if one.id != list.last?.id { Rectangle().fill(Theme.border).frame(height: 1) }
                    }
                }
                .background(RoundedRectangle(cornerRadius: 8).fill(Theme.surfaceHigh))
            }
        }
        .animation(Theme.settle, value: list.map(\.id))
        .animation(Theme.settle, value: status)
    }

    private func label(_ text: String) -> some View {
        Text(text).font(.system(size: 11)).foregroundStyle(Theme.muted)
    }

    private func clock(_ label: String, value: Binding<Int>, values: [Int]) -> some View {
        Picker(label, selection: value) {
            ForEach(values, id: \.self) { Text(String(format: "%02d", $0)).tag($0) }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .frame(width: 96, height: 40)
        .background(RoundedRectangle(cornerRadius: 8).fill(Theme.surfaceHigh))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.border))
        .accessibilityLabel(label)
    }

    private var problem: Bool {
        guard let at else { return false }
        return at < now || at > now.addingTimeInterval(Self.ahead)
    }

    private var verdict: String {
        guard let at else { return t("Choose a day.") }
        if at < now { return t("That time has already gone.") }
        if at > now.addingTimeInterval(Self.ahead) { return t("That is more than sixty days away.") }
        return t("Arranged for {when}", ["when": Self.said(at)])
    }

    /// meeting.ts `whenSaid`: the date, the time and the zone, in the reader's
    /// own language.
    static func said(_ date: Date) -> String {
        date.formatted(Date.FormatStyle(date: .abbreviated, time: .shortened, locale: Localization.shared.language.locale)
            .timeZone(.specificName(.short)))
    }

    private func meetingRow(_ one: Arrangement) -> some View {
        let when = AppModel.instant(one.at).map(Self.said) ?? one.at
        let state = one.ended ? " · \(t("Over"))" : one.started ? " · \(t("Under way"))" : ""
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 2) {
                if one.ended {
                    Text(one.room).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Theme.muted)
                } else {
                    Button { if let url = link(one) { Task { await app.open(url) } } } label: {
                        Text(one.room).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Theme.foreground).underline()
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(t("Open {room}", ["room": one.room]))
                }
                Text(when + state).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
            }
            Spacer(minLength: 6)
            small(copied == one.id ? "checkmark" : "doc.on.doc", copied == one.id ? t("Link copied") : t("Copy link")) { copy(one) }
            #if os(macOS)
            small("calendar.badge.plus", t("Add to calendar")) { addToCalendar(one) }
            #else
            if let file = calendarFile(one) {
                ShareLink(item: file) {
                    Image(systemName: "calendar.badge.plus").font(.system(size: 12)).foregroundStyle(Theme.muted)
                        .frame(width: 30, height: 30)
                }
                .accessibilityLabel(t("Add to calendar"))
            }
            #endif
            small("xmark", t("Cancel meeting")) { cancel(one) }
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
    }

    private func small(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 12)).foregroundStyle(Theme.muted)
                .frame(width: 30, height: 30).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .help(label)
    }

    /// meeting.ts `linkFor`: the page, the meeting's token, the room.
    private func link(_ one: Arrangement) -> URL? {
        guard let base = app.deployment?.baseUrl(), let token = one.token else { return nil }
        let root = base.hasSuffix("/") ? base : base + "/"
        let encoded = token.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? token
        return URL(string: "\(root)?meeting=\(encoded)#/\(one.room)")
    }

    private func copy(_ one: Arrangement) {
        guard let link = link(one)?.absoluteString else { return }
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

    /// calendar.ts `icsFor`, written where the system can hand it to a
    /// calendar.
    private func calendarFile(_ one: Arrangement) -> URL? {
        guard let link = link(one)?.absoluteString, let start = AppModel.instant(one.at) else { return nil }
        let stamp: (Date) -> String = { date in
            let format = DateFormatter()
            format.locale = Locale(identifier: "en_US_POSIX")
            format.timeZone = TimeZone(identifier: "UTC")
            format.dateFormat = "yyyyMMdd'T'HHmmss'Z'"
            return format.string(from: date)
        }
        let escape: (String) -> String = {
            $0.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: ";", with: "\\;")
                .replacingOccurrences(of: ",", with: "\\,").replacingOccurrences(of: "\n", with: "\\n")
        }
        let lines = [
            "BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Tomoshibi//Arranged meeting//EN", "CALSCALE:GREGORIAN",
            "METHOD:PUBLISH", "BEGIN:VEVENT", "UID:\(one.id)@tomoshibi", "DTSTAMP:\(stamp(Date()))",
            "DTSTART:\(stamp(start))", "SUMMARY:\(escape(one.room))", "URL:\(link)", "DESCRIPTION:\(escape(link))",
            "END:VEVENT", "END:VCALENDAR",
        ]
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(one.room).ics")
        guard (try? (lines.joined(separator: "\r\n") + "\r\n").write(to: file, atomically: true, encoding: .utf8)) != nil else { return nil }
        return file
    }

    #if os(macOS)
    private func addToCalendar(_ one: Arrangement) {
        if let file = calendarFile(one) { NSWorkspace.shared.open(file) }
    }
    #endif

    private func refresh() async {
        guard let deployment = app.deployment else { return }
        status = "loading"
        do {
            list = try await deployment.arrangements()
            status = "ready"
        } catch let ProtocolError.Refused(_, code) where code == "not_signed_in" {
            status = "signed out"
        } catch {
            status = "failed"
        }
    }

    private func plan() {
        guard !arranging, let at, let deployment = app.deployment else { return }
        now = Date()
        guard !problem else { return }
        let wanted = normaliseRoomName(typed: planning)
        let room = validRoomName(name: wanted) ? wanted : generateRoomName()
        arranging = true
        Task {
            defer { arranging = false }
            do {
                let made = try await deployment.arrangeMeeting(room: room, atMs: Int64(at.timeIntervalSince1970 * 1000),
                                                              relay: app.relayChoice ?? "")
                list = (list + [made]).sorted { $0.at < $1.at }
                status = "ready"
                planning = ""
                day = nil
                copy(made)
            } catch {
                app.problem = Self.explain(error)
            }
        }
    }

    private func cancel(_ one: Arrangement) {
        guard let deployment = app.deployment else { return }
        Task {
            do {
                try await deployment.cancelMeeting(id: one.id)
                list.removeAll { $0.id == one.id }
            } catch {
                app.problem = Self.explain(error)
            }
        }
    }

    /// Schedule.tsx `explainArranging`.
    static func explain(_ error: Error) -> String {
        guard case let ProtocolError.Refused(_, code) = error else { return t("Something went wrong. Try again.") }
        switch code {
        case "room_already_arranged": return t("That room already has a meeting arranged.")
        case "bad_time": return t("That time is not one a meeting can be arranged for.")
        case "meetings_need_invites": return t("Meetings here ask for an account, so a link cannot let anybody in.")
        case "not_yours": return t("That is somebody else's room.")
        case "too_many_meetings": return t("You have as many meetings arranged as you can have.")
        case "invalid_room": return t("Room names can only use lowercase letters, numbers and dashes.")
        case "scoped_room": return t("A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.")
        case "relay_not_allowed": return t("Access denied. That server is for administrators.")
        case "not_signed_in": return t("Your session has expired. Sign in again.")
        case "not_allowed": return t("Arranging meetings has not been opened to you.")
        default: return t("Something went wrong. Try again.")
        }
    }
}

// MARK: - Before anything is known

/// While the deployment the app opens on is first asked about: the ground and
/// the mark, rather than a form asking for an address it already has.
struct StartingScreen: View {
    @State private var breathing = false

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Theme.background.ignoresSafeArea()
                Glow(size: geometry.size)
                VStack(spacing: 18) {
                    Mark(size: 44)
                        .opacity(breathing ? 1 : 0.55)
                        .animation(.easeInOut(duration: 1.1).repeatForever(), value: breathing)
                    ProgressView().controlSize(.small).tint(Theme.muted)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onAppear { breathing = true }
    }
}
