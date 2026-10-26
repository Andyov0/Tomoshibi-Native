import Messages
import SwiftUI
import TomoshibiBasics

/// The extension's one screen, which is two: sending, in the strip Messages
/// gives an app under the conversation, and a pressed card, in the larger
/// view Messages opens for it.
///
/// Drawn as the app draws its lobby's sheets, the schedule's above all, since
/// this is the schedule's list seen from Messages: the room's ground, light
/// or dark as the system is and as Messages is around it, the content as
/// plain surfaces with a hairline, the primary action in the
/// person's colour, and glass only for what floats over the content, the
/// sentence at the foot (Glass.swift, which is the app's own file compiled in
/// here). Everything that comes or goes does so with the app's arrival and
/// leaving (Theme), as the owner asks of every screen.
struct InviteRoot: View {
    @Bindable var model: InviteModel
    @State private var appearance = Appearance.shared
    @State private var localization = Localization.shared

    var body: some View {
        ZStack {
            Backdrop()
            if model.style == .expanded, let opened = model.opened {
                OpenedCard(model: model, opened: opened)
                    .transition(Theme.arrive)
            } else {
                Composer(model: model)
                    .transition(Theme.arrive)
            }
        }
        .overlay(alignment: .bottom) {
            if let problem = model.problem {
                Text(problem)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .glassPane(cornerRadius: 16)
                    .padding(12)
                    .transition(Theme.slideUp)
            }
        }
        .animation(Theme.settle, value: model.opened)
        .animation(Theme.settle, value: model.style)
        .animation(Theme.settle, value: model.problem)
        .tint(appearance.accent.color)
        .environment(\.locale, localization.language.locale)
    }
}

// MARK: - Sending

private struct Composer: View {
    @Bindable var model: InviteModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                header
                switch model.status {
                case .signedOut:
                    signIn(t("Sign in to Tomoshibi to invite people from here."))
                        .transition(Theme.arrive)
                case .expired:
                    signIn(t("Your session has expired. Sign in again."))
                        .transition(Theme.arrive)
                default:
                    start
                        .transition(Theme.arrive)
                    list
                        .transition(Theme.arrive)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 20)
            .contentLayer()
        }
        .scrollIndicators(.never)
        .scrollBounceBehavior(.basedOnSize)
        .animation(Theme.settle, value: model.status)
        .animation(Theme.settle, value: model.meetings)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Mark(size: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(t("Invite to a meeting")).font(.system(size: 15, weight: .semibold))
                if let account = model.record?.account, model.status != .signedOut {
                    Text(account).font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                        .transition(.opacity)
                }
            }
            Spacer(minLength: 0)
        }
        .foregroundStyle(Theme.foreground)
    }

    private func signIn(_ sentence: String) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "person.crop.circle.badge.exclamationmark").foregroundStyle(Theme.muted)
                Text(sentence).font(.system(size: 13.5)).foregroundStyle(Theme.foreground)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Button(t("Open Tomoshibi")) { model.openApp() }
                .buttonStyle(.glassCapsule(40, prominent: true))
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassPane(cornerRadius: 20)
    }

    private var start: some View {
        VStack(alignment: .leading, spacing: 7) {
            Button {
                Task { await model.startNow() }
            } label: {
                ZStack {
                    if model.starting {
                        ProgressView().controlSize(.small).tint(Theme.tallyForeground).transition(.opacity)
                    } else {
                        Label(t("Start a meeting"), systemImage: "video.badge.plus").transition(.opacity)
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glassCapsule(44, prominent: true))
            .disabled(model.starting || model.sending != nil || !model.arranges)
            .animation(Theme.settle, value: model.starting)
            Text(model.arranges ? t("Sends a link for right now. It starts when you join.")
                                : t("Arranging meetings has not been opened to you."))
                .font(.system(size: 11.5)).foregroundStyle(Theme.muted)
                .fixedSize(horizontal: false, vertical: true)
                .contentTransition(.opacity)
        }
    }

    @ViewBuilder
    private var list: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(t("Scheduled meetings"))
                .font(.system(size: 12, weight: .semibold)).foregroundStyle(Theme.muted)
            if model.status == .failed {
                HStack(spacing: 10) {
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(Theme.muted)
                    Text(t("Could not load your meetings.")).font(.system(size: 13)).foregroundStyle(Theme.muted)
                    Spacer(minLength: 8)
                    Button(t("Try again")) { Task { await model.refresh() } }.buttonStyle(.glassCapsule(30))
                }
                .padding(12)
                .glassPane(cornerRadius: 16)
                .transition(Theme.arrive)
            }
            if model.meetings.isEmpty {
                ZStack {
                    if model.status == .loading {
                        ProgressView().controlSize(.small).transition(.opacity)
                    } else {
                        VStack(spacing: 8) {
                            Image(systemName: "calendar.badge.clock").font(.system(size: 22)).foregroundStyle(Theme.muted)
                            Text(t("No meetings arranged yet.")).font(.system(size: 13, weight: .semibold))
                        }
                        .transition(.opacity)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 96)
                .glassPane(cornerRadius: 18)
                .transition(Theme.arrive)
            } else {
                VStack(spacing: 0) {
                    ForEach(model.meetings) { one in
                        VStack(spacing: 0) {
                            row(one)
                            if one.id != model.meetings.last?.id {
                                Divider().overlay(Theme.ink.opacity(0.06)).padding(.leading, 60)
                            }
                        }
                        .transition(.asymmetric(
                            insertion: .move(edge: .top).combined(with: .opacity).animation(Theme.settle),
                            removal: .opacity.combined(with: .scale(scale: 0.96)).animation(Theme.leave)))
                    }
                }
                .glassPane(cornerRadius: 18)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                .transition(Theme.arrive)
            }
        }
    }

    /// One meeting, sent as a card at a press, as the lobby draws it
    /// (Lobby.swift `MeetingRow`).
    private func row(_ one: Meeting) -> some View {
        Button {
            Task { await model.send(one) }
        } label: {
            HStack(spacing: 12) {
                Image(systemName: one.started ? "video.fill" : "calendar")
                    .font(.system(size: 15))
                    .foregroundStyle(one.started ? Theme.tallyInk : Theme.muted)
                    .frame(width: 34, height: 34)
                    .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(one.started ? Theme.tally.opacity(0.18) : Theme.ink.opacity(0.08)))
                VStack(alignment: .leading, spacing: 2) {
                    Text(one.room).font(.system(size: 14, weight: .semibold)).lineLimit(1).truncationMode(.middle)
                    Text(one.instant.map(MeetingTimes.short) ?? one.at)
                        .font(.system(size: 12)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                Spacer(minLength: 4)
                ZStack {
                    if model.sending == one.id {
                        ProgressView().controlSize(.small).transition(.opacity)
                    } else {
                        Image(systemName: "arrow.up.circle.fill")
                            .font(.system(size: 22))
                            .foregroundStyle(Theme.tallyInk)
                            .transition(.opacity)
                    }
                }
                .frame(width: 26)
                .animation(Theme.settle, value: model.sending)
            }
            .foregroundStyle(Theme.foreground)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowPress())
        .disabled(model.sending != nil || model.starting)
        .accessibilityElement(children: .combine)
        .accessibilityHint(t("Send"))
    }
}

/// A row pressed: lit a little by night and shaded a little by day, as the
/// app's plain surfaces are.
private struct RowPress: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(Theme.ink.opacity(configuration.isPressed ? 0.06 : 0))
            .animation(Theme.settle, value: configuration.isPressed)
    }
}

// MARK: - A card that was pressed

private struct OpenedCard: View {
    @Bindable var model: InviteModel
    let opened: MeetingLink.Opened

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                InvitePicture()
                    .aspectRatio(3 / 2, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(Theme.border, lineWidth: 1))
                    .frame(maxWidth: 420)
                VStack(spacing: 8) {
                    Text(opened.room.isEmpty ? t("Meeting link") : opened.room)
                        .font(.system(size: 22, weight: .bold))
                        .foregroundStyle(Theme.foreground)
                        .multilineTextAlignment(.center)
                        .lineLimit(2)
                    said
                        .frame(minHeight: 22)
                }
                Button { model.join() } label: {
                    Text(t("Join")).frame(maxWidth: .infinity)
                }
                .buttonStyle(.glassCapsule(48, prominent: true))
                .frame(maxWidth: 420)
                .disabled(!joinable)
                .animation(Theme.settle, value: joinable)
            }
            .padding(24)
            .frame(maxWidth: .infinity)
            .contentLayer()
        }
        .scrollIndicators(.never)
        .scrollBounceBehavior(.basedOnSize)
        .animation(Theme.settle, value: model.reading)
    }

    /// Whether there is a meeting to go into. One that could not be asked
    /// about is still offered: the app asks again, and says why if not.
    private var joinable: Bool {
        switch model.reading {
        case .gone, .elsewhere: false
        case let .known(meeting): !meeting.ended
        case .asking, .unreachable: true
        }
    }

    /// What the deployment says of it, in the words the app's own waiting
    /// screen uses (Screens.swift, Waiting.tsx).
    @ViewBuilder
    private var said: some View {
        switch model.reading {
        case .asking:
            ProgressView().controlSize(.small).transition(.opacity)
        case let .known(meeting) where meeting.ended:
            line(t("That meeting has ended."))
        case let .known(meeting) where meeting.started:
            Text(t("Under way")).font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.tallyForeground)
                .padding(.horizontal, 10).padding(.vertical, 4)
                .background(Capsule().fill(Theme.tally))
                .transition(Theme.arrive)
        case let .known(meeting):
            let when = meeting.instant.map(MeetingTimes.said) ?? meeting.at
            line(meeting.mine ? t("Your meeting is arranged for {when}. Joining starts it.", ["when": when])
                              : t("This meeting is arranged for {when}.", ["when": when]))
        case .gone:
            line(t("That invitation is no longer good. Ask for another."))
        case .unreachable:
            line(t("Could not reach the server."))
        case .elsewhere:
            line(t("That link is for another server."))
        }
    }

    private func line(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 14))
            .foregroundStyle(Theme.muted)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .transition(Theme.arrive)
    }
}
