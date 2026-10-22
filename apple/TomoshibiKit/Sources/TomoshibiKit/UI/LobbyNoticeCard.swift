import SwiftUI
import TomoshibiBasics

/// The deployment's notice, at the top of the lobby (Remote `notice`): what
/// it says in the reader's language, and a close that puts this one away on
/// this device for good, until the deployment puts up another.
///
/// Drawn as the update banner is (UpdateBanner), flat, with its badge: the
/// person's colour for something to know, and for a warning the colour and
/// symbol the app says a problem in. Slid down from the top and faded in,
/// back up and out when it is closed or taken down, and pushing what is
/// below it down rather than lying over it: on the Mac's stage it lay across
/// the greeting.
///
/// Only in the lobby and never in a call, which Remote decides; nothing here
/// is drawn where there is no notice, not even its padding.
struct LobbyNoticeCard: View {
    let remote: Remote
    /// What it keeps clear of above, to either side and below, inside
    /// whatever it is put in, and only while it is there.
    var top: CGFloat = 12
    var sides: CGFloat = 16
    var bottom: CGFloat = 0
    var maxWidth: CGFloat = 560

    var body: some View {
        let notice = remote.notice
        // Read here, so that a change of language redraws the notice in the
        // new one: its words are not a phrase, and do not go through t().
        let language = Localization.shared.language
        ZStack {
            if let notice, let text = notice.text[language] {
                card(notice, text)
                    .padding(.horizontal, sides)
                    .padding(.top, top)
                    .padding(.bottom, bottom)
                    .transition(.asymmetric(
                        insertion: .move(edge: .top).combined(with: .opacity)
                            .combined(with: .scale(scale: 0.96, anchor: .top)).animation(Theme.settle),
                        removal: .move(edge: .top).combined(with: .opacity).animation(Theme.leave)))
            }
        }
        .frame(maxWidth: .infinity)
        .animation(Theme.settle, value: notice?.id)
        .animation(Theme.settle, value: notice.flatMap { $0.text[language] })
    }

    private func card(_ notice: LobbyNotice, _ text: String) -> some View {
        let tint = notice.warning ? Theme.danger : Theme.tally
        return HStack(alignment: .top, spacing: 12) {
            ZStack {
                Circle().fill(tint.opacity(0.18))
                Image(systemName: notice.warning ? "exclamationmark.circle.fill" : "megaphone.fill")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(tint)
            }
            .frame(width: 30, height: 30)
            .accessibilityHidden(true)
            Text(text)
                .font(.system(size: 13.5))
                .foregroundStyle(Theme.foreground)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
                .contentTransition(.opacity)
            RoundButton(symbol: "xmark", label: t("Dismiss"), size: Self.close) {
                withAnimation(Theme.leave) { remote.close(notice) }
            }
            .accessibilityIdentifier("lobby.notice.close")
        }
        .padding(.leading, 14).padding(.trailing, 10).padding(.vertical, 12)
        .frame(maxWidth: maxWidth)
        .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous)
            .strokeBorder(notice.warning ? Theme.danger.opacity(0.45) : Theme.border, lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 18, y: 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("lobby.notice")
    }

    /// The close's size: a panel's close on the Mac (PanelHeading), and a
    /// little larger under a finger.
    private static var close: CGFloat {
        #if os(iOS)
        32
        #else
        26
        #endif
    }
}
