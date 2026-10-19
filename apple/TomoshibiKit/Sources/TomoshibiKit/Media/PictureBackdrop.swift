import CoreGraphics
import CoreVideo
import Foundation
import QuartzCore
import TomoshibiBasics

/// What lies around a camera's picture while it is shown whole rather than
/// filling its place (Framing): the same picture, filling the place, blurred
/// out of all detail and darkened, so that the picture sits in its own light
/// instead of between two bars. Bars around a face on a phone read as a
/// broken video; a blurred copy reads as the room the face is in.
///
/// It is a backdrop and not a second video. A frame is taken from the
/// renderer once a second, shrunk on the processor to a few dozen pixels on
/// its long side by averaging, blurred and darkened there, and handed to a
/// plain layer that draws it scaled up to fill, which smooths it further; a
/// new one crossfades over the last. Nothing is decoded twice and nothing
/// runs per frame, and it exists only while the picture is shown whole. It is
/// content, under the picture, and never glass: glass belongs to the controls
/// laid over a call, not to anything in it.
@MainActor
final class PictureBackdrop {
    /// The long side of the shrunk picture, in pixels.
    nonisolated static let side = 48
    /// The blur, in those pixels: an eighth of the short side of a 4:3
    /// picture, which leaves colour and light and nothing that could be read
    /// as a face beside the face.
    nonisolated static let sigma = 4.5
    /// What a pixel keeps of its light: the picture under black at about 35%,
    /// so that the picture in front stays the brightest thing in its place.
    nonisolated static let kept: Float = 0.65
    /// How long one copy takes to give way to the next.
    static let crossfade: CFTimeInterval = 0.6

    /// One queue for every backdrop, off the decoder's threads.
    nonisolated private static let work = DispatchQueue(label: "app.tomoshibi.backdrop", qos: .utility)

    let layer = CALayer()
    private let image = CALayer()
    private var rotation = 0
    private var bounds = CGRect.zero
    /// Whether the first copy is up.
    private(set) var shown = false
    private var gone = false
    /// Called once, when the first copy goes up.
    var onShown: (@MainActor () -> Void)?

    init() {
        layer.masksToBounds = true
        layer.opacity = 0
        image.contentsGravity = .resizeAspectFill
        image.masksToBounds = true
        image.magnificationFilter = .linear
        layer.addSublayer(image)
    }

    /// What the renderer hands a sampled frame to: the copy is made on the
    /// shared queue and shown on the main thread, if this is still wanted.
    nonisolated func taker() -> @Sendable (CVPixelBuffer, Int) -> Void {
        { [weak self] buffer, rotation in
            // Read and let go on the one queue; nothing writes a decoded
            // frame after it is handed out.
            nonisolated(unsafe) let buffer = buffer
            Self.work.async {
                guard let picture = Self.copy(of: buffer) else { return }
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { self?.show(picture, rotation: rotation) }
                }
            }
        }
    }

    func layout(_ bounds: CGRect) {
        self.bounds = bounds
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer.frame = bounds
        place()
        CATransaction.commit()
    }

    /// The copy turned as the picture is (VideoRenderer.turn) and filling the
    /// place once turned.
    private func place() {
        let turned = rotation == 90 || rotation == 270
        image.setAffineTransform(VideoRenderer.turn(rotation))
        image.bounds = CGRect(x: 0, y: 0, width: turned ? bounds.height : bounds.width, height: turned ? bounds.width : bounds.height)
        image.position = CGPoint(x: bounds.width / 2, y: bounds.height / 2)
    }

    private func show(_ picture: CGImage, rotation: Int) {
        guard !gone else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if rotation != self.rotation {
            self.rotation = rotation
            place()
        }
        if shown {
            let fade = CATransition()
            fade.type = .fade
            fade.duration = Self.crossfade
            fade.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            image.add(fade, forKey: "refresh")
        }
        image.contents = picture
        CATransaction.commit()
        // The first copy fades the backdrop in, as anything arriving does.
        if !shown {
            shown = true
            fade(to: 1, seconds: Theme.settleSeconds, timing: Theme.settleTiming)
            let then = onShown
            onShown = nil
            then?()
        }
    }

    /// Fades out, or goes at once, and takes its layer away after.
    func leave(animated: Bool) {
        gone = true
        guard animated, shown else {
            layer.removeFromSuperlayer()
            return
        }
        fade(to: 0, seconds: Theme.leaveSeconds, timing: Theme.leaveTiming) { [layer] in
            layer.removeFromSuperlayer()
        }
    }

    private func fade(to opacity: Float, seconds: CFTimeInterval, timing: CAMediaTimingFunction, then: (() -> Void)? = nil) {
        let from = layer.presentation()?.opacity ?? layer.opacity
        CATransaction.begin()
        CATransaction.setCompletionBlock(then)
        let animation = CABasicAnimation(keyPath: "opacity")
        animation.fromValue = from
        animation.toValue = opacity
        animation.duration = seconds
        animation.timingFunction = timing
        layer.opacity = opacity
        layer.add(animation, forKey: "fade")
        CATransaction.commit()
    }

    // MARK: The copy

    /// The frame shrunk to `side` pixels on its long side, each pixel the
    /// average of a grid of points in the block of the frame it stands for,
    /// then blurred and darkened, as an sRGB image in the frame's own
    /// orientation; nil for a buffer of a kind it does not read.
    ///
    /// Read on the processor rather than with Core Image, which cannot read
    /// the planar 4:2:0 the core hands over for every picture a software
    /// decoder made (VideoRenderer.pixelBuffer; a VP8 camera's, for one): it
    /// made an empty image of one. And a few thousand points a second need
    /// no graphics context of their own. The hardware decoder's biplanar
    /// buffers and plain BGRA are read here too.
    nonisolated static func copy(of buffer: CVPixelBuffer) -> CGImage? {
        let width = CVPixelBufferGetWidth(buffer), height = CVPixelBufferGetHeight(buffer)
        guard width > 0, height > 0 else { return nil }
        let w = width >= height ? side : max(1, side * width / height)
        let h = width >= height ? max(1, side * height / width) : side
        guard var rgb = reduce(buffer, width: width, height: height, to: w, h) else { return nil }
        blur(&rgb, w, h, sigma: sigma)
        var bytes = [UInt8](repeating: 255, count: w * h * 4)
        for i in 0..<(w * h) {
            // Blue, green, red, then the byte the image skips.
            for c in 0..<3 {
                let value = min(max(rgb[i * 3 + 2 - c] * kept, 0), 1)
                bytes[i * 4 + c] = UInt8(value * 255 + 0.5)
            }
        }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        return CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4, space: space,
                       bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    /// Red, green and blue from 0 to 1 for each pixel of a `w` by `h`
    /// shrinking of the frame. Four by four points of each block are
    /// averaged, which is plenty under a blur this wide and a few thousand
    /// reads in all whatever the frame's size.
    nonisolated private static func reduce(_ buffer: CVPixelBuffer, width: Int, height: Int, to w: Int, _ h: Int) -> [Float]? {
        let format = CVPixelBufferGetPixelFormatType(buffer)
        let planar = format == kCVPixelFormatType_420YpCbCr8Planar || format == kCVPixelFormatType_420YpCbCr8PlanarFullRange
        let biplanar = format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        let bgra = format == kCVPixelFormatType_32BGRA
        guard planar || biplanar || bgra else { return nil }
        guard CVPixelBufferLockBaseAddress(buffer, .readOnly) == kCVReturnSuccess else { return nil }
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }

        let full = format == kCVPixelFormatType_420YpCbCr8PlanarFullRange || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        // The matrix the buffer says it was made with, and otherwise BT.601,
        // which is what WebRTC's software decoders produce.
        var bt709 = false
        if let matrix = CVBufferCopyAttachment(buffer, kCVImageBufferYCbCrMatrixKey, nil) as? String {
            bt709 = matrix == (kCVImageBufferYCbCrMatrix_ITU_R_709_2 as String)
        }
        let grid = 4
        var out = [Float](repeating: 0, count: w * h * 3)
        func point(_ index: Int, of count: Int, in blocks: Int, block: Int) -> Int {
            // The index-th of `grid` points across the block-th of `blocks`
            // equal blocks of `count`, at the middle of its share.
            min(count - 1, ((block * grid + index) * 2 + 1) * count / (blocks * grid * 2))
        }

        if bgra {
            guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
            let stride = CVPixelBufferGetBytesPerRow(buffer)
            let bytes = base.assumingMemoryBound(to: UInt8.self)
            for oy in 0..<h {
                for ox in 0..<w {
                    var sum: (Float, Float, Float) = (0, 0, 0)
                    for gy in 0..<grid {
                        let row = bytes + point(gy, of: height, in: h, block: oy) * stride
                        for gx in 0..<grid {
                            let pixel = row + point(gx, of: width, in: w, block: ox) * 4
                            sum.0 += Float(pixel[2]); sum.1 += Float(pixel[1]); sum.2 += Float(pixel[0])
                        }
                    }
                    let n = Float(grid * grid) * 255
                    let i = (oy * w + ox) * 3
                    out[i] = sum.0 / n; out[i + 1] = sum.1 / n; out[i + 2] = sum.2 / n
                }
            }
            return out
        }

        guard let lumaBase = CVPixelBufferGetBaseAddressOfPlane(buffer, 0) else { return nil }
        let luma = lumaBase.assumingMemoryBound(to: UInt8.self)
        let lumaStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 0)
        let chromaWidth = (width + 1) / 2, chromaHeight = (height + 1) / 2
        // Planar: blue difference in plane 1, red in plane 2. Biplanar: the
        // two interleaved in plane 1.
        guard let blueBase = CVPixelBufferGetBaseAddressOfPlane(buffer, 1) else { return nil }
        let blue = blueBase.assumingMemoryBound(to: UInt8.self)
        let blueStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 1)
        var red = blue, redStride = blueStride
        if planar {
            guard let redBase = CVPixelBufferGetBaseAddressOfPlane(buffer, 2) else { return nil }
            red = redBase.assumingMemoryBound(to: UInt8.self)
            redStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 2)
        }
        for oy in 0..<h {
            for ox in 0..<w {
                var y: Float = 0, cb: Float = 0, cr: Float = 0
                for gy in 0..<grid {
                    let row = point(gy, of: height, in: h, block: oy)
                    let chromaRow = min(chromaHeight - 1, row / 2)
                    for gx in 0..<grid {
                        let column = point(gx, of: width, in: w, block: ox)
                        let chromaColumn = min(chromaWidth - 1, column / 2)
                        y += Float(luma[row * lumaStride + column])
                        if planar {
                            cb += Float(blue[chromaRow * blueStride + chromaColumn])
                            cr += Float(red[chromaRow * redStride + chromaColumn])
                        } else {
                            cb += Float(blue[chromaRow * blueStride + chromaColumn * 2])
                            cr += Float(blue[chromaRow * blueStride + chromaColumn * 2 + 1])
                        }
                    }
                }
                let n = Float(grid * grid)
                y /= n; cb = cb / n - 128; cr = cr / n - 128
                if full {
                    y /= 255; cb /= 255; cr /= 255
                } else {
                    y = (y - 16) / 219; cb /= 224; cr /= 224
                }
                let i = (oy * w + ox) * 3
                if bt709 {
                    out[i] = y + 1.5748 * cr
                    out[i + 1] = y - 0.187324 * cb - 0.468124 * cr
                    out[i + 2] = y + 1.8556 * cb
                } else {
                    out[i] = y + 1.402 * cr
                    out[i + 1] = y - 0.344136 * cb - 0.714136 * cr
                    out[i + 2] = y + 1.772 * cb
                }
            }
        }
        return out
    }

    /// A Gaussian blur across then down, the edges held as they are, so that
    /// the copy does not darken towards its sides.
    nonisolated private static func blur(_ rgb: inout [Float], _ w: Int, _ h: Int, sigma: Double) {
        let radius = Int((sigma * 3).rounded(.up))
        var weights = (-radius...radius).map { Float(exp(-Double($0 * $0) / (2 * sigma * sigma))) }
        let total = weights.reduce(0, +)
        weights = weights.map { $0 / total }
        var pass = [Float](repeating: 0, count: rgb.count)
        for y in 0..<h {
            for x in 0..<w {
                var sum: (Float, Float, Float) = (0, 0, 0)
                for k in -radius...radius {
                    let j = (y * w + min(max(x + k, 0), w - 1)) * 3
                    let weight = weights[k + radius]
                    sum.0 += rgb[j] * weight; sum.1 += rgb[j + 1] * weight; sum.2 += rgb[j + 2] * weight
                }
                let i = (y * w + x) * 3
                pass[i] = sum.0; pass[i + 1] = sum.1; pass[i + 2] = sum.2
            }
        }
        for y in 0..<h {
            for x in 0..<w {
                var sum: (Float, Float, Float) = (0, 0, 0)
                for k in -radius...radius {
                    let j = (min(max(y + k, 0), h - 1) * w + x) * 3
                    let weight = weights[k + radius]
                    sum.0 += pass[j] * weight; sum.1 += pass[j + 1] * weight; sum.2 += pass[j + 2] * weight
                }
                let i = (y * w + x) * 3
                rgb[i] = sum.0; rgb[i + 1] = sum.1; rgb[i + 2] = sum.2
            }
        }
    }
}
