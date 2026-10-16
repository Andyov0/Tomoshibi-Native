import AVFoundation
import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A camera or a microphone, for the two things said about either: whether
/// the system lets this app use it, and what to say when it would not start.
public enum DeviceKind: Sendable {
    case camera, microphone

    var media: AVMediaType { self == .camera ? .video : .audio }
}

/// The system said no: the person refused the camera or the microphone, or a
/// profile forbids it. The one failure with a place to go and fix it.
public struct DeviceRefused: Error, Sendable {
    public let kind: DeviceKind
}

enum DeviceAccess {
    /// Asks the system for the device where it has not been asked, and throws
    /// `DeviceRefused` where the answer was no.
    ///
    /// Asked before the device is opened rather than learned from it, because
    /// the microphone does not fail when it is refused: the engine starts and
    /// hands over silence, and a call that sends silence while its button
    /// says "on" was the fault this exists for. A camera refused does throw,
    /// but with an error that does not say which of the many reasons it was.
    static func ensure(_ kind: DeviceKind) async throws {
        switch AVCaptureDevice.authorizationStatus(for: kind.media) {
        case .authorized:
            return
        case .notDetermined:
            let granted = await AVCaptureDevice.requestAccess(for: kind.media)
            if !granted { throw DeviceRefused(kind: kind) }
        default:
            throw DeviceRefused(kind: kind)
        }
    }

    /// Opens the place the permission is given: the camera's or the
    /// microphone's own list in Privacy & Security on a Mac, this app's page
    /// of Settings on a phone, where both switches are.
    @MainActor
    static func openSettings(for kind: DeviceKind) {
        #if os(macOS)
        let pane = kind == .camera ? "Privacy_Camera" : "Privacy_Microphone"
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
            NSWorkspace.shared.open(url)
        }
        #else
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
        #endif
    }
}

extension CallModel {
    /// Says that a camera or a microphone would not start, and stays until it
    /// is dismissed (notices.ts `deviceFailed`): it is something somebody has
    /// to go and deal with.
    ///
    /// Two whole phrases rather than one with the device put in, because a
    /// sentence built around a noun has to agree with it in most languages
    /// (notices.ts). Where the system refused, the toast says where to allow
    /// it and has a button that goes there; the web points at the address
    /// bar's icon, which an app does not have. Anything else is the web's
    /// "Something else may be using it.", because pointing somebody at a
    /// permission they already gave sends them looking in the one place the
    /// fault is not.
    public func deviceFailed(_ kind: DeviceKind, _ error: Error) {
        let refused = (error as? DeviceRefused) != nil
        let title = kind == .camera ? t("Can't use your camera") : t("Can't use your microphone")
        #if os(macOS)
        let allow = t("Allow Tomoshibi in System Settings, then try again.")
        let open = t("Open System Settings")
        #else
        let allow = t("Allow Tomoshibi in Settings, then try again.")
        let open = t("Open Settings")
        #endif
        let toast = CallToast(title, detail: refused ? allow : t("Something else may be using it."),
                              kind: .failure, lasts: nil, actionTitle: refused ? open : nil,
                              key: kind == .camera ? "device.camera" : "device.microphone")
        if refused {
            announce(toast) { DeviceAccess.openSettings(for: kind) }
        } else {
            announce(toast)
        }
    }
}
