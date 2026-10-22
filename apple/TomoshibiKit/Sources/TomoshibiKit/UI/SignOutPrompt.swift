import SwiftUI
import TomoshibiBasics

/// Asked before signing out (owner, 2026-10-22): Sign out signed out on the
/// press, and a press meant for the item beside it did it too. The window
/// behind goes to frosted glass, a little dimmed, so that the question is
/// the one thing in it; the card grows in as the dialogs do and goes on
/// either answer. A press on the glass is Cancel. The Windows and Android
/// apps ask the same, in the same words.
struct SignOutPrompt: ViewModifier {
    let app: AppModel
    /// Where this one hangs: the lobby's window, or the settings' (a window of
    /// its own on the Mac, a sheet on the iPhone), so that it comes up where
    /// Sign out was pressed and nowhere else.
    let place: AppModel.SignOutPlace

    func body(content: Content) -> some View {
        content.overlay {
            ZStack {
                if app.signOutAsked == place {
                    Rectangle()
                        .fill(.ultraThinMaterial)
                        .overlay(Color.black.opacity(0.28))
                        .ignoresSafeArea()
                        .contentShape(Rectangle())
                        .onTapGesture { answer(signOut: false) }
                        .transition(.asymmetric(insertion: .opacity.animation(Theme.settle), removal: .opacity.animation(Theme.leave)))
                        // Fixed order while both leave: without it SwiftUI drew
                        // the departing glass over the departing card, so on
                        // Cancel the card went dark in the first frame and came
                        // back as the glass faded (owner, 2026-10-22: "the animation hitches
                        // after Cancel"; a recording read frame by frame showed the
                        // red button fall from 245 to 68 at once, then rise).
                        .zIndex(0)
                    card
                        .transition(.asymmetric(
                            insertion: .scale(scale: 0.96).combined(with: .opacity).animation(Theme.settle),
                            removal: .scale(scale: 0.98).combined(with: .opacity).animation(Theme.leave)))
                        .zIndex(1)
                }
            }
            .animation(Theme.settle, value: app.signOutAsked == place)
        }
    }

    private var card: some View {
        VStack(spacing: 14) {
            ZStack {
                Circle().fill(Theme.danger.opacity(0.16))
                Image(systemName: "rectangle.portrait.and.arrow.right")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Theme.danger)
            }
            .frame(width: 46, height: 46)
            VStack(spacing: 6) {
                Text(t("Sign out of Tomoshibi?"))
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                Text(t("To come back, you will sign in again with your name and passphrase."))
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.muted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 10) {
                PillButton(title: t("Cancel"), height: 36) { answer(signOut: false) }
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("signout.cancel")
                PillButton(title: t("Sign out"), role: .destructive, height: 36) { answer(signOut: true) }
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("signout.confirm")
            }
            .padding(.top, 4)
        }
        .padding(22)
        .frame(maxWidth: 340)
        .background(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(Theme.border, lineWidth: 1))
        .shadow(color: .black.opacity(0.4), radius: 28, y: 12)
        .padding(.horizontal, 24)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("signout.prompt")
    }

    private func answer(signOut: Bool) {
        withAnimation(Theme.leave) { app.signOutAsked = nil }
        if signOut { Task { await app.signOut() } }
    }
}

extension View {
    /// The question before signing out, over this window (SignOutPrompt).
    func signOutPrompt(_ app: AppModel, at place: AppModel.SignOutPlace) -> some View {
        modifier(SignOutPrompt(app: app, place: place))
    }
}
