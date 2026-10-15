#if os(iOS)
import ReplayKit
import SwiftUI
import TomoshibiBasics

/// The system's broadcast picker, kept out of sight and pressed by the share
/// button: a share of the whole phone can only be started from the system's
/// own sheet, and the picker is the only way an app may open it.
struct BroadcastPicker: UIViewRepresentable {
    @MainActor private static weak var current: RPSystemBroadcastPickerView?

    func makeUIView(context: Context) -> RPSystemBroadcastPickerView {
        let picker = RPSystemBroadcastPickerView(frame: CGRect(x: 0, y: 0, width: 44, height: 44))
        picker.preferredExtension = BroadcastWire.extensionBundle
        picker.showsMicrophoneButton = false
        Self.current = picker
        return picker
    }

    func updateUIView(_ view: RPSystemBroadcastPickerView, context: Context) {}

    @MainActor static func press() {
        guard let picker = current else { return }
        for case let button as UIButton in picker.subviews {
            button.sendActions(for: .touchUpInside)
        }
    }
}
#endif
