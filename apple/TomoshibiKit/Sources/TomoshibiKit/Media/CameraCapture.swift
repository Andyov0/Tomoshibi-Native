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

    /// Starts sending `device` (or the default camera) to `input`.
    public func start(device chosen: AVCaptureDevice? = nil, into input: VideoInput) throws {
        let device: AVCaptureDevice
        if let chosen {
            device = chosen
        } else {
            #if os(iOS)
            guard let front = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .front) ?? AVCaptureDevice.default(for: .video) else {
                throw CocoaError(.featureUnsupported)
            }
            device = front
            #else
            guard let any = AVCaptureDevice.default(for: .video) else { throw CocoaError(.featureUnsupported) }
            device = any
            #endif
        }
        session.beginConfiguration()
        for existing in session.inputs { session.removeInput(existing) }
        if session.canSetSessionPreset(.hd1280x720) { session.sessionPreset = .hd1280x720 }
        let capture = try AVCaptureDeviceInput(device: device)
        if session.canAddInput(capture) { session.addInput(capture) }
        if !session.outputs.contains(output), session.canAddOutput(output) { session.addOutput(output) }
        #if os(iOS)
        if let connection = output.connection(with: .video) {
            if connection.isVideoRotationAngleSupported(90) { connection.videoRotationAngle = 90 }
            if device.position == .front, connection.isVideoMirroringSupported { connection.isVideoMirrored = false }
        }
        #endif
        session.commitConfiguration()
        self.device = device
        queue.sync { self.input = input }
        let session = self.session
        queue.async { session.startRunning() }
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
