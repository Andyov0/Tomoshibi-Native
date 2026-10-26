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

    /// The part of the lobby a debug launch opens, given to the first lobby
    /// only: read again by the lobby a call came back to, it opened the start
    /// sheet over it once more after the test had left the call (the Android
    /// lobby's LobbyState found the same).
    @MainActor private static var taken = false

    @MainActor static func take() -> String? {
        guard !taken else { return nil }
        taken = true
        return open
    }

    /// TOMOSHIBI_DEBUG_ROOM: what is typed into the start and join sheet's
    /// room field when it opens, which takes the keyboard.
    static let room: String? = {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_ROOM"]
        #else
        nil
        #endif
    }()

    static func sheet(_ opened: String?) -> MeetingSheet.Mode? {
        switch opened {
        case "start": .start
        case "join": .join("")
        case "again": .again(room ?? "")
        default: nil
        }
    }
}

/// Everything the lobby has open, kept above the choice between the phone's
/// layout and the sidebar's, so that an iPhone Duo opened or closed in the
/// middle of choosing a time or typing a room keeps it: the phone's schedule
/// sheet is the sidebar's schedule section, its pushed server page the server
/// section, and the start sheet, the settings and the join bar's words are
/// the same in both (the Android lobby's LobbyState, for a Fold). Kept inside
/// either layout, everything there went with the layout when the size class
/// changed, the section somebody was in and whatever was half typed.
@MainActor
@Observable
final class LobbyState {
    /// Where in the lobby somebody is, beside its home.
    enum Place: Hashable { case home, scheduled, servers }

    var place: Place
    /// The start and join sheet, which keeps its own draft for as long as it
    /// is open: it is presented above both layouts, so it stays open through
    /// a change between them.
    var sheet: MeetingSheet.Mode?
    var settings: Bool
    /// The phone's join bar, half typed.
    var typed = ""
    /// What the schedule's form holds, whichever layout draws it.
    let schedule = ScheduleDraft()
    /// One set of popovers for whichever layout is drawn; each column of the
    /// sidebar's draws its own, and a click in either closes whichever is
    /// open.
    let popovers = Popovers()

    init() {
        let opened = LobbyDebug.take()
        place = opened == "scheduled" ? .scheduled : opened == "servers" ? .servers : .home
        sheet = LobbyDebug.sheet(opened)
        settings = opened == "settings"
    }
}

/// Signed in or not is the whole difference; and, signed in, how wide the
/// window is. On an iPad, an iPhone Duo opened out or a phone whose app shares
/// the screen, a regular width is the Mac's window with a sidebar; a compact
/// one, the outer screen of a Duo among them, is the phone's home. Decided by
/// the size class rather than by what the device is: a Duo is both.
struct LobbyView: View {
    @Bindable var app: AppModel
    @State private var state = LobbyState()
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.layoutDirection) private var direction
    #endif

    var body: some View {
        Group {
            if let account = app.account {
                signedIn(account)
            } else {
                SignInView(app: app)
            }
        }
        .task(id: app.account?.name) {
            await app.lobby.refreshArrangements()
        }
        // The server section is for choosing a machine; where there is none to
        // choose it has nothing to offer, and it goes.
        .onChange(of: app.offersRelayChoice) { if !app.offersRelayChoice, state.place == .servers { state.place = .home } }
        .onChange(of: state.place) { state.popovers.close() }
        // A newer build is said to somebody not signed in as well, at the
        // head of the sign-in under the deployment's notice (SignInView);
        // signed in, it is in the sidebar's foot (SidebarLobby), and on the
        // iPhone at the head of its page under the notice (PhoneLobby).
        // Over everything in the window, sidebar and stage alike.
        .signOutPrompt(app, at: .lobby)
    }

    @ViewBuilder
    private func signedIn(_ account: Account) -> some View {
        #if os(macOS)
        SidebarLobby(app: app, account: account, state: state)
            .sheet(item: $state.sheet) { mode in
                MeetingSheet(app: app, mode: mode)
            }
        #else
        // The inner display of an iPhone Duo on its side is laid out as its
        // outer display is, the phone's home with everything in the column
        // down its side (VerticalBar `sideways`), rather than as an iPad.
        let sideways = VerticalBarWindow.shared.sideways(regular: sizeClass == .regular,
                                                         rightToLeft: direction == .rightToLeft)
        let wide = sizeClass == .regular && !sideways
        ZStack {
            if wide {
                SidebarLobby(app: app, account: account, state: state)
                    .transition(.asymmetric(insertion: .opacity.animation(Theme.settle),
                                            removal: .opacity.animation(Theme.leave)))
            } else {
                PhoneLobby(app: app, account: account, state: state, sideways: sideways)
                    .transition(.asymmetric(insertion: .opacity.animation(Theme.settle),
                                            removal: .opacity.animation(Theme.leave)))
            }
        }
        .onChange(of: wide) { state.popovers.close() }
        .onChange(of: sideways) { state.popovers.close() }
        // Presented above both layouts, so that what they hold outlives a
        // change between them: presented from inside one, a sheet went with
        // it, and a Duo opened with the start sheet up lost the room typed
        // into it and the join under way.
        .fullScreenCover(item: $state.sheet) { mode in
            MeetingSheet(app: app, mode: mode)
        }
        // The settings and the schedule cover the whole of the inner display
        // on its side, as a sheet covers the outer display, with their close
        // in the column down its side: as a sheet there they stood in the
        // middle of the screen, short of the column, and kept their close on
        // themselves. Each is one or the other by the layout, and the binding
        // of the one not drawn neither shows it nor closes it, so that turning
        // the phone or opening it takes one down and puts the other up with
        // the settings or the schedule still open.
        //
        // Closed or not is judged by the layout as it is when the system
        // says one went (`layoutNow`), not as it was when the binding was
        // made: taken down by opening the phone, the outer display's
        // schedule sheet was said to go with the binding of the outer
        // display, upright and compact, and closed the schedule, and the
        // inner display opened on the lobby's home.
        .sheet(isPresented: Binding(get: { state.settings && !sideways },
                                    set: { shown in if !shown && !layoutNow().sideways { state.settings = false } })) {
            SettingsView(app: app)
        }
        .fullScreenCover(isPresented: Binding(get: { state.settings && sideways },
                                              set: { shown in if !shown && layoutNow().sideways { state.settings = false } })) {
            SettingsView(app: app)
                .presentationBackground(Theme.background)
        }
        // The phone's schedule is a sheet; the sidebar's is a section beside
        // it, with the same draft. Taken down by the layout changing, not by
        // somebody closing it, it leaves the section where it was: only the
        // phone's own binding closes the section, and only while the phone's
        // layout is drawn.
        .sheet(isPresented: Binding(get: { !wide && !sideways && state.place == .scheduled },
                                    set: { shown in
                                        let now = layoutNow()
                                        if !shown && !now.wide && !now.sideways && state.place == .scheduled { state.place = .home }
                                    })) {
            // Its close goes home, which takes the sheet down.
            ScheduleView(app: app, account: account, draft: state.schedule, close: { state.place = .home })
                .presentationDragIndicator(.visible)
                .presentationBackground(Theme.background)
        }
        .fullScreenCover(isPresented: Binding(get: { sideways && state.place == .scheduled },
                                              set: { shown in
                                                  if !shown && layoutNow().sideways && state.place == .scheduled { state.place = .home }
                                              })) {
            ScheduleView(app: app, account: account, draft: state.schedule, close: { state.place = .home })
                .presentationBackground(Theme.background)
        }
        #endif
    }

    #if os(iOS)
    /// The lobby's layout as the window has it now: the sidebar's (`wide`),
    /// the outer display's on the inner display on its side (`sideways`), or
    /// the phone's, neither. The window's own size class rather than the
    /// environment's, which a binding made in an earlier frame still holds.
    @MainActor
    private func layoutNow() -> (wide: Bool, sideways: Bool) {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        let window = windows.first(where: \.isKeyWindow) ?? windows.first
        let regular = window.map { $0.traitCollection.horizontalSizeClass == .regular } ?? (sizeClass == .regular)
        let sideways = VerticalBarWindow.shared.sideways(regular: regular, rightToLeft: direction == .rightToLeft)
        return (regular && !sideways, sideways)
    }
    #endif
}

// MARK: - The mark and small pieces

// The product's own mark is Mark, in TomoshibiBasics, where the Messages
// extension draws it too.

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

/// Something to tell the person, at the foot of the screen (Notices.tsx).
///
/// Two kinds, as the web has them. Something that happened and is over fades
/// on its own (`actionFailed`, ten seconds). Something about why they are not
/// in a meeting stays, with a way to close it, until they do (`joinFailed`):
/// an invitation that opens tomorrow said when and was gone in five seconds,
/// while the person was still reading it.
struct ToastModifier: ViewModifier {
    @Bindable var app: AppModel

    /// Above the phone's join bar; on the Mac nothing sits at the foot.
    private static var clearance: CGFloat {
        #if os(iOS)
        92
        #else
        28
        #endif
    }

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            ZStack {
                if let text = app.problem {
                    HStack(spacing: 10) {
                        Image(systemName: app.problemStays ? "exclamationmark.circle.fill" : "info.circle")
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(app.problemStays ? Theme.tallyInk : Theme.muted)
                        Text(text)
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.foreground)
                            .multilineTextAlignment(.leading)
                            .fixedSize(horizontal: false, vertical: true)
                        if app.problemStays {
                            Button { app.problem = nil } label: {
                                Image(systemName: "xmark").font(.system(size: 10, weight: .bold))
                            }
                            .buttonStyle(.glassCircle(24))
                            .accessibilityLabel(t("Close"))
                        }
                    }
                    .padding(.leading, 14).padding(.trailing, app.problemStays ? 7 : 16).padding(.vertical, 9)
                    .glassSurface(Capsule())
                    .frame(maxWidth: 560)
                    // On the page the reader is on where a book's fold would
                    // cut a notice centred on the window in two.
                    .keptToOnePage(.reading)
                    .padding(.horizontal, 20).padding(.bottom, Self.clearance)
                    .onTapGesture { if !app.problemStays { app.problem = nil } }
                    .task(id: text) {
                        guard !app.problemStays else { return }
                        try? await Task.sleep(for: .seconds(10))
                        if app.problem == text, !app.problemStays { app.problem = nil }
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

/// The camera, live, where somebody has it on; a flat ground otherwise.
///
/// Framed (a corner given), it is a card on a page, the lobby's or the start
/// sheet's, and with the camera off it is the surface colour (Theme.surface):
/// at night the grey of a camera-off tile in the call, which is what the
/// person will look like when they join (the call's own darkest ground
/// there vanished into the black page round the iPhone's lobby card), and by
/// day the white of the day's cards, since the page round it is light.
/// Filling the screen it is the screen's ground (Backdrop), so the glass card
/// over it stands out of it.
struct CameraStage: View {
    @Bindable var app: AppModel
    var cornerRadius: CGFloat = 0
    /// With the camera off, a pane of glass with people drawn on it instead
    /// of the grey (PreviewGlass): the phone lobby's card, and only that.
    var glassWhenOff = false
    /// The card is covered, and its drawing holds still.
    var glassHeld = false
    @State private var preview = CameraCapture()

    var body: some View {
        ZStack {
            if cornerRadius > 0 && glassWhenOff {
                // The glass and its people go as the camera comes on, and the
                // grey a picture stands on comes with it.
                if app.startWithCamera {
                    Theme.surface.transition(.opacity)
                } else {
                    PreviewGlass(cornerRadius: cornerRadius, held: glassHeld).transition(.opacity)
                }
            } else if cornerRadius > 0 {
                Theme.surface
            } else {
                Backdrop()
            }
            if app.startWithCamera {
                SelfView(session: preview.session, capture: preview)
                    .transition(.opacity.animation(Theme.settle))
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        .animation(Theme.settle, value: app.startWithCamera)
        .onChange(of: app.startWithCamera) { show(app.startWithCamera) }
        // Blur switched off or back on by the deployment while the preview
        // runs: the preview follows, as the next call will.
        .onChange(of: app.remote.allows(.backgroundBlur)) {
            preview.blurring = app.deviceMemory.blur && app.remote.allows(.backgroundBlur)
        }
        .onAppear { show(app.startWithCamera) }
        .onDisappear { preview.session.stopRunning() }
    }

    private func show(_ on: Bool) {
        let session = preview.session
        if on {
            // The camera the call will start on: the one last picked, where it
            // is still plugged in, and the system's otherwise (CallModel
            // `restoreDevices`). The web previews the remembered one for the
            // same reason (PreJoin.tsx ~373: "a preview of another camera
            // would be a rehearsal of the wrong picture"); this showed the
            // system's default whatever the call was about to send. Looked
            // up again each time the camera goes on, so a choice made in the
            // settings meanwhile is the one shown.
            // Blurred as the call will send it, when blur is chosen: the
            // preview takes its frames through the same blur (CameraCapture
            // `preview`). Not while the deployment has blur switched off,
            // when the call will not blur it either (CallModel `blurKept`).
            preview.blurring = app.deviceMemory.blur && app.remote.allows(.backgroundBlur)
            if let device = Self.joiningCamera(app.deviceMemory) {
                preview.preview(device: device)
            } else {
                DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
            }
        } else {
            DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
        }
    }

    /// The camera a join will start, by what is remembered and what is
    /// plugged in now; with nothing remembered, the one CameraCapture falls
    /// back on, which on a phone is the front camera; the system's default
    /// there is the back one, which is not the camera the call sends.
    static func joiningCamera(_ memory: DeviceMemory) -> AVCaptureDevice? {
        let cameras = CameraCapture.cameras()
        if let id = memory.restorable(.camera, present: cameras.map(\.uniqueID)),
           let remembered = cameras.first(where: { $0.uniqueID == id }) {
            return remembered
        }
        return CameraCapture.fallback()
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

/// What was typed, if it is a link: an invitation or a room's address pasted
/// where a room's name goes. The app opens it as it opens one handed over by
/// the system, and refuses it there if it is another server's.
func pastedLink(_ text: String) -> URL? {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let url = URL(string: trimmed), let scheme = url.scheme?.lowercased(),
          ["https", "http", "tomoshibi"].contains(scheme), url.host() != nil else { return nil }
    return url
}

/// The cross that takes a room off the recent list.
struct RecentForget: View {
    let room: String
    var size: CGFloat = 24
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: size * 0.36, weight: .bold))
                .foregroundStyle(Theme.muted)
                .frame(width: size, height: size)
                .background(Circle().fill(Theme.ink.opacity(0.07)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(t("Remove {room} from recent", ["room": room]))
        .help(t("Remove {room} from recent", ["room": room]))
    }
}

/// A recent room in the sidebar: the row opens it. On the Mac the cross that
/// takes it off the list shows under the pointer, as the sidebar's own rows
/// show what they offer; a finger has no pointer to rest, so on a touch
/// screen Remove is under a swipe to the left, as it is on the phone's list
/// of recent rooms.
struct RecentSidebarRow: View {
    let recent: LobbyModel.Recent
    let open: () -> Void
    let forget: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            Button(action: open) {
                HStack(spacing: 10) {
                    Initial(name: recent.room, size: 26)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(recent.room).font(.system(size: 13, weight: .medium)).lineLimit(1)
                        Text(recent.at.formatted(.relative(presentation: .named)))
                            .font(.system(size: 11)).foregroundStyle(Theme.muted)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            #if os(macOS)
            RecentForget(room: recent.room, size: 20, action: forget)
                .opacity(hovering ? 1 : 0)
                .allowsHitTesting(hovering)
            #endif
        }
        #if os(macOS)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        #else
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button(role: .destructive, action: forget) {
                Label(t("Remove"), systemImage: "trash")
            }
            .accessibilityLabel(t("Remove {room} from recent", ["room": recent.room]))
        }
        #endif
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
    @State private var popovers = Popovers()
    /// The language and the paste control stand in the system's column down
    /// the side of an iPhone Duo's outer display (SystemColumn), and are
    /// taken out of the corner and the foot while they do.
    @State private var inColumn = false
    #if os(iOS)
    @Environment(\.colorScheme) private var scheme

    /// The paste control's colour. The system writes its words in white
    /// whatever colour it is given, which at night on the raised grey is a
    /// quiet control; on the day's raised grey the white could not be read
    /// (1.2 to 1), so by day it is a deeper grey, which the white reads on at
    /// 6 to 1.
    private var pasteTint: Color {
        scheme == .dark ? Theme.surfaceHigh : Color(red: 99 / 255, green: 99 / 255, blue: 102 / 255)
    }

    /// A phone on its side, and the Duo's outer display on its side or as a
    /// tent: the heading, the card and the paste control stacked were taller
    /// than the screen, and the paste control at its foot sat over the
    /// passphrase and Sign in. The heading stands beside the card there, with
    /// the paste control under the words that mention the link. One layout
    /// either way, so turning the phone keeps the field being typed into.
    @Environment(\.verticalSizeClass) private var heightClass
    private var short: Bool { heightClass == .compact }
    #else
    private let short = false
    #endif

    /// Whether the deployment's notice or a newer build is said above the
    /// mark, which then comes up closer under them.
    private var headSays: Bool {
        app.remote.notice != nil || app.updates?.shown == true
    }

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                let layout = short ? AnyLayout(HStackLayout(alignment: .center, spacing: 32))
                                   : AnyLayout(VStackLayout(spacing: 22))
                VStack(spacing: 0) {
                    // The deployment's notice, which is for whoever has the
                    // app, signed in or not (as the deployment's own
                    // management page says), and under it a newer build: in
                    // the page's flow, clear of the language menu in the
                    // corner, and never one over the other, as wide as each
                    // other. The build was laid over the top of the sign-in,
                    // and a notice there would have lain under it.
                    LobbyNoticeCard(remote: app.remote, top: 56, sides: 16, bottom: 0, maxWidth: 520)
                    if let updates = app.updates {
                        UpdateBanner(updates: updates, top: app.remote.notice == nil ? 56 : 10)
                    }
                    layout {
                        VStack(spacing: 12) {
                            Mark(size: 76)
                                .shadow(color: Theme.shadow(0.45), radius: 18, y: 8)
                            Text(t("Sign in")).font(.system(size: 28, weight: .bold))
                            Text(t("Meetings here are started by the people who belong here. If somebody sent you a link, open that instead."))
                                .font(.system(size: 14)).foregroundStyle(Theme.muted)
                                .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
                            #if os(iOS)
                            if short && !inColumn { pasteLink.padding(.top, 6).transition(.opacity) }
                            #endif
                        }
                        .frame(maxWidth: short ? 280 : nil)
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
                                .background(Capsule().fill(Theme.sunk(0.22)))
                                .overlay(Capsule().stroke(Theme.ink.opacity(0.12)))
                                Button(t("Join"), action: openLink)
                                    .buttonStyle(.glassCapsule(38))
                                    .fixedSize()
                                    .disabled(URL(string: link.trimmingCharacters(in: .whitespaces))?.host() == nil)
                            }
                            #endif
                        }
                        .padding(18)
                        .glassPane(cornerRadius: 30)
                        .frame(maxWidth: short ? 400 : nil)
                        .rise(0.06)
                    }
                    .frame(maxWidth: short ? 720 : 420)
                    .padding(.horizontal, 16)
                    .padding(.top, headSays ? 32 : 72)
                    .padding(.bottom, short ? 24 : 140)
                    .frame(maxWidth: .infinity)
                    .animation(Theme.settle, value: short)
                }
                .animation(Theme.settle, value: app.remote.notice?.id)
                .animation(Theme.settle, value: app.updates?.shown)
                .putsKeyboardAway()
            }
            .scrollIndicators(.never)
            .putsKeyboardAway()
            // A card centred on a Duo held half open sat across its fold;
            // it keeps to the page it is read on, or above a fold across it,
            // where the keyboard does not come up over the fields.
            .keptToOneSide(.reading)
        }
        #if os(iOS)
        .safeAreaInset(edge: .bottom) {
            if !short && !inColumn {
                pasteLink
                    .keptToOnePage(.reading)
                    .padding(.bottom, 8)
                    .transition(.opacity)
            }
        }
        #endif
        .overlay(alignment: .topTrailing) {
            if !inColumn {
                LanguageMenu().padding(16).transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: inColumn)
        // The language under the clock, where the system stands a screen's
        // own items, and the paste control at the column's foot, where the
        // thumb is; the words that mention a link stay on the card.
        .systemColumn($inColumn, least: 160) {
            LanguageMenu(size: 48, glyph: 18)
        } foot: {
            #if os(iOS)
            columnPaste
            #endif
        }
        .toast(app)
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
    }

    #if os(iOS)
    /// Opening the link you were sent, without typing it.
    private var pasteLink: some View {
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
            .tint(pasteTint)
        }
    }

    /// The same control in the system's column: the system's own paste
    /// button, its symbol alone on a circle, since a column 84 points wide
    /// has no room for the word beside it.
    private var columnPaste: some View {
        PasteButton(payloadType: String.self) { strings in
            guard let text = strings.first?.trimmingCharacters(in: .whitespacesAndNewlines),
                  let url = URL(string: text), url.host() != nil else { return }
            Task { await app.open(url) }
        }
        .labelStyle(.iconOnly)
        .buttonBorderShape(.circle)
        .controlSize(.large)
        .tint(pasteTint)
    }
    #endif

    /// One box holding two fields, as the web holds them: they are one answer.
    private var credentials: some View {
        VStack(spacing: 0) {
            row(symbol: "person", lit: false) {
                TextField("", text: $name, prompt: Text(t("Name")).foregroundStyle(Theme.muted))
                    .textContentType(.username)
            }
            Rectangle().fill(Theme.ink.opacity(0.07)).frame(height: 1)
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
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.sunk(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Theme.ink.opacity(0.10)))
    }

    private func row<Field: View>(symbol: String, lit: Bool, @ViewBuilder field: () -> Field) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 15))
                .foregroundStyle(lit ? Theme.tallyInk : Theme.muted)
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

/// The language, before a call as on every web page; following the system
/// until somebody chooses. A popover of the screen's (`floatingPopoverLayer`).
struct LanguageMenu: View {
    /// 36 points in a screen's corner, 48 under the clock in the system's
    /// column, the size of the system's own items there.
    var size: CGFloat = 36
    var glyph: CGFloat = 14
    @Environment(\.popovers) private var popovers
    @State private var localization = Localization.shared

    var body: some View {
        if let popovers {
            Button { popovers.toggle("language") } label: {
                Image(systemName: "globe").font(.system(size: glyph, weight: .medium))
            }
            .buttonStyle(.glassCircle(size))
            .accessibilityLabel(t("Language"))
            .help(t("Language"))
            .floatingPopover("language", in: popovers, width: 230) {
                MenuCard {
                    MenuItem(title: t("Same as the system"), symbol: "globe",
                             detail: Language.detect(from: Locale.preferredLanguages).name,
                             checked: localization.followsSystem) {
                        localization.followSystem()
                        popovers.close()
                    }
                    MenuRule()
                    ForEach(Language.allCases) { one in
                        MenuItem(title: one.name, checked: !localization.followsSystem && localization.language == one) {
                            localization.language = one
                            popovers.close()
                        }
                    }
                }
            }
        }
    }
}

// MARK: - A window with a sidebar

/// A window with a sidebar, as the Mac's own apps are: what to do and what is
/// coming in the sidebar, and the picture of yourself filling the rest. The
/// Mac's lobby, and the lobby wherever a phone or a tablet gives the app a
/// regular width (an iPhone Duo opened out, an iPad). The system's split view
/// rather than one of the app's own: it collapses, tiles and keeps its
/// columns clear of a Duo's fold by itself, evening them either side of the
/// hinge of a phone held half open as a book, which a sidebar laid out by
/// hand would have to be taught.
struct SidebarLobby: View {
    @Bindable var app: AppModel
    let account: Account
    @Bindable var state: LobbyState
    /// Both columns, always: the sidebar is where everything is started, and
    /// a phone's split view folded it away behind a button whenever the
    /// window was narrower than an iPad's, under a greeting that pointed at
    /// it.
    @State private var columns = NavigationSplitViewVisibility.all

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            LobbySidebar(app: app, account: account, state: state)
                .navigationSplitViewColumnWidth(min: 240, ideal: 284, max: 340)
                .floatingPopoverLayer(state.popovers)
                #if os(iOS)
                // The sidebar's own head is the two things to do; a bar over
                // it would hold nothing but the button that hides it.
                .toolbar(removing: .sidebarToggle)
                .toolbar(.hidden, for: .navigationBar)
                #endif
        } detail: {
            Group {
                switch state.place {
                case .home: HomeStage(app: app, account: account)
                // Each on the page of a book the sidebar leaves, as the
                // greeting is: across the fold, the planning form's fields
                // lay half on either page.
                case .scheduled: ScheduleView(app: app, account: account, draft: state.schedule, close: { state.place = .home })
                    .keptToOnePage(.reading)
                case .servers: ServersView(app: app, ownColumn: true).keptToOnePage(.reading)
                }
            }
            // One identity for each section: the same Group switching what
            // is in it is one view changing, and the transition below never
            // played, so a section arrived as a cut.
            .id(state.place)
            .contentLayer()
            .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 10)).animation(Theme.settle),
                                    removal: .opacity.animation(Theme.leave)))
            .animation(Theme.settle, value: state.place)
            .floatingPopoverLayer(state.popovers)
            // The deployment's notice at the top of the main area, whichever
            // section it is on, with what is under it moved down: the
            // picture of yourself still fills the stage behind it, and the
            // greeting and the server menu start below it.
            .safeAreaInset(edge: .top, spacing: 0) {
                LobbyNoticeCard(remote: app.remote).keptToOnePage(.reading)
            }
            .animation(Theme.settle, value: app.remote.notice?.id)
        }
        #if os(iOS)
        .navigationSplitViewStyle(.balanced)
        .onChange(of: columns) { if columns != .all { columns = .all } }
        #endif
        .toast(app)
        .onAppear { state.popovers.openFromDebug() }
    }
}

private struct LobbySidebar: View {
    @Bindable var app: AppModel
    let account: Account
    @Bindable var state: LobbyState

    /// The list's selection is optional; a press on nothing leaves the
    /// section where it was rather than emptying the window.
    private var selection: Binding<LobbyState.Place?> {
        Binding(get: { state.place }, set: { if let place = $0 { state.place = place } })
    }

    var body: some View {
        List(selection: selection) {
            Section {
                item(t("Home"), "house").tag(LobbyState.Place.home)
                item(t("Scheduled meetings"), "calendar")
                    .badge(app.lobby.upcoming.count)
                    .tag(LobbyState.Place.scheduled)
                if app.offersRelayChoice {
                    item(t("Server"), "server.rack").tag(LobbyState.Place.servers)
                }
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
                        RecentSidebarRow(recent: recent, open: { state.sheet = .again(recent.room) },
                                         forget: { withAnimation(Theme.settle) { app.lobby.forget(recent) } })
                            .transition(Theme.arrive)
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .tint(Theme.tally)
        .safeAreaInset(edge: .top) {
            HStack(spacing: 8) {
                Button { state.sheet = .start } label: {
                    Label(t("Start"), systemImage: "video.fill").frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(platformTall, prominent: true))
                .help(t("Start a meeting"))
                Button { state.sheet = .join("") } label: {
                    Label(t("Join"), systemImage: "link").frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(platformTall))
                .help(t("Join a meeting"))
            }
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, 12).padding(.top, 6).padding(.bottom, 4)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                // A newer build, in the foot above the account, where it
                // covers nothing: over the stage it lay across the greeting.
                if let updates = app.updates { UpdateBanner(updates: updates, inSidebar: true) }
                FleetLine(app: app).padding(.horizontal, 6)
                AccountMenu(app: app, account: account, state: state)
            }
            .padding(10)
        }
    }

    /// A fingertip's height on a touch screen, a pointer's on the Mac.
    private var platformTall: CGFloat {
        #if os(iOS)
        44
        #else
        36
        #endif
    }
}

extension LobbySidebar {
    /// A sidebar entry with its symbol drawn in the person's colour, as the
    /// system's own sidebars draw theirs.
    func item(_ title: String, _ symbol: String) -> some View {
        Label {
            Text(title)
        } icon: {
            Image(systemName: symbol).foregroundStyle(Theme.tallyInk)
        }
    }
}

/// The account, the server and everything that is not a meeting, as the
/// app's own popover over the sidebar's foot.
struct AccountMenu: View {
    @Bindable var app: AppModel
    let account: Account
    let state: LobbyState
    #if os(macOS)
    @Environment(\.openSettings) private var openSettings
    #endif
    @Environment(\.popovers) private var popovers

    var body: some View {
        if let popovers {
            Button { popovers.toggle("account") } label: {
                HStack(spacing: 10) {
                    AccountAvatar(name: account.name, address: app.avatarAddress, size: 30)
                    Text(account.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.up.chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .padding(.horizontal, 10).padding(.vertical, 8)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .glassSurface(RoundedRectangle(cornerRadius: 16, style: .continuous))
            .accessibilityLabel(t("Settings"))
            .floatingPopover("account", in: popovers, edge: .top, alignment: .leading, width: 262) {
                // No pages of the web's here, the user console and the
                // administrators' among them: an app that sends people out to
                // a website for its own settings is one App Review questions,
                // and the account's own settings are in Settings.
                MenuCard {
                    MenuItem(title: t("Settings"), symbol: "gearshape") {
                        popovers.close()
                        #if os(macOS)
                        openSettings()
                        #else
                        state.settings = true
                        #endif
                    }
                    MenuRule()
                    MenuItem(title: t("Sign out"), symbol: "rectangle.portrait.and.arrow.right", role: .destructive) {
                        popovers.close()
                        withAnimation(Theme.settle) { app.signOutAsked = .lobby }
                    }
                }
            }
        }
    }
}

/// Home: the picture of yourself, a greeting, and the switches for how you
/// will arrive.
private struct HomeStage: View {
    @Bindable var app: AppModel
    let account: Account
    /// The microphone and the camera, and the server, stand in a column the
    /// system keeps down the side of the stage (SystemColumn), and leave the
    /// foot and the corner of the stage while they do. Only the stage beside
    /// the sidebar: Apple keeps a split view's sidebar under bars across its
    /// top, and its Start and Join stay at its head. The inner display of an
    /// iPhone Duo on its side, where this stood its controls first, has no
    /// sidebar now (PhoneLobby `sideways`).
    @State private var inColumn = false

    var body: some View {
        ZStack(alignment: .topLeading) {
            CameraStage(app: app)
                .ignoresSafeArea()
                .backgroundExtended()
            VStack(alignment: .leading, spacing: 6) {
                Text(t("Hello, {name}", ["name": account.name])).font(.system(size: 34, weight: .bold))
                Text(t("Start one from the sidebar when you are ready.")).font(.system(size: 15)).foregroundStyle(Theme.ink.opacity(0.78))
            }
            .shadow(color: Theme.shadow(0.45), radius: 16, y: 2)
            .padding(.leading, 36).padding(.top, 56)
            .frame(maxWidth: .infinity, alignment: .leading)
            // On the page beside the sidebar's of a Duo held as a book: the
            // greeting started on the sidebar's page and ran over the fold.
            // The picture behind stays under both pages.
            .keptToOnePage(.reading)
            .rise()
        }
        .overlay(alignment: .bottom) {
            if !inColumn {
                DeviceSwitches(app: app)
                    .padding(8)
                    .glassSurface(Capsule())
                    .keptToOnePage(.reading)
                    .padding(.bottom, 28)
                    .rise(0.08)
                    .transition(Theme.arrive)
            }
        }
        // In the stage's corner rather than the toolbar: a toolbar item is
        // drawn apart from the window's content, and the app's popover could
        // not reach out of it. Only where there is a machine to choose.
        .overlay(alignment: .topTrailing) {
            if app.offersRelayChoice && !inColumn {
                ServerMenu(app: app)
                    .padding(.top, 12).padding(.trailing, 16)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: app.offersRelayChoice)
        .animation(Theme.settle, value: inColumn)
        #if os(iOS)
        // The server under the column's head, where the system stands a
        // page's own items, and the microphone and the camera at its foot,
        // as the outer display's lobby has them.
        .systemColumn($inColumn, least: 200) {
            ZStack {
                if app.offersRelayChoice {
                    ServerMenu(app: app, inColumn: true).transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: app.offersRelayChoice)
        } foot: {
            ColumnCapsule { ColumnDeviceSwitches(app: app) }
        }
        #endif
    }
}

extension View {
    /// Content that carries on under a floating sidebar, mirrored and blurred,
    /// where the system can (macOS 26, iOS 26); nothing before it.
    @ViewBuilder
    func backgroundExtended() -> some View {
        if #available(macOS 26.0, iOS 26.0, *), !GlassSupport.forcedFallback {
            backgroundExtensionEffect()
        } else {
            self
        }
    }
}

// MARK: - The phone

#if os(iOS)
/// A large-titled home: the picture of yourself in a card with the two things
/// to do on it, what is coming, the server, and a field at the foot to join
/// by the name you were given.
struct PhoneLobby: View {
    @Bindable var app: AppModel
    let account: Account
    /// The sheets, the server page and the join bar's words, which outlive
    /// this layout (LobbyState).
    @Bindable var state: LobbyState
    /// The inner display of an iPhone Duo on its side (VerticalBar
    /// `sideways`), laid out as the outer display is: the preview beside the
    /// lists, as on a short screen, and the column down its side holding the
    /// account and the server at its head as well as everything at its foot.
    /// The system's bars are not down its side on the simulator's stand-in
    /// for the inner display, which says nothing of them, so the column's
    /// head is the app's own there rather than the navigation bar's.
    var sideways = false
    /// The height of a recent room's row, which the list's own height is
    /// counted in: the list does not scroll, so it is as tall as its rows.
    private static let recentRow: CGFloat = 58
    /// The lists beside the preview on the inner display on its side: as
    /// wide as they are on the outer display upright, beside its column, so
    /// that a row reads the same on either, and the preview has the rest.
    private static let sideLists: CGFloat = 350

    /// The fold down the inner display on its side held half open as a
    /// book, across the preview and the lists: the preview keeps to the page
    /// before it and the lists to the page after it, eight points clear of
    /// it either side. Laid out as on a flat screen, the preview ran a dozen
    /// points over the fold. Its band and the width it was measured across,
    /// in the coordinates of the two together.
    struct BookFold: Equatable {
        var crease: CGRect
        var width: CGFloat
    }
    @State private var book: BookFold?

    /// The lists' width beside the preview on the inner display on its side:
    /// the page after the fold of a book, or the outer display's width.
    private var sideListsWidth: CGFloat {
        guard let book else { return Self.sideLists }
        return max(240, book.width - book.crease.maxX - 8)
    }

    /// The server page, pushed while the lobby is at the servers. Popped by
    /// the system (Back, the swipe from the edge) only while this layout is
    /// drawn: taken down by the size class changing, the navigation stack may
    /// say the page went, and the sidebar that replaces it should still be
    /// on the servers.
    @Environment(\.horizontalSizeClass) private var sizeClass
    private var serversShown: Binding<Bool> {
        let drawn = sizeClass != .regular || sideways
        return Binding(get: { state.place == .servers },
                       set: { shown in if !shown && drawn && state.place == .servers { state.place = .home } })
    }

    /// A phone on its side, and the iPhone Duo's outer display on its side or
    /// set down as a tent: too short for the preview above the lists, which
    /// left Start under the join bar and the lists out of sight below it. The
    /// preview stands beside them instead, as tall as the screen allows.
    /// Turning the phone draws the preview again, and its camera starts
    /// again; the words in the join bar are the lobby's (LobbyState).
    @Environment(\.verticalSizeClass) private var heightClass
    private var short: Bool { heightClass == .compact }
    /// The preview beside the lists rather than above them.
    private var beside: Bool { short || sideways }
    /// The column's head is the app's: the account and the server, or the
    /// way back from the server page, which the system stands under the
    /// clock of the outer display as items of its navigation bar.
    private var ownHead: Bool { sideways }
    /// The controls are the column's rather than the page's: where the
    /// column has been found, and on the inner display on its side from the
    /// first frame, where it always is. A lobby drawn afresh there, the
    /// inner display turned from upright, took a frame to find its column,
    /// and its preview's Start and the join bar showed through the fade
    /// before going.
    private var columned: Bool { inColumn || sideways }
    private var listed: Bool {
        !app.lobby.upcoming.isEmpty || !app.lobby.recentHere.isEmpty || app.offersRelayChoice
    }
    /// The deployment's notice or a newer build is being said (`head`).
    private var saying: Bool {
        app.remote.notice != nil || app.updates?.shown == true
    }
    /// The lobby's controls stand in the system's column down the side of an
    /// iPhone Duo's outer display (SystemColumn): the microphone and the
    /// camera, arranging, joining and starting at its foot, the account under
    /// the clock. The preview's buttons and the join bar leave the page while
    /// they do, which then holds the preview and the lists alone; the join
    /// bar's field could not stand in a column 84 points wide, and Join there
    /// opens the join sheet, whose field takes the keyboard, as the sidebar's
    /// Join does.
    @State private var inColumn = false
    /// The column the lobby keeps for itself on the inner display on its
    /// side, which its pages are laid out beside: the navigation stack lays
    /// them out by the window's safe area, not by a column added to its own.
    @State private var keptColumn = KeptColumn()

    var body: some View {
        NavigationStack {
            ZStack {
                if beside {
                    // At the head of the column beside the preview, above the
                    // lists and scrolling with them: above the preview and
                    // the column both, the two cards took the height the
                    // preview stands in, and its Start was drawn over the
                    // update.
                    HStack(alignment: .top, spacing: sideways ? book.map { $0.crease.width + 16 } ?? 16 : 16) {
                        hero(height: nil).rise()
                        if listed || saying {
                            ScrollView {
                                VStack(spacing: 0) {
                                    head(gap: 16, sides: 0)
                                    VStack(alignment: .leading, spacing: 22) { lists }
                                        .padding(.bottom, 24)
                                        .animation(Theme.settle, value: app.offersRelayChoice)
                                }
                                .animation(Theme.settle, value: app.remote.notice?.id)
                                .animation(Theme.settle, value: app.updates?.shown)
                            }
                            .scrollIndicators(.never)
                            // Half the width each on a phone on its side; on
                            // the inner display, half of 870 points would
                            // stretch a row of the lists to the width of the
                            // preview.
                            .frame(maxWidth: sideways ? sideListsWidth : .infinity)
                            .transition(Theme.arrive)
                        }
                    }
                    .onGeometryChange(for: BookFold?.self) { proxy in
                        MainActor.assumeIsolated {
                            let fold = Fold.of(proxy)
                            guard fold.posture(in: proxy.size) == .book, let crease = fold.crease else { return nil }
                            return BookFold(crease: crease, width: proxy.size.width)
                        }
                    } action: { book = $0 }
                    .animation(Theme.settle, value: book)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 10)
                    .animation(Theme.settle, value: listed || saying)
                    .transition(.opacity)
                } else {
                    ScrollView {
                        VStack(spacing: 0) {
                            // Under the title, at the head of the page, so
                            // that it scrolls with the page rather than lying
                            // over the preview.
                            head(gap: 22, sides: 16)
                            VStack(alignment: .leading, spacing: 22) {
                                hero(height: 292).rise()
                                lists
                            }
                            .padding(.horizontal, 16)
                            .padding(.bottom, 24)
                            .animation(Theme.settle, value: app.offersRelayChoice)
                        }
                        // The page moving down for the notice or an update
                        // and back up, with them rather than ahead of them.
                        .animation(Theme.settle, value: app.remote.notice?.id)
                        .animation(Theme.settle, value: app.updates?.shown)
                        .putsKeyboardAway()
                    }
                    .scrollIndicators(.never)
                    .transition(.opacity)
                }
            }
            .animation(Theme.settle, value: beside)
            .besideKeptColumn(keptColumn)
            .putsKeyboardAway()
            // The page is content; the join bar below floats over it as a
            // toolbar does, and stays glass.
            .contentLayer()
            .background(Theme.page.ignoresSafeArea())
            .navigationTitle("Tomoshibi")
            .toolbar {
                if !ownHead {
                    AccountItem(name: account.name, address: app.avatarAddress) { state.settings = true }
                }
            }
            .safeAreaInset(edge: .bottom) {
                if !columned { joinBar.transition(Theme.arrive) }
            }
            .navigationDestination(isPresented: serversShown) {
                ServersView(app: app, measureInColumn: columned)
                    .modifier(OnePageWhenSideways(sideways: sideways))
                    .besideKeptColumn(keptColumn)
                    // Its way back is the column's (`columnHead`).
                    .navigationBarBackButtonHidden(ownHead)
            }
        }
        .animation(Theme.settle, value: columned)
        // On the stack rather than on its home page, so that the column
        // changes with the page: the server page's own control takes the
        // place of the lobby's as the page is pushed, and the lobby's come
        // back with the swipe that pops it. The head is left to the system
        // on the outer display, which stands the account there, and the
        // back button in its place on the server page.
        // The account and the server at the head as well, on the inner
        // display on its side.
        .systemColumn($inColumn, least: sideways ? 420 : 300, kept: $keptColumn) {
            columnHead
        } foot: {
            ZStack(alignment: .bottom) {
                if state.place == .servers {
                    ColumnCapsule {
                        BarButton(symbol: "arrow.clockwise", label: t("Measure again")) {
                            Task { await app.lobby.measure() }
                        }
                        .disabled(app.lobby.measuring)
                    }
                    .transition(Theme.arrive)
                } else {
                    ColumnCapsule {
                        ColumnDeviceSwitches(app: app)
                        ColumnRule()
                        BarButton(symbol: "calendar", label: t("Schedule a meeting")) { state.place = .scheduled }
                        BarButton(symbol: "link", label: t("Join a meeting")) { state.sheet = .join("") }
                        // A plus rather than the camera the button starts
                        // a call with: just under the camera switch, the
                        // owner took the camera's picture on it for another
                        // camera switch (2026-10-26).
                        BarButton(symbol: "plus", label: t("Start a meeting"), lit: true) { state.sheet = .start }
                    }
                    .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: state.place == .servers)
        }
        .toast(app)
        .floatingPopoverLayer(state.popovers)
        .onAppear { state.popovers.openFromDebug() }
    }

    /// The head of the column on the inner display on its side, where it is
    /// the app's (`ownHead`): the account, which opens the settings as the
    /// outer display's does, with the server under it, or on the server page
    /// the way back. The inner display's column is taller than the outer
    /// display's, and has room for the server as well as the lists' row for
    /// it; the server opens the same menu as the start sheet's. Nothing on
    /// the outer display, whose head the system keeps.
    @ViewBuilder
    private var columnHead: some View {
        if sideways {
            ZStack(alignment: .top) {
                if state.place == .servers {
                    ColumnHeadButton(symbol: "chevron.left", label: t("Back")) { state.place = .home }
                        .transition(Theme.arrive)
                } else {
                    VStack(spacing: 12) {
                        Button { state.settings = true } label: {
                            AccountAvatar(name: account.name, address: app.avatarAddress, size: 30)
                        }
                        .buttonStyle(.glassCircle(48))
                        .accessibilityLabel(t("Settings"))
                        .help(t("Settings"))
                        if app.offersRelayChoice {
                            ServerMenu(app: app, inColumn: true).transition(Theme.arrive)
                        }
                    }
                    .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: state.place == .servers)
            .animation(Theme.settle, value: app.offersRelayChoice)
        }
    }

    /// What the lobby has to say at the head of its page, under the title:
    /// the deployment's notice, then a newer build. Both in the page's flow,
    /// one under the other and everything else under them, so that neither
    /// lies over the other or over the preview; whichever comes or goes, the
    /// other and the page move with it. The update was laid over the page
    /// under the title, and with a notice there it covered the notice.
    /// `gap` is what each leaves below itself, only while it is there, and
    /// `sides` its margins, none in a column that has its own.
    @ViewBuilder
    private func head(gap: CGFloat, sides: CGFloat) -> some View {
        LobbyNoticeCard(remote: app.remote, top: 0, sides: sides, bottom: gap, maxWidth: .infinity)
        if let updates = app.updates {
            UpdateBanner(updates: updates, top: 0, bottom: gap, maxWidth: .infinity, sides: sides)
        }
    }

    /// The meetings to come, the rooms been in and the server: under the
    /// preview on a tall screen, beside it on a short one.
    @ViewBuilder
    private var lists: some View {
        if !app.lobby.upcoming.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(t("Upcoming")).font(.system(size: 20, weight: .bold))
                    Spacer()
                    Button(t("All of them")) { state.place = .scheduled }.foregroundStyle(Theme.tallyInk)
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
                // In a List of its own, which does not scroll, for the
                // system's swipe to the left: Remove is out of sight
                // until the row is swiped. Made by hand on a plain
                // stack first, the row moved and its touch did not,
                // and pressing Remove opened the room instead.
                let shown = Array(app.lobby.recentHere.prefix(4))
                List {
                    ForEach(shown) { recent in
                        Button { state.sheet = .again(recent.room) } label: {
                            HStack(spacing: 12) {
                                Initial(name: recent.room, size: 36)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(recent.room).font(.system(size: 16, weight: .semibold))
                                    Text(recent.at.formatted(.relative(presentation: .named))).font(.system(size: 13)).foregroundStyle(Theme.muted)
                                }
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted.opacity(0.6))
                            }
                            .padding(.horizontal, 14)
                            .frame(height: Self.recentRow)
                            .contentShape(Rectangle())
                            .overlay(alignment: .bottom) {
                                if recent.id != shown.last?.id {
                                    Divider().padding(.leading, 62)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Theme.surface)
                        .listRowSeparator(.hidden)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                withAnimation(Theme.settle) { app.lobby.forget(recent) }
                            } label: {
                                Label(t("Remove"), systemImage: "trash")
                            }
                            .accessibilityLabel(t("Remove {room} from recent", ["room": recent.room]))
                        }
                    }
                }
                .listStyle(.plain)
                .scrollDisabled(true)
                .scrollContentBackground(.hidden)
                .environment(\.defaultMinListRowHeight, Self.recentRow)
                .frame(height: Self.recentRow * CGFloat(shown.count))
                .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
            }
            .rise(0.1)
        }
        if app.offersRelayChoice {
            Button { state.place = .servers } label: {
                HStack(spacing: 12) {
                    Image(systemName: "server.rack").font(.system(size: 16)).foregroundStyle(Theme.muted)
                        .frame(width: 36, height: 36)
                        .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Theme.ink.opacity(0.08)))
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
            .transition(Theme.arrive)
        }
    }

    /// The camera's preview with Start at its foot: 292 points tall above the
    /// lists, or as tall as the space beside them.
    private func hero(height: CGFloat?) -> some View {
        ZStack {
            CameraStage(app: app, cornerRadius: 30, glassWhenOff: true,
                        glassHeld: state.sheet != nil || state.settings || state.place == .scheduled)
        }
        .frame(height: height)
        .frame(maxWidth: .infinity, maxHeight: height == nil ? .infinity : nil)
        .overlay(alignment: .topTrailing) {
            if !columned {
                DeviceSwitches(app: app, size: 40).padding(12).transition(Theme.arrive)
            }
        }
        .overlay(alignment: .bottom) {
            if !columned {
                GlassGroup(spacing: 8) {
                    HStack(spacing: 8) {
                        Button { state.sheet = .start } label: {
                            Label(t("Start a meeting"), systemImage: "video.fill").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.glassCapsule(52, prominent: true))
                        Button { state.place = .scheduled } label: {
                            Label(t("Schedule a meeting"), systemImage: "calendar").labelStyle(.iconOnly)
                        }
                        .buttonStyle(.glassCircle(52))
                        .accessibilityLabel(t("Schedule a meeting"))
                    }
                }
                .font(.system(size: 16, weight: .semibold))
                .padding(12)
                .transition(Theme.arrive)
            }
        }
        // The card is a surface of its own, for the buttons on it to be
        // told apart from it by day (Theme.lifted).
        .lifted()
    }

    /// Joining by the name you were given, from the foot of the screen where a
    /// thumb already is.
    private var joinBar: some View {
        HStack(alignment: .bottom, spacing: 10) {
            ScopedRoomField(prompt: t("Room name or invite link"), text: $state.typed, scopes: account.scopes, admin: account.admin,
                            capsule: true, onSubmit: joinTyped)
            Button(action: joinTyped) {
                Image(systemName: "arrow.right").font(.system(size: 20, weight: .bold))
            }
            .buttonStyle(.glassCircle(54, prominent: true))
            .disabled(!validRoomName(name: normaliseRoomName(typed: state.typed)) && pastedLink(state.typed) == nil)
            .accessibilityLabel(t("Join"))
        }
        .padding(.horizontal, 16).padding(.bottom, 6)
    }

    private func joinTyped() {
        // An invitation pasted in is opened as the link it is.
        if let url = pastedLink(state.typed) {
            state.typed = ""
            putKeyboardAway()
            Task { await app.open(url) }
            return
        }
        let room = normaliseRoomName(typed: state.typed)
        guard validRoomName(name: room) else { return }
        state.sheet = .join(room)
        state.typed = ""
    }
}

/// The server page over the whole of the inner display on its side, on the
/// page of a book after its fold when it is held half open, as the server
/// section beside the sidebar was: one list of rows down the middle of the
/// screen was bent down the middle by the fold. Nothing anywhere else, the
/// outer display's page among them.
private struct OnePageWhenSideways: ViewModifier {
    let sideways: Bool

    func body(content: Content) -> some View {
        if sideways {
            // The page's ground under both pages, not moved with the list.
            content.keptToOnePage(.reading).background(Backdrop())
        } else {
            content
        }
    }
}

/// The phone lobby's account button, in its navigation bar. The system
/// judges an item whose label is a picture of the app's own (the avatar) to
/// fit only across the top, and on an iPhone Duo's outer display it was left
/// at the head of the page beside the title while the column beside it stood
/// empty under the clock. Marked as preferring the system's column
/// (`axisBehavior`, iOS 27.1) it stands there under the clock, as the server
/// page's back button does; where the system lays no bar down the side the
/// mark changes nothing.
private struct AccountItem: ToolbarContent {
    let name: String
    let address: URL?
    let open: () -> Void

    var body: some ToolbarContent {
        if #available(iOS 27.1, *) {
            item.axisBehavior(.verticalPreferred)
        } else {
            item
        }
    }

    private var item: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button(action: open) { AccountAvatar(name: name, address: address, size: 30) }
                .accessibilityLabel(t("Settings"))
        }
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
        /// A room from the recent list: joined if a meeting is live in it,
        /// started again under the same name if not. Opened as a join, it was
        /// refused whenever the meeting had ended ("No meeting is happening
        /// under that name"), which for a room somebody has been in before is
        /// most of the time.
        case again(String)

        var id: String {
            switch self {
            case .start: "start"
            case let .join(room): "join:" + room
            case let .again(room): "again:" + room
            }
        }
    }

    @Bindable var app: AppModel
    @State var mode: Mode
    @State private var room = ""
    @State private var sealing = false
    @State private var working = false
    /// The press of Start or Join, while it is asking whether the meeting is
    /// live and then joining: Cancel calls it off.
    @State private var going: Task<Void, Never>?
    @State private var popovers = Popovers()
    @Environment(\.dismiss) private var dismiss
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.verticalSizeClass) private var heightClass
    /// A wide screen, or a short one: a phone on its side, and the Duo's
    /// outer display on its side or as a tent, where across the foot the
    /// card was 680 points long and as tall as the screen.
    private var wide: Bool { sizeClass == .regular || heightClass == .compact }
    #endif
    /// The close and the microphone and camera stand in the system's column
    /// down the side of an iPhone Duo's outer display (SystemColumn), and
    /// leave the card while they do, which keeps the choice, the form and
    /// Start: a field and the button that answers it are the card's.
    @State private var inColumn = false

    init(app: AppModel, mode: Mode) {
        self.app = app
        _mode = State(initialValue: mode)
        if case let .join(name) = mode { _room = State(initialValue: name) }
        if case let .again(name) = mode { _room = State(initialValue: name) }
        if let typed = LobbyDebug.room { _room = State(initialValue: typed) }
        // A word already typed for this sitting is shown, and the switch on.
        _sealing = State(initialValue: !app.sealingWord.isEmpty)
    }

    private var starting: Bool { if case .start = mode { return true } else { return false } }

    private var again: Bool { if case .again = mode { return true } else { return false } }

    /// Settles a room from the recent list into the choice the sheet shows:
    /// Join while a meeting is live in it, Start, with its name, while none
    /// is. Not over a choice made meanwhile, and left as it is when the
    /// deployment does not say, which `go` then takes either way.
    private func settleAgain() async {
        guard case let .again(name) = mode, let deployment = app.deployment else { return }
        let live = (try? await deployment.roomLive(room: name)) ?? nil
        guard again, let live else { return }
        withAnimation(Theme.settle) { mode = live ? .join(room) : .start }
    }

    var body: some View {
        Group {
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
            .contentLayer()
            .background(Backdrop().opacity(0.55))
            #else
            // At the foot of a phone, where the thumb is; in the middle of a
            // wide screen, at a card's width, where across the whole foot of
            // an opened Duo it was a bar 950 points long. The same card in
            // both, only placed, so a fold or an unfold keeps the room typed
            // into it and the keyboard where it was.
            ZStack {
                CameraStage(app: app).ignoresSafeArea()
                ZStack(alignment: wide ? .center : .bottom) {
                    VStack(spacing: 16) {
                        header
                        if !inColumn {
                            DeviceSwitches(app: app, size: 52).transition(Theme.arrive)
                        }
                        form
                        footer
                    }
                    .padding(18)
                    .concentricGlassShell(minimum: 34)
                    .frame(maxWidth: wide ? 520 : .infinity)
                    .padding(10)
                }
                // On one page of a Duo held as a book, and above the fold of
                // one standing on a table, rather than across either.
                .keptToOneSide(.reading)
            }
            .putsKeyboardAway()
            .animation(Theme.settle, value: wide)
            .animation(Theme.settle, value: inColumn)
            .systemColumn($inColumn, least: 220) {
                ColumnHeadButton(symbol: "xmark", label: t("Cancel")) { cancel() }
            } foot: {
                ColumnCapsule { ColumnDeviceSwitches(app: app) }
            }
            #endif
        }
        .animation(Theme.settle, value: sealing)
        .animation(Theme.settle, value: app.problem)
        .animation(Theme.settle, value: starting)
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
        .task { await settleAgain() }
    }

    /// Closes the sheet, and calls off whatever its press set going: Cancel
    /// and the cross stayed live while a join was out and did nothing to it,
    /// so somebody who pressed Cancel was put in the call a moment later
    /// with the microphone on.
    private func cancel() {
        going?.cancel()
        going = nil
        app.cancelJoin()
        dismiss()
    }

    private var header: some View {
        HStack {
            if !inColumn {
                Button { cancel() } label: { Image(systemName: "xmark").font(.system(size: 13, weight: .bold)) }
                    .buttonStyle(.glassCircle(32))
                    .accessibilityLabel(t("Cancel"))
                    .transition(Theme.arrive)
            }
            #if os(macOS)
            Spacer()
            choice.frame(maxWidth: 300)
            Spacer()
            Color.clear.frame(width: 32, height: 32)
            #else
            // The whole width beside the close button: centred between two
            // buttons' worth of space it cut "Start new meeting" short on a
            // phone held upright.
            choice
            #endif
        }
    }

    private var choice: some View {
        SegmentedChoice(options: [(true, t("Start new meeting")), (false, t("Join a meeting"))],
                        selection: Binding(get: { starting }, set: { mode = $0 ? .start : .join(room) }), height: 32)
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 10) {
            // The scope half of a name completes from the account's own
            // (ScopedRoomInput.tsx), in both directions as on the web.
            ScopedRoomField(prompt: starting ? t("Name it yourself, or leave it blank for one nobody has used.") : t("Room name or invite link"),
                            text: $room, scopes: app.account?.scopes ?? [], admin: app.account?.admin ?? false,
                            focusOnAppear: LobbyDebug.room != nil, onSubmit: go)
            VStack(spacing: 0) {
                if app.offersRelayChoice {
                    HStack(spacing: 12) {
                        Image(systemName: "server.rack").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted).frame(width: 18)
                        Text(t("Server")).font(.system(size: 14))
                        Spacer(minLength: 8)
                        ServerMenu(app: app, compact: true, id: "sheet.server")
                    }
                    .padding(.horizontal, 14).frame(minHeight: 48)
                    Rectangle().fill(Theme.ink.opacity(0.06)).frame(height: 1)
                }
                SealControl(on: $sealing, word: $app.sealingWord)
                    .padding(.horizontal, 14).padding(.vertical, 12)
            }
            .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.sunk(0.22)))
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Theme.ink.opacity(0.08)))
        }
    }

    private var footer: some View {
        VStack(spacing: 10) {
            if let problem = app.problem {
                Text(problem).font(.system(size: 12.5)).foregroundStyle(Theme.dangerInk)
                    .multilineTextAlignment(.center).transition(Theme.arrive)
            }
            HStack(spacing: 10) {
                if app.canKnock {
                    Button(t("Ask to be let in")) { Task { await app.knock(); dismiss() } }
                        .buttonStyle(.glassCapsule(40))
                        .disabled(app.busy)
                        .transition(Theme.arrive)
                }
                // On the Mac the buttons sit at the trailing end, after the
                // space. On an iPhone the one button is the whole width, and
                // the space before it, its minimum and the stack's spacing,
                // pushed it eighteen points off centre.
                #if os(macOS)
                Spacer()
                Button(t("Cancel")) { cancel() }.buttonStyle(.glassCapsule(36)).keyboardShortcut(.cancelAction)
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
                // And while any join is out, a called-off one included: a
                // second beside it would go in as the same participant.
                .disabled(working || app.busy || (!starting && !validRoomName(name: normaliseRoomName(typed: room)) && pastedLink(room) == nil))
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
        guard !working, !app.busy, let deployment = app.deployment else { return }
        // An invitation pasted in where a name goes is opened as the link it
        // is: a room's name taken out of it would leave the invitation behind.
        if !starting, let url = pastedLink(room) {
            if !sealing { app.sealingWord = "" }
            dismiss()
            Task { await app.open(url) }
            return
        }
        let typed = normaliseRoomName(typed: room)
        let target = starting ? (validRoomName(name: typed) ? typed : generateRoomName()) : typed
        guard validRoomName(name: target) else { return }
        if !sealing { app.sealingWord = "" }
        working = true
        app.problem = nil
        going = Task {
            defer { working = false }
            // A join from the press, the question included: an update
            // installing behind the sheet is given up now rather than left
            // to replace the app under the call this is about to enter.
            await app.headingIn {
                let live = (try? await deployment.roomLive(room: target)) ?? nil
                // Cancelled while the question was out: nothing more is asked.
                if Task.isCancelled { return }
                if starting, live == true {
                    app.problem = t("A meeting is already happening under that name.")
                    return
                }
                // A room from the recent list goes in whether or not it is live:
                // pressed before the sheet had settled which it is, it is that room
                // somebody means, and entering it starts it again if it has ended.
                if !starting, !again, live == false {
                    app.problem = t("No meeting is happening under that name.")
                    return
                }
                await app.enter(room: target)
            }
        }
    }
}

/// A room name, with the part after `@` filled in from the account's own
/// scopes (ScopedRoomInput.tsx). The scope is the half nobody should have to
/// type, and a slip in it is not another room but a refusal at the door, so
/// typing `@` offers the list, a single scope is simply written in, and a
/// scope the account is not in is said under the field before anything is
/// pressed. Only completed, never rewritten; an administrator belongs to every
/// scope and is never warned.
struct ScopedRoomField: View {
    let prompt: String
    @Binding var text: String
    let scopes: [String]
    var admin = false
    /// The phone's join bar, a capsule of glass rather than a field in a card.
    var capsule = false
    /// Takes the keyboard when it appears; a test's way to see the list.
    var focusOnAppear = false
    var onSubmit: () -> Void = {}
    @State private var dismissed = false
    @State private var active = 0
    @FocusState private var focused: Bool

    private var at: String.Index? { text.lastIndex(of: "@") }
    private var typedScope: String { at.map { String(text[text.index(after: $0)...]).lowercased() } ?? "" }
    private var matches: [String] { at == nil ? [] : scopes.filter { $0.hasPrefix(typedScope) } }
    private var open: Bool {
        focused && !dismissed && !matches.isEmpty && !(matches.count == 1 && matches[0] == typedScope)
    }
    private var strange: Bool {
        at != nil && !typedScope.isEmpty && !admin && !scopes.contains(typedScope) && !open
    }
    private var chosen: Int { min(active, max(0, matches.count - 1)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if open && capsule { suggestions.transition(Theme.slideUp) }
            field
            if open && !capsule { suggestions.transition(Theme.arrive) }
            if strange {
                Text(t("You are not in {scope}, so this room will not let you in.", ["scope": typedScope]))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.dangerInk)
                    .padding(.horizontal, 6)
                    .fixedSize(horizontal: false, vertical: true)
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: open)
        .animation(Theme.settle, value: strange)
        .onAppear {
            guard focusOnAppear else { return }
            Task {
                try? await Task.sleep(for: .seconds(0.8))
                focused = true
            }
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: capsule ? "link" : "number")
                .font(.system(size: capsule ? 16 : 14, weight: .medium))
                .foregroundStyle(Theme.muted)
                .frame(width: 18)
            TextField("", text: $text, prompt: Text(prompt).foregroundStyle(Theme.muted))
                .textFieldStyle(.plain)
                .font(.system(size: capsule ? 16 : 14.5, design: text.isEmpty ? .default : .monospaced))
                .foregroundStyle(Theme.foreground)
                .focused($focused)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.join)
                #endif
                .onSubmit {
                    if open { accept(matches[chosen]) } else { onSubmit() }
                }
                .onChange(of: text) { old, new in
                    dismissed = false
                    active = 0
                    // The @ just typed, at the end, by somebody with one
                    // scope: there is nothing to choose, so it is written in.
                    if new == old + "@", scopes.count == 1, !old.contains("@") {
                        text = new + scopes[0]
                        dismissed = true
                    }
                }
                .onKeyPress(.downArrow) { move(1) }
                .onKeyPress(.upArrow) { move(-1) }
                .onKeyPress(.tab) {
                    guard open else { return .ignored }
                    accept(matches[chosen])
                    return .handled
                }
                .onKeyPress(.escape) {
                    guard open else { return .ignored }
                    dismissed = true
                    return .handled
                }
        }
        .padding(.horizontal, capsule ? 18 : 14)
        .frame(height: capsule ? 54 : 46)
        .modifier(FieldGround(capsule: capsule))
    }

    private var suggestions: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(matches.indices, id: \.self) { index in
                let scope = matches[index]
                Button { accept(scope) } label: {
                    HStack(spacing: 2) {
                        Text(verbatim: "@").foregroundStyle(Theme.muted)
                        Text(scope).foregroundStyle(Theme.foreground)
                        Spacer()
                        if index == chosen {
                            Image(systemName: "return").font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.muted)
                        }
                    }
                    .font(.system(size: 13.5, design: .monospaced))
                    .padding(.horizontal, 10).frame(minHeight: 32)
                    .background(RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(Theme.ink.opacity(index == chosen ? 0.10 : 0)))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { if $0 { active = index } }
            }
        }
        .padding(5)
        .floatingPane(cornerRadius: 14)
        .accessibilityLabel(t("Your scopes"))
    }

    private func move(_ step: Int) -> KeyPress.Result {
        guard open else { return .ignored }
        active = (chosen + step + matches.count) % matches.count
        return .handled
    }

    private func accept(_ scope: String) {
        guard let at else { return }
        text = String(text[...at]) + scope
        active = 0
        dismissed = true
    }
}

private struct FieldGround: ViewModifier {
    let capsule: Bool

    func body(content: Content) -> some View {
        if capsule {
            content.glassSurface(Capsule())
        } else {
            content
                .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.sunk(0.24)))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.ink.opacity(0.10)))
        }
    }
}

/// Where the call is held, as the app's own popover: automatic or one relay,
/// each with how quickly it answered from here. Drawn only where there is a
/// choice to make (`AppModel.offersRelayChoice`).
struct ServerMenu: View {
    @Bindable var app: AppModel
    var compact = false
    var id = "server"
    /// Under the head of the column down the side of an iPhone Duo's inner
    /// display on its side (PhoneLobby), or of a system's column beside the
    /// sidebar's stage (HomeStage): the symbol alone, on a circle the size of
    /// the system's own items there, since the column is 84 points wide. The
    /// choice and its round trip are said by the button's label and shown
    /// by the menu it opens.
    var inColumn = false
    @Environment(\.popovers) private var popovers

    var body: some View {
        if let popovers {
            Group {
                if inColumn {
                    Button { popovers.toggle(id) } label: {
                        Image(systemName: "server.rack").font(.system(size: 16, weight: .semibold))
                    }
                    .buttonStyle(.glassCircle(48))
                    .accessibilityLabel(t("Server"))
                    .accessibilityValue(app.relayChoice.map { app.lobby.label($0) } ?? t("Automatic"))
                    .help(t("Server"))
                } else {
                    Button { popovers.toggle(id) } label: {
                        HStack(spacing: 6) {
                            if !compact { Image(systemName: "server.rack").font(.system(size: 12)) }
                            Text(app.relayChoice.map { app.lobby.label($0) } ?? t("Automatic")).lineLimit(1)
                            if let ms = current {
                                Text("\(Int(ms)) ms").monospacedDigit().foregroundStyle(latencyColour(ms))
                            }
                            Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .semibold)).foregroundStyle(Theme.muted)
                        }
                        .font(.system(size: 12.5))
                    }
                    .buttonStyle(.glassCapsule(compact ? 30 : 34))
                    .fixedSize()
                }
            }
            .floatingPopover(id, in: popovers, width: 290) {
                ScrollView {
                    MenuCard {
                        MenuItem(title: t("Automatic"), symbol: "wand.and.stars",
                                 detail: app.lobby.fastest.map { "\(t("Whichever answers fastest")) · \(app.lobby.label($0.name))" }
                                    ?? t("Whichever answers fastest"),
                                 checked: app.relayChoice == nil) {
                            app.relayChoice = nil
                            popovers.close()
                        }
                        ForEach(app.lobby.grouped(), id: \.region) { group in
                            if !group.region.isEmpty { MenuHeading(title: group.region) }
                            ForEach(group.relays, id: \.name) { relay in
                                MenuItem(title: relay.adminOnly ? "\(say(relay.label ?? relay.name)) [Admin]" : say(relay.label ?? relay.name),
                                         detail: relay.maintenance ? t("Not taking calls") : millis(relay.name),
                                         checked: app.relayChoice == relay.name) {
                                    app.relayChoice = relay.name
                                    popovers.close()
                                }
                                .disabled(relay.maintenance)
                            }
                        }
                    }
                }
                .frame(maxHeight: 420)
                .fixedSize(horizontal: false, vertical: true)
            }
            .task { if app.lobby.measured == nil { await app.lobby.measure() } }
        }
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
}

/// ServerPicker.tsx `Latency`: green to 150 ms, the signal colour to 400,
/// red beyond. Written as words, so in the colours' inks (Theme.tallyInk).
@MainActor
func latencyColour(_ ms: Double) -> Color {
    ms <= 150 ? Theme.goodInk : ms <= 400 ? Theme.tallyInk : Theme.dangerInk
}

/// One arranged meeting as a row.
struct MeetingRow: View {
    let one: Arrangement
    @Bindable var app: AppModel

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: one.started ? "video.fill" : "calendar")
                .font(.system(size: 15))
                .foregroundStyle(one.started ? Theme.tallyInk : Theme.muted)
                .frame(width: 34, height: 34)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(one.started ? Theme.tally.opacity(0.18) : Theme.ink.opacity(0.08)))
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
        let date = AppModel.instant(one.at).map(Self.said) ?? one.at
        if let relay = one.relay, !relay.isEmpty { return "\(date) · \(app.lobby.label(relay))" }
        return date
    }

    /// A meeting's time as a row has room for it (MeetingTimes `short`).
    static func said(_ date: Date) -> String {
        MeetingTimes.short(date)
    }
}

// MARK: - Arranging

/// What the schedule's form has typed and chosen: the room's name, the day
/// and the time. Kept by the lobby rather than by the form, because the form
/// is drawn in a sheet on a phone and beside the sidebar on a wider screen,
/// and an iPhone Duo opened or closed with it showing draws it in the other;
/// kept in the form, the name half typed and the day chosen went with the
/// layout.
@MainActor
@Observable
final class ScheduleDraft {
    var planning = ""
    /// The next quarter hour's day, hour and minute (ScheduleClock
    /// `start`): from 23:45 on that is the next day's midnight, which the
    /// form offered on today's date, already gone.
    var day: Date
    var hour: Int
    var minute: Int

    init(now: Date = Date(), calendar: Calendar = .current) {
        let start = ScheduleClock.start(now: now, calendar: calendar)
        day = start.day
        hour = start.hour
        minute = start.minute
    }
}

/// Schedule.tsx: which name, which day, what time, which machine, and the
/// meetings already arranged with their links.
struct ScheduleView: View {
    @Bindable var app: AppModel
    let account: Account
    @Bindable var draft: ScheduleDraft
    /// Back to the lobby's home: on a phone the sheet closes, beside the
    /// sidebar the section goes back to the picture. No default, so that no
    /// caller is left with a cross that does nothing, as the phone's sheet
    /// was for a while.
    let close: () -> Void
    @State private var arranging = false
    @State private var copied: String?
    @State private var now = Date()
    /// The fold of a book across the form and the list (`columns`).
    @State private var book: CGRect?
    @State private var popovers = Popovers()
    /// The close stands in the system's column down the side of an iPhone
    /// Duo's outer display (SystemColumn), under the clock, and leaves the
    /// heading while it does.
    @State private var inColumn = false

    private static let ahead: TimeInterval = 60 * 24 * 60 * 60

    private var canArrange: Bool { account.can?.contains("arrange") ?? true }
    private var at: Date? {
        ScheduleClock.instant(day: draft.day, hour: draft.hour, minute: draft.minute, calendar: .current, now: now)
    }
    private var problem: Bool {
        guard let at else { return true }
        return at < now || at > now.addingTimeInterval(Self.ahead)
    }

    // The page was the system's on a phone (a navigation bar over plain
    // black, nothing to close it with), its parts did not line up (the time
    // pushed to the left of a row it did not fill, the summary beside the
    // button breaking "GMT+8" onto a line of its own, "No meetings arranged
    // yet." floating at the top of an empty column on a Mac), and nothing on
    // it moved: a meeting arranged or called off appeared and vanished, as
    // did the summary's warnings (the owner, 2026-10-17). Now it is drawn as
    // the start and join sheet is, on the room's ground with a close
    // of its own on a phone; the time sits at the end of its row and the
    // summary has the card's width above a button that has it too; the list
    // is a card at the height of the form's, empty or not; and what comes and
    // goes does so with the app's own arrival and leaving.
    var body: some View {
        ScrollView {
            #if os(macOS)
            VStack(alignment: .leading, spacing: 20) {
                heading(size: 26)
                columns
            }
            .padding(28)
            #else
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top, spacing: 12) {
                    heading(size: 22)
                    Spacer(minLength: 0)
                    if !inColumn {
                        Button(action: close) { Image(systemName: "xmark").font(.system(size: 13, weight: .bold)) }
                            .buttonStyle(.glassCircle(32))
                            .accessibilityLabel(t("Close"))
                            .transition(Theme.arrive)
                    }
                }
                // Clear of the sheet's grabber, which it sat against.
                .padding(.top, 22)
                columns
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
            #endif
        }
        .scrollIndicators(.never)
        .contentLayer()
        #if os(iOS)
        .background(Backdrop().opacity(0.6).ignoresSafeArea())
        .putsKeyboardAway()
        .animation(Theme.settle, value: inColumn)
        .systemColumn($inColumn, least: 120) {
            ColumnHeadButton(symbol: "xmark", label: t("Close"), action: close)
        } foot: {
            EmptyView()
        }
        #else
        .background(Backdrop().opacity(0.6))
        #endif
        .task { await app.lobby.refreshArrangements() }
        .animation(Theme.settle, value: app.lobby.arrangements.map(\.id))
        .animation(Theme.settle, value: listing)
        .floatingPopoverLayer(popovers)
        .onAppear { popovers.openFromDebug() }
    }

    /// Side by side where the list has room for a meeting's name, its time
    /// and its three buttons, as beside a sidebar on a Mac or an opened Duo;
    /// in a narrower window, or a phone's sheet, it was squeezed to "…"
    /// beside the form, so it goes under it.
    ///
    /// Over the whole of an iPhone Duo's inner display on its side held
    /// half open as a book, the form keeps to the page before the fold and
    /// the list to the page after it: side by side as on a flat screen, the
    /// list started some seventy points before the fold and its rows were
    /// bent across it.
    @ViewBuilder
    private var columns: some View {
        Group {
            if let book {
                HStack(alignment: .top, spacing: book.width + 16) {
                    form.frame(width: max(0, book.minX - 8))
                    list.frame(maxWidth: .infinity, alignment: .top)
                }
            } else {
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 20) {
                        form.frame(width: 360)
                        list.frame(minWidth: 420, maxWidth: .infinity, alignment: .top)
                    }
                    VStack(alignment: .leading, spacing: 20) {
                        form.frame(maxWidth: 560)
                        list.frame(maxWidth: 560)
                    }
                }
            }
        }
        .onGeometryChange(for: CGRect?.self) { proxy in
            MainActor.assumeIsolated {
                let fold = Fold.of(proxy)
                // A form of its own width at least before the fold, as
                // the form has on a flat screen.
                guard fold.posture(in: proxy.size) == .book, let crease = fold.crease, crease.minX >= 368 else { return nil }
                return crease
            }
        } action: { book = $0 }
        .animation(Theme.settle, value: book)
    }

    private func heading(size: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(t("Scheduled meetings")).font(.system(size: size, weight: .bold))
            Text(t("Pick a time and hand out the link before it starts."))
                .font(.system(size: size > 24 ? 14 : 13)).foregroundStyle(Theme.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// A label at the start of one of the form's rows.
    private func rowLabel(_ text: String, _ symbol: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: symbol).font(.system(size: 12, weight: .semibold))
            Text(text)
        }
        .font(.system(size: 13))
        .foregroundStyle(Theme.muted)
    }

    @ViewBuilder
    private var form: some View {
        VStack(alignment: .leading, spacing: 14) {
            if !canArrange {
                Text(t("Arranging meetings has not been opened to you.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
            } else {
                GlassField(prompt: t("Room name"), text: $draft.planning, symbol: "number", mono: !draft.planning.isEmpty, height: 40)
                Text(t("Name it yourself, or leave it blank for one nobody has used.")).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
                // The app's own month rather than the system's calendar: the
                // chosen day in the person's colour, the days past and beyond
                // sixty dimmed (Schedule.tsx offers no further ahead).
                MonthCalendar(day: $draft.day, earliest: Date(), latest: Date().addingTimeInterval(Self.ahead))
                    .padding(12)
                    .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.sunk(0.18)))
                HStack(spacing: 8) {
                    rowLabel(t("Time"), "clock")
                    Spacer(minLength: 8)
                    ClockChoice(id: "schedule.clock", hour: $draft.hour, minute: $draft.minute)
                }
                if app.offersRelayChoice {
                    HStack(spacing: 8) {
                        rowLabel(t("Server"), "server.rack")
                        Spacer(minLength: 8)
                        ServerMenu(app: app, compact: true, id: "schedule.server")
                    }
                }
                Divider().overlay(Theme.ink.opacity(0.06))
                HStack(alignment: .firstTextBaseline, spacing: 7) {
                    Image(systemName: problem ? "exclamationmark.circle" : "calendar.badge.clock")
                        .font(.system(size: 12, weight: .semibold))
                        .contentTransition(.symbolEffect(.replace))
                    Text(verdict).font(.system(size: 12.5))
                        .fixedSize(horizontal: false, vertical: true)
                        .contentTransition(.opacity)
                }
                .foregroundStyle(problem ? Theme.dangerInk : Theme.muted)
                .frame(maxWidth: .infinity, alignment: .leading)
                .animation(Theme.settle, value: verdict)
                Button(action: plan) {
                    ZStack {
                        if arranging {
                            ProgressView().controlSize(.small).tint(Theme.tallyForeground).transition(.opacity)
                        } else {
                            Text(t("Arrange")).transition(.opacity)
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(40, prominent: true))
                .disabled(arranging || problem)
                .animation(Theme.settle, value: arranging)
            }
        }
        .padding(18)
        .glassPane(cornerRadius: 24)
    }

    private var verdict: String {
        guard let at else { return t("Choose a day.") }
        if at < now { return t("That time has already gone.") }
        if at > now.addingTimeInterval(Self.ahead) { return t("That is more than sixty days away.") }
        return t("Arranged for {when}", ["when": LobbyModel.said(at)])
    }

    /// What the list has to say, which its card changes between.
    private enum Listing: Hashable { case signedOut, failed, loading, empty, meetings }

    private var listing: Listing {
        switch app.lobby.status {
        case .signedOut: return .signedOut
        case .failed: return .failed
        case .loading where app.lobby.arrangements.isEmpty: return .loading
        default: return app.lobby.arrangements.isEmpty ? .empty : .meetings
        }
    }

    @ViewBuilder
    private var list: some View {
        VStack(alignment: .leading, spacing: 10) {
            switch listing {
            case .signedOut:
                notice(t("Your session has expired. Sign in again."), "person.crop.circle.badge.exclamationmark")
                    .transition(Theme.arrive)
            case .failed:
                notice(t("Could not load your meetings."), "exclamationmark.triangle", retry: true)
                    .transition(Theme.arrive)
            case .loading:
                ProgressView().controlSize(.small)
                    .frame(maxWidth: .infinity, minHeight: 140)
                    .glassPane(cornerRadius: 22)
                    .transition(Theme.arrive)
            case .empty:
                VStack(spacing: 10) {
                    Image(systemName: "calendar.badge.clock").font(.system(size: 26)).foregroundStyle(Theme.muted)
                    Text(t("No meetings arranged yet.")).font(.system(size: 14, weight: .semibold))
                }
                .frame(maxWidth: .infinity, minHeight: 140)
                .glassPane(cornerRadius: 22)
                .transition(Theme.arrive)
            case .meetings:
                EmptyView()
            }
            if !app.lobby.arrangements.isEmpty {
                #if os(iOS)
                phoneMeetings
                    .transition(Theme.arrive)
                #else
                VStack(spacing: 0) {
                    ForEach(app.lobby.arrangements, id: \.id) { one in
                        VStack(spacing: 0) {
                            row(one)
                            if one.id != app.lobby.arrangements.last?.id { Divider().overlay(Theme.ink.opacity(0.06)) }
                        }
                        .transition(.asymmetric(
                            insertion: .move(edge: .top).combined(with: .opacity).animation(Theme.settle),
                            removal: .opacity.combined(with: .scale(scale: 0.96)).animation(Theme.leave)
                        ))
                    }
                }
                .glassPane(cornerRadius: 22)
                .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                .transition(Theme.arrive)
                #endif
            }
        }
    }

    #if os(iOS)
    private static let meetingRow: CGFloat = 60

    /// On a phone Cancel is out of sight until a meeting is swiped to the
    /// left, as Remove is on the recent rooms (the owner asked for it there),
    /// which leaves the row room for its time beside the two buttons kept.
    private var phoneMeetings: some View {
        let shown = app.lobby.arrangements
        return List {
            ForEach(shown, id: \.id) { one in
                row(one, cancel: false)
                    .frame(height: Self.meetingRow)
                    .overlay(alignment: .bottom) {
                        if one.id != shown.last?.id { Divider().overlay(Theme.ink.opacity(0.06)).padding(.leading, 60) }
                    }
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                        Button(role: .destructive) {
                            Task { if let said = await app.lobby.cancel(one) { app.problem = said } }
                        } label: {
                            Label(t("Cancel meeting"), systemImage: "xmark")
                        }
                    }
            }
        }
        .listStyle(.plain)
        .scrollDisabled(true)
        .scrollContentBackground(.hidden)
        .environment(\.defaultMinListRowHeight, Self.meetingRow)
        .frame(height: Self.meetingRow * CGFloat(shown.count))
        .glassPane(cornerRadius: 22)
        .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
    }
    #endif

    private func notice(_ text: String, _ symbol: String, retry: Bool = false) -> some View {
        HStack(spacing: 10) {
            Image(systemName: symbol).font(.system(size: 14)).foregroundStyle(Theme.muted)
            Text(text).font(.system(size: 13)).foregroundStyle(Theme.muted)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 8)
            if retry {
                Button(t("Try again")) { Task { await app.lobby.refreshArrangements() } }.buttonStyle(.glassCapsule(30))
            }
        }
        .padding(14)
        .glassPane(cornerRadius: 18)
    }

    private func row(_ one: Arrangement, cancel: Bool = true) -> some View {
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
            #if os(iOS)
            // The share sheet, for the link to go to Messages as an
            // invitation rather than as a bare address (LinkShare).
            LinkShareButton(room: one.room, link: { app.lobby.link(one) }) {
                Self.rowCircle("square.and.arrow.up")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(t("Share link"))
            #endif
            #if os(macOS)
            small("calendar.badge.plus", t("Add to calendar")) {
                if let file = app.lobby.calendarFile(one) { NSWorkspace.shared.open(file) }
            }
            #else
            if let file = app.lobby.calendarFile(one) {
                ShareLink(item: file) {
                    Self.rowCircle("calendar.badge.plus")
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(t("Add to calendar"))
            }
            #endif
            if cancel {
                small("xmark", t("Cancel meeting"), destructive: true) {
                    Task { if let said = await app.lobby.cancel(one) { app.problem = said } }
                }
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
    }

    private func small(_ symbol: String, _ label: String, destructive: Bool = false, action: @escaping () -> Void) -> some View {
        #if os(iOS)
        // In a List row a button of a style of the app's own is not pressed
        // by itself: the row takes the touch, and Copy link did nothing under
        // a finger (UITests/LobbyTouchTests, "Link copied" never came). A
        // borderless button is pressed by itself, and draws its own circle.
        Button(action: action) {
            Self.rowCircle(symbol, destructive: destructive)
        }
        .buttonStyle(.borderless)
        .accessibilityLabel(label)
        #else
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 12, weight: .semibold))
                .foregroundStyle(destructive ? Theme.dangerInk : Theme.foreground)
        }
        .buttonStyle(.glassCircle(32))
        .accessibilityLabel(label)
        .help(label)
        #endif
    }

    #if os(iOS)
    /// A row's small round button as the app's other buttons in the content
    /// layer are drawn: a light fill and a hairline.
    static func rowCircle(_ symbol: String, destructive: Bool = false) -> some View {
        Image(systemName: symbol).font(.system(size: 12, weight: .semibold))
            .foregroundStyle(destructive ? Theme.dangerInk : Theme.foreground)
            .frame(width: 32, height: 32)
            .background(Circle().fill(Theme.lifted(0.07, nested: true)))
            .overlay(Circle().stroke(Theme.ink.opacity(0.09), lineWidth: 1))
            .contentShape(Circle())
    }
    #endif

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
        guard !arranging else { return }
        now = Date()
        guard let at, !problem else { return }
        let wanted = normaliseRoomName(typed: draft.planning)
        let room = validRoomName(name: wanted) ? wanted : generateRoomName()
        arranging = true
        Task {
            defer { arranging = false }
            switch await app.lobby.arrange(room: room, at: at, relay: app.relayChoice) {
            case let .success(made):
                draft.planning = ""
                copy(made)
            case let .failure(refused):
                app.problem = refused.sentence
            }
        }
    }
}

// MARK: - The relays

/// Every relay grouped by region with its round trip from here, and the
/// automatic choice above them (ServerPicker.tsx `Rows`), as the app's own
/// cards rather than the system's grouped list.
struct ServersView: View {
    @Bindable var app: AppModel
    /// Measuring again is the column's beside the phone's lobby on an
    /// iPhone Duo's outer display (PhoneLobby), and leaves the page's head
    /// while it is.
    var measureInColumn = false
    /// The page is the section beside the lobby's sidebar (SidebarLobby),
    /// and stands its own Measure again at the foot of a column down the side
    /// of the screen, as the phone's lobby stands it in the outer display's.
    /// Not inside the phone's lobby, whose column already holds it.
    var ownColumn = false
    @State private var columnShown = false

    var body: some View {
        #if os(iOS)
        if ownColumn {
            page
                .animation(Theme.settle, value: columnShown)
                .systemColumn($columnShown, least: 120) {
                    EmptyView()
                } foot: {
                    ColumnCapsule {
                        BarButton(symbol: "arrow.clockwise", label: t("Measure again")) {
                            Task { await app.lobby.measure() }
                        }
                        .disabled(app.lobby.measuring)
                    }
                }
        } else {
            page
        }
        #else
        page
        #endif
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack(alignment: .center) {
                    #if os(macOS)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(t("Server")).font(.system(size: 26, weight: .bold))
                        FleetLine(app: app)
                    }
                    #else
                    FleetLine(app: app)
                    #endif
                    Spacer()
                    if !measureInColumn && !columnShown {
                        Button {
                            Task { await app.lobby.measure() }
                        } label: {
                            Label(t("Measure again"), systemImage: "arrow.clockwise")
                                .labelStyle(.titleAndIcon)
                                .font(.system(size: 12.5, weight: .semibold))
                        }
                        .buttonStyle(.glassCapsule(32))
                        .disabled(app.lobby.measuring)
                        .transition(Theme.arrive)
                    }
                }
                .frame(minHeight: 32)
                .animation(Theme.settle, value: measureInColumn)
                .animation(Theme.settle, value: columnShown)
                .rise()
                SettingsSection(title: t("Automatic")) {
                    row(title: t("Automatic"), detail: automaticDetail, symbol: "wand.and.stars", chosen: app.relayChoice == nil,
                        ms: nil, off: false, last: true) {
                        app.relayChoice = nil
                    }
                }
                .rise(0.04)
                ForEach(app.lobby.grouped(), id: \.region) { group in
                    SettingsSection(title: group.region.isEmpty ? t("Server") : group.region) {
                        ForEach(group.relays, id: \.name) { relay in
                            row(title: relay.adminOnly ? "\(say(relay.label ?? relay.name)) [Admin]" : say(relay.label ?? relay.name),
                                detail: relay.maintenance ? t("Not taking calls") : relay.fallback ? "[Fallback]" : nil,
                                symbol: nil, chosen: app.relayChoice == relay.name,
                                ms: app.lobby.measured.map { $0[relay.name] ?? nil }, off: relay.maintenance,
                                last: relay.name == group.relays.last?.name) {
                                app.relayChoice = relay.name
                            }
                        }
                    }
                    .rise(0.08)
                }
            }
            .padding(.horizontal, platformPadding)
            .padding(.vertical, platformPadding)
            .frame(maxWidth: 680, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .scrollIndicators(.never)
        #if os(iOS)
        .navigationTitle(t("Server"))
        .navigationBarTitleDisplayMode(.inline)
        .background(Backdrop())
        #else
        .background(Backdrop().opacity(0.6))
        #endif
        .task { if app.lobby.measured == nil { await app.lobby.measure() } }
        .animation(Theme.settle, value: app.relayChoice)
    }

    private var platformPadding: CGFloat {
        #if os(iOS)
        16
        #else
        28
        #endif
    }

    private var automaticDetail: String {
        if let fastest = app.lobby.fastest { return "\(t("Whichever answers fastest")) · \(app.lobby.label(fastest.name))" }
        return t("Whichever answers fastest")
    }

    /// `ms` is nil when nothing has been measured yet, .some(nil) for a relay
    /// that did not answer, and the round trip otherwise.
    private func row(title: String, detail: String?, symbol: String?, chosen: Bool, ms: Double??, off: Bool, last: Bool,
                     pick: @escaping () -> Void) -> some View {
        Button(action: pick) {
            SettingsRow(title: title, detail: detail, symbol: symbol ?? "server.rack", last: last) {
                HStack(spacing: 10) {
                    if symbol == nil {
                        if ms == nil {
                            if app.lobby.measuring { ProgressView().controlSize(.mini) }
                        } else if let value = ms ?? nil {
                            Text("\(Int(value.rounded())) ms").monospacedDigit().font(.system(size: 13)).foregroundStyle(latencyColour(value))
                        } else if !off {
                            Text(t("Timeout")).font(.system(size: 12)).foregroundStyle(Theme.dangerInk)
                        }
                    }
                    Image(systemName: "checkmark.circle.fill").font(.system(size: 15)).foregroundStyle(Theme.tallyInk)
                        .opacity(chosen ? 1 : 0).scaleEffect(chosen ? 1 : 0.6)
                }
            }
            .background(chosen ? Theme.tally.opacity(0.08) : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(off)
        .opacity(off ? 0.45 : 1)
        .animation(Theme.settle, value: chosen)
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
