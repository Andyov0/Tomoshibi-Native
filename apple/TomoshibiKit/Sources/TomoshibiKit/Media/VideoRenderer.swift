import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import QuartzCore
import TomoshibiCore

/// Draws one remote picture.
///
/// The core hands over each decoded frame as the CVPixelBuffer the hardware
/// decoder produced; it is wrapped in a sample buffer marked to be shown at
/// once and given to a display layer, so nothing is copied or converted on the
/// way to the screen. Timing is the network's business and has already been
/// done by the time a frame arrives here.
public final class VideoRenderer: @unchecked Sendable {
    public let layer = AVSampleBufferDisplayLayer()
    private var format: CMVideoFormatDescription?
    private var lastSize: (Int, Int) = (0, 0)
    private let lock = NSLock()

    /// The size of the last frame and its rotation, for the view to fit the
    /// picture rather than the box it sits in (web SurfaceTile.tsx ~93-124).
    public private(set) var pictureSize: CGSize = .zero
    public var onPictureSize: (@Sendable (CGSize) -> Void)?

    public init() {
        layer.videoGravity = .resizeAspect
    }

    /// One frame, from the hub that fans a track out to its renderers.
    func show(_ buffer: CVPixelBuffer, rotation: Int) {
        enqueue(buffer, rotation: rotation)
    }

    private func enqueue(_ buffer: CVPixelBuffer, rotation: Int) {
        lock.lock()
        defer { lock.unlock() }
        let width = CVPixelBufferGetWidth(buffer), height = CVPixelBufferGetHeight(buffer)
        if format == nil || lastSize != (width, height) || !CMVideoFormatDescriptionMatchesImageBuffer(format!, imageBuffer: buffer) {
            var description: CMVideoFormatDescription?
            CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: buffer, formatDescriptionOut: &description)
            format = description
            lastSize = (width, height)
            let turned = rotation == 90 || rotation == 270
            let size = CGSize(width: turned ? height : width, height: turned ? width : height)
            pictureSize = size
            onPictureSize?(size)
        }
        guard let format else { return }
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()), decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        CMSampleBufferCreateReadyWithImageBuffer(allocator: nil, imageBuffer: buffer, formatDescription: format, sampleTiming: &timing, sampleBufferOut: &sample)
        guard let sample else { return }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true) as? [NSMutableDictionary], let first = attachments.first {
            first[kCMSampleAttachmentKey_DisplayImmediately] = true
        }
        let renderer = layer.sampleBufferRenderer
        if renderer.status == .failed { renderer.flush() }
        renderer.enqueue(sample)
        let angle = CGFloat(rotation) * .pi / 180
        if rotation != 0 || layer.affineTransform() != .identity {
            DispatchQueue.main.async { [layer] in
                layer.setAffineTransform(CGAffineTransform(rotationAngle: angle))
            }
        }
    }

    /// The rare picture that did not come from the hardware decoder: its
    /// planes copied into a buffer the display layer can take.
    static func pixelBuffer(i420: Data, width: Int, height: Int) -> CVPixelBuffer? {
        var out: CVPixelBuffer?
        let attributes = [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary
        guard CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_420YpCbCr8Planar, attributes, &out) == kCVReturnSuccess, let out else {
            return nil
        }
        CVPixelBufferLockBaseAddress(out, [])
        defer { CVPixelBufferUnlockBaseAddress(out, []) }
        let chromaWidth = (width + 1) / 2, chromaHeight = (height + 1) / 2
        var offset = 0
        for (plane, w, h) in [(0, width, height), (1, chromaWidth, chromaHeight), (2, chromaWidth, chromaHeight)] {
            guard let base = CVPixelBufferGetBaseAddressOfPlane(out, plane) else { continue }
            let stride = CVPixelBufferGetBytesPerRowOfPlane(out, plane)
            i420.withUnsafeBytes { source in
                for row in 0..<h {
                    memcpy(base + row * stride, source.baseAddress! + offset + row * w, w)
                }
            }
            offset += w * h
        }
        return out
    }
}
