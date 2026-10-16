import AVFoundation
import Foundation
#if os(iOS)
import UIKit
#endif
import TomoshibiCore
import VideoToolbox

/// The camera, sent to a video input.
///
/// The size is the web's (share.rs `camera_capture`, room.ts ~562-566): a
/// phone sends 640x360 to spare its uplink, a tablet or a computer 1280x720.
/// Captured in the 4:2:0 bi-planar format the hardware encoder takes directly;
/// a phone's camera offers 720p rather than 360p, so its frames go through the
/// hardware scaler on the way and are otherwise untouched.
public final class CameraCapture: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate, @unchecked Sendable {
    public let session = AVCaptureSession()
    private let output = AVCaptureVideoDataOutput()
    private let queue = DispatchQueue(label: "tomoshibi.camera")
    private var input: VideoInput?
    public private(set) var device: AVCaptureDevice?
    #if os(iOS)
    /// Keeps the picture upright however the phone is held: the system says
    /// which way is up for this camera, and the connection is turned to match.
    private var rotation: AVCaptureDevice.RotationCoordinator?
    private var rotationWatch: NSKeyValueObservation?
    #endif
    /// What this device sends, by the web's rule for its kind.
    public static let size: TomoshibiCore.CameraCapture = {
        #if os(iOS)
        return cameraCapture(device: UIDevice.current.userInterfaceIdiom == .pad ? .tablet : .phone)
        #else
        return cameraCapture(device: .computer)
        #endif
    }()
    public static var width: UInt32 { size.width }
    public static var height: UInt32 { size.height }
    private var scaler: VTPixelTransferSession?
    private var pool: CVPixelBufferPool?

    public override init() {
        super.init()
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange]
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: queue)
    }

    public static func cameras() -> [AVCaptureDevice] {
        #if os(macOS)
        let types: [AVCaptureDevice.DeviceType] = [.builtInWideAngleCamera, .external, .continuityCamera, .deskViewCamera]
        #else
        let types: [AVCaptureDevice.DeviceType] = [.builtInWideAngleCamera, .builtInUltraWideCamera, .builtInTrueDepthCamera]
        #endif
        return AVCaptureDevice.DiscoverySession(deviceTypes: types, mediaType: .video, position: .unspecified).devices
    }

    /// The camera picked for the next time the camera goes on, by its unique
    /// ID: one chosen from the devices panel while the camera was off, or
    /// remembered from an earlier call (`meet-live.device-choice`). Choosing
    /// a camera with the camera off used to do nothing at all, so the camera
    /// came up as the default and had to be chosen again once it was on.
    public var preferred: String?

    /// The camera that is sending, or that would be sent if the camera went
    /// on now: what the devices panel ticks.
    public var current: String? {
        if let device, session.isRunning { return device.uniqueID }
        let present = Self.cameras().map(\.uniqueID)
        if let preferred, present.contains(preferred) { return preferred }
        return Self.fallback()?.uniqueID
    }

    /// The camera used when nothing was picked: the front one on a phone or
    /// a tablet, the system's default on a Mac.
    private static func fallback() -> AVCaptureDevice? {
        #if os(iOS)
        return AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .front) ?? AVCaptureDevice.default(for: .video)
        #else
        return AVCaptureDevice.default(for: .video)
        #endif
    }

    /// Whether one's own picture from this camera is shown mirrored
    /// (live/facing.ts): a camera that films its owner is, because an
    /// unmirrored view of yourself makes reaching for anything feel backwards;
    /// one pointed at the world is not, because flipping the world puts every
    /// sign and every page the wrong way round. A phone's back camera faces
    /// away, and so does Desk View, which films the desk from above: drawn
    /// mirrored, what was written on it read backwards. A Mac's cameras say
    /// nothing about their position, and like a laptop webcam in the browser,
    /// they face their owner.
    public static func facesUser(_ device: AVCaptureDevice) -> Bool {
        #if os(macOS)
        if device.deviceType == .deskViewCamera { return false }
        #endif
        return device.position != .back
    }

    /// Whether the picture `session` is showing should be mirrored, by the
    /// camera feeding it; mirrored where nothing feeds it yet, as before.
    public static func mirrors(_ session: AVCaptureSession) -> Bool {
        guard let device = session.inputs.lazy.compactMap({ ($0 as? AVCaptureDeviceInput)?.device }).first else { return true }
        return facesUser(device)
    }

    /// Posted, with the session as its object, when a camera starts feeding
    /// it: a picture drawn from the session reads `mirrors` again then. A
    /// switch of camera keeps the same session running, so nothing about the
    /// session itself says the camera underneath has changed (useMirrored.ts
    /// has the same problem with a restarted track).
    public static let changedCamera = Notification.Name("tomoshibi.camera.changed")

    /// Starts sending `device` (or the preferred camera, or the default) to
    /// `input`.
    public func start(device chosen: AVCaptureDevice? = nil, into input: VideoInput) throws {
        let present = Self.cameras()
        guard let device = chosen
            ?? preferred.flatMap({ id in present.first { $0.uniqueID == id } })
            ?? Self.fallback() else {
            throw CocoaError(.featureUnsupported)
        }
        // Opened before the session is touched: a camera that will not open
        // (held by another app, refused) used to throw between beginning the
        // configuration and committing it, with the old camera already taken
        // out, which left a session that showed nothing and could not be
        // configured again.
        let capture = try AVCaptureDeviceInput(device: device)
        session.beginConfiguration()
        for existing in session.inputs { session.removeInput(existing) }
        if session.canSetSessionPreset(.hd1280x720) { session.sessionPreset = .hd1280x720 }
        if session.canAddInput(capture) { session.addInput(capture) }
        if !session.outputs.contains(output), session.canAddOutput(output) { session.addOutput(output) }
        #if os(iOS)
        if let connection = output.connection(with: .video) {
            if device.position == .front, connection.isVideoMirroringSupported { connection.isVideoMirrored = false }
            let coordinator = AVCaptureDevice.RotationCoordinator(device: device, previewLayer: nil)
            rotation = coordinator
            let apply = { [weak connection] (angle: CGFloat) in
                guard let connection, connection.isVideoRotationAngleSupported(angle) else { return }
                connection.videoRotationAngle = angle
            }
            apply(coordinator.videoRotationAngleForHorizonLevelCapture)
            rotationWatch = coordinator.observe(\.videoRotationAngleForHorizonLevelCapture, options: [.new]) { coordinator, _ in
                apply(coordinator.videoRotationAngleForHorizonLevelCapture)
            }
        }
        #endif
        session.commitConfiguration()
        self.device = device
        preferred = device.uniqueID
        queue.sync { self.input = input }
        let session = self.session
        queue.async { session.startRunning() }
        NotificationCenter.default.post(name: Self.changedCamera, object: session)
    }

    public func stop() {
        let session = self.session
        queue.async {
            session.stopRunning()
            self.input = nil
        }
    }

    public func captureOutput(_ output: AVCaptureOutput, didOutput sample: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let input, let captured = CMSampleBufferGetImageBuffer(sample) else { return }
        let buffer = scaled(captured) ?? captured
        let time = CMSampleBufferGetPresentationTimeStamp(sample)
        let micros = Int64(CMTimeGetSeconds(time) * 1_000_000)
        input.push(pixelBuffer: UInt64(UInt(bitPattern: Unmanaged.passRetained(buffer).toOpaque())), timestampUs: micros, rotation: 0)
    }

    /// The frame at the size this device sends, through the hardware scaler,
    /// or nil when it already is that size.
    private func scaled(_ buffer: CVPixelBuffer) -> CVPixelBuffer? {
        let (width, height) = (Int(Self.width), Int(Self.height))
        guard CVPixelBufferGetWidth(buffer) > width || CVPixelBufferGetHeight(buffer) > height else { return nil }
        if scaler == nil {
            VTPixelTransferSessionCreate(allocator: nil, pixelTransferSessionOut: &scaler)
            let attributes: [String: Any] = [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height,
                kCVPixelBufferIOSurfacePropertiesKey as String: [:],
            ]
            CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool)
        }
        guard let scaler, let pool else { return nil }
        var out: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out)
        guard let out, VTPixelTransferSessionTransferImage(scaler, from: buffer, to: out) == noErr else { return nil }
        return out
    }
}
