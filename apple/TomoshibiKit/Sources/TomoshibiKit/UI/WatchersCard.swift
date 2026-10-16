import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// Who has this participant's shared screen on their stage, from everybody's
/// `watching` attribute (components/room/Watchers.tsx, live/watching.ts).
///
/// Somebody sharing is doing it blind: they can see that eleven people are in
/// the call and not whether any of them has the picture in front of them, and
/// a share sitting unopened in the strip is a share nobody is watching. The
/// list is what people say rather than what the server knows, since the media
/// server tells a publisher nothing about who subscribed.
///
/// Two sizes: the card, with names, where there is room beside whatever else
/// is open; the eye and the count alone where there is not, so that opening a
/// panel on a phone moves it rather than hiding it.
struct WatchersCard: View {
    @Bindable var model: CallModel
    var compact = false

    var body: some View {
        let watching = Self.watching(model)
        Group {
            if compact {
                HStack(spacing: 5) {
                    Image(systemName: "eye.fill").font(.system(size: 10.5, weight: .semibold))
                    Text("\(watching.count)")
                        .font(.system(size: 12, weight: .semibold).monospacedDigit())
                        .contentTransition(.numericText(value: Double(watching.count)))
                }
                .foregroundStyle(watching.isEmpty ? Theme.muted : Theme.foreground)
                .padding(.horizontal, 10)
                .frame(height: 28)
                .glassSurface(Capsule())
                .help(t("Watching"))
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(t("Watching"))
                .accessibilityValue("\(watching.count)")
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        Image(systemName: "eye.fill").font(.system(size: 10, weight: .semibold))
                        Text(t("Watching"))
                        Spacer(minLength: 8)
                        Text("\(watching.count)")
                            .monospacedDigit()
                            .contentTransition(.numericText(value: Double(watching.count)))
                    }
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Theme.muted)
                    if watching.isEmpty {
                        // Said rather than left blank: an empty box beside a
                        // share reads as something that has not loaded, and
                        // the useful reading is that nobody has opened it.
                        Text(t("Nobody has this open yet."))
                            .font(.system(size: 11.5))
                            .foregroundStyle(Theme.muted.opacity(0.85))
                            .fixedSize(horizontal: false, vertical: true)
                            .transition(.opacity)
                    } else {
                        VStack(alignment: .leading, spacing: 3) {
                            ForEach(watching) { person in
                                HStack(spacing: 7) {
                                    Circle().fill(Theme.tally).frame(width: 5, height: 5)
                                    Text(person.name.isEmpty ? person.id : person.name)
                                        .font(.system(size: 12.5))
                                        .foregroundStyle(Theme.foreground)
                                        .lineLimit(1)
                                }
                                .transition(Theme.arrive)
                            }
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .frame(width: 172, alignment: .leading)
                .glassSurface(RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
        }
        .animation(Theme.settle, value: watching.map(\.id))
    }

    /// Everybody whose `watching` names this participant, in roster order:
    /// the protocol crate's `watchers`, which is watching.ts `useWatchers`.
    static func watching(_ model: CallModel) -> [Person] {
        if !StageDebug.watchers.isEmpty {
            return model.people.filter { StageDebug.watchers.contains($0.name) }
        }
        let roster = model.people.map { RosterMember(identity: $0.id, attributes: $0.attributes) }
        let ids = Set(watchers(me: model.localIdentity, sharing: true, roster: roster))
        return model.people.filter { ids.contains($0.id) }
    }
}
