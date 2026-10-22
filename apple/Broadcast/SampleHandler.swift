import CoreImage
import Foundation
import Network
import ReplayKit
import TomoshibiBasics
import VideoToolbox

/// The iPhone's screen, captured by ReplayKit in an extension of its own and
/// handed to the app (see `BroadcastWire` for why the app publishes and this
/// only carries).
///
/// Frames are scaled down with the hardware scaler and compressed to JPEG on
/// the GPU, at most one in flight at a time: a frame that cannot be sent yet
/// is dropped rather than queued, because the extension has fifty megabytes
/// and a queue of full-size frames is how it gets killed.
///
/// Under the deployment's ceiling as well, where it has one (ShareCeiling,
/// left in the app group by the app, which hears it): no taller than its
/// height and no more frames a second than its rate, and while the phone is
/// hot its lower rate. Held to here rather than in the app, which publishes,
/// because a frame dropped here is a frame never scaled, compressed or sent
/// over the socket, and that work is what heats the phone.
final class SampleHandler: RPBroadcastSampleHandler, @unchecked Sendable {
    private let queue = DispatchQueue(label: "tomoshibi.broadcast")
    private var connection: NWConnection?
    private var sending = false
    /// Read as the broadcast starts: a ceiling changed during it applies to
    /// the next one, as a Mac's share takes it at its next start.
    private var ceiling = ShareCeiling.none
    /// When the last frame sent was captured, for the ceiling's rate.
    private var lastSent: CMTime?
    private let context = CIContext(options: [.cacheIntermediates: false])
    private var scaler: VTPixelTransferSession?
    private var pool: CVPixelBufferPool?
    private var poolSize = (0, 0)

    override func broadcastStarted(withSetupInfo setupInfo: [String: NSObject]?) {
        let group = UserDefaults(suiteName: BroadcastWire.group)
        // Not at all while the deployment has screen sharing switched off,
        // which the app's button already says by being gone; a broadcast
        // started from Control Center would go round it. Asked only here:
        // one already running when sharing is switched off goes on, as a
        // share on the Mac does.
        if BroadcastWire.screenShareOff(in: group) {
            finishBroadcastWithError(NSError(domain: "Tomoshibi", code: 3, userInfo: [NSLocalizedDescriptionKey: BroadcastWire.switchedOff]))
            return
        }
        ceiling = ShareCeiling(kept: group)
        guard let url = BroadcastWire.socketURL else {
            finishBroadcastWithError(NSError(domain: "Tomoshibi", code: 1, userInfo: [NSLocalizedDescriptionKey: t("Open Tomoshibi and join a call first.")]))
            return
        }
        let connection = NWConnection(to: .unix(path: url.path), using: .tcp)
        connection.stateUpdateHandler = { [weak self] state in
            if case .failed = state { self?.finish() }
        }
        connection.start(queue: queue)
        self.connection = connection
    }

    override func broadcastFinished() {
        send(BroadcastWire.Header(kind: .ended), payload: Data(), always: true)
        queue.asyncAfter(deadline: .now() + 0.3) { [connection] in connection?.cancel() }
    }

    private func finish() {
        finishBroadcastWithError(NSError(domain: "Tomoshibi", code: 2, userInfo: [NSLocalizedDescriptionKey: t("Open Tomoshibi and join a call first.")]))
    }

    override func processSampleBuffer(_ sample: CMSampleBuffer, with type: RPSampleBufferType) {
        switch type {
        case .video: video(sample)
        case .audioApp: audio(sample)
        default: break
        }
    }

    private func video(_ sample: CMSampleBuffer) {
        guard !sending, let pixels = CMSampleBufferGetImageBuffer(sample) else { return }
        // No more frames a second than the ceiling, by the frames' own clock.
        // A tenth of the gap is given back, since ReplayKit's frames do not
        // arrive on the dot: held to the exact gap, every frame a hair early
        // would be dropped, and a ceiling of thirty over a sixty-hertz screen
        // would come out at twenty.
        let at = CMSampleBufferGetPresentationTimeStamp(sample)
        if let rate = ceiling.limit(hot: ShareCeiling.hot(ProcessInfo.processInfo.thermalState)), let lastSent,
           CMTimeGetSeconds(CMTimeSubtract(at, lastSent)) < 0.9 / Double(rate) {
            return
        }
        var rotation = 0
        if let raw = CMGetAttachment(sample, key: RPVideoSampleOrientationKey as CFString, attachmentModeOut: nil) as? NSNumber,
           let orientation = CGImagePropertyOrientation(rawValue: raw.uint32Value) {
            switch orientation {
            case .left: rotation = 90
            case .down: rotation = 180
            case .right: rotation = 270
            default: rotation = 0
            }
        }
        let scaled = scale(pixels, turned: rotation == 90 || rotation == 270) ?? pixels
        let image = CIImage(cvPixelBuffer: scaled)
        guard let jpeg = context.jpegRepresentation(of: image, colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!,
                                                    options: [kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.85]) else { return }
        lastSent = at
        var header = BroadcastWire.Header(kind: .video)
        header.width = CVPixelBufferGetWidth(scaled)
        header.height = CVPixelBufferGetHeight(scaled)
        header.rotation = rotation
        header.timestampMicros = Int64(CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample)) * 1_000_000)
        send(header, payload: jpeg)
    }

    /// The frame with its longer side brought down to `BroadcastWire.longest`,
    /// and its height, once it is turned upright (`turned`, a phone on its
    /// side), to the ceiling's: the height of the picture people see, which
    /// is the frame's width when the phone lies on its side.
    private func scale(_ pixels: CVPixelBuffer, turned: Bool) -> CVPixelBuffer? {
        let width = CVPixelBufferGetWidth(pixels), height = CVPixelBufferGetHeight(pixels)
        let longer = max(width, height)
        var factor = longer > BroadcastWire.longest ? Double(BroadcastWire.longest) / Double(longer) : 1
        if let ceiling = ceiling.height {
            factor = min(factor, Double(ceiling) / Double(turned ? width : height))
        }
        guard factor < 1 else { return nil }
        let size = (max(2, Int(Double(width) * factor) & ~1), max(2, Int(Double(height) * factor) & ~1))
        if scaler == nil { VTPixelTransferSessionCreate(allocator: nil, pixelTransferSessionOut: &scaler) }
        if pool == nil || poolSize != size {
            pool = nil
            let attributes: [String: Any] = [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                kCVPixelBufferWidthKey as String: size.0,
                kCVPixelBufferHeightKey as String: size.1,
                kCVPixelBufferIOSurfacePropertiesKey as String: [:],
            ]
            CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool)
            poolSize = size
        }
        guard let scaler, let pool else { return nil }
        var out: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out)
        guard let out, VTPixelTransferSessionTransferImage(scaler, from: pixels, to: out) == noErr else { return nil }
        return out
    }

    /// The app's sound as 16-bit interleaved samples at whatever rate ReplayKit
    /// gives; the app resamples to the call's 48 kHz.
    private func audio(_ sample: CMSampleBuffer) {
        // Read through the one reader both platforms share, which takes
        // sound in one buffer or in one per channel (SampleAudio).
        guard let read = SampleAudio.interleaved(sample) else { return }
        let samples = SampleAudio.int16(read.samples)
        var header = BroadcastWire.Header(kind: .audio)
        header.channels = read.channels
        header.rate = Int(read.rate)
        send(header, payload: samples.withUnsafeBufferPointer { Data(buffer: $0) }, always: true)
    }

    private func send(_ header: BroadcastWire.Header, payload: Data, always: Bool = false) {
        guard let connection else { return }
        let isVideo = header.kind == .video
        if isVideo { sending = true }
        connection.send(content: BroadcastWire.frame(header, payload: payload), completion: .contentProcessed { [weak self] _ in
            if isVideo { self?.queue.async { self?.sending = false } }
        })
        _ = always
    }
}
