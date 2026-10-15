import AVFoundation
import AVKit
import SwiftUI
import TomoshibiCore

/// Which camera, microphone and speakers, and how the voice is treated
/// (components/room/DeviceMenu.tsx), with the language beside them.
///
/// Background blur is the system's on Apple platforms: the camera's own video
/// effects, which every app gets for nothing and which run on the Neural
/// Engine rather than in a canvas. The panel opens the system's control for
/// them instead of carrying a segmentation model as the web must.
struct DevicesPanel: View {
    @Bindable var model: CallModel
    let close: () -> Void
    @State private var original = false
    @State private var cameras: [AVCaptureDevice] = []
    @State private var microphones: [AudioDevice] = []
    @State private var speakers: [AudioDevice] = []
    @State private var language = Localization.shared.language

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(t("Devices")).font(.system(size: 12.5, weight: .semibold)).foregroundStyle(Theme.foreground)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t("Close"))
            }
            section(t("Camera")) {
                if cameras.isEmpty {
                    Text(t("No devices found")).foregroundStyle(Theme.muted)
                }
                ForEach(cameras, id: \.uniqueID) { camera in
                    choice(camera.localizedName, chosen: model.camera.device?.uniqueID == camera.uniqueID) {
                        try? model.useCamera(camera)
                    }
                }
                Button(t("Blur my background")) { AVCaptureDevice.showSystemUserInterface(.videoEffects) }
                    .buttonStyle(.plain).foregroundStyle(Theme.foreground)
            }
            section(t("Microphone")) {
                if microphones.isEmpty {
                    Text(t("No devices found")).foregroundStyle(Theme.muted)
                }
                ForEach(microphones) { microphone in
                    choice(microphone.name, chosen: false) { model.audio.use(microphone, input: true) }
                }
                Toggle(isOn: $original) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(t("Original sound")).foregroundStyle(Theme.foreground)
                        Text(t("No noise suppression or automatic volume. For headphones and music."))
                            .font(.system(size: 11)).foregroundStyle(Theme.muted)
                    }
                }
                .onChange(of: original) { Task { try? await model.setOriginalSound(original) } }
            }
            section(t("Speakers")) {
                #if os(iOS)
                RoutePicker().frame(width: 32, height: 32)
                #else
                ForEach(speakers) { speaker in
                    choice(speaker.name, chosen: false) { model.audio.use(speaker, input: false) }
                }
                #endif
            }
            section(t("Language")) {
                Picker(t("Language"), selection: $language) {
                    ForEach(Language.allCases, id: \.self) { language in
                        Text(language.name).tag(language)
                    }
                }
                .labelsHidden()
                .onChange(of: language) { Localization.shared.language = language }
            }
        }
        .font(.system(size: 13))
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 12).fill(Theme.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.border))
        .onAppear {
            original = model.audio.originalSound
            cameras = CameraCapture.cameras()
            microphones = CallAudio.devices(input: true)
            speakers = CallAudio.devices(input: false)
        }
    }

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.muted)
            content()
        }
    }

    private func choice(_ name: String, chosen: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Text(name).foregroundStyle(Theme.foreground).lineLimit(1)
                Spacer()
                if chosen { Image(systemName: "checkmark").foregroundStyle(Theme.tally).transition(Theme.arrive) }
            }
        }
        .buttonStyle(.plain)
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
