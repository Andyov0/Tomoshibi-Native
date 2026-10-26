import SwiftUI
import TomoshibiBasics

// The pieces the song desk and watching together are drawn with, beside
// Kit.swift's: a slider, a spinner, a field, a panel's header and its small
// round buttons, artwork, and the listener's row for one sound.
//
// Drawn rather than borrowed for the reason Kit.swift gives: the system's
// Slider is a hairline with a tick under it on the Mac and a different
// control on the iPhone, its ProgressView is grey on glass, and a plain
// TextField has no edge at all over a picture. These wear the same glass,
// the same chosen colour where something is on, and the same two curves
// (Theme.settle and Theme.leave) as everything else.

// MARK: - Slider

/// A capsule track that fills as it rises, with a knob: for a volume and for
/// a place in a video. Dragged anywhere along it, and adjusted by the
/// keyboard or VoiceOver a twentieth at a time.
struct MediaSlider: View {
    @Binding var value: Double
    var range: ClosedRange<Double> = 0...1
    /// The value is rounded to this as it is set: the web's volume ranges
    /// move in twentieths (`step={0.05}`).
    var step: Double?
    var tint: Color = Theme.foreground
    /// Told when a change starts and ends, for a scrubber that only seeks
    /// once it is let go. Every way of changing the value ends what it
    /// started: a drag that is let go, a drag the system takes away (a touch
    /// cancelled, the view going from under it), and a step from VoiceOver,
    /// which is a whole change on its own. Only a drag let
    /// go ended one, and a VoiceOver step or an interrupted drag left the
    /// watch scrubber editing for good: it never sought, its place stood
    /// still, and the controls over the video never hid again.
    var editing: (Bool) -> Void = { _ in }
    @State private var dragging = false
    @State private var hovering = false
    /// True while a drag is under way, and put back by the system however the
    /// drag ends, a cancelled one included, which `onEnded` never hears of.
    @GestureState private var held = false
    @Environment(\.isEnabled) private var enabled

    private var fraction: Double {
        let span = range.upperBound - range.lowerBound
        guard span > 0 else { return 0 }
        return min(1, max(0, (value - range.lowerBound) / span))
    }

    var body: some View {
        GeometryReader { geometry in
            let width = max(1, geometry.size.width)
            let lifted = dragging || hovering
            let track: CGFloat = lifted ? 6 : 4
            let knob: CGFloat = 14
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.ink.opacity(0.14)).frame(height: track)
                Capsule().fill(tint.opacity(enabled ? 1 : 0.5))
                    .frame(width: max(track, (width - knob) * fraction + knob / 2), height: track)
                Circle()
                    .fill(Color.white)
                    .shadow(color: .black.opacity(0.35), radius: 2, y: 1)
                    .frame(width: knob, height: knob)
                    .scaleEffect(dragging ? 1.18 : 1)
                    .offset(x: (width - knob) * fraction)
            }
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .updating($held) { _, held, _ in held = true }
                    .onChanged { drag in
                        if !dragging {
                            dragging = true
                            editing(true)
                        }
                        set(Double((drag.location.x - knob / 2) / (width - knob)))
                    }
                    .onEnded { drag in
                        // Not after a finish already said, whose change would
                        // then be one nobody is told the end of.
                        if dragging { set(Double((drag.location.x - knob / 2) / (width - knob))) }
                        finish()
                    }
            )
        }
        .frame(height: 22)
        .onHover { hovering = $0 }
        // A drag that ended without `onEnded`: cancelled by the system.
        .onChange(of: held) { _, now in
            if !now { finish() }
        }
        .onDisappear { finish() }
        .animation(Theme.settle, value: dragging)
        .animation(.easeOut(duration: 0.15), value: hovering)
        .accessibilityElement()
        .accessibilityValue("\(Int((fraction * 100).rounded()))%")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: step(by: 1)
            case .decrement: step(by: -1)
            @unknown default: break
            }
        }
    }

    /// One step of a twentieth, from VoiceOver: begun, moved and finished at
    /// once, so a scrubber seeks to it as it would to a drag let go there.
    func step(by steps: Double) {
        let nudge = (range.upperBound - range.lowerBound) / 20
        let next = min(range.upperBound, max(range.lowerBound, value + steps * nudge))
        editing(true)
        if next != value { value = next }
        editing(false)
    }

    /// The end of a drag, however it came: said once.
    private func finish() {
        guard dragging else { return }
        dragging = false
        editing(false)
    }

    private func set(_ fraction: Double) {
        let clamped = min(1, max(0, fraction))
        var next = range.lowerBound + clamped * (range.upperBound - range.lowerBound)
        if let step, step > 0 { next = (next / step).rounded() * step }
        if next != value { value = next }
    }
}

// MARK: - Waiting

/// An arc that turns: the app's own spinner, light on glass and on a picture
/// alike, where the system's is a grey that disappears on either.
struct MediaSpinner: View {
    var size: CGFloat = 16
    var tint: Color = Theme.foreground
    @State private var turning = false

    var body: some View {
        Circle()
            .trim(from: 0.1, to: 0.78)
            .stroke(tint.opacity(0.85), style: StrokeStyle(lineWidth: max(1.6, size / 9), lineCap: .round))
            .frame(width: size, height: size)
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(.linear(duration: 0.85).repeatForever(autoreverses: false), value: turning)
            .onAppear { turning = true }
            .accessibilityHidden(true)
    }
}

/// How far through something is, as a thin bar that fills with the chosen
/// colour.
struct MediaProgress: View {
    let fraction: Double
    var tint: Color = Theme.tally

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.ink.opacity(0.12))
                Capsule().fill(tint).frame(width: geometry.size.width * min(1, max(0, fraction)))
            }
        }
        .frame(height: 3)
        .animation(.linear(duration: 0.5), value: fraction)
        .accessibilityHidden(true)
    }
}

// MARK: - Typing

/// A field in a sunk capsule, whose edge takes the chosen colour
/// while it is being typed in. `limit` holds it to as many UTF-16 units as
/// the web's field takes (`maxLength`).
struct MediaField: View {
    let prompt: String
    @Binding var text: String
    var symbol: String?
    var limit: Int?
    var url = false
    let submit: () -> Void
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(focused ? Theme.foreground : Theme.muted)
            }
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
                .foregroundStyle(Theme.foreground)
                .focused($focused)
                .onSubmit(submit)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(url ? .URL : .default)
                .submitLabel(.go)
                #endif
        }
        .padding(.horizontal, 12)
        .frame(height: 34)
        .background(Capsule().fill(Theme.sunk(0.24)))
        .overlay(Capsule().stroke(focused ? Theme.tally.opacity(0.75) : Theme.ink.opacity(0.09), lineWidth: focused ? 1.5 : 1))
        .contentShape(Capsule())
        .onTapGesture { focused = true }
        .animation(Theme.settle, value: focused)
        .onChange(of: text) {
            if let limit, text.utf16.count > limit { text = WatchWords.cut(text, limit) }
        }
    }
}

// MARK: - Panels

/// The top of a panel beside the call: its name, and the button that closes
/// it.
struct MediaPanelHeader: View {
    let title: String
    var symbol: String?
    let closeLabel: String
    let close: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 11.5, weight: .bold))
                    .foregroundStyle(Theme.tallyForeground)
                    .frame(width: 24, height: 24)
                    .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Theme.tally))
            }
            Text(title)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Theme.foreground)
            Spacer(minLength: 8)
            MediaIconButton(symbol: "xmark", label: closeLabel, action: close)
        }
        .padding(.leading, 14)
        .padding(.trailing, 10)
        .padding(.vertical, 10)
    }
}

extension View {
    /// The glass a panel beside the call is drawn on, darkened from inside.
    ///
    /// These panels sit over the stage, and the stage is often a film: over
    /// a bright picture the system's clear glass let the colours straight
    /// through, and grey words on it could not be read (seen with the watch
    /// panel over a test card). A shade of the call's own black under the
    /// content keeps the glass's edge and light and gives the words a ground.
    func mediaPanelGlass(cornerRadius: CGFloat = 22) -> some View {
        background(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).fill(Theme.background.opacity(0.68)))
            .glassShell(cornerRadius: cornerRadius)
    }
}

/// A hairline between the parts of a panel.
struct MediaRule: View {
    var body: some View {
        Rectangle().fill(Theme.ink.opacity(0.07)).frame(height: 1)
    }
}

/// A small round button that lights under the pointer: closing, taking a
/// song off, playing one next. `lit` fills it with the chosen colour, for
/// something that is on.
struct MediaIconButton: View {
    let symbol: String
    let label: String
    var size: CGFloat = 26
    var lit = false
    var prominent = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size * 0.42, weight: .semibold))
                .foregroundStyle(lit || prominent ? Theme.tallyForeground : hovering ? Theme.foreground : Theme.muted)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: size, height: size)
                .background(Circle().fill(lit || prominent ? Theme.tally : Theme.ink.opacity(hovering && enabled ? 0.12 : 0)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .animation(Theme.settle, value: lit)
        .accessibilityLabel(label)
        .help(label)
    }
}

/// A button inside a panel: a capsule of light glass, the chosen colour for
/// the one thing the panel is for, or no fill at all for a quiet one.
struct MediaButtonStyle: ButtonStyle {
    var prominent = false
    var quiet = false
    var height: CGFloat = 30
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12.5, weight: .semibold))
            .lineLimit(1)
            .foregroundStyle(prominent ? Theme.tallyForeground : quiet ? Theme.muted : Theme.foreground)
            .padding(.horizontal, 12)
            .frame(minHeight: height)
            .background(Capsule().fill(prominent ? Theme.tally : quiet ? Color.clear : Theme.ink.opacity(configuration.isPressed ? 0.16 : 0.10)))
            .overlay(Capsule().stroke(Theme.ink.opacity(prominent || quiet ? 0 : 0.08)))
            .contentShape(Capsule())
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .opacity(enabled ? 1 : 0.4)
            .animation(Theme.settle, value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == MediaButtonStyle {
    static func media(prominent: Bool = false, quiet: Bool = false, height: CGFloat = 30) -> MediaButtonStyle {
        MediaButtonStyle(prominent: prominent, quiet: quiet, height: height)
    }
}

/// Something said quietly in a panel where there is nothing to show yet:
/// a symbol in a soft circle, and a sentence.
struct MediaEmpty: View {
    let symbol: String
    let text: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(Theme.muted)
                .frame(width: 44, height: 44)
                .background(Circle().fill(Theme.ink.opacity(0.07)))
            Text(text)
                .font(.system(size: 12))
                .foregroundStyle(Theme.muted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
        .padding(.vertical, 28)
    }
}

/// A line of red words in a panel, for what did not work.
struct MediaProblem: View {
    let text: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: "exclamationmark.circle.fill").font(.system(size: 11))
            Text(text).font(.system(size: 12)).fixedSize(horizontal: false, vertical: true)
        }
        .foregroundStyle(Theme.dangerInk)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Theme.danger.opacity(0.12)))
    }
}

/// A row of a list in a panel that lights under the pointer.
struct MediaRow<Content: View>: View {
    var lit = false
    @ViewBuilder var content: (Bool) -> Content
    @State private var hovering = false

    var body: some View {
        content(hovering)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Theme.ink.opacity(lit ? 0.09 : hovering ? 0.06 : 0)))
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .onHover { hovering = $0 }
            .animation(.easeOut(duration: 0.12), value: hovering)
    }
}

// MARK: - Pictures

/// A cover or a poster where there is one, and a square in the chosen colour
/// with a symbol where there is not; faded in when it arrives.
///
/// The square is flat: the raised grey with the colour laid over it at 18
/// per cent, as MyShareFace's plate is. It ran from the colour into the grey
/// corner to corner before the owner asked for fewer gradients (2026-10-20).
struct MediaArtwork: View {
    let url: String
    var symbol = "music.note"
    var corner: CGFloat = 8

    var body: some View {
        let placeholder = ZStack {
            Theme.surfaceHigh
            Theme.tally.opacity(0.18)
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Theme.foreground.opacity(0.7))
        }
        ZStack {
            placeholder
            if let link = URL(string: url), url.hasPrefix("http") {
                AsyncImage(url: link, transaction: Transaction(animation: Theme.settle)) { phase in
                    if let image = phase.image {
                        image.resizable().scaledToFill().transition(.opacity)
                    }
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: corner, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: corner, style: .continuous).stroke(Theme.ink.opacity(0.08)))
        .accessibilityHidden(true)
    }
}

/// A few words in a small capsule: what a song is, that it is paused.
struct MediaBadge: View {
    let text: String
    var symbol: String?
    var lit = false

    var body: some View {
        HStack(spacing: 4) {
            if let symbol { Image(systemName: symbol).font(.system(size: 8.5, weight: .bold)) }
            Text(text).font(.system(size: 10.5, weight: .semibold)).lineLimit(1)
        }
        .foregroundStyle(lit ? Theme.tallyInk : Theme.foreground.opacity(0.75))
        .padding(.horizontal, 7)
        .padding(.vertical, 3)
        .background(Capsule().fill(lit ? Theme.tally.opacity(0.16) : Theme.ink.opacity(0.09)))
        .overlay(Capsule().stroke(lit ? Theme.tally.opacity(0.35) : .clear))
    }
}

// MARK: - Hearing

/// The listener's own setting for one sound, as the sound panel's row has it
/// (SoundPanel.tsx `SoundRow`): a button that blocks it at the media server,
/// a volume, and the volume said in figures, because a slider near one end
/// tells nobody whether it is at nought.
struct HearingRow: View {
    let name: String
    let setting: HearingSetting
    let setVolume: (Double) -> Void
    let setBlocked: (Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(name)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(setting.silenced ? Theme.muted : Theme.foreground)
                .lineLimit(1)
            HStack(spacing: 8) {
                // Named for what pressing it does, as the sound panel's is.
                MediaIconButton(symbol: setting.blocked ? "speaker.slash.fill" : "speaker.wave.2.fill",
                            label: setting.blocked ? t("Unmute {name}", ["name": name]) : t("Mute {name}", ["name": name]),
                            size: 28, lit: setting.blocked) {
                    setBlocked(!setting.blocked)
                }
                // Disabled rather than hidden while blocked: nothing arrives
                // to be made louder, and a slider that moves while the sound
                // stays off is a control that lies.
                MediaSlider(value: Binding(get: { setting.volume }, set: setVolume), step: 0.05)
                    .disabled(setting.blocked)
                    .opacity(setting.blocked ? 0.45 : 1)
                    .accessibilityLabel(t("{name}'s volume", ["name": name]))
                Text(setting.blocked ? t("Muted") : "\(Int((setting.volume * 100).rounded()))%")
                    .font(.system(size: 11, weight: .medium).monospacedDigit())
                    .foregroundStyle(Theme.muted)
                    .frame(width: 42, alignment: .trailing)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .contentTransition(.numericText())
            }
        }
        .animation(Theme.settle, value: setting.blocked)
    }
}
