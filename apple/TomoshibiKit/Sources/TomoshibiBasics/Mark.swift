import SwiftUI
#if canImport(UIKit)
import UIKit
#endif

/// The product's own mark, as the favicon and the app icon draw it: a room of
/// four pictures with one of them live (apple/icon/make-icon.swift, from the
/// web's favicon.svg).
public struct Mark: View {
    public var size: CGFloat

    public init(size: CGFloat = 20) {
        self.size = size
    }

    public var body: some View {
        Canvas { context, area in
            let unit = area.width / 32
            context.fill(Path(roundedRect: CGRect(origin: .zero, size: area), cornerRadius: 8 * unit, style: .continuous),
                         with: .color(Mark.ground))
            Mark.drawTiles(in: CGRect(origin: .zero, size: area), on: &context)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    public static let ground = Color(red: 0x0F / 255, green: 0x0E / 255, blue: 0x0D / 255)
    public static let idle = Color(red: 0x55 / 255, green: 0x52 / 255, blue: 0x4E / 255)
    public static let live = Color(red: 0xFF / 255, green: 0xA2 / 255, blue: 0x3B / 255)

    /// The four pictures in a square, on the favicon's 32-unit grid drawn at
    /// 0.86 about the middle, as the icon draws them.
    static func drawTiles(in square: CGRect, on context: inout GraphicsContext) {
        let inner = square.width / 32 * 0.86
        let offset = (square.width - 32 * inner) / 2
        for (x, y, live) in [(6.0, 8.0, false), (17, 8, false), (6, 17, false), (17, 17, true)] {
            let tile = CGRect(x: square.minX + offset + x * inner, y: square.minY + offset + y * inner,
                              width: 9 * inner, height: 7 * inner)
            context.fill(Path(roundedRect: tile, cornerRadius: 2 * inner), with: .color(live ? Mark.live : Mark.idle))
        }
    }
}

/// The picture an invitation is sent with: the mark's room of pictures on the
/// icon's ground, wide, as Messages shows the picture of a card and of a rich
/// link (MSMessageTemplateLayout, LPLinkMetadata). One picture for both ways a
/// link leaves the phone, so that a card from the Messages extension and a
/// link shared from the app look like the same thing, which they are.
///
/// No words in it. The words go in the card's captions and the link's title,
/// which are text a reader's phone can enlarge and read aloud; written into
/// the picture they would be neither, and in the sender's language for good.
public struct InvitePicture: View {
    public init() {}

    public var body: some View {
        Canvas { context, area in
            context.fill(Path(CGRect(origin: .zero, size: area)), with: .color(Mark.ground))
            let side = min(area.width, area.height) * 0.92
            let square = CGRect(x: (area.width - side) / 2, y: (area.height - side) / 2, width: side, height: side)
            Mark.drawTiles(in: square, on: &context)
        }
        .accessibilityHidden(true)
    }

    #if canImport(UIKit)
    /// Drawn at `size` points, for a card or a link to carry: three by two,
    /// the shape Messages gives a card's picture without cropping it.
    @MainActor
    public static func image(size: CGSize = CGSize(width: 600, height: 400), scale: CGFloat = 2) -> UIImage? {
        let renderer = ImageRenderer(content: InvitePicture().frame(width: size.width, height: size.height))
        renderer.scale = scale
        renderer.isOpaque = true
        return renderer.uiImage
    }

    /// The mark alone, square, for a link's icon.
    @MainActor
    public static func icon(side: CGFloat = 120, scale: CGFloat = 2) -> UIImage? {
        let renderer = ImageRenderer(content: Mark(size: side))
        renderer.scale = scale
        return renderer.uiImage
    }
    #endif
}
