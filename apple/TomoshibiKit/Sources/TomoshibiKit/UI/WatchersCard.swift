import SwiftUI

/// Who has this participant's shared screen on their stage, from everybody's
/// `watching` attribute (components/room/Watchers.tsx, live/watching.ts): a
/// sharer talking to a room wants to know whether anybody is looking.
struct WatchersCard: View {
    @Bindable var model: CallModel

    var body: some View {
        let watching = model.people.filter { $0.attributes["watching"] == model.localIdentity }
        VStack(alignment: .leading, spacing: 4) {
            Text(t("Watching")).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
            if watching.isEmpty {
                Text(t("Nobody has this open yet.")).font(.system(size: 12)).foregroundStyle(Theme.foreground)
            } else {
                ForEach(watching) { person in
                    Text(person.name).font(.system(size: 12)).foregroundStyle(Theme.foreground).lineLimit(1)
                        .transition(Theme.arrive)
                }
            }
        }
        .padding(10)
        .frame(maxWidth: 200, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10).fill(Theme.surfaceHigh))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.border))
        .animation(Theme.settle, value: watching.map(\.id))
    }
}
