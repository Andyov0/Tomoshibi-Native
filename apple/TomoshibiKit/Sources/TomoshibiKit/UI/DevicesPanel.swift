import AVFoundation
import AVKit
import os
import SwiftUI
import TomoshibiCore

/// Which microphone, speakers and camera, and how the voice is treated
/// (components/room/DeviceMenu.tsx), with the language beside them.
///
/// Each list ticks what the call is using now and follows the machine: a
/// headset plugged in while the panel is open is in it at once, as the web's
/// menu listens for `devicechange`. A press moves the tick at once and moves
/// it back if the device would not come (DeviceMenu.tsx `select`), and a
/// choice that took is remembered for the next call.
///
/// Background blur is the system's on Apple platforms: the camera's own video
/// effects, which every app gets for nothing and which run on the Neural
/// Engine rather than in a canvas. The panel opens the system's control for
/// them instead of carrying a segmentation model as the web must.
struct DevicesPanel: View {
    @Bindable var model: CallModel
    let close: () -> Void
    @State private var lists = DeviceLists()
    @State private var changingOriginal = false
    /// Shown from the camera's own state, which is not observed: read when
    /// the panel opens and written by the switch.
    @State private var blurring = false
    @State private var language = Localization.shared.language

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            microphone
            #if os(macOS)
            speakers
            #else
            SettingsSection(title: t("Speakers")) {
                SettingsRow(title: t("Speakers"), symbol: "speaker.wave.2.fill", last: true) {
                    RoutePicker().frame(width: 30, height: 30)
                }
            }
            #endif
            camera
            SettingsSection(title: t("Language")) {
                ForEach(Array(Language.allCases.enumerated()), id: \.element) { index, one in
                    ChoiceRow(title: one.name, checked: language == one, last: index == Language.allCases.count - 1) {
                        withAnimation(Theme.settle) { language = one }
                        Localization.shared.language = one
                    }
                }
            }
        }
        .padding(16)
        // Opaque, as every panel is (Kit `panelSurface`).
        .panelSurface()
        .animation(Theme.settle, value: lists.microphones)
        .animation(Theme.settle, value: lists.speakers)
        .animation(Theme.settle, value: lists.cameras)
        .animation(Theme.settle, value: model.microphoneOn)
        .onAppear {
            lists.start(model)
            blurring = model.blurring
        }
        .onDisappear { lists.stop() }
    }

    private var header: some View {
        HStack {
            Text(t("Devices"))
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Theme.foreground)
            Spacer()
            CloseButton(label: t("Close"), action: close)
        }
    }

    // MARK: The microphone

    private var microphone: some View {
        SettingsSection(title: t("Microphone"), footer: lists.choosable ? nil : choosingElsewhere) {
            if lists.microphones.isEmpty {
                empty
            }
            ForEach(lists.microphones) { one in
                ChoiceRow(title: one.name, symbol: "mic", checked: lists.microphone == one.id) {
                    choose(one, input: true)
                }
                .disabled(!lists.choosable)
                .transition(Theme.arrive)
            }
            // How loud it is as it goes, while it is on (MicLevel.tsx): the
            // question somebody opening this panel is asking is "is it this
            // one", and a meter that moves when they speak answers it.
            if model.microphoneOn {
                MicrophoneMeterRow(audio: model.audio)
                    .transition(Theme.arrive)
            }
            // Under the microphone because it is a property of the voice, the
            // way blur is a property of the picture; applied to the live
            // microphone, so the difference can be heard before deciding.
            SettingsRow(title: t("Original sound"), detail: t("No noise suppression or automatic volume. For headphones and music."),
                        symbol: "waveform.path", last: true) {
                Toggle("", isOn: Binding(get: { lists.original }, set: setOriginal))
                    .labelsHidden()
                    .toggleStyle(.glassSwitch)
                    .disabled(changingOriginal)
            }
        }
    }

    private func setOriginal(_ on: Bool) {
        guard !changingOriginal else { return }
        lists.original = on
        changingOriginal = true
        Task {
            // A failure is said by the call (deviceFailed), and the refresh
            // puts the switch back where the voice really is.
            try? await model.setOriginalSound(on)
            changingOriginal = false
            lists.refresh()
        }
    }

    /// Why the lists cannot be chosen from: only the voice-processing unit
    /// takes a device for each direction (CallAudio `choosable`).
    private var choosingElsewhere: String {
        model.audio.originalSound
            ? t("Turn off Original sound to choose another here.")
            : t("This Mac uses the microphone and speakers chosen in System Settings.")
    }

    // MARK: The speakers

    #if os(macOS)
    private var speakers: some View {
        SettingsSection(title: t("Speakers")) {
            if lists.speakers.isEmpty {
                empty
            }
            ForEach(Array(lists.speakers.enumerated()), id: \.element.id) { index, one in
                ChoiceRow(title: one.name, symbol: "speaker.wave.2", checked: lists.speaker == one.id,
                          last: index == lists.speakers.count - 1) {
                    choose(one, input: false)
                }
                .disabled(!lists.choosable)
                .transition(Theme.arrive)
            }
        }
    }
    #endif

    private func choose(_ device: AudioDevice, input: Bool) {
        let before = input ? lists.microphone : lists.speaker
        withAnimation(Theme.settle) {
            if input { lists.microphone = device.id } else { lists.speaker = device.id }
        }
        do {
            try model.useAudio(device, input: input)
        } catch {
            withAnimation(Theme.settle) {
                if input { lists.microphone = before } else { lists.speaker = before }
            }
        }
        lists.refresh()
    }

    // MARK: The camera

    private var camera: some View {
        SettingsSection(title: t("Camera")) {
            if lists.cameras.isEmpty {
                empty
            }
            ForEach(lists.cameras, id: \.uniqueID) { one in
                ChoiceRow(title: one.localizedName, symbol: "video",
                          checked: lists.camera == one.uniqueID,
                          last: !model.blurOffered && one.uniqueID == lists.cameras.last?.uniqueID) {
                    chooseCamera(one)
                }
                .transition(Theme.arrive)
            }
            // The app's own blur (BackgroundBlur), from the next frame, in
            // what is sent and in the picture of yourself. It opened the
            // system's video effects, a sheet of the system's that did not
            // look like the app and on an iPhone did not reach the call.
            // Gone where the deployment has switched blur off (Remote); a
            // blur already on stays on for this call (CallModel `blurKept`).
            if model.blurOffered {
                SettingsRow(title: t("Blur my background"), symbol: "person.and.background.dotted", last: true) {
                    Toggle("", isOn: Binding(get: { blurring }, set: { on in
                        blurring = on
                        model.setBlur(on)
                    }))
                    .labelsHidden()
                    .toggleStyle(.glassSwitch)
                }
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: model.blurOffered)
    }

    /// A camera picked with the camera off is the one it comes on with: the
    /// tick moves, and nothing is opened until then.
    private func chooseCamera(_ device: AVCaptureDevice) {
        let before = lists.camera
        withAnimation(Theme.settle) { lists.camera = device.uniqueID }
        do {
            try model.useCamera(device)
        } catch {
            withAnimation(Theme.settle) { lists.camera = before }
        }
    }

    /// An empty list says so, rather than being a heading over nothing.
    private var empty: some View {
        Text(t("No devices found"))
            .font(.system(size: 13))
            .foregroundStyle(Theme.muted)
            .padding(.horizontal, 14)
            .frame(maxWidth: .infinity, minHeight: 40, alignment: .leading)
            .transition(Theme.arrive)
    }
}

/// What the devices panel lists, and what the call is using, kept up to date
/// while the panel is open.
@MainActor
@Observable
final class DeviceLists {
    var microphones: [AudioDevice] = []
    var speakers: [AudioDevice] = []
    var cameras: [AVCaptureDevice] = []
    var microphone: String?
    var speaker: String?
    var camera: String?
    var choosable = true
    /// Whether the voice goes as it is, read from the engine: it is also
    /// turned on from outside the panel (a remembered choice at the join).
    var original = false
    @ObservationIgnored private weak var model: CallModel?
    @ObservationIgnored private var watch: DeviceWatch?

    static let log = Logger(subsystem: "app.tomoshibi", category: "devices")

    func start(_ model: CallModel) {
        self.model = model
        refresh()
        watch = DeviceWatch { [weak self] in self?.soon() }
        // Which way each camera is drawn, for a test to read (facing.ts).
        for one in cameras {
            Self.log.notice("camera \(one.localizedName, privacy: .public): \(CameraCapture.facesUser(one) ? "mirrored" : "as it is", privacy: .public)")
        }
    }

    func stop() {
        watch?.stop()
        watch = nil
    }

    @ObservationIgnored private var pending = false

    /// One refresh for a burst of changes: an engine starting adds and takes
    /// away its aggregate devices and changes the defaults, which is half a
    /// dozen notices within a few milliseconds.
    private func soon() {
        guard !pending else { return }
        pending = true
        Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard let self else { return }
            self.pending = false
            withAnimation(Theme.settle) { self.refresh() }
        }
    }

    func refresh() {
        guard let model else { return }
        microphones = CallAudio.devices(input: true)
        speakers = CallAudio.devices(input: false)
        cameras = CameraCapture.cameras()
        microphone = model.audio.current(input: true)
        speaker = model.audio.current(input: false)
        camera = model.camera.current
        choosable = model.audio.choosable
        original = model.audio.originalSound
    }
}

/// The microphone's level as it is sent, and what that means in words
/// (MicLevel.tsx): "Say something" until anything has been heard, "Hearing
/// you" once it has, "No sound from it" when the microphone hands over
/// nothing at all, which a meter that never moves cannot tell from silence.
private struct MicrophoneMeterRow: View {
    let audio: CallAudio
    @State private var heard = false
    @State private var opened = Date()

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30)) { context in
            let level = audio.microphoneLevel
            let shown = MicrophoneLevel.shown(rms: level?.rms ?? 0)
            // Nothing delivered at all two seconds after it was opened: a tap
            // that never fires is a device that is not there, not a quiet room.
            let silent = (level?.lastChunk ?? 0) == 0 && context.date.timeIntervalSince(opened) > 2
            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    Image(systemName: "waveform")
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(Theme.muted)
                        .frame(width: 26)
                    LevelMeter(level: shown, segments: 18)
                        .accessibilityLabel(t("Microphone level"))
                    Spacer(minLength: 8)
                    Text(silent ? t("No sound from it") : heard ? t("Hearing you") : t("Say something"))
                        .font(.system(size: 11.5))
                        .foregroundStyle(heard && !silent ? Theme.foreground : Theme.muted)
                        .lineLimit(1)
                        .contentTransition(.opacity)
                        .animation(Theme.settle, value: heard)
                }
                .padding(.horizontal, 14)
                .frame(minHeight: 40)
                .onChange(of: shown > MicrophoneLevel.heard) { _, loud in
                    if loud && !heard { heard = true }
                }
                Rectangle().fill(Theme.ink.opacity(0.06)).frame(height: 1)
                    .padding(.leading, 52)
            }
        }
        .onAppear { opened = Date() }
    }
}

/// The small round close button at the top of a panel.
private struct CloseButton: View {
    let label: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        // Thirty-two to the eye and a fingertip to the touch, as every
        // panel's close is (PanelHeading).
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(hovering ? Theme.foreground : Theme.muted)
                .frame(width: 32, height: 32)
                .background(Circle().fill(Theme.ink.opacity(hovering ? 0.16 : 0.08)))
                .contentShape(Circle())
                .fingertip(32)
        }
        .buttonStyle(BarPress())
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.12), value: hovering)
        .accessibilityLabel(label)
        .help(label)
    }
}

#if os(iOS)
/// The system's own choice of where sound goes: speaker, receiver, AirPods.
private struct RoutePicker: UIViewRepresentable {
    func makeUIView(context: Context) -> AVRoutePickerView {
        let view = AVRoutePickerView()
        view.tintColor = UIColor(Theme.foreground)
        view.activeTintColor = UIColor(Theme.tally)
        return view
    }
    func updateUIView(_ view: AVRoutePickerView, context: Context) {}
}
#endif
