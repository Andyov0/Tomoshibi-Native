import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// The call's messages (components/room/ChatPanel.tsx): kept only for the
/// call, as the web says at the foot of the panel.
struct ChatPanel: View {
    @Bindable var model: CallModel
    let close: () -> Void
    @State private var draft = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("Messages")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close messages"))
            }
            .padding(12)
            Divider().overlay(Theme.border)
            ScrollViewReader { reader in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 10) {
                        ForEach(model.messages) { line in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(line.mine ? t("{name} (you)", ["name": line.name]) : line.name)
                                    .font(.system(size: 11, weight: .semibold))
                                    .foregroundStyle(Theme.muted)
                                Text(line.text)
                                    .font(.system(size: 13))
                                    .foregroundStyle(Theme.foreground)
                                    .textSelection(.enabled)
                            }
                            .id(line.id)
                            .transition(Theme.arrive)
                        }
                    }
                    .padding(12)
                }
                .onChange(of: model.messages.count) {
                    if let last = model.messages.last { withAnimation(Theme.settle) { reader.scrollTo(last.id, anchor: .bottom) } }
                }
            }
            Divider().overlay(Theme.border)
            HStack(spacing: 8) {
                TextField(t("Say something"), text: $draft)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .onSubmit(send)
                    .accessibilityLabel(t("Say something"))
                Button(action: send) {
                    Image(systemName: "arrow.up").font(.system(size: 12, weight: .bold))
                        .foregroundStyle(Theme.tallyForeground)
                        .frame(width: 26, height: 26)
                        .background(Circle().fill(draft.isEmpty ? Theme.border : Theme.tally))
                }
                .buttonStyle(.plain)
                .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                .accessibilityLabel(t("Send"))
            }
            .padding(10)
            Text(t("Messages disappear when the call ends."))
                .font(.system(size: 10.5))
                .foregroundStyle(Theme.muted)
                .padding(.bottom, 8)
        }
        .glassPane(cornerRadius: 22)
    }

    private func send() {
        let text = draft
        draft = ""
        Task { try? await model.say(text) }
    }
}
