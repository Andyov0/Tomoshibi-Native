import SwiftUI
import TomoshibiCore

/// How loud everybody else is (components/room/SoundPanel.tsx).
///
/// A list of what can be heard rather than of what can be seen: the person who
/// is too loud may be on no tile at all. One row per thing there is to hear,
/// because a shared screen's sound and the voice describing it arrive on
/// separate tracks and are turned down separately; nobody hears their own
/// microphone, so there is no row for it.
struct SoundPanelView: View {
    let feature: HostFeature
    let close: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 0)
            VStack(spacing: 0) {
                HStack {
                    Text(t("Sound")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                    Spacer()
                    Button(action: close) {
                        Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(t("Close sound"))
                }
                .padding(12)
                Divider().overlay(Theme.border)
                ViewThatFits(in: .vertical) {
                    rows
                    ScrollView { rows }
                }
            }
            .glassPane(cornerRadius: 22)
        }
        .onAppear { feature.soundOpen = true }
        .onDisappear { feature.soundOpen = false }
    }

    @ViewBuilder
    private var rows: some View {
        let others = feature.call.people
        VStack(spacing: 4) {
            if others.isEmpty {
                Text(t("Nobody else is here."))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.muted)
                    .padding(.vertical, 28)
                    .frame(maxWidth: .infinity)
                    .transition(Theme.arrive)
            } else {
                ForEach(others) { person in
                    PersonSounds(feature: feature, person: person)
                        .transition(Theme.arrive)
                }
            }
        }
        .padding(8)
        .animation(Theme.settle, value: others.map(\.id))
    }
}

/// Everything one person can be heard through.
private struct PersonSounds: View {
    let feature: HostFeature
    let person: Person

    var body: some View {
        let name = person.name.isEmpty ? person.id : person.name
        // `isMicrophoneEnabled`: there and not muted. Said rather than drawn as
        // a state of the slider, which is about what the listener chose.
        let microphoneOn = person.track(.microphone).map { !$0.muted } ?? false
        VStack(spacing: 4) {
            SoundRow(feature: feature, person: person, sound: .voice, name: name,
                     note: microphoneOn ? nil : t("microphone off"))
            // A screen's row only while it is shared with sound: a share whose
            // sound was never granted has nothing to turn down.
            if person.track(.screenShareAudio) != nil {
                SoundRow(feature: feature, person: person, sound: .screen, name: t("{name} (screen)", ["name": name]))
                    .transition(Theme.arrive)
            }
            if person.musicTrack != nil {
                SoundRow(feature: feature, person: person, sound: .music, name: t("{name} (music)", ["name": name]))
                    .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: person.track(.screenShareAudio)?.sid)
        .animation(Theme.settle, value: person.musicTrack?.sid)
    }
}

/// One sound's mute and volume (SoundPanel.tsx `SoundRow`).
private struct SoundRow: View {
    let feature: HostFeature
    let person: Person
    let sound: Sound
    let name: String
    var note: String?
    @State private var hovering = false

    var body: some View {
        let setting = feature.setting(person, sound)
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(name)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(setting.silenced ? Theme.muted : Theme.foreground)
                    .lineLimit(1)
                if let note {
                    Text(note)
                        .font(.system(size: 10.5))
                        .foregroundStyle(Theme.muted)
                        .transition(.opacity.animation(Theme.settle))
                }
            }
            HStack(spacing: 8) {
                // Named for what pressing it does, not for the state it is in:
                // the two read as opposites, and only one is being decided.
                Button {
                    feature.setBlocked(person, sound, !setting.blocked)
                } label: {
                    Image(systemName: setting.blocked ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .font(.system(size: 12))
                        .foregroundStyle(setting.blocked ? Theme.foreground : Theme.muted)
                        .frame(width: 26, height: 26)
                        .background(Circle().fill(setting.blocked ? Theme.surfaceHigh : .clear))
                        .contentTransition(.symbolEffect(.replace))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(setting.blocked ? t("Unmute {name}", ["name": name]) : t("Mute {name}", ["name": name]))
                .accessibilityAddTraits(setting.blocked ? .isSelected : [])

                // Disabled rather than hidden while blocked: nothing is
                // arriving to be made louder, and a slider that moves while the
                // sound stays off is a control that lies.
                // In twentieths, as the web's range steps, but rounded here
                // rather than given as a step: a stepped slider on the Mac
                // draws a tick for every one of them.
                Slider(value: Binding(
                    get: { feature.setting(person, sound).volume },
                    set: { feature.setVolume(person, sound, ($0 * 20).rounded() / 20) }
                ), in: 0...1)
                .controlSize(.small)
                .tint(Theme.foreground)
                .disabled(setting.blocked)
                .opacity(setting.blocked ? 0.4 : 1)
                .accessibilityLabel(t("{name}'s volume", ["name": name]))

                // Read at a glance, because a slider near one end tells nobody
                // whether it is at nought.
                Text(setting.blocked ? t("Muted") : "\(Int((setting.volume * 100).rounded()))%")
                    .font(.system(size: 10.5, design: .monospaced))
                    .monospacedDigit()
                    .foregroundStyle(Theme.muted)
                    .frame(width: 40, alignment: .trailing)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 8)
        .background(RoundedRectangle(cornerRadius: 8).fill(hovering ? Theme.surfaceHigh.opacity(0.6) : .clear))
        .onHover { hovering = $0 }
        .animation(Theme.settle, value: setting.blocked)
        .animation(Theme.settle, value: hovering)
    }
}
