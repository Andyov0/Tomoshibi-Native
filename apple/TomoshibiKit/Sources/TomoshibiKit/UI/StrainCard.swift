#if os(macOS)
import SwiftUI
import TomoshibiCore

/// Said to the person sharing when their line cannot carry the share
/// (components/room/ShareStrain.tsx). It offers a setting that fits and
/// changes nothing on its own: the owner banned lowering a share by itself.
struct StrainCard: View {
    let hint: StrainHint
    let model: CallModel
    let sharing: Sharing
    @State private var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: "gauge.with.dots.needle.33percent")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Theme.tallyInk)
                    .frame(width: 20)
                VStack(alignment: .leading, spacing: 4) {
                    Text(t("Your connection is sending about {have} Mbps, and this share needs about {need} Mbps, so it stutters.",
                           ["have": strainMegabits(bits: hint.have), "need": strainMegabits(bits: hint.need)]))
                        .foregroundStyle(Theme.foreground)
                        .fixedSize(horizontal: false, vertical: true)
                    Text(t("Switch to {size} at {rate} frames a second?",
                           ["size": t(Self.label(hint.to.quality)), "rate": String(hint.to.frameRate)]))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .font(.system(size: 12.5))
            }
            HStack(spacing: 8) {
                Spacer()
                Button(t("Not now")) { model.call.strainDecline() }
                    .buttonStyle(.glassCapsule(30))
                Button(action: take) {
                    HStack(spacing: 6) {
                        if busy { ProgressView().controlSize(.mini).transition(Theme.arrive) }
                        Text(t("Switch"))
                    }
                }
                .buttonStyle(.glassCapsule(30, prominent: true))
                .animation(Theme.settle, value: busy)
            }
            .disabled(busy)
        }
        .padding(14)
        .frame(maxWidth: 340)
        // Opaque, as the web's share notice now is.
        .menuSurface(cornerRadius: Theme.Radius.medium)
    }

    static func label(_ quality: ShareQuality) -> String {
        switch quality {
        case .auto: return "Automatic"
        case .p1080: return "1080p"
        case .p1440: return "1440p"
        case .p4k: return "4K"
        }
    }

    private func take() {
        guard case let .apply(setting) = model.call.strainAccept() else { return }
        busy = true
        Task {
            let quality: ScreenCapture.Quality = switch setting.quality {
            case .auto: .auto
            case .p1080: .p1080
            case .p1440: .p1440
            case .p4k: .p2160
            }
            var took = true
            do { try await sharing.restart(quality: quality, fps: Int(setting.frameRate), on: model.call) } catch { took = false }
            _ = model.call.strainAccepted(took: took)
            busy = false
        }
    }
}
#endif
