import SwiftUI
import TomoshibiBasics

/// What somebody just said, in one corner, for six seconds (Said.tsx): one
/// place, always the same, so a message is seen rather than hunted for, and
/// nothing here while the messages are open, where it is already on screen.
///
/// The bottom-leading corner, above the controls, where nothing else in the
/// call is drawn: the notices have the other corner, and a message has a
/// face, a name and somebody's own words where a notice is a line about an
/// event, so the two are told apart by shape as well as place (Notices.tsx).
/// Where the window is too narrow for both, the notices rise above these
/// rather than covering them (CornerLanes).
///
/// Always on screen, drawing nothing when there is nothing to say, so that
/// each card leaves with its own animation, the last one included: a corner
/// that went with its last card took that card with it in the corner's own
/// way out.
struct SaidView: View {
    let model: CallModel

    /// Three is what a notification stack shows; the panel holds the rest and
    /// the dot on its button says it is there (Said.tsx `AT_MOST`).
    private static let most = 3

    var body: some View {
        let lines = Array(model.arriving.suffix(Self.most))
        let speakers = lines.map(\.identity)
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(lines.enumerated()), id: \.element.id) { index, line in
                SaidCard(line: line, run: ChatRules.continues(speakers, at: index))
                    // Said.tsx: in with `arrive` (up eight points from 94 per
                    // cent), out with `depart` (down four to 97 per cent).
                    .transition(.asymmetric(
                        insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: .bottomLeading))
                            .combined(with: .offset(y: 8)).animation(Theme.settle),
                        removal: .opacity.combined(with: .scale(scale: 0.97, anchor: .leading))
                            .combined(with: .offset(y: 4)).animation(Theme.leave)
                    ))
            }
        }
        .frame(maxWidth: 290, alignment: .bottomLeading)
        .animation(Theme.settle, value: lines.map(\.id))
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame in
            let now = lines.isEmpty ? nil : frame
            if model.saidFrame != now { model.saidFrame = now }
        }
        .onChange(of: lines.isEmpty) { _, empty in if empty { model.saidFrame = nil } }
        .onDisappear { model.saidFrame = nil }
    }
}

/// One message: a face, a name and the words, the first two left out for a
/// run from the same person, the same rule the panel uses so that both read
/// as one conversation.
private struct SaidCard: View {
    let line: ChatLine
    let run: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            Monogram(name: line.name, identity: line.identity, size: 24)
                .opacity(run ? 0 : 1)
            VStack(alignment: .leading, spacing: 2) {
                if !run {
                    Text(line.name)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Theme.muted)
                        .lineLimit(1)
                }
                // Links work here as in the panel: the commonest thing typed
                // during a call is an address, and it is wanted now.
                LinkedText(text: line.text, size: 13)
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(5)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.leading, 8)
        .padding(.trailing, 12)
        .padding(.vertical, run ? 7 : 8)
        // Opaque, as the web's message bubbles now are: words over a
        // moving picture are read on a ground of their own.
        .menuSurface(cornerRadius: Theme.Radius.medium)
        .accessibilityElement(children: .combine)
    }
}
