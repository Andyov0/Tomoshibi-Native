import SwiftUI
import TomoshibiBasics

/// The call's toasts, in the corner above the controls (the web draws sonner
/// at the bottom right): glass cards that slide in, stack, and go.
struct ToastStack: View {
    @Bindable var model: CallModel

    var body: some View {
        VStack(alignment: .trailing, spacing: 8) {
            ForEach(model.toasts) { toast in
                ToastCard(toast: toast, act: { model.act(on: toast.id) }, close: { withAnimation(Theme.leave) { model.dismiss(toast.id) } })
                    .transition(Theme.slideIn)
            }
        }
        .animation(Theme.settle, value: model.toasts)
        .frame(maxWidth: 340, alignment: .trailing)
    }
}

private struct ToastCard: View {
    let toast: CallToast
    let act: () -> Void
    let close: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(colour)
                .frame(width: 18, height: 18)
            VStack(alignment: .leading, spacing: 3) {
                Text(toast.title).font(.system(size: 13, weight: .semibold))
                    .fixedSize(horizontal: false, vertical: true)
                if let detail = toast.detail {
                    Text(detail).font(.system(size: 12)).foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let actionTitle = toast.actionTitle {
                    Button(actionTitle, action: act)
                        .buttonStyle(.glassCapsule(28))
                        .padding(.top, 4)
                }
            }
            Spacer(minLength: 0)
            Button(action: close) {
                Image(systemName: "xmark").font(.system(size: 9.5, weight: .bold)).foregroundStyle(Theme.muted)
                    .frame(width: 20, height: 20)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(hovering || toast.lasts == nil ? 1 : 0.5)
            .accessibilityLabel(t("Close"))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(minWidth: 220, alignment: .leading)
        .glassPane(cornerRadius: 16)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityElement(children: .combine)
    }

    private var symbol: String {
        switch toast.kind {
        case .info: "info.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .failure: "xmark.octagon.fill"
        }
    }

    private var colour: Color {
        switch toast.kind {
        case .info: Theme.foreground.opacity(0.8)
        case .warning: Theme.tally
        case .failure: Theme.danger
        }
    }
}
