import CoreImage
import CoreVideo
import Foundation
import Vision

/// The background behind somebody blurred, frame by frame, before the frame is
/// sent and before it is shown to them (live/blur.ts).
///
/// "Blur my background" used to open the system's video effects: a sheet of
/// the system's own that the owner found ugly, which on an iPhone did nothing
/// to the call unless the app was a VoIP app or had opted in, and which no
/// Mac or iPhone setting tied to the switch the web offers. This is the web's
/// switch done here: the person is found with Vision's person segmentation,
/// the rest of the frame is blurred with Core Image, and the two are blended
/// through the mask.
///
/// Called on the camera's queue, one frame at a time. Vision's sequence
/// handler is kept across frames so the mask follows a moving person rather
/// than being worked out afresh each frame.
final class BackgroundBlur {
    /// How much to blur, at a picture 720 rows high: the web's (blur.ts
    /// `RADIUS`), "enough that a room is unreadable and not so much that the
    /// edge of somebody's hair becomes a smear". A smaller picture is blurred
    /// in proportion, so a phone's 360 rows look the same as a Mac's 720.
    static let radiusAt720: Double = 12

    private let context: CIContext
    private let sequence = VNSequenceRequestHandler()
    private let request: VNGeneratePersonSegmentationRequest
    private var pool: CVPixelBufferPool?
    private var poolSize = (width: 0, height: 0, format: OSType(0))

    init() {
        context = CIContext(options: [.cacheIntermediates: false, .name: "tomoshibi.blur"])
        request = VNGeneratePersonSegmentationRequest()
        // Balanced is the one made for video: fast is a blocky mask that
        // flickers at the hair, accurate is a still photograph's and costs a
        // frame's time several times over.
        request.qualityLevel = .balanced
        request.outputPixelFormat = kCVPixelFormatType_OneComponent8
    }

    /// The frame with its background blurred, in a buffer of the same size
    /// and format; nil where the person could not be found or the frame not
    /// drawn, in which case the frame goes as it came rather than not at all.
    func process(_ frame: CVPixelBuffer) -> CVPixelBuffer? {
        do {
            try sequence.perform([request], on: frame)
        } catch {
            return nil
        }
        guard let mask = request.results?.first?.pixelBuffer else { return nil }
        let width = CVPixelBufferGetWidth(frame)
        let height = CVPixelBufferGetHeight(frame)
        let format = CVPixelBufferGetPixelFormatType(frame)
        guard let out = buffer(width: width, height: height, format: format) else { return nil }

        let picture = CIImage(cvPixelBuffer: frame)
        let extent = picture.extent
        let sigma = Self.radiusAt720 * Double(height) / 720
        let blurred = picture.clampedToExtent()
            .applyingGaussianBlur(sigma: sigma)
            .cropped(to: extent)
        // The mask comes at its own size; stretched to the frame's, and its
        // edge softened a little so the outline of a person does not step.
        var person = CIImage(cvPixelBuffer: mask)
        person = person.transformed(by: CGAffineTransform(scaleX: extent.width / person.extent.width,
                                                          y: extent.height / person.extent.height))
        person = person.clampedToExtent().applyingGaussianBlur(sigma: max(1, sigma / 6)).cropped(to: extent)
        let blended = picture.applyingFilter("CIBlendWithMask", parameters: [
            kCIInputBackgroundImageKey: blurred,
            kCIInputMaskImageKey: person,
        ])
        context.render(blended, to: out, bounds: extent, colorSpace: CGColorSpaceCreateDeviceRGB())
        return out
    }

    /// A buffer from a pool of frames the size and format of the camera's,
    /// made again when either changes (a camera switched, a phone turned).
    private func buffer(width: Int, height: Int, format: OSType) -> CVPixelBuffer? {
        if pool == nil || poolSize != (width, height, format) {
            let attributes: [String: Any] = [
                kCVPixelBufferPixelFormatTypeKey as String: format,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height,
                kCVPixelBufferIOSurfacePropertiesKey as String: [:],
            ]
            pool = nil
            CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool)
            poolSize = (width, height, format)
        }
        guard let pool else { return nil }
        var out: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out)
        return out
    }
}
