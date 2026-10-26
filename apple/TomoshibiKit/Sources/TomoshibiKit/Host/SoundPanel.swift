import SwiftUI
import TomoshibiCore

/// How loud everybody else is (components/room/SoundPanel.tsx).
///
/// A list of what can be heard rather than of what can be seen: the person who
/// is too loud may be on no tile at all. One row per thing there is to hear,
/// because a shared screen's sound and the voice describing it arrive on
/// separate tracks and are turned down separately; nobody hears their own
/// microphone, so there is no row for it. Each person's rows share a card, so
/// that whose sound a slider is never has to be read twice.
struct SoundPanelView: View {
    let feature: HostFeature
    let close: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 0)
            VStack(spacing: 0) {
                PanelHeading(title: t("Sound"), symbol: "speaker.wave.2.fill", closeLabel: t("Close sound"), close: close)
                PanelRule()
                ViewThatFits(in: .vertical) {
                    rows
                    ScrollView { rows }.scrollIndicators(.hidden)
                }
            }
            .glassPane(cornerRadius: 22)
        }
    }

    @ViewBuilder
    private var rows: some View {
        let others = feature.call.people
        VStack(spacing: 8) {
            if others.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: "speaker.wave.2")
                        .font(.system(size: 22, weight: .light))
                        .foregroundStyle(Theme.foreground.opacity(0.4))
                    Text(t("Nobody else is here."))
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.muted)
                }
                .padding(.vertical, 26)
                .frame(maxWidth: .infinity)
                .transition(Theme.arrive)
            } else {
                ForEach(others) { person in
                    PersonSounds(feature: feature, person: person)
                        .transition(Theme.arrive)
                }
            }
        }
        .padding(10)
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
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 9) {
                Monogram(name: name, identity: person.id, size: 26)
                Text(name)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(1)
                if !microphoneOn {
                    Text(t("microphone off"))
                        .font(.system(size: 10.5, weight: .medium))
                        .foregroundStyle(Theme.muted)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(Capsule().fill(Theme.ink.opacity(0.07)))
                        .transition(.opacity.combined(with: .scale(scale: 0.9)))
                }
                Spacer(minLength: 0)
            }
            SoundRow(feature: feature, person: person, sound: .voice, name: name)
            // A screen's row only while it is shared with sound: a share whose
            // sound was never granted has nothing to turn down.
            if person.track(.screenShareAudio) != nil {
                SoundRow(feature: feature, person: person, sound: .screen, name: t("{name} (screen)", ["name": name]),
                         symbol: "rectangle.on.rectangle")
                    .transition(Theme.arrive)
            }
            // And the song desk's music, a track of its own beside a screen's
            // sound, turned down on its own.
            if person.musicTrack != nil {
                SoundRow(feature: feature, person: person, sound: .music, name: t("{name} (music)", ["name": name]),
                         symbol: "music.note")
                    .transition(Theme.arrive)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .panelCard()
        .animation(Theme.settle, value: microphoneOn)
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
    /// Drawn with its name above the slider when it is not the voice, which
    /// the card's own heading already names.
    var symbol: String?

    var body: some View {
        let setting = feature.setting(person, sound)
        VStack(alignment: .leading, spacing: 4) {
            if let symbol {
                HStack(spacing: 6) {
                    Image(systemName: symbol)
                        .font(.system(size: 10, weight: .semibold))
                    Text(name)
                        .font(.system(size: 11.5, weight: .medium))
                        .lineLimit(1)
                }
                .foregroundStyle(setting.silenced ? Theme.faint(0.7) : Theme.muted)
                .padding(.leading, 2)
            }
            HStack(spacing: 10) {
                // Named for what pressing it does, not for the state it is in:
                // the two read as opposites, and only one is being decided.
                Button {
                    feature.setBlocked(person, sound, !setting.blocked)
                } label: {
                    Image(systemName: setting.blocked ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .font(.system(size: 11.5, weight: .semibold))
                        .foregroundStyle(setting.blocked ? Theme.foreground : Theme.muted)
                        .contentTransition(.symbolEffect(.replace))
                        .frame(width: 28, height: 28)
                        .background(Circle().fill(setting.blocked ? Theme.ink.opacity(0.16) : Theme.ink.opacity(0.06)))
                        .overlay(Circle().stroke(Theme.ink.opacity(setting.blocked ? 0.16 : 0.06), lineWidth: 1))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(setting.blocked ? t("Unmute {name}", ["name": name]) : t("Mute {name}", ["name": name]))
                .accessibilityAddTraits(setting.blocked ? .isSelected : [])
                .help(setting.blocked ? t("Unmute {name}", ["name": name]) : t("Mute {name}", ["name": name]))

                // Disabled rather than hidden while blocked: nothing is
                // arriving to be made louder, and a slider that moves while the
                // sound stays off is a control that lies.
                GlassSlider(value: Binding(
                    get: { feature.setting(person, sound).volume },
                    set: { feature.setVolume(person, sound, $0) }
                ), label: t("{name}'s volume", ["name": name]))
                .disabled(setting.blocked)

                // Read at a glance, because a slider near one end tells nobody
                // whether it is at nought.
                Text(setting.blocked ? t("Muted") : "\(Int((setting.volume * 100).rounded()))%")
                    .font(.system(size: 11, weight: .medium).monospacedDigit())
                    .foregroundStyle(Theme.muted)
                    .lineLimit(1)
                    .fixedSize()
                    .frame(minWidth: 42, alignment: .trailing)
            }
        }
        .animation(Theme.settle, value: setting.blocked)
    }
}
