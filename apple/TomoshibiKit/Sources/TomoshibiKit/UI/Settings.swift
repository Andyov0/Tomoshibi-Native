import AVFoundation
import ImageIO
import SwiftUI
import TomoshibiBasics
import TomoshibiCore
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
import CoreAudio
#else
import PhotosUI
import UIKit
#endif

// The settings, drawn as the app's own rather than as a preferences pane: the
// owner found the grouped form too plain and did not want the system's stock
// pieces (2026-10-17). On the Mac the Settings window (⌘,) is a glass sidebar
// of sections over the backdrop; on the phone the same sections are a
// sheet that drills in. What they hold is the web's: the account page
// (account/Account.tsx) and the choices it writes where a call reads them
// (live/preferences.ts, live/controls.ts, live/duck.ts, live/sound.ts), under
// the web's storage names, so that a choice made here is the one a call makes.

// MARK: - The sections

public enum SettingsTopic: String, CaseIterable, Identifiable, Sendable {
    case general, media, watching, music, account, about

    public var id: String { rawValue }

    var title: String {
        switch self {
        case .general: t("General")
        case .media: t("Audio & video")
        case .watching: t("Watch together")
        case .music: t("Music")
        case .account: t("Account")
        case .about: t("About")
        }
    }

    var symbol: String {
        switch self {
        case .general: "slider.horizontal.3"
        case .media: "video.fill"
        case .watching: "play.rectangle.fill"
        case .music: "music.note"
        case .account: "person.crop.circle.fill"
        case .about: "info.circle.fill"
        }
    }

    var detail: String {
        switch self {
        case .general: t("Language, colour, and how a call is laid out.")
        case .media: t("What starts on when you join, and which devices.")
        case .watching: t("How sharp a video watched together may get here.")
        case .music: t("How music sounds in a call.")
        case .account: t("Who you are here, and what you can use.")
        case .about: t("The app, and where its code can be read.")
        }
    }

    /// TOMOSHIBI_DEBUG_SETTINGS_SCROLL=end, in a debug build, scrolls a page
    /// to its foot, for a test to see the half below the fold.
    @MainActor
    static func debugScroll(_ reader: ScrollViewProxy) {
        #if DEBUG
        guard ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_SETTINGS_SCROLL"] == "end" else { return }
        Task {
            try? await Task.sleep(for: .seconds(1.5))
            reader.scrollTo("end", anchor: .bottom)
        }
        #endif
    }

    /// TOMOSHIBI_DEBUG_SETTINGS, honoured only in a debug build, opens one of
    /// the sections, for a test to look at it without a pointer.
    static var debug: SettingsTopic? {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_SETTINGS"].flatMap(SettingsTopic.init(rawValue:))
        #else
        nil
        #endif
    }
}

/// The choices a call reads that have no model of their own outside one,
/// under the web's storage names and with its values. Nothing is written in a
/// rehearsal: a test drives the very app somebody uses, and its settings are
/// that person's.
///
/// One for the app (AppModel `preferences`) rather than one per Settings
/// window, and read again from storage whenever storage changes: a call
/// writes some of these too, and a copy read once when the window opened
/// went on saying, and writing back, what had been true then.
@MainActor
@Observable
final class CallPreferences {
    @ObservationIgnored let keeps: Bool
    /// Where the choices about one's own devices are kept: the same store as
    /// the app's DeviceMemory, which the call reads and the devices panel in
    /// a call writes; nil in a rehearsal that keeps nothing.
    @ObservationIgnored private let deviceDefaults: UserDefaults?
    @ObservationIgnored private var refreshing = false
    @ObservationIgnored private var observer: NSObjectProtocol?

    /// sound.ts `meet-live.original-sound`: "on", or nothing.
    var originalSound: Bool {
        didSet { put(DeviceMemory.originalKey, originalSound ? "on" : nil, in: deviceDefaults) }
    }

    /// preferences.ts `meet-live.watch-quality`: a ceiling, "auto" kept as
    /// nothing.
    var watchQuality: String {
        didSet { keep("meet-live.watch-quality", watchQuality == "auto" ? nil : watchQuality) }
    }

    /// preferences.ts `meet-live.music-quality`: "best", or nothing for
    /// lossless. The music desk reads the same key when a call opens it.
    var musicQuality: String {
        didSet { keep("meet-live.music-quality", musicQuality == "best" ? "best" : nil) }
    }

    /// duck.ts `meet-live.music-duck`: on unless "off".
    var musicDucks: Bool {
        didSet { keep("meet-live.music-duck", musicDucks ? nil : "off") }
    }

    /// preferences.ts `meet-live.device-choice`: the device last chosen of
    /// each kind, under the web's kind names. A device's identifier here is
    /// the system's own (a CoreAudio UID, a capture device's unique ID), which
    /// means nothing on another machine, as the web says of its own.
    ///
    /// Changed one kind at a time (`choose`), over what is stored at that
    /// moment. The whole object used to be written from the copy read when
    /// the window opened, so a camera picked here erased the headset picked
    /// in the call a minute before.
    private(set) var devices: [String: String]

    static let choiceKey = DeviceMemory.choiceKey
    static let watchQualities = ["auto", "1080p", "720p"]

    init(keeps: Bool, deviceDefaults: UserDefaults?) {
        self.keeps = keeps
        self.deviceDefaults = deviceDefaults
        originalSound = false
        watchQuality = "auto"
        musicQuality = "lossless"
        musicDucks = true
        devices = [:]
        refresh()
        observer = NotificationCenter.default.addObserver(forName: UserDefaults.didChangeNotification, object: nil,
                                                          queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
    }

    /// Reads everything again, assigning only what changed, and writing
    /// nothing back.
    func refresh() {
        refreshing = true
        defer { refreshing = false }
        let defaults = UserDefaults.standard
        let original = deviceDefaults?.string(forKey: DeviceMemory.originalKey) == "on"
        if original != originalSound { originalSound = original }
        let stored = defaults.string(forKey: "meet-live.watch-quality") ?? "auto"
        let watch = Self.watchQualities.contains(stored) ? stored : "auto"
        if watch != watchQuality { watchQuality = watch }
        let music = defaults.string(forKey: "meet-live.music-quality") == "best" ? "best" : "lossless"
        if music != musicQuality { musicQuality = music }
        let ducks = defaults.string(forKey: "meet-live.music-duck") != "off"
        if ducks != musicDucks { musicDucks = ducks }
        // A rehearsal that keeps nothing has nothing stored to read, and
        // shows what it chose in this run.
        if let deviceDefaults {
            let fresh = Self.readDevices(deviceDefaults)
            if fresh != devices { devices = fresh }
        }
    }

    /// One kind's device, or the system's (nil), written over what is stored
    /// now rather than over what this showed.
    func choose(_ kind: String, _ id: String?) {
        guard let deviceDefaults else {
            devices[kind] = id
            return
        }
        var next = Self.readDevices(deviceDefaults)
        next[kind] = id
        devices = next
        write(next, to: deviceDefaults)
    }

    /// Forgets every device choice.
    func forgetDevices() {
        devices = [:]
        deviceDefaults?.removeObject(forKey: Self.choiceKey)
    }

    private func write(_ choice: [String: String], to defaults: UserDefaults) {
        if choice.isEmpty {
            defaults.removeObject(forKey: Self.choiceKey)
        } else if let data = try? JSONSerialization.data(withJSONObject: choice, options: [.sortedKeys]),
                  let text = String(data: data, encoding: .utf8) {
            defaults.set(text, forKey: Self.choiceKey)
        }
    }

    /// The web's reading: an object of strings no longer than any device's
    /// identifier, and nothing for anything else (DeviceMemory reads it the
    /// same way).
    private static func readDevices(_ defaults: UserDefaults) -> [String: String] {
        guard let text = defaults.string(forKey: choiceKey),
              let parsed = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any] else { return [:] }
        var out: [String: String] = [:]
        for kind in DeviceMemory.Kind.allCases.map(\.rawValue) {
            if let id = parsed[kind] as? String, !id.isEmpty, id.count <= 256 { out[kind] = id }
        }
        return out
    }

    private func keep(_ key: String, _ value: String?) {
        guard keeps else { return }
        put(key, value, in: .standard)
    }

    private func put(_ key: String, _ value: String?, in defaults: UserDefaults?) {
        guard !refreshing, let defaults else { return }
        if let value { defaults.set(value, forKey: key) } else { defaults.removeObject(forKey: key) }
    }
}

// MARK: - The view

/// The settings: on the Mac the Settings window, on the phone a sheet from
/// the home's account button.
public struct SettingsView: View {
    @Bindable var app: AppModel
    @State private var section: SettingsTopic
    /// The phone's drill-in: nil is the list of sections.
    @State private var open: SettingsTopic?
    @State private var popovers = Popovers()
    @State private var preferences: CallPreferences
    @State private var localization = Localization.shared
    @Namespace private var highlight
    @Environment(\.dismiss) private var dismiss

    public init(app: AppModel) {
        self.app = app
        _section = State(initialValue: SettingsTopic.debug ?? .general)
        _open = State(initialValue: SettingsTopic.debug)
        _preferences = State(initialValue: app.preferences)
    }

    public var body: some View {
        content
            .floatingPopoverLayer(popovers)
            .signOutPrompt(app, at: .settings)
            .preferredColorScheme(.dark)
            .tint(Theme.tally)
            .environment(\.locale, localization.language.locale)
            .onChange(of: section) { popovers.close() }
            .onChange(of: open) { popovers.close() }
            .onAppear { popovers.openFromDebug() }
    }

    @ViewBuilder
    private var content: some View {
        #if os(macOS)
        ZStack {
            Backdrop()
            HStack(spacing: 0) {
                sidebar
                    .frame(width: 236)
                    .padding(10)
                ScrollViewReader { reader in
                    ScrollView {
                        page(section)
                            .contentLayer()
                            .frame(maxWidth: 620, alignment: .leading)
                            .padding(.horizontal, 28)
                            .padding(.top, 44)
                            .padding(.bottom, 32)
                            .frame(maxWidth: .infinity)
                        Color.clear.frame(height: 1).id("end")
                    }
                    .onAppear { SettingsTopic.debugScroll(reader) }
                }
                .scrollIndicators(.automatic)
                .hidesScrollEdge()
                .id(section)
                .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 10)).animation(Theme.settle),
                                        removal: .opacity.animation(Theme.leave)))
            }
            // Up under the window's buttons, which sit on the sidebar's glass.
            .ignoresSafeArea(.container, edges: .top)
        }
        .frame(minWidth: 720, idealWidth: 820, minHeight: 520, idealHeight: 640)
        .background(SettingsWindowStyle())
        .hidesToolbarBackground()
        .removesWindowTitle()
        .animation(Theme.settle, value: section)
        #else
        // A list that drills into each section where the sheet is a phone's
        // width; a sidebar of sections beside the chosen one where it is as
        // wide as an opened iPhone Duo or an iPad gives it, as the Mac's
        // window and the Android app's wide settings are. One section open
        // for both, so that folding with a section showing keeps it in front,
        // and the section's page in the same place in either, so that what
        // is half typed into it (a passphrase being changed, a guest link
        // being made) stays: drawn again in the other layout, the page's own
        // state went with the old one.
        GeometryReader { proxy in
            let wide = proxy.size.width >= Self.wide
            // Across the fold of a book, should the system leave the sheet
            // there: the sidebar takes the first page to the fold, and the
            // section the second page after it. Laid out as on a flat
            // screen, the section's heading and its first cards lay over
            // the fold.
            let fold = Fold.of(proxy)
            let book = wide && fold.posture(in: proxy.size) == .book
            let first = fold.pane(.first, in: proxy.size)
            let second = fold.pane(.second, in: proxy.size)
            let sidebar = book ? max(200, first.width - 20) : 256
            let page = book ? second.width : min(proxy.size.width - 276, 652)
            ZStack {
                Backdrop()
                HStack(spacing: 0) {
                    if wide {
                        phoneSidebar
                            .frame(width: sidebar)
                            .padding(10)
                            .transition(.asymmetric(
                                insertion: .offset(x: -60).combined(with: .opacity).animation(Theme.settle),
                                removal: .offset(x: -60).combined(with: .opacity).animation(Theme.leave)))
                    }
                    ZStack {
                        if let shown = wide ? open ?? section : open {
                            drilled(shown, back: !wide, width: wide ? page : proxy.size.width)
                                .id(shown)
                                .transition(wide
                                    ? .asymmetric(insertion: .opacity.combined(with: .offset(y: 10)).animation(Theme.settle),
                                                  removal: .opacity.animation(Theme.leave))
                                    : .asymmetric(
                                        insertion: .move(edge: .trailing).combined(with: .opacity).animation(Theme.settle),
                                        removal: .move(edge: .trailing).combined(with: .opacity).animation(Theme.leave)))
                                .zIndex(1)
                        }
                        if !wide && open == nil {
                            home
                                .transition(.asymmetric(
                                    insertion: .offset(x: -60).combined(with: .opacity).animation(Theme.settle),
                                    removal: .offset(x: -60).combined(with: .opacity).animation(Theme.leave)))
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(.leading, book ? max(0, second.minX - first.maxX) : 0)
                }
            }
            .animation(Theme.settle, value: wide)
            .animation(Theme.settle, value: book)
        }
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(34)
        #endif
    }

    // MARK: The Mac's sidebar

    #if os(macOS)
    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 3) {
            // Under the window's buttons, which sit on the glass.
            Spacer().frame(height: 34)
            Text(t("Settings"))
                .font(.system(size: 21, weight: .bold))
                .padding(.horizontal, 10)
                .padding(.bottom, 10)
            ForEach(SettingsTopic.allCases) { one in
                sidebarRow(one)
            }
            Spacer(minLength: 12)
            if let account = app.account {
                HStack(spacing: 10) {
                    AccountAvatar(name: account.name, address: app.avatarAddress, size: 32)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(account.name).font(.system(size: 12.5, weight: .semibold)).lineLimit(1)
                        Text(account.trip).font(.system(size: 11, design: .monospaced)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.05)))
                .transition(Theme.arrive)
            }
        }
        .padding(10)
        .frame(maxHeight: .infinity, alignment: .top)
        .glassPane(cornerRadius: 22)
        .animation(Theme.settle, value: app.account?.name)
    }

    private func sidebarRow(_ one: SettingsTopic) -> some View {
        let chosen = section == one
        return Button {
            withAnimation(Theme.settle) { section = one }
        } label: {
            HStack(spacing: 10) {
                SectionTile(symbol: one.symbol, lit: chosen, size: 26)
                Text(one.title)
                    .font(.system(size: 13.5, weight: chosen ? .semibold : .regular))
                    .foregroundStyle(chosen ? Theme.foreground : Theme.foreground.opacity(0.82))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background {
                if chosen {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(Color.white.opacity(0.11))
                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.white.opacity(0.07)))
                        .matchedGeometryEffect(id: "chosen", in: highlight)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
    #endif

    // MARK: The phone's list

    #if os(iOS)
    /// The width from which the sheet is a sidebar beside a page, the
    /// Android app's (ui/settings/SettingsView.kt `WIDE`).
    private static let wide: CGFloat = 600

    /// The sections down the side of a wide sheet, with the way out at the
    /// head and the account at the foot, as the Mac's window has them.
    private var phoneSidebar: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(t("Settings")).font(.system(size: 21, weight: .bold))
                Spacer(minLength: 8)
                Button { dismiss() } label: {
                    Image(systemName: "xmark").font(.system(size: 13, weight: .bold))
                }
                .buttonStyle(.glassCircle(36))
                .accessibilityLabel(t("Close"))
            }
            .padding(.leading, 10).padding(.top, 14).padding(.bottom, 10)
            ScrollView {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(SettingsTopic.allCases) { one in
                        let chosen = (open ?? section) == one
                        Button {
                            withAnimation(Theme.settle) { open = one }
                        } label: {
                            HStack(spacing: 10) {
                                SectionTile(symbol: one.symbol, lit: chosen, size: 30)
                                Text(one.title)
                                    .font(.system(size: 15, weight: chosen ? .semibold : .regular))
                                    .foregroundStyle(chosen ? Theme.foreground : Theme.foreground.opacity(0.82))
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, 8).padding(.vertical, 7)
                            .background {
                                if chosen {
                                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                                        .fill(Color.white.opacity(0.11))
                                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.white.opacity(0.07)))
                                        .matchedGeometryEffect(id: "chosen", in: highlight)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(chosen ? .isSelected : [])
                    }
                }
            }
            .scrollIndicators(.never)
            if let account = app.account {
                HStack(spacing: 10) {
                    AccountAvatar(name: account.name, address: app.avatarAddress, size: 32)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(account.name).font(.system(size: 12.5, weight: .semibold)).lineLimit(1)
                        Text(account.trip).font(.system(size: 11, design: .monospaced)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.05)))
                .transition(Theme.arrive)
            }
        }
        .padding(10)
        .frame(maxHeight: .infinity, alignment: .top)
        .glassPane(cornerRadius: 22)
        .animation(Theme.settle, value: app.account?.name)
    }

    private var home: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text(t("Settings")).font(.system(size: 32, weight: .bold))
                    Spacer()
                    Button { dismiss() } label: {
                        Image(systemName: "xmark").font(.system(size: 13, weight: .bold))
                    }
                    .buttonStyle(.glassCircle(36))
                    .accessibilityLabel(t("Close"))
                }
                .padding(.top, 22)
                .rise()

                Button { withAnimation(Theme.settle) { open = .account } } label: {
                    HStack(spacing: 14) {
                        if let account = app.account {
                            AccountAvatar(name: account.name, address: app.avatarAddress, size: 54)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(account.name).font(.system(size: 19, weight: .semibold))
                                Text(account.trip).font(.system(size: 13, design: .monospaced)).foregroundStyle(Theme.muted)
                            }
                        } else {
                            Mark(size: 54)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(t("Account")).font(.system(size: 19, weight: .semibold))
                                Text(t("Sign in to change your passphrase or picture.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                        Spacer(minLength: 6)
                        chevron
                    }
                    .padding(16)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .glassPane(cornerRadius: 26)
                .rise(0.05)

                VStack(spacing: 0) {
                    let rows: [SettingsTopic] = [.general, .media, .watching, .music, .about]
                    ForEach(rows) { one in
                        Button { withAnimation(Theme.settle) { open = one } } label: {
                            HStack(spacing: 13) {
                                SectionTile(symbol: one.symbol, lit: true, size: 32)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(one.title).font(.system(size: 16, weight: .medium)).foregroundStyle(Theme.foreground)
                                    Text(summary(one)).font(.system(size: 12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                                }
                                Spacer(minLength: 6)
                                chevron
                            }
                            .padding(.horizontal, 14).padding(.vertical, 11)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        if one != rows.last {
                            Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1).padding(.leading, 59)
                        }
                    }
                }
                .glassPane(cornerRadius: 26)
                .rise(0.1)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 30)
        }
        .scrollIndicators(.never)
        .contentLayer()
    }

    private var chevron: some View {
        Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted.opacity(0.7))
    }

    /// What a section is set to now, under its name in the list.
    private func summary(_ one: SettingsTopic) -> String {
        switch one {
        case .general: localization.followsSystem ? t("Same as the system") : localization.language.name
        case .media: [app.startWithMicrophone ? t("Microphone") : nil, app.startWithCamera ? t("Camera") : nil]
            .compactMap { $0 }.joined(separator: " · ").nilIfEmpty ?? one.detail
        case .watching: watchLabel(preferences.watchQuality)
        case .music: preferences.musicQuality == "best" ? t("Highest") : t("Lossless")
        default: one.detail
        }
    }

    /// One section's page, with the way back to the list where there is a
    /// list to go back to; beside the sidebar of a wide sheet there is none.
    private func drilled(_ one: SettingsTopic, back: Bool, width: CGFloat) -> some View {
        ScrollView {
            ScrollViewReader { reader in
            VStack(alignment: .leading, spacing: 0) {
                if back {
                    Button { withAnimation(Theme.settle) { open = nil } } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "chevron.left").font(.system(size: 13, weight: .bold))
                            Text(t("Settings")).font(.system(size: 14, weight: .semibold)).fixedSize()
                        }
                    }
                    .buttonStyle(.glassCapsule(36))
                    .padding(.bottom, 18)
                    .transition(Theme.arrive)
                }
                page(one)
                    .contentLayer()
                Color.clear.frame(height: 1).id("end")
            }
            .padding(.top, 22)
            .animation(Theme.settle, value: back)
            // Held to the width it is given whatever a row inside would
            // like, so the page can never sit off its left edge: measured by
            // the sheet rather than the screen, since beside a sidebar the
            // screen's width put the page's right half outside the sheet.
            .frame(width: max(0, width - 32))
            .padding(.horizontal, 16)
            .padding(.bottom, 34)
            .onAppear { SettingsTopic.debugScroll(reader) }
            }
        }
        .scrollIndicators(.never)
        // Back with a swipe from the left edge, as every drill-in on the phone
        // goes back; beside a sidebar there is nothing to go back to.
        .simultaneousGesture(DragGesture(minimumDistance: 20).onEnded { drag in
            if back, drag.startLocation.x < 36, drag.translation.width > 80 {
                withAnimation(Theme.settle) { open = nil }
            }
        })
    }
    #endif

    // MARK: One page

    private func page(_ one: SettingsTopic) -> some View {
        VStack(alignment: .leading, spacing: 24) {
            SettingsPageHeading(title: one.title, symbol: one.symbol, detail: one.detail)
                .rise()
            Group {
                switch one {
                case .general: GeneralSettings()
                case .media: MediaSettings(app: app, preferences: preferences)
                case .watching: WatchingSettings(preferences: preferences)
                case .music: MusicSettings(app: app, preferences: preferences)
                case .account: AccountSettings(app: app)
                case .about: AboutSettings(app: app)
                }
            }
            .rise(0.05)
        }
    }
}

func watchLabel(_ quality: String) -> String {
    switch quality {
    case "1080p": t("1080p")
    case "720p": t("720p")
    default: t("Automatic")
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}

/// A row whose control goes under its words where the two will not fit side
/// by side, as on a phone held upright.
private struct AdaptiveRow<Control: View>: View {
    let title: String
    var detail: String?
    var symbol: String?
    var last = false
    @ViewBuilder var control: Control

    var body: some View {
        ViewThatFits(in: .horizontal) {
            SettingsRow(title: title, detail: detail, symbol: symbol, last: last) { control }
            VStack(alignment: .leading, spacing: 0) {
                SettingsRow(title: title, detail: detail, symbol: symbol, last: true) { EmptyView() }
                control
                    .padding(.leading, symbol == nil ? 14 : 52)
                    .padding(.trailing, 14)
                    .padding(.bottom, 12)
                if !last {
                    Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1).padding(.leading, symbol == nil ? 14 : 52)
                }
            }
        }
    }
}

// MARK: - General

private struct GeneralSettings: View {
    @State private var appearance = Appearance.shared
    @State private var localization = Localization.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            SettingsSection(title: t("Language")) {
                LanguageTiles(localization: localization).padding(12)
            }
            SettingsSection(title: t("Accent colour")) {
                VStack(spacing: 16) {
                    Swatches(appearance: appearance)
                    AccentPreview()
                }
                .padding(14)
            }
            SettingsSection(title: t("Connection badge")) {
                VStack(spacing: 14) {
                    BadgePreview(mode: appearance.connectionBadge)
                    SegmentedChoice(options: ConnectionBadge.allCases.map { ($0, $0.name) },
                                    selection: $appearance.connectionBadge, height: 32)
                }
                .padding(14)
            }
            SettingsSection(title: t("Controls")) {
                PlacementCards(appearance: appearance).padding(12)
            }
        }
    }
}

/// The languages as tiles, each in its own words, with the system's first.
private struct LanguageTiles: View {
    let localization: Localization

    var body: some View {
        VStack(spacing: 8) {
            tile(chosen: localization.followsSystem, symbol: "globe", title: t("Same as the system"),
                 detail: Language.detect(from: Locale.preferredLanguages).name) {
                localization.followSystem()
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], spacing: 8) {
                ForEach(Language.allCases) { one in
                    tile(chosen: !localization.followsSystem && localization.language == one, symbol: nil,
                         title: one.name, detail: one.rawValue) {
                        localization.language = one
                    }
                }
            }
        }
    }

    private func tile(chosen: Bool, symbol: String?, title: String, detail: String, pick: @escaping () -> Void) -> some View {
        Button {
            withAnimation(Theme.settle) { pick() }
        } label: {
            HStack(spacing: 10) {
                if let symbol {
                    Image(systemName: symbol).font(.system(size: 15, weight: .medium))
                        .foregroundStyle(chosen ? Theme.tally : Theme.muted)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(title).font(.system(size: 14, weight: chosen ? .semibold : .regular)).foregroundStyle(Theme.foreground)
                        .lineLimit(1)
                    Text(detail).font(.system(size: 11)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                Spacer(minLength: 4)
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 15))
                    .foregroundStyle(Theme.tally)
                    .opacity(chosen ? 1 : 0)
                    .scaleEffect(chosen ? 1 : 0.6)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: 50)
            .background(RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(chosen ? Theme.tally.opacity(0.13) : Color.white.opacity(0.05)))
            .overlay(RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(chosen ? Theme.tally.opacity(0.55) : Color.white.opacity(0.07), lineWidth: chosen ? 1.5 : 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(chosen ? .isSelected : [])
        .animation(Theme.settle, value: chosen)
    }
}

/// The colours, as flat discs with a hairline edge, the chosen one ringed
/// and ticked.
///
/// They were lit glass beads, a highlight at the upper left, a shade at the
/// foot and a glow of their own colour under each, until the owner asked
/// for fewer gradients (2026-10-20). The ring says which one is chosen; the
/// glow, which grew under the chosen one, said it twice. Android's swatches
/// (ui/settings/General.kt) are drawn the same.
private struct Swatches: View {
    let appearance: Appearance
    @State private var hovering: Accent?

    private static var columns: Int {
        #if os(iOS)
        5
        #else
        9
        #endif
    }

    var body: some View {
        // All nine in a row on the Mac; five and four on a phone, which
        // reads as two rows chosen rather than eight and one left over.
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: Self.columns), spacing: 12) {
            ForEach(Accent.allCases) { accent in
                let chosen = appearance.accent == accent
                Button {
                    withAnimation(Theme.settle) { appearance.accent = accent }
                } label: {
                    ZStack {
                        Circle()
                            .fill(accent.color)
                        Circle()
                            .stroke(Color.white.opacity(0.25), lineWidth: 0.5)
                        if chosen {
                            Image(systemName: "checkmark")
                                .font(.system(size: 13, weight: .heavy))
                                .foregroundStyle(accent.onColor)
                                .transition(.scale.combined(with: .opacity))
                        }
                    }
                    .frame(width: 34, height: 34)
                    .padding(4)
                    .overlay(Circle().stroke(accent.color, lineWidth: 2).opacity(chosen ? 1 : 0).scaleEffect(chosen ? 1 : 0.8))
                    .scaleEffect(hovering == accent && !chosen ? 1.08 : 1)
                    .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .onHover { hovering = $0 ? accent : (hovering == accent ? nil : hovering) }
                .accessibilityLabel(accent.name)
                .accessibilityAddTraits(chosen ? .isSelected : [])
                .help(accent.name)
                .animation(Theme.settle, value: chosen)
                .animation(.easeOut(duration: 0.15), value: hovering)
            }
        }
    }
}

/// What the colour is spent on, drawn small: somebody speaking, the button
/// that joins, a switch that is on, something unread.
private struct AccentPreview: View {
    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                scene
                Spacer(minLength: 8)
                name.padding(.trailing, 6)
            }
            VStack(alignment: .leading, spacing: 10) {
                name
                HStack(spacing: 12) {
                    scene
                    Spacer(minLength: 0)
                }
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Color.black.opacity(0.22)))
        .accessibilityHidden(true)
        .animation(Theme.settle, value: Appearance.shared.accent)
    }

    /// The colour's name, in itself.
    private var name: some View {
        Text(Appearance.shared.accent.name)
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle(Theme.tally)
            .lineLimit(1)
            .fixedSize()
            .contentTransition(.opacity)
    }

    private var scene: some View {
        HStack(spacing: 12) {
            ZStack(alignment: .bottomLeading) {
                // The grey of a camera-off tile, which is what it is a
                // picture of; it was a blue wash of its own.
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Theme.surface)
                Initial(name: "Mika", size: 30)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                HStack(spacing: 4) {
                    Image(systemName: "waveform").font(.system(size: 8, weight: .bold)).foregroundStyle(Theme.tally)
                    Text(verbatim: "Mika").font(.system(size: 9, weight: .semibold))
                }
                .padding(.horizontal, 6).padding(.vertical, 3)
                .background(Capsule().fill(Color.black.opacity(0.45)))
                .padding(6)
            }
            .frame(width: 104, height: 66)
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Theme.tally, lineWidth: 2))
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    Text(t("Join"))
                        .fixedSize()
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(Theme.tallyForeground)
                        .padding(.horizontal, 16).frame(height: 28)
                        .background(Capsule().fill(Theme.tally))
                    Toggle("", isOn: .constant(true)).labelsHidden().toggleStyle(.glassSwitch).allowsHitTesting(false)
                }
                HStack(spacing: 6) {
                    Image(systemName: "bubble.left.fill").font(.system(size: 14)).foregroundStyle(Theme.foreground.opacity(0.85))
                        .overlay(alignment: .topTrailing) {
                            Circle().fill(Theme.tally).frame(width: 7, height: 7).offset(x: 3, y: -2)
                        }
                    Image(systemName: "hand.raised.fill").font(.system(size: 14)).foregroundStyle(Theme.tally)
                }
            }
        }
        .fixedSize()
    }
}

/// The badge in the corner of a small picture of a call, as it will be.
private struct BadgePreview: View {
    let mode: ConnectionBadge

    var body: some View {
        ZStack(alignment: .topLeading) {
            Backdrop()
            HStack(spacing: 6) {
                ForEach(0..<3) { index in
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(Color.white.opacity(index == 1 ? 0.13 : 0.08))
                }
            }
            .padding(.horizontal, 14).padding(.top, 46).padding(.bottom, 14)
            ZStack(alignment: .topLeading) {
                switch mode {
                case .hidden:
                    Color.clear.frame(width: 1, height: 28)
                case .signal, .detailed:
                    SignalBadge(reading: .preview, detailed: mode == .detailed, open: .constant(false))
                        .transition(Theme.arrive)
                }
            }
            .padding(10)
        }
        .frame(height: 132)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Color.white.opacity(0.08)))
        .animation(Theme.settle, value: mode)
        .allowsHitTesting(false)
    }
}

/// The three places the controls can sit (controls.ts), each as a small
/// drawing of a call with them there. The three are as tall as the
/// tallest: a name that runs to two lines made its card taller than the
/// others.
private struct PlacementCards: View {
    let appearance: Appearance

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            ForEach(ControlsPlacement.allCases) { placement in
                let chosen = appearance.controlsPlacement == placement
                Button {
                    withAnimation(Theme.settle) { appearance.controlsPlacement = placement }
                } label: {
                    VStack(spacing: 9) {
                        Schematic(placement: placement, lit: chosen)
                            .aspectRatio(1.45, contentMode: .fit)
                        Text(placement.name)
                            .font(.system(size: 11.5, weight: chosen ? .semibold : .regular))
                            .foregroundStyle(chosen ? Theme.foreground : Theme.muted)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity)
                    }
                    .padding(8)
                    .frame(maxHeight: .infinity, alignment: .top)
                    .background(RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(chosen ? Theme.tally.opacity(0.12) : Color.white.opacity(0.04)))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .stroke(chosen ? Theme.tally.opacity(0.6) : Color.white.opacity(0.07), lineWidth: chosen ? 1.5 : 1))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chosen ? .isSelected : [])
                .animation(Theme.settle, value: chosen)
            }
        }
        // The row as tall as its tallest card, which the others then fill.
        .fixedSize(horizontal: false, vertical: true)
    }

    private struct Schematic: View {
        let placement: ControlsPlacement
        let lit: Bool

        var body: some View {
            GeometryReader { geometry in
                let size = geometry.size
                let side = placement == .side
                ZStack {
                    RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.black.opacity(0.35))
                    // Four pictures, narrower where the controls take a strip of width.
                    let width = size.width - (side ? 26 : 12)
                    let height = size.height - (side ? 12 : 24)
                    VStack(spacing: 3) {
                        HStack(spacing: 3) { tile; tile }
                        HStack(spacing: 3) { tile; tile }
                    }
                    .frame(width: width, height: height)
                    .position(x: 6 + width / 2, y: 6 + height / 2)
                    bar(size: size)
                }
            }
        }

        private var tile: some View {
            RoundedRectangle(cornerRadius: 3, style: .continuous).fill(Color.white.opacity(0.13))
        }

        @ViewBuilder
        private func bar(size: CGSize) -> some View {
            let colour = lit ? Theme.tally : Color.white.opacity(0.55)
            switch placement {
            case .always:
                Capsule().fill(colour).frame(width: size.width * 0.42, height: 7)
                    .position(x: size.width / 2, y: size.height - 9)
            case .idle:
                Capsule().strokeBorder(colour.opacity(0.8), style: StrokeStyle(lineWidth: 1.2, dash: [2.5, 2]))
                    .frame(width: size.width * 0.42, height: 7)
                    .position(x: size.width / 2, y: size.height - 9)
            case .side:
                Capsule().fill(colour).frame(width: 7, height: size.height * 0.5)
                    .position(x: size.width - 9, y: size.height / 2)
            }
        }
    }
}

extension View {
    /// The window's title, out of its toolbar where SwiftUI would put it.
    @ViewBuilder
    func removesWindowTitle() -> some View {
        #if os(macOS)
        if #available(macOS 15.0, *) {
            toolbar(removing: .title)
        } else {
            self
        }
        #else
        self
        #endif
    }

    /// The title bar's own ground, which the Settings window drew as a band
    /// across the top of the content column.
    @ViewBuilder
    func hidesToolbarBackground() -> some View {
        #if os(macOS)
        if #available(macOS 15.0, *) {
            toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
        } else {
            self
        }
        #else
        self
        #endif
    }

    /// The fade macOS 26 draws where a scroll view meets the title bar,
    /// which over the settings' backdrop read as a grey band across the
    /// top of the page.
    @ViewBuilder
    func hidesScrollEdge() -> some View {
        if #available(macOS 26.0, iOS 26.0, *) {
            scrollEdgeEffectHidden(true, for: .top)
        } else {
            self
        }
    }
}

extension ConnectionReading {
    /// A healthy reading, for showing what the badge looks like.
    static var preview: ConnectionReading {
        ConnectionReading(grade: .good, rttMs: 32, lossPercent: 0, upKbps: 820, downKbps: 2400, jitterMs: 4,
                          share: nil, ownAddress: nil, holding: nil, measured: true)
    }
}

// MARK: - Audio and video

private struct MediaSettings: View {
    @Bindable var app: AppModel
    let preferences: CallPreferences
    @State private var microphones: [AudioDevice] = []
    @State private var speakers: [AudioDevice] = []
    @State private var cameras: [AVCaptureDevice] = []
    @State private var check = DeviceCheck()
    /// The blur switch, read from what is remembered when the page opens.
    @State private var blurWanted = false
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            preview
            SettingsSection(title: t("When you join")) {
                SettingsRow(title: t("Start with the microphone on"), symbol: "mic.fill") {
                    Toggle("", isOn: $app.startWithMicrophone).labelsHidden().toggleStyle(.glassSwitch)
                }
                SettingsRow(title: t("Start with the camera on"), symbol: "video.fill") {
                    Toggle("", isOn: $app.startWithCamera).labelsHidden().toggleStyle(.glassSwitch)
                }
                SettingsRow(title: t("Original sound"), detail: t("No noise suppression or automatic volume. For headphones and music."),
                            symbol: "waveform", last: true) {
                    Toggle("", isOn: Binding(get: { preferences.originalSound }, set: { preferences.originalSound = $0 }))
                        .labelsHidden().toggleStyle(.glassSwitch)
                }
            }
            SettingsSection(title: t("Devices"),
                            footer: preferences.devices.isEmpty ? nil : t("Calls start on the microphone, camera and speakers you last picked in one, while they are plugged in.")) {
                AdaptiveRow(title: t("Camera"), symbol: "video") {
                    GlassChoice(id: "settings.camera", options: choices(cameras.map { ($0.uniqueID, $0.localizedName) }),
                                selection: binding("videoinput"), width: 280)
                }
                // The web's "Blur my background" tick, kept as the web keeps
                // it (`meet-live.blur`) and applied by the app's own blur from
                // the next call, and at once in a call that is on. It opened
                // the system's video effects, which the owner found did not
                // look like the app and which on an iPhone did not reach the
                // call (BackgroundBlur).
                SettingsRow(title: t("Blur my background"), symbol: "person.and.background.dotted") {
                    Toggle("", isOn: Binding(get: { blurWanted }, set: { on in
                        blurWanted = on
                        app.deviceMemory.remember(blur: on)
                        app.call?.setBlur(on)
                    }))
                    .labelsHidden()
                    .toggleStyle(.glassSwitch)
                }
                AdaptiveRow(title: t("Microphone"), symbol: "mic", last: speakers.isEmpty && preferences.devices.isEmpty) {
                    GlassChoice(id: "settings.microphone", options: choices(microphones.map { ($0.id, $0.name) }),
                                selection: binding("audioinput"), width: 280)
                }
                if !speakers.isEmpty {
                    AdaptiveRow(title: t("Speakers"), symbol: "speaker.wave.2", last: preferences.devices.isEmpty) {
                        GlassChoice(id: "settings.speakers", options: choices(speakers.map { ($0.id, $0.name) }),
                                    selection: binding("audiooutput"), width: 280)
                    }
                }
                if !preferences.devices.isEmpty {
                    HStack {
                        Spacer()
                        Button(t("Forget")) {
                            withAnimation(Theme.settle) { preferences.forgetDevices() }
                        }
                        .buttonStyle(.glassCapsule(30))
                    }
                    .padding(.horizontal, 14).padding(.vertical, 9)
                    .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: preferences.devices.isEmpty)
        }
        .onAppear {
            blurWanted = app.deviceMemory.blur
            cameras = CameraCapture.cameras()
            microphones = CallAudio.devices(input: true)
            speakers = CallAudio.devices(input: false)
            preferences.refresh()
            checkDevices()
        }
        .onDisappear { check.stop() }
        .onChange(of: preferences.devices) { checkDevices() }
        // Stopped when the app goes to the background, not only when the page
        // goes: on the phone Home leaves the sheet where it was, so the page
        // never disappears, and with the background audio mode nothing else
        // stops the meter's engine, which went on recording, the microphone
        // light on, for as long as the app was away. Back in front, the page
        // is still showing, and the devices open again.
        .onChange(of: scenePhase) {
            if scenePhase == .background {
                check.stop()
            } else if scenePhase == .active, !check.started {
                checkDevices()
            }
        }
    }

    private func checkDevices() {
        guard scenePhase != .background else { return }
        check.start(camera: preferences.devices["videoinput"], microphone: preferences.devices["audioinput"])
    }

    /// The camera as the room will see it, with how loudly the microphone
    /// hears you over its foot: the web's join screen answers exactly those
    /// two questions about the two devices (PreJoin.tsx, MicLevel.tsx), and
    /// this is where they are chosen ahead of any call.
    private var preview: some View {
        ZStack {
            // A camera-off tile's grey, as CameraStage's framed preview is.
            Theme.surface
            if check.cameraRunning {
                SelfView(session: check.session)
                    .transition(.opacity.animation(Theme.settle))
            } else {
                Image(systemName: check.cameraRefused ? "video.slash.fill" : "video.fill")
                    .font(.system(size: 28, weight: .medium))
                    .foregroundStyle(Theme.muted)
                    .transition(Theme.arrive)
            }
        }
        .aspectRatio(16 / 9, contentMode: .fit)
        .frame(maxWidth: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(Color.white.opacity(0.08)))
        .overlay(alignment: .bottomLeading) {
            HStack(spacing: 9) {
                Image(systemName: check.microphoneRefused ? "mic.slash.fill" : "mic.fill")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(check.level > 0.08 ? Theme.tally : Theme.foreground)
                Text(t("Hearing you")).font(.system(size: 12, weight: .medium))
                LevelMeter(level: check.level, segments: 12)
            }
            .padding(.horizontal, 12).frame(height: 32)
            .glassSurface(Capsule())
            .padding(12)
        }
        .animation(Theme.settle, value: check.cameraRunning)
    }

    private func choices(_ devices: [(String, String)]) -> [GlassChoice<String>.Option] {
        [GlassChoice<String>.Option(value: "", label: t("Same as the system"), symbol: "sparkles")]
            + devices.map { GlassChoice<String>.Option(value: $0.0, label: $0.1) }
    }

    private func binding(_ kind: String) -> Binding<String> {
        Binding(get: { preferences.devices[kind] ?? "" }, set: { chosen in
            preferences.choose(kind, chosen.isEmpty ? nil : chosen)
        })
    }
}

/// The camera and the microphone, opened only while the page that shows them
/// is open: a light that stays on after the page has gone would be a light
/// that lies.
@MainActor
@Observable
final class DeviceCheck {
    private(set) var level = 0.0
    private(set) var cameraRunning = false
    private(set) var cameraRefused = false
    private(set) var microphoneRefused = false
    @ObservationIgnored let session = AVCaptureSession()
    @ObservationIgnored private var engine: AVAudioEngine?
    @ObservationIgnored private let reading = SoundLevel()
    @ObservationIgnored private var ticking: Task<Void, Never>?
    @ObservationIgnored private var generation = 0
    /// Whether the devices are meant to be open: between `start` and `stop`.
    @ObservationIgnored private(set) var started = false

    func start(camera: String?, microphone: String?) {
        started = true
        Self.debugLog("started")
        generation += 1
        let mine = generation
        startCamera(camera, generation: mine)
        startMicrophone(microphone, generation: mine)
    }

    func stop() {
        started = false
        Self.debugLog("stopped")
        generation += 1
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
        cameraRunning = false
        stopMicrophone()
    }

    /// TOMOSHIBI_DEBUG_DEVICE_CHECK=log, in a debug build: says in the
    /// system log when the devices open and close, for a test of when they
    /// do on a phone it cannot look inside.
    private static func debugLog(_ what: String) {
        #if DEBUG
        guard ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_DEVICE_CHECK"] == "log" else { return }
        NSLog("tomoshibi device check: %@", what)
        #endif
    }

    // MARK: Camera

    private func startCamera(_ chosen: String?, generation mine: Int) {
        #if DEBUG
        // TOMOSHIBI_DEBUG_NO_CAMERA: the page photographed without the room
        // the machine happens to be in.
        if ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_NO_CAMERA"] != nil { return }
        #endif
        Task {
            guard await Self.allowed(.video) else {
                cameraRefused = true
                return
            }
            guard mine == generation else { return }
            // With nothing chosen, the camera a call would start on: the
            // system's default on a phone is the back camera, which this
            // page showed while every call sent the front one.
            let device = chosen.flatMap { id in CameraCapture.cameras().first { $0.uniqueID == id } }
                ?? CameraCapture.fallback()
            guard let device, let input = try? AVCaptureDeviceInput(device: device) else { return }
            let session = session
            session.beginConfiguration()
            for existing in session.inputs { session.removeInput(existing) }
            if session.canAddInput(input) { session.addInput(input) }
            session.commitConfiguration()
            guard mine == generation else { return }
            DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
            withAnimation(Theme.settle) { cameraRunning = true }
        }
    }

    // MARK: Microphone

    private func startMicrophone(_ chosen: String?, generation mine: Int) {
        stopMicrophone()
        Task { [weak self] in
            guard await Self.allowed(.audio) else {
                self?.microphoneRefused = true
                return
            }
            guard let self, mine == self.generation else { return }
            #if os(iOS)
            let audio = AVAudioSession.sharedInstance()
            try? audio.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers, .defaultToSpeaker, .allowBluetoothHFP])
            try? audio.setActive(true)
            if let chosen, let port = audio.availableInputs?.first(where: { $0.uid == chosen }) {
                try? audio.setPreferredInput(port)
            }
            #endif
            let engine = AVAudioEngine()
            let input = engine.inputNode
            #if os(macOS)
            if let chosen, let id = Self.deviceID(uid: chosen), let unit = input.audioUnit {
                var device = id
                AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                     &device, UInt32(MemoryLayout<AudioDeviceID>.size))
            }
            #endif
            let format = input.outputFormat(forBus: 0)
            guard format.sampleRate > 0, format.channelCount > 0 else { return }
            let reading = reading
            let rate = format.sampleRate
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
                guard let samples = buffer.floatChannelData?[0] else { return }
                reading.add(samples, frames: Int(buffer.frameLength), stride: buffer.stride, rate: rate)
            }
            do {
                try engine.start()
            } catch {
                input.removeTap(onBus: 0)
                return
            }
            // The page may have gone while permission was being asked.
            guard mine == self.generation else {
                input.removeTap(onBus: 0)
                engine.stop()
                return
            }
            self.engine = engine
            self.ticking = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(50))
                    guard let self else { return }
                    // MicLevel.tsx's curve: the square root, lifted, so a
                    // voice at an ordinary distance fills most of the meter.
                    let shown = min(1, self.reading.rms.squareRoot() * 2.4)
                    self.level = shown > self.level ? shown : self.level * 0.8 + shown * 0.2
                }
            }
        }
    }

    private func stopMicrophone() {
        ticking?.cancel()
        ticking = nil
        if let engine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
        engine = nil
        level = 0
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        #endif
    }

    private static func allowed(_ media: AVMediaType) async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: media) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: media)
        default: return false
        }
    }

    #if os(macOS)
    private static func deviceID(uid: String) -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyTranslateUIDToDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var cf = uid as CFString
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = withUnsafeMutablePointer(to: &cf) { pointer in
            AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address,
                                       UInt32(MemoryLayout<CFString>.size), pointer, &size, &id)
        }
        return status == noErr && id != 0 ? id : nil
    }
    #endif
}

// MARK: - Watching together

private struct WatchingSettings: View {
    let preferences: CallPreferences

    var body: some View {
        // Offered to everybody, whether or not they may start a showing: it is
        // about what this screen fetches, and everybody in a room is a viewer
        // (Account.tsx `WatchSettings`).
        VStack(alignment: .leading, spacing: 18) {
        WatchPreview(quality: preferences.watchQuality)
        SettingsSection(title: t("Watch together"),
                        footer: preferences.watchQuality == "auto" ? nil
                            : t("Where a video comes in several qualities, it is never fetched sharper than this here. It applies from the next video you open.")) {
            AdaptiveRow(title: t("Picture quality"), symbol: "sparkles.tv", last: true) {
                SegmentedChoice(options: CallPreferences.watchQualities.map { ($0, watchLabel($0)) },
                                selection: Binding(get: { preferences.watchQuality }, set: { preferences.watchQuality = $0 }),
                                height: 30)
                    .fixedSize()
            }
        }
        }
        .animation(Theme.settle, value: preferences.watchQuality)
    }
}

/// A film on the backdrop, wearing the ceiling chosen for it: drawn
/// softer the lower the ceiling, so the choice is seen as well as read.
private struct WatchPreview: View {
    let quality: String

    var body: some View {
        ZStack {
            FilmStill()
                .blur(radius: quality == "720p" ? 6 : quality == "1080p" ? 2.5 : 0)
            Image(systemName: "play.fill")
                .font(.system(size: 20, weight: .bold))
                .foregroundStyle(Theme.foreground)
                .frame(width: 54, height: 54)
                .glassSurface(Circle())
        }
        .aspectRatio(2.6, contentMode: .fit)
        .frame(maxWidth: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        .overlay(alignment: .bottomTrailing) {
            Chip(title: watchLabel(quality), symbol: "sparkles.tv", lit: quality != "auto")
                .padding(12)
                .contentTransition(.opacity)
        }
        .animation(Theme.settle, value: quality)
        .accessibilityHidden(true)
    }
}

/// The film WatchPreview softens: a still of flat shapes, the call's grey
/// with a lighter band along its foot and a pale disc high on the right.
/// It was the screens' glowing backdrop, blurred; once that went flat a
/// blur of it changed nothing that could be seen, and the ceiling's choice
/// was read but no longer seen. Shapes with edges show the softening without
/// a gradient. CallSettings.kt's FilmStill draws the same.
private struct FilmStill: View {
    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            ZStack {
                Theme.surface
                Theme.surfaceHigh
                    .frame(height: size.height * 0.4)
                    .frame(maxHeight: .infinity, alignment: .bottom)
                Circle()
                    .fill(Theme.foreground.opacity(0.18))
                    .frame(width: size.height * 0.28, height: size.height * 0.28)
                    .position(x: size.width * 0.75, y: size.height * 0.35)
            }
        }
    }
}

// MARK: - Music

private struct MusicSettings: View {
    let app: AppModel
    let preferences: CallPreferences

    var body: some View {
        // Lowering it under speech is the listener's choice, and somebody who
        // may not open the library still hears whatever another person plays,
        // so it is offered to everybody; the library's quality only to whoever
        // may fetch from it (Account.tsx `MusicSettings`).
        VStack(alignment: .leading, spacing: 18) {
        DuckPreview(ducks: preferences.musicDucks)
        SettingsSection(title: t("Music"),
                        footer: preferences.musicQuality == "best" && app.holds("music")
                            ? t("Hi-Res and masters are large, and take longer to start.") : nil) {
            SettingsRow(title: t("Lower the music while anybody talks"), symbol: "speaker.wave.1.fill", last: !app.holds("music")) {
                Toggle("", isOn: Binding(get: { preferences.musicDucks }, set: { preferences.musicDucks = $0 }))
                    .labelsHidden().toggleStyle(.glassSwitch)
            }
            if app.holds("music") {
                AdaptiveRow(title: t("Quality"), symbol: "waveform.path", last: true) {
                    SegmentedChoice(options: [("lossless", t("Lossless")), ("best", t("Highest"))],
                                    selection: Binding(get: { preferences.musicQuality }, set: { preferences.musicQuality = $0 }),
                                    height: 30)
                        .fixedSize()
                }
                .transition(Theme.arrive)
            }
        }
        }
        .animation(Theme.settle, value: preferences.musicQuality)
        .animation(Theme.settle, value: preferences.musicDucks)
    }
}

/// The music as a row of bars and a voice under it: where somebody speaks
/// the music sinks, if it is asked to (duck.ts), and stays put if not.
private struct DuckPreview: View {
    let ducks: Bool
    private let count = 44

    var body: some View {
        VStack(spacing: 10) {
            row(symbol: "music.note") { index in
                let base = 0.45 + 0.4 * abs(sin(Double(index) * 0.55)) + 0.15 * abs(sin(Double(index) * 1.7))
                return speaking(index) && ducks ? base * 0.32 : base
            } colour: { _ in Theme.tally }
            row(symbol: "person.wave.2.fill") { index in
                speaking(index) ? 0.3 + 0.6 * abs(sin(Double(index) * 1.3)) : 0.05
            } colour: { index in speaking(index) ? Theme.foreground.opacity(0.85) : Color.white.opacity(0.18) }
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Color.white.opacity(0.08)))
        .accessibilityHidden(true)
    }

    private func speaking(_ index: Int) -> Bool { index >= count * 2 / 5 && index < count * 4 / 5 }

    private func row(symbol: String, height: @escaping (Int) -> Double, colour: @escaping (Int) -> Color) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted).frame(width: 20)
            HStack(alignment: .center, spacing: 3) {
                ForEach(0..<count, id: \.self) { index in
                    Capsule().fill(colour(index))
                        .frame(maxWidth: .infinity)
                        .frame(height: max(3, 34 * height(index)))
                }
            }
            .frame(height: 36)
        }
    }
}

// MARK: - Account

private struct AccountSettings: View {
    @Bindable var app: AppModel
    @State private var current = ""
    @State private var next = ""
    @State private var changing = false
    @State private var changed = false
    @State private var picturing = false
    @State private var said: String?
    #if os(iOS)
    @State private var photo: PhotosPickerItem?
    #endif

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            if let account = app.account {
                profile(account)
                abilities(account)
                passphrase
                if app.holds("invite"), account.admin || !account.scopes.isEmpty || debugGuestLinks {
                    GuestLinksCard(app: app, account: account)
                }
            } else {
                Text(t("Sign in to change your passphrase or picture."))
                    .font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let said {
                Text(said).font(.system(size: 12.5)).foregroundStyle(Theme.danger)
                    .transition(Theme.arrive)
                    .onTapGesture { self.said = nil }
            }
        }
        .animation(Theme.settle, value: said)
        .animation(Theme.settle, value: app.account?.avatar)
    }

    /// TOMOSHIBI_DEBUG_GUEST_LINKS, in a rehearsal: the guest links card for
    /// an account with no scope of its own, for a test to work its controls.
    private var debugGuestLinks: Bool { Self.debugGuestLinks && app.rehearsal }
    private static var debugGuestLinks: Bool {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_GUEST_LINKS"] != nil
        #else
        false
        #endif
    }

    // MARK: Who

    private func profile(_ account: Account) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: 16) {
                ZStack(alignment: .bottomTrailing) {
                    AccountAvatar(name: account.name, address: app.avatarAddress, size: 72)
                        .shadow(color: .black.opacity(0.35), radius: 10, y: 4)
                    choosePicture {
                        Image(systemName: picturing ? "hourglass" : "camera.fill").font(.system(size: 11, weight: .bold))
                    }
                    .buttonStyle(.glassCircle(28, prominent: true))
                    .offset(x: 4, y: 4)
                    .accessibilityLabel(t("Choose a picture"))
                }
                VStack(alignment: .leading, spacing: 4) {
                    Text(account.name).font(.system(size: 21, weight: .bold)).lineLimit(1)
                    // The signature: the public half of the account, and what
                    // proves the name in a room; this is the one page where
                    // somebody has a reason to read their own (Account.tsx).
                    Text(account.trip).font(.system(size: 12, design: .monospaced)).foregroundStyle(Theme.muted)
                        .textSelection(.enabled)
                }
                Spacer(minLength: 0)
            }
            .padding(16)
            Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
            VStack(alignment: .leading, spacing: 10) {
                Text(t("Shown beside your name in a call. Scaled down before it is sent."))
                    .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
                // Wrapping rather than in one row, which on a phone held
                // upright was wider than the screen.
                FlowRow(spacing: 8) {
                    choosePicture {
                        ProfileButtonLabel(title: t("Choose a picture"), symbol: "photo")
                    }
                    .buttonStyle(.glassCapsule(32))
                    .fixedSize()
                    if account.avatar != nil {
                        Button {
                            Task { await run { await app.removePicture() } }
                        } label: {
                            ProfileButtonLabel(title: t("Remove"), symbol: "trash")
                        }
                        .buttonStyle(.glassCapsule(32))
                        .fixedSize()
                        .transition(Theme.arrive)
                    }
                    Button {
                        withAnimation(Theme.settle) { app.signOutAsked = .settings }
                    } label: {
                        ProfileButtonLabel(title: t("Sign out"), symbol: "rectangle.portrait.and.arrow.right")
                    }
                    .buttonStyle(.glassCapsule(32))
                    .fixedSize()
                }
                .labelStyle(.titleAndIcon)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(14)
        }
        .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Color.white.opacity(0.05)))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(Color.white.opacity(0.07)))
        .disabled(picturing)
    }

    /// The way to a picture: the Mac's open panel, the phone's photo library.
    @ViewBuilder
    private func choosePicture<Label: View>(@ViewBuilder label: () -> Label) -> some View {
        #if os(macOS)
        Button {
            let panel = NSOpenPanel()
            panel.allowedContentTypes = [.image]
            panel.allowsMultipleSelection = false
            guard panel.runModal() == .OK, let url = panel.url, let data = try? Data(contentsOf: url) else { return }
            send(data)
        } label: { label() }
        #else
        PhotosPicker(selection: $photo, matching: .images) { label() }
            .onChange(of: photo) {
                guard let photo else { return }
                Task {
                    if let data = try? await photo.loadTransferable(type: Data.self) { send(data) }
                    self.photo = nil
                }
            }
        #endif
    }

    private func send(_ data: Data) {
        guard let picture = PictureShrinker.dataURL(from: data) else {
            said = t("That picture could not be used.")
            return
        }
        Task { await run { await app.setPicture(picture) } }
    }

    private func run(_ work: () async -> String?) async {
        picturing = true
        said = await work()
        picturing = false
    }

    // MARK: What they may use

    private func abilities(_ account: Account) -> some View {
        let permissions = ["watch", "emby", "music", "arrange", "invite", "open"]
        let held = permissions.filter { app.holds($0) }
        // Emby is left off what was withheld: it is given one at a time and
        // spends the deployment's money, and a list of it to everybody without
        // it would be an advertisement for asking (Account.tsx `Abilities`).
        let withheld = permissions.filter { $0 != "emby" && !app.holds($0) }
        return SettingsSection(title: t("What you can use"), footer: t("Set by whoever runs this deployment.")) {
            VStack(alignment: .leading, spacing: 12) {
                if !held.isEmpty {
                    FlowRow(spacing: 6) {
                        ForEach(held, id: \.self) { one in
                            Chip(title: Self.permissionName(one), symbol: Self.permissionSymbol(one), lit: one == "emby")
                        }
                    }
                }
                if !withheld.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(t("Not opened to this account")).font(.system(size: 11)).foregroundStyle(Theme.muted)
                        FlowRow(spacing: 6) {
                            ForEach(withheld, id: \.self) { one in
                                Chip(title: Self.permissionName(one), symbol: Self.permissionSymbol(one), withheld: true)
                            }
                        }
                    }
                }
                if !account.scopes.isEmpty || account.created != nil {
                    Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
                    Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 14, verticalSpacing: 9) {
                        if !account.scopes.isEmpty {
                            GridRow {
                                Text(t("Your scopes")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                                FlowRow(spacing: 5) {
                                    ForEach(account.scopes, id: \.self) { scope in Chip(title: "@" + scope, mono: true) }
                                }
                            }
                        }
                        if let created = account.created.flatMap(AppModel.instant) {
                            GridRow {
                                Text(t("Account created")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                                Text(created.formatted(date: .long, time: .omitted)).font(.system(size: 12.5))
                            }
                        }
                    }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// manage/grants.ts `PERMISSION_NAMES`: the words the management pages
    /// tick, so "an administrator gave me Music" and this say the same word.
    static func permissionName(_ one: String) -> String {
        switch one {
        case "watch": t("Watch together")
        case "emby": t("Emby library")
        case "music": t("Music")
        case "arrange": t("Arrange meetings")
        case "invite": t("Guest links")
        default: t("Open new rooms")
        }
    }

    /// The symbol of the button each opens in a call (Account.tsx
    /// `PERMISSION_ICONS`).
    static func permissionSymbol(_ one: String) -> String {
        switch one {
        case "watch": "play.rectangle"
        case "emby": "film"
        case "music": "music.note"
        case "arrange": "calendar.badge.clock"
        case "invite": "link"
        default: "door.left.hand.open"
        }
    }

    // MARK: Where


    // MARK: The passphrase

    private var passphrase: some View {
        SettingsSection(title: t("Passphrase"),
                        footer: t("Your passphrase is not stored here and cannot be sent to you. If you lose it, an administrator sets a new one.")) {
            VStack(alignment: .leading, spacing: 10) {
                Text(t("This is how you are recognised, in a call and here. At least eight characters."))
                    .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
                GlassField(prompt: t("Current passphrase"), text: $current, symbol: "key", lights: true, secret: true, height: 40)
                GlassField(prompt: t("New passphrase"), text: $next, symbol: "key.fill", lights: true, secret: true, height: 40,
                           onSubmit: change)
                HStack(spacing: 10) {
                    Button(action: change) {
                        Group {
                            if changing { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Text(t("Change it")) }
                        }
                    }
                    .buttonStyle(.glassCapsule(32, prominent: true))
                    .disabled(changing || current.isEmpty || next.count < 8)
                    if changed {
                        Label(t("Changed."), systemImage: "checkmark.circle.fill")
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.good)
                            .transition(Theme.arrive)
                    }
                }
            }
            .padding(14)
        }
        .animation(Theme.settle, value: changed)
    }

    private func change() {
        guard !changing, !current.isEmpty, next.count >= 8 else { return }
        changing = true
        changed = false
        Task {
            let refused = await app.changePassphrase(current: current, next: next)
            changing = false
            if let refused {
                said = refused
            } else {
                current = ""
                next = ""
                said = nil
                changed = true
            }
        }
    }
}

/// Account.tsx `GuestLinks`: a link that lets somebody outside a scope into
/// one of its rooms, for a name, between two moments either of which may be
/// left open; and the way to stop every link to a room, which outlives the
/// meeting it was made for.
private struct GuestLinksCard: View {
    @Bindable var app: AppModel
    let account: Account
    @State private var typed = ""
    @State private var scope = ""
    @State private var name = ""
    @State private var from: Date?
    @State private var until: Date?
    @State private var busy: String?
    @State private var made: MadeInvite?
    @State private var said: String?
    @State private var saidIsGood = false

    /// The room as the server will read it. An @ typed into the room half is
    /// folded away rather than allowed to start a second scope: the scope is
    /// chosen beside it.
    private var full: String { normaliseRoomName(typed: typed.replacingOccurrences(of: "@", with: "-")) + "@" + scope }
    private var usable: Bool { validRoomName(name: full) && validScope(scope: scope) }

    var body: some View {
        SettingsSection(title: t("Guest links"),
                        footer: t("A link lets somebody outside your scope into one of its rooms as a guest. Leave the end empty for a link that works until it is stopped.")) {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    GlassField(prompt: t("Room name"), text: $typed, symbol: "number", mono: !typed.isEmpty, height: 40)
                    Text(verbatim: "@").font(.system(size: 15, design: .monospaced)).foregroundStyle(Theme.muted)
                    scopeControl
                }
                GlassField(prompt: t("Their name, if the link is for one person"), text: $name, symbol: "person", height: 40)
                // Side by side where both fit, one above the other on a phone,
                // where a chosen moment is wider than half the card.
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .top, spacing: 12) {
                        moment(t("Works from"), id: "guest.from", value: $from)
                        moment(t("Works until"), id: "guest.until", value: $until)
                    }
                    VStack(alignment: .leading, spacing: 10) {
                        moment(t("Works from"), id: "guest.from", value: $from)
                        moment(t("Works until"), id: "guest.until", value: $until)
                    }
                }
                FlowRow(spacing: 8) {
                    Button(action: make) {
                        HStack(spacing: 6) {
                            if busy == "make" { ProgressView().controlSize(.small).tint(Theme.tallyForeground) } else { Image(systemName: "link") }
                            Text(t("Make a link"))
                        }
                    }
                    .buttonStyle(.glassCapsule(34, prominent: true))
                    .fixedSize()
                    .disabled(busy != nil || !usable)
                    // A quieter button than the one that makes, and allowed a
                    // second line: it is a long sentence, and on a phone the
                    // capsule cut it off halfway through.
                    Button(action: stop) {
                        HStack(spacing: 6) {
                            if busy == "stop" { ProgressView().controlSize(.small) } else { Image(systemName: "xmark.circle") }
                            Text(t("Stop every link to this room working"))
                                .multilineTextAlignment(.leading)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.foreground.opacity(0.85))
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .frame(minHeight: 34)
                        .background(Capsule().fill(Color.white.opacity(0.06)))
                        .overlay(Capsule().stroke(Color.white.opacity(0.10)))
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .opacity(busy != nil || !usable ? 0.45 : 1)
                    .disabled(busy != nil || !usable)
                }
                if let made {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(alignment: .top, spacing: 8) {
                            Text(made.link)
                                .font(.system(size: 11.5, design: .monospaced))
                                .foregroundStyle(Theme.foreground.opacity(0.85))
                                .textSelection(.enabled)
                                .padding(10)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.black.opacity(0.25)))
                            CopyLinkButton(link: made.link)
                        }
                        Text(describe(made)).font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .transition(Theme.arrive)
                }
                if let said {
                    Text(said).font(.system(size: 12)).foregroundStyle(saidIsGood ? Theme.good : Theme.danger)
                        .transition(Theme.arrive)
                }
            }
            .padding(14)
        }
        .animation(Theme.settle, value: made?.token)
        .animation(Theme.settle, value: said)
        .onAppear { if scope.isEmpty { scope = account.scopes.first ?? "" } }
    }

    /// Typed by an administrator, who belongs to every scope; chosen by
    /// anybody else from the ones on their account, and simply named where
    /// there is only one.
    @ViewBuilder
    private var scopeControl: some View {
        if account.admin {
            GlassField(prompt: t("Scope"), text: $scope, mono: true, height: 40)
                .frame(width: 140)
                .onChange(of: scope) { scope = scope.lowercased().trimmingCharacters(in: .whitespaces) }
        } else if account.scopes.count > 1 {
            GlassChoice(id: "guest.scope", options: account.scopes.map { .init(value: $0, label: $0) }, selection: $scope, height: 34)
        } else {
            Text(scope).font(.system(size: 14, design: .monospaced)).foregroundStyle(Theme.foreground)
        }
    }

    private func moment(_ label: String, id: String, value: Binding<Date?>) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label).font(.system(size: 11)).foregroundStyle(Theme.muted).fixedSize()
            MomentChoice(id: id, moment: value).fixedSize()
        }
    }

    private func describe(_ made: MadeInvite) -> String {
        let at = { (text: String) in AppModel.instant(text).map(LobbyModel.said) ?? text }
        var line: String
        if let until = made.until {
            line = made.from.map { t("This link works from {from} until {until}.", ["from": at($0), "until": at(until)]) }
                ?? t("This link works until {until}.", ["until": at(until)])
        } else {
            line = made.from.map { t("This link works from {from} until somebody stops it.", ["from": at($0)]) }
                ?? t("This link works until somebody stops it.")
        }
        if let name = made.name, !name.isEmpty { line += " " + t("Whoever uses it joins as {name}.", ["name": name]) }
        return line
    }

    private func make() {
        guard busy == nil, usable else { return }
        busy = "make"
        said = nil
        Task {
            switch await app.makeGuestLink(room: full, name: name, from: from, until: until) {
            case let .success(link):
                made = link
                // Copied as well as shown, as the panel in a call does:
                // whoever made it is about to paste it somewhere.
                CopiedText.put(link.link)
            case let .failure(refused):
                saidIsGood = false
                said = refused.sentence
            }
            busy = nil
        }
    }

    private func stop() {
        guard busy == nil, usable else { return }
        busy = "stop"
        said = nil
        Task {
            switch await app.stopGuestLinks(room: full) {
            case let .success(count):
                made = nil
                saidIsGood = true
                said = count == 1 ? t("One link stopped working.") : t("{count} links stopped working.", ["count": String(count)])
            case let .failure(refused):
                saidIsGood = false
                said = refused.sentence
            }
            busy = nil
        }
    }
}

/// A symbol and words, measured whole: the system's label, drawn in the
/// glass capsule, was measured short and cut "Remove" to "Remo…".
private struct ProfileButtonLabel: View {
    let title: String
    let symbol: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: symbol)
            Text(title).fixedSize()
        }
        .font(.system(size: 12.5, weight: .semibold))
    }
}

/// Account.tsx `shrink`: 256 pixels square, cut from the middle, as JPEG at
/// 0.82. A photograph off a phone is megabytes of something drawn at
/// forty-eight pixels, and the server refuses one past its limit anyway.
enum PictureShrinker {
    static func dataURL(from data: Data) -> String? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
        else { return nil }
        let side = min(image.width, image.height)
        let crop = CGRect(x: (image.width - side) / 2, y: (image.height - side) / 2, width: side, height: side)
        guard let square = image.cropping(to: crop),
              let context = CGContext(data: nil, width: 256, height: 256, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        context.interpolationQuality = .high
        context.draw(square, in: CGRect(x: 0, y: 0, width: 256, height: 256))
        guard let scaled = context.makeImage() else { return nil }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, scaled, [kCGImageDestinationLossyCompressionQuality: 0.82] as CFDictionary)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return "data:image/jpeg;base64," + (out as Data).base64EncodedString()
    }
}

// MARK: - About

private struct AboutSettings: View {
    let app: AppModel
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 18) {
                Mark(size: 76).shadow(color: .black.opacity(0.4), radius: 14, y: 6)
                VStack(alignment: .leading, spacing: 4) {
                    Text(verbatim: "Tomoshibi").font(.system(size: 26, weight: .bold))
                    Text(version).font(.system(size: 13, design: .monospaced)).foregroundStyle(Theme.muted)
                }
            }
            .padding(.vertical, 4)
            SettingsSection(title: t("About")) {
                SettingsRow(title: t("Version"), symbol: "app.badge") {
                    Text(version).font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Theme.muted)
                }
                // Only where the app looks for updates at all: the Mac, and
                // not a debug build started by hand, which would ask the
                // real deployment.
                if let updates = app.updates { UpdateRow(updates: updates) }
                SettingsRow(title: t("Media core"), symbol: "cpu", last: app.info == nil) {
                    Text(coreVersion()).font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Theme.muted)
                }
                if let info = app.info {
                    // The server's build and not its address: which server this is
                    // is not something the app says anywhere (AppModel `home`).
                    SettingsRow(title: t("Server"), symbol: "server.rack", last: info.source.isEmpty) {
                        Text(info.build.isEmpty ? "—" : String(info.build.prefix(12)))
                            .font(.system(size: 12.5, design: .monospaced)).foregroundStyle(Theme.muted)
                    }
                    // Where the code running there can be read: an obligation
                    // under the AGPL's thirteenth section, which only a page
                    // somebody uses can discharge, and from the server because
                    // a deployment running a changed copy owes that copy. Not
                    // translated, as on the web: it is the licence's name.
                    if let url = URL(string: info.source), !info.source.isEmpty {
                        Button { openURL(url) } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "chevron.left.forwardslash.chevron.right")
                                    .font(.system(size: 12.5, weight: .semibold))
                                    .frame(width: 26, height: 26)
                                    .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Color.white.opacity(0.10)))
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: "AGPL-3.0 · source").font(.system(size: 13.5))
                                    Text(info.source).font(.system(size: 11.5)).foregroundStyle(Theme.muted).lineLimit(1)
                                }
                                Spacer(minLength: 10)
                                Image(systemName: "arrow.up.right").font(.system(size: 11.5, weight: .bold)).foregroundStyle(Theme.muted)
                            }
                            .padding(.horizontal, 14).padding(.vertical, 9)
                            .frame(minHeight: 48)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    /// The version as people are told it, a later build in brackets: 0.1.4,
    /// 0.1.4 (2) (ClientDescription, versionLabel).
    private var version: String {
        versionLabel(version: ClientDescription.version)
    }

}

/// Where the app's updates stand, and the check somebody can ask for
/// (About.kt `UpdateRow`): the version on offer with "Update", a download's
/// progress with "Cancel", and the answer to a check, which says so even when
/// there is nothing newer. What it says changes with a fade.
private struct UpdateRow: View {
    let updates: Updates

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12.5, weight: .semibold))
                    .frame(width: 26, height: 26)
                    .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Color.white.opacity(0.10)))
                VStack(alignment: .leading, spacing: 2) {
                    Text(t("Updates")).font(.system(size: 13.5))
                    if let line {
                        Text(line)
                            .font(.system(size: 11.5))
                            .foregroundStyle(problem ? Theme.danger : Theme.muted)
                            .fixedSize(horizontal: false, vertical: true)
                            .id(line)
                            .transition(.asymmetric(insertion: .opacity.animation(Theme.settle), removal: .opacity.animation(Theme.leave)))
                    }
                }
                .animation(Theme.settle, value: line)
                Spacer(minLength: 10)
                action
                    .transition(.asymmetric(insertion: .opacity.animation(Theme.settle), removal: .opacity.animation(Theme.leave)))
            }
            .padding(.horizontal, 14).padding(.vertical, 9)
            .frame(minHeight: 48)
            Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1).padding(.leading, 52)
        }
        .accessibilityIdentifier("settings.updates")
    }

    private var problem: Bool {
        if case .offered(_, problem: .some) = updates.step { return true }
        return false
    }

    private var line: String? {
        switch updates.step {
        case let .downloading(_, received, total), let .held(_, received, total):
            return t("{received} of {total}", ["received": megabytes(received), "total": megabytes(total)])
        case let .installing(offer):
            return t("Installing Tomoshibi {version}", ["version": offer.label])
        case let .misplaced(_, place):
            return UpdateBanner.explain(place)
        case let .offered(offer, problem):
            return problem ?? t("Version {version} is available.", ["version": offer.label])
        case .none:
            switch updates.said {
            case .checking: return t("Checking for updates…")
            case .current: return t("You have the latest version.")
            case .failed: return t("Could not check for updates. Try again later.")
            case nil: return nil
            }
        }
    }

    @ViewBuilder
    private var action: some View {
        switch updates.step {
        case .offered:
            PillButton(title: t("Update"), role: .prominent) { updates.accept() }
                .accessibilityIdentifier("settings.update")
        case .downloading:
            PillButton(title: t("Cancel")) { updates.cancel() }
                .accessibilityIdentifier("settings.cancelUpdate")
        default:
            PillButton(title: t("Check for updates"), busy: updates.said == .checking) { updates.checkNow() }
                .disabled(updates.step != .none || updates.said == .checking)
                .accessibilityIdentifier("settings.checkUpdates")
        }
    }
}

// MARK: - The Mac's window

#if os(macOS)
/// The Settings window with its title bar gone, so the glass sidebar runs to
/// the top with the window's buttons on it, as the lobby's does.
///
/// On it, not across its edge. With no toolbar the title bar was 28 points
/// tall and the buttons sat at its very top, over the line where the sidebar's
/// glass begins ten points down and on the curve of its corner: the owner
/// found it untidy (2026-10-17). An empty unified toolbar makes the bar 52
/// points tall with the buttons centred in it, which puts them inside the
/// glass with room on every side, as a macOS 26 sidebar has them (Finder,
/// Mail). The title was also seen back over the sidebar now and then, so it
/// is taken out of the toolbar as well (`toolbar(removing: .title)`), and the
/// window is styled again whenever it becomes key.
/// TOMOSHIBI_DEBUG_SETTINGS_WINDOW=<width>x<height> sizes it, in a debug
/// build, for a test to look at it at more than one size.
private struct SettingsWindowStyle: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { Styler() }

    func updateNSView(_ view: NSView, context: Context) {
        (view as? Styler)?.style()
    }

    /// Styles the window when it is placed in one, and again a moment
    /// later: SwiftUI writes the window's title after the first pass, and
    /// "Tomoshibi Settings" came back over the sidebar on some launches.
    final class Styler: NSView {
        private var keyWatch: NSObjectProtocol?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            style()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in self?.style() }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in self?.style() }
            if let keyWatch { NotificationCenter.default.removeObserver(keyWatch) }
            keyWatch = window.map { window in
                NotificationCenter.default.addObserver(forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main) { [weak self] _ in
                    MainActor.assumeIsolated { self?.style() }
                }
            }
        }

        private static let toolbarID = NSToolbar.Identifier("app.tomoshibi.settings")

        func style() {
            guard let window else { return }
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.styleMask.insert(.fullSizeContentView)
            window.isMovableByWindowBackground = true
            window.titlebarSeparatorStyle = .none
            if window.toolbar?.identifier != Self.toolbarID {
                let bar = NSToolbar(identifier: Self.toolbarID)
                bar.allowsUserCustomization = false
                bar.displayMode = .iconOnly
                window.toolbar = bar
            }
            window.toolbarStyle = .unified
            #if DEBUG
            if !sized, let size = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_SETTINGS_WINDOW"]?
                .split(separator: "x").compactMap({ Double($0) }), size.count == 2 {
                sized = true
                window.setContentSize(NSSize(width: size[0], height: size[1]))
            }
            #endif
        }

        private var sized = false
    }
}
#endif
