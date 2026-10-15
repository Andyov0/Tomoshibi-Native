import SwiftUI

/// What somebody just said, in one corner, for six seconds (Said.tsx): one
/// place, always the same, so a message is seen rather than hunted for, and
/// nothing here while the messages are open, where it is already on screen.
struct SaidView: View {
    let lines: [ChatLine]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(lines.suffix(3)) { line in
                HStack(alignment: .top, spacing: 8) {
                    Avatar(name: line.name, identity: line.identity).frame(width: 26, height: 26)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(line.name).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                        Text(line.text).font(.system(size: 13)).foregroundStyle(Theme.foreground).lineLimit(4)
                    }
                }
                .padding(.horizontal, 10).padding(.vertical, 8)
                .background(RoundedRectangle(cornerRadius: 10).fill(Theme.surfaceHigh))
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.border))
                .transition(Theme.slideUp)
            }
        }
    }
}
