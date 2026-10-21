import SwiftUI
import TomoshibiBasics

/// A newer build, said at the top of the lobby without stopping anything
/// (UpdateBanner.kt on Android): what it is and what changed, "Update" and
/// "Later"; then how far the download has got, with a way to stop it; then,
/// where this copy cannot replace itself, why and what to do; then a moment
/// of "Installing" before the new copy opens, which the window over the whole
/// app says rather than the banner (UpdateInstalling).
///
/// Shown only while the lobby is the screen (Updates `shown`), so that no
/// call is ever interrupted by it. Slid down from the top and faded, as the
/// notices at the foot slide up; what is said inside changes with a
/// cross-fade and a change of height, never a cut. What it said last is kept
/// while it leaves, so it does not go blank on its way out.
struct UpdateBanner: View {
    let updates: Updates
    /// What it clears below the top of where it is put: the server menu in
    /// the corner of the lobby's stage, the sign-in's language menu.
    var top: CGFloat = 12
    @State private var said: Updates.Step = .none

    var body: some View {
        ZStack {
            if updates.shown && !installing {
                card
                    .transition(.asymmetric(
                        insertion: .move(edge: .top).combined(with: .opacity).animation(Theme.settle),
                        removal: .move(edge: .top).combined(with: .opacity).animation(Theme.leave)))
            }
        }
        .animation(Theme.settle, value: updates.shown && !installing)
        .onAppear { said = updates.step }
        .onChange(of: updates.step) { if updates.shown || updates.step != .none { said = updates.step } }
    }

    private var installing: Bool {
        if case .installing = updates.step { true } else { false }
    }

    private var card: some View {
        content(said)
            .id(kind(said))
            .transition(.asymmetric(insertion: .opacity.animation(Theme.settle), removal: .opacity.animation(Theme.leave)))
            .animation(Theme.settle, value: kind(said))
            .padding(14)
            .frame(maxWidth: 520)
            // Flat, as the screens are drawn: the surface colour with the
            // theme's edge, and a shadow because it floats over the stage.
            // Not glass: it is something to read, not a control.
            .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.surface))
            .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Theme.border, lineWidth: 1))
            .shadow(color: .black.opacity(0.35), radius: 18, y: 8)
            .padding(.horizontal, 16)
            .padding(.top, top)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("update.banner")
    }

    @ViewBuilder
    private func content(_ step: Updates.Step) -> some View {
        switch step {
        case .none:
            EmptyView()
        case let .offered(offer, problem):
            Said(badge: problem == nil ? "sparkles" : "exclamationmark.circle.fill", warning: problem != nil,
                 title: t("Tomoshibi {version} is available", ["version": offer.version]),
                 lines: [offer.notes(Localization.shared.language.rawValue)].compactMap { $0 },
                 problem: problem) {
                PillButton(title: t("Later"), height: 30) { updates.putAway() }
                    .accessibilityIdentifier("update.later")
                PillButton(title: problem == nil ? t("Update") : t("Try again"), role: .prominent, height: 30) { updates.accept() }
                    .accessibilityIdentifier("update.accept")
            }
        case let .downloading(offer, received, total), let .held(offer, received, total):
            Said(badge: "arrow.clockwise",
                 title: t("Downloading Tomoshibi {version}", ["version": offer.version]),
                 lines: [t("{received} of {total}", ["received": megabytes(received), "total": megabytes(total)])],
                 progress: total > 0 ? min(1, Double(received) / Double(total)) : 0) {
                PillButton(title: t("Cancel"), height: 30) { updates.cancel() }
                    .accessibilityIdentifier("update.cancel")
            }
        case let .misplaced(_, place):
            Said(badge: "exclamationmark.circle.fill", warning: true,
                 title: t("Tomoshibi cannot update itself here"),
                 lines: [Self.explain(place)]) {
                PillButton(title: t("Later"), height: 30) { updates.putAway() }
                    .accessibilityIdentifier("update.later")
                PillButton(title: t("Show in Finder"), role: .prominent, height: 30) { updates.reveal() }
                    .accessibilityIdentifier("update.reveal")
            }
        case let .installing(offer):
            Said(badge: nil,
                 title: t("Installing Tomoshibi {version}", ["version": offer.version]),
                 lines: [t("Tomoshibi will open again when it is done.")]) {
                EmptyView()
            }
        }
    }

    static func explain(_ place: Misplacement) -> String {
        switch place {
        case .diskImage:
            t("Tomoshibi is running from its disk image. Drag it into Applications, open it from there, and update again.")
        case .translocated:
            t("macOS is running this copy of Tomoshibi from a temporary place. Move Tomoshibi into Applications in Finder, open it from there, and update again.")
        case .readOnly:
            t("You are not allowed to change Tomoshibi where it is. Move it into Applications, or ask whoever manages this Mac to update it.")
        }
    }

    private func kind(_ step: Updates.Step) -> Int {
        switch step {
        case .none: 0
        case .offered: 1
        case .downloading, .held: 2
        case .misplaced: 3
        case .installing: 4
        }
    }
}

/// While a build is put in place (Updates `installing`): the app dimmed, and
/// over it the app's mark, what is being installed, that it opens again by
/// itself, and a bar that follows the install as it goes (MacInstaller), as the
/// Windows setup shows its own (Tomoshibi.iss). Nothing in it can be pressed,
/// and nothing under it: the install is not stopped by then, and the app ends
/// when it is done. It comes in growing and fading as the dialogs do.
struct UpdateInstalling: View {
    let updates: Updates

    var body: some View {
        ZStack {
            if case let .installing(offer) = updates.step {
                Color.black.opacity(0.45)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .transition(.asymmetric(insertion: .opacity.animation(Theme.settle), removal: .opacity.animation(Theme.leave)))
                card(offer)
                    .transition(.asymmetric(
                        insertion: .scale(scale: 0.96).combined(with: .opacity).animation(Theme.settle),
                        removal: .opacity.animation(Theme.leave)))
            }
        }
        .animation(Theme.settle, value: installing)
    }

    private var installing: Bool {
        if case .installing = updates.step { true } else { false }
    }

    private func card(_ offer: UpdateOffer) -> some View {
        HStack(alignment: .center, spacing: 16) {
            Mark(size: 56)
            VStack(alignment: .leading, spacing: 6) {
                Text(t("Installing Tomoshibi {version}", ["version": offer.version]))
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                Text(t("Tomoshibi will open again when it is done."))
                    .font(.system(size: 12.5))
                    .foregroundStyle(Theme.muted)
                    .fixedSize(horizontal: false, vertical: true)
                ProgressTrack(fraction: updates.installed).padding(.top, 6)
            }
            .frame(width: 280, alignment: .leading)
        }
        .padding(20)
        .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Theme.border, lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 18, y: 8)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("update.installing")
    }
}

/// The symbol in a lit circle, the title, what there is to say, and the
/// buttons at the trailing end.
private struct Said<Buttons: View>: View {
    let badge: String?
    var warning = false
    let title: String
    let lines: [String]
    var problem: String?
    var progress: Double?
    @ViewBuilder var buttons: Buttons

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            let tint = warning ? Theme.danger : Theme.tally
            ZStack {
                Circle().fill(tint.opacity(0.18))
                if let badge {
                    Image(systemName: badge).font(.system(size: 13, weight: .semibold)).foregroundStyle(tint)
                } else {
                    MediaSpinner(size: 14, tint: tint)
                }
            }
            .frame(width: 30, height: 30)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.foreground)
                ForEach(lines, id: \.self) { line in
                    Text(line).font(.system(size: 12.5)).foregroundStyle(Theme.muted).lineLimit(6)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let problem {
                    Text(problem).font(.system(size: 12.5)).foregroundStyle(Theme.danger).fixedSize(horizontal: false, vertical: true)
                }
                if let progress { ProgressTrack(fraction: progress).padding(.top, 4) }
                HStack(spacing: 8) {
                    Spacer(minLength: 0)
                    buttons
                }
                .padding(.top, 6)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// How much has arrived, as a filling capsule that moves smoothly between
/// reports.
private struct ProgressTrack: View {
    let fraction: Double

    var body: some View {
        GeometryReader { space in
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.surfaceHigh)
                Capsule().fill(Theme.tally).frame(width: max(5, space.size.width * fraction))
            }
        }
        .frame(height: 5)
        .animation(Theme.settle, value: fraction)
        .accessibilityElement()
        .accessibilityValue(Text("\(Int(fraction * 100))%"))
    }
}

/// Bytes as megabytes to a tenth, in the reader's own way of writing a number.
func megabytes(_ bytes: Int64) -> String {
    let number = (Double(bytes) / 1_000_000).formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: Localization.shared.language.rawValue)))
    return "\(number) MB"
}
