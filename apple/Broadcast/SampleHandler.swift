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
final class SampleHandler: RPBroadcastSampleHandler, @unchecked Sendable {
    private let queue = DispatchQueue(label: "tomoshibi.broadcast")
    private var connection: NWConnection?
    private var sending = false
    private let context = CIContext(options: [.cacheIntermediates: false])
    private var scaler: VTPixelTransferSession?
    private var pool: CVPixelBufferPool?
    private var poolSize = (0, 0)

    override func broadcastStarted(withSetupInfo setupInfo: [String: NSObject]?) {
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
        let scaled = scale(pixels) ?? pixels
        let image = CIImage(cvPixelBuffer: scaled)
        guard let jpeg = context.jpegRepresentation(of: image, colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!,
                                                    options: [kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.85]) else { return }
        var header = BroadcastWire.Header(kind: .video)
        header.width = CVPixelBufferGetWidth(scaled)
        header.height = CVPixelBufferGetHeight(scaled)
        header.rotation = rotation
        header.timestampMicros = Int64(CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample)) * 1_000_000)
        send(header, payload: jpeg)
    }

    /// The frame with its longer side brought down to `BroadcastWire.longest`.
    private func scale(_ pixels: CVPixelBuffer) -> CVPixelBuffer? {
        let width = CVPixelBufferGetWidth(pixels), height = CVPixelBufferGetHeight(pixels)
        let longer = max(width, height)
        guard longer > BroadcastWire.longest else { return nil }
        let factor = Double(BroadcastWire.longest) / Double(longer)
        let size = (Int(Double(width) * factor) & ~1, Int(Double(height) * factor) & ~1)
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
        guard let description = CMSampleBufferGetFormatDescription(sample),
              let stream = CMAudioFormatDescriptionGetStreamBasicDescription(description)?.pointee else { return }
        var list = AudioBufferList()
        var block: CMBlockBuffer?
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sample, bufferListSizeNeededOut: nil, bufferListOut: &list, bufferListSize: MemoryLayout<AudioBufferList>.size,
            blockBufferAllocator: nil, blockBufferMemoryAllocator: nil, flags: 0, blockBufferOut: &block) == noErr,
            let data = list.mBuffers.mData else { return }
        let frames = CMSampleBufferGetNumSamples(sample)
        let channels = Int(stream.mChannelsPerFrame)
        var samples = [Int16](repeating: 0, count: frames * channels)
        let float = stream.mFormatFlags & kAudioFormatFlagIsFloat != 0
        let bigEndian = stream.mFormatFlags & kAudioFormatFlagIsBigEndian != 0
        if float {
            let source = data.assumingMemoryBound(to: Float.self)
            for i in 0..<samples.count { samples[i] = Int16(max(-1, min(1, source[i])) * 32767) }
        } else {
            let source = data.assumingMemoryBound(to: Int16.self)
            for i in 0..<samples.count { samples[i] = bigEndian ? Int16(bigEndian: source[i]) : source[i] }
        }
        var header = BroadcastWire.Header(kind: .audio)
        header.channels = channels
        header.rate = Int(stream.mSampleRate)
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
