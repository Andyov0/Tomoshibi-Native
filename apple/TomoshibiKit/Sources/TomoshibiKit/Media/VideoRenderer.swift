import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import os
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
    /// When the layer last took a frame, and how many it has turned away
    /// since (`enqueue`).
    private var lastTaken = CFAbsoluteTimeGetCurrent()
    private var turnedAway = 0
    private var lastShown: CFAbsoluteTime = 0
    private var interval: CFTimeInterval = 0

    /// At most one frame in this many seconds, or every frame at zero: the
    /// small window's renderer kept warm while the app is in front.
    func show(atMostEvery seconds: CFTimeInterval) {
        lock.lock()
        interval = seconds
        lock.unlock()
    }
    private static let log = Logger(subsystem: "app.tomoshibi", category: "video")

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
        if interval > 0 {
            let now = CFAbsoluteTimeGetCurrent()
            guard now - lastShown >= interval else { return }
            lastShown = now
        }
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
        // Only a frame the layer is ready for. Every frame was given to it
        // whether it was taking them or not, and a layer that is not (off
        // the screen, under another, between layouts) keeps what it is given
        // queued, each frame holding the decoder's surface: on an iPhone the
        // app, the system's media renderer, the video decoder and the
        // compositor grew together past three gigabytes each until the
        // compositor was killed for it and the screen came back at the home
        // screen (Jetsam, 2026-10-17). A picture of a call only ever wants
        // its newest frame, so one the layer is not ready for is dropped;
        // and a layer that has taken nothing for two seconds is flushed, and
        // said, in case it is waiting on something that will not come.
        guard renderer.isReadyForMoreMediaData else {
            turnedAway += 1
            let now = CFAbsoluteTimeGetCurrent()
            if now - lastTaken > 2 {
                Self.log.notice("a picture took no frame for \(now - self.lastTaken, privacy: .public) s, \(self.turnedAway, privacy: .public) turned away; flushing it")
                renderer.flush()
                lastTaken = now
                turnedAway = 0
            }
            return
        }
        renderer.enqueue(sample)
        lastTaken = CFAbsoluteTimeGetCurrent()
        turnedAway = 0
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
