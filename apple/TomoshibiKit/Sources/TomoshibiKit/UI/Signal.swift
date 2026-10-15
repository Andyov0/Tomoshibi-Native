import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// How the call is going, in the corner (components/room/Signal.tsx).
///
/// Two readings of the same thing, and which one somebody wants depends on
/// why they looked: most of the time "is it me?", answered by bars; now and
/// then "what exactly is wrong?", which only the figures answer. The owner
/// chose the bars alone by default, the figures as a setting, and a setting
/// to hide the badge entirely (2026-10-15); the web opens the figures from
/// the badge and remembers it, which is what the detailed setting does here.
struct SignalBadge: View {
    let reading: ConnectionReading
    let detailed: Bool
    @Binding var open: Bool
    var server: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                guard detailed else { return }
                withAnimation(Theme.settle) { open.toggle() }
            } label: {
                HStack(spacing: 7) {
                    Bars(grade: reading.grade)
                    if detailed {
                        Text(reading.rttMs.map { "\(Int($0)) ms" } ?? "—")
                            .font(.system(size: 11.5, weight: .medium).monospacedDigit())
                            .foregroundStyle(Theme.muted)
                            .transition(.opacity)
                    }
                }
                .padding(.horizontal, detailed ? 11 : 9)
                .frame(height: 28)
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .glassSurface(Capsule(), interactive: detailed)
            .accessibilityLabel(title)
            .help(title)

            if detailed && open {
                Detail(reading: reading, server: server)
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: .topTrailing)).animation(Theme.settle),
                        removal: .opacity.animation(Theme.leave)))
            }
        }
        .fixedSize()
        .animation(Theme.settle, value: reading.grade)
        .animation(Theme.settle, value: detailed)
    }

    private var title: String {
        switch reading.grade {
        case .lost: t("Connection lost")
        case .poor: t("Poor connection")
        case .fair: t("Fair connection")
        case .good: t("Good connection")
        }
    }
}

/// Three bars, as a phone draws them: colour and height say the same thing,
/// which keeps it legible to somebody who cannot tell the colours apart.
private struct Bars: View {
    let grade: ConnectionGrade

    var body: some View {
        let lit = grade == .good ? 3 : grade == .fair ? 2 : grade == .poor ? 1 : 0
        let colour = grade == .good ? Theme.good : grade == .fair ? Theme.tally : Theme.danger
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(0..<3, id: \.self) { index in
                RoundedRectangle(cornerRadius: 1)
                    .fill(index < lit ? colour : Theme.muted.opacity(0.35))
                    .frame(width: 3.5, height: [6, 10, 14][index])
            }
        }
        .accessibilityHidden(true)
    }
}

/// The figures, as Signal.tsx `Detail` lists them.
private struct Detail: View {
    let reading: ConnectionReading
    let server: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if let server, !server.isEmpty { figure(t("Server"), say(server)) }
            if let address = reading.ownAddress { figure(t("Your address"), address) }
            figure(t("Round trip"), reading.rttMs.map { "\(Int($0)) ms" } ?? "—")
            figure(t("Lost"), reading.lossPercent.map { String(format: "%.1f%%", $0) } ?? "—", warn: (reading.lossPercent ?? 0) > 2)
            figure(t("Jitter"), reading.jitterMs.map { "\(Int($0)) ms" } ?? "—")
            if let share = reading.share {
                figure(share.sending ? t("Sharing") : t("Watching"),
                       share.height.flatMap { height in share.fps.map { "\(Int(height))p · \(Int($0)) fps" } } ?? t("Measuring…"),
                       warn: (share.fps ?? 30) < 10)
                if let limited = share.limited {
                    figure(t("Held back by"), {
                        switch limited {
                        case .cpu: t("this machine")
                        case .bandwidth: t("the connection")
                        case .other: t("something the browser will not name")
                        }
                    }(), warn: true)
                }
            }
            figure(t("Sending"), rate(reading.upKbps))
            figure(t("Receiving"), rate(reading.downKbps))
            if !reading.measured {
                Text(t("Measuring…")).foregroundStyle(Theme.muted)
            }
        }
        .font(.system(size: 11.5))
        .padding(.horizontal, 12).padding(.vertical, 10)
        // A fixed width: left to itself the row's spacer took the whole of
        // the room's width, and the figures ended up under the side panel.
        .frame(width: 236)
        .glassSurface(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func figure(_ label: String, _ value: String, warn: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 16) {
            Text(label).foregroundStyle(Theme.muted)
            Spacer(minLength: 8)
            Text(value).monospacedDigit().foregroundStyle(warn ? Theme.tally : Theme.foreground)
        }
    }

    private func rate(_ kbps: Double?) -> String {
        guard let kbps else { return "—" }
        return kbps >= 1000 ? String(format: "%.1f Mbps", kbps / 1000) : "\(Int(kbps.rounded())) kbps"
    }
}
