import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The settings: the colour the interface spends, the language, how the
/// connection is shown in a call, and the account. On the Mac the app's own
/// Settings window (⌘,), on the phone a sheet from the home's account button.
public struct SettingsView: View {
    @Bindable var app: AppModel
    @State private var appearance = Appearance.shared
    @State private var localization = Localization.shared
    @Environment(\.dismiss) private var dismiss

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        Form {
            Section(t("Appearance")) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(t("Accent colour")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 9), spacing: 10) {
                        ForEach(Accent.allCases) { accent in
                            Button {
                                withAnimation(Theme.settle) { appearance.accent = accent }
                            } label: {
                                Circle()
                                    .fill(accent.color)
                                    .frame(width: 26, height: 26)
                                    .overlay {
                                        if appearance.accent == accent {
                                            Image(systemName: "checkmark").font(.system(size: 11, weight: .heavy)).foregroundStyle(accent.onColor)
                                                .transition(.scale.combined(with: .opacity))
                                        }
                                    }
                                    .overlay(Circle().stroke(Color.white.opacity(appearance.accent == accent ? 0.9 : 0.15), lineWidth: appearance.accent == accent ? 2 : 1).padding(-3))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(accent.name)
                            .accessibilityAddTraits(appearance.accent == accent ? .isSelected : [])
                            .help(accent.name)
                        }
                    }
                    Text(appearance.accent.name).font(.system(size: 12)).foregroundStyle(Theme.muted)
                }
                .padding(.vertical, 4)
            }

            Section(t("Language")) {
                Picker(t("Language"), selection: Binding(
                    get: { localization.followsSystem ? nil : localization.language },
                    set: { chosen in
                        if let chosen { localization.language = chosen } else { localization.followSystem() }
                    }
                )) {
                    Text(t("Same as the system")).tag(Language?.none)
                    ForEach(Language.allCases) { language in
                        Text(language.name).tag(Language?.some(language))
                    }
                }
            }

            Section(t("Call")) {
                Picker(t("Connection badge"), selection: $appearance.connectionBadge) {
                    ForEach(ConnectionBadge.allCases) { mode in
                        Text(mode.name).tag(mode)
                    }
                }
                ConnectionBadgePreview(mode: appearance.connectionBadge)
            }

            if let account = app.account {
                Section(t("User console")) {
                    HStack(spacing: 12) {
                        Initial(name: account.name, size: 34)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(account.name).font(.system(size: 14, weight: .semibold))
                            Text(app.deployment.flatMap { URL(string: $0.baseUrl())?.host() } ?? "").font(.system(size: 12)).foregroundStyle(Theme.muted)
                        }
                    }
                    if let url = page("/account") { Link(t("User console"), destination: url) }
                    if account.admin, let url = page("/admin") { Link(t("Admin console"), destination: url) }
                    Button(t("Change server")) {
                        app.forgetServer()
                        dismiss()
                    }
                    Button(t("Sign out"), role: .destructive) {
                        Task { await app.signOut() }
                        dismiss()
                    }
                }
            }
        }
        .formStyle(.grouped)
        .tint(Theme.tally)
        #if os(macOS)
        .frame(width: 460)
        .frame(minHeight: 420)
        #else
        .navigationTitle(t("Settings"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button(t("Close")) { dismiss() }
            }
        }
        #endif
        .preferredColorScheme(.dark)
        .environment(\.locale, localization.language.locale)
    }

    private func page(_ path: String) -> URL? {
        app.deployment.flatMap { URL(string: $0.baseUrl().trimmingCharacters(in: CharacterSet(charactersIn: "/")) + path) }
    }
}

/// What the chosen badge will look like in a call.
private struct ConnectionBadgePreview: View {
    let mode: ConnectionBadge

    var body: some View {
        HStack {
            Spacer()
            ZStack {
                switch mode {
                case .hidden:
                    Text(t("Hidden")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                case .signal, .detailed:
                    SignalBadge(reading: .preview, detailed: mode == .detailed, open: .constant(false))
                }
            }
            .frame(height: 40)
            .animation(Theme.settle, value: mode)
            Spacer()
        }
        .padding(.vertical, 6)
        .background(Backdrop().clipShape(RoundedRectangle(cornerRadius: 10)))
        .listRowInsets(EdgeInsets())
    }
}

extension ConnectionReading {
    /// A healthy reading, for showing what the badge looks like.
    static var preview: ConnectionReading {
        ConnectionReading(grade: .good, rttMs: 32, lossPercent: 0, upKbps: 820, downKbps: 2400, jitterMs: 4,
                          share: nil, ownAddress: nil, holding: nil, measured: true)
    }
}
