import SwiftUI
@testable import TomoshibiBasics
import XCTest

/*
 * The palette as it is drawn, in each appearance: every colour rendered and
 * read back as pixels. The night must come out exactly as the colours did
 * before there was a day, which were plain `Color(red:green:blue:)` values
 * (the owner's rule: the dark screens do not change by a pixel), and the day
 * must come out as the palette the four clients share. Rendered rather than
 * compared as numbers, because what can go wrong is the drawing: a colour
 * that ignores the appearance a view is told to take (the call is drawn dark
 * inside a light app) is the same colour as a number either way.
 */
@MainActor
final class ThemeTests: XCTestCase {
    /// One pixel of `color` drawn over `under` in `scheme`.
    private func pixel(_ color: Color, _ scheme: ColorScheme, over under: Color = .clear) -> [UInt8] {
        let view = ZStack {
            under
            color
        }
        .frame(width: 4, height: 4)
        .environment(\.colorScheme, scheme)
        let renderer = ImageRenderer(content: view)
        renderer.scale = 1
        guard let image = renderer.cgImage,
              let context = CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
            XCTFail("nothing was drawn")
            return []
        }
        context.draw(image, in: CGRect(x: -1, y: -1, width: image.width, height: image.height))
        guard let data = context.data else { return [] }
        let bytes = data.bindMemory(to: UInt8.self, capacity: 4)
        return [bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    /// The night, colour by colour, against what the apps drew before.
    func testTheNightIsWhatItWas() {
        let before: [(String, Color, Color)] = [
            ("background", Theme.background, Color(red: 0.043, green: 0.043, blue: 0.051)),
            ("surface", Theme.surface, Color(red: 0.110, green: 0.110, blue: 0.118)),
            ("surfaceHigh", Theme.surfaceHigh, Color(red: 0.173, green: 0.173, blue: 0.180)),
            ("foreground", Theme.foreground, Color(red: 0.961, green: 0.961, blue: 0.969)),
            ("good", Theme.good, Color(red: 0.188, green: 0.820, blue: 0.345)),
            ("danger", Theme.danger, Color(red: 1.0, green: 0.271, blue: 0.227)),
            ("page", Theme.page, Color.black),
            ("ink", Theme.ink, Color.white),
        ]
        for (name, now, was) in before {
            XCTAssertEqual(pixel(now, .dark), pixel(was, .dark), name)
        }
        // The translucent ones, over the darkest ground and over white.
        let translucent: [(String, Color, Color)] = [
            ("border", Theme.border, Color.white.opacity(0.10)),
            ("muted", Theme.muted, Color(red: 0.922, green: 0.922, blue: 0.961).opacity(0.6)),
            ("ink at 7%", Theme.ink.opacity(0.07), Color.white.opacity(0.07)),
            ("lifted", Theme.lifted(0.05), Color.white.opacity(0.05)),
            ("lifted, nested", Theme.lifted(0.07, nested: true), Color.white.opacity(0.07)),
            ("sunk", Theme.sunk(0.22), Color.black.opacity(0.22)),
            ("shadow", Theme.shadow(0.35), Color.black.opacity(0.35)),
        ]
        for (name, now, was) in translucent {
            for ground in [Color.black, Color(red: 0.110, green: 0.110, blue: 0.118), Color.white] {
                XCTAssertEqual(pixel(now, .dark, over: ground), pixel(was, .dark, over: ground), name)
            }
        }
        let accents: [(Accent, Color)] = [
            (.amber, Color(red: 1.0, green: 0.635, blue: 0.231)),
            (.orange, Color(red: 1.0, green: 0.624, blue: 0.039)),
            (.pink, Color(red: 1.0, green: 0.216, blue: 0.373)),
            (.purple, Color(red: 0.749, green: 0.353, blue: 0.949)),
            (.indigo, Color(red: 0.369, green: 0.361, blue: 0.902)),
            (.blue, Color(red: 0.039, green: 0.518, blue: 1.0)),
            (.teal, Color(red: 0.251, green: 0.784, blue: 0.878)),
            (.green, Color(red: 0.188, green: 0.820, blue: 0.345)),
            (.graphite, Color(red: 0.596, green: 0.596, blue: 0.616)),
        ]
        for (accent, was) in accents {
            XCTAssertEqual(pixel(accent.color, .dark), pixel(was, .dark), accent.rawValue)
            // Words in the colour are the colour itself at night.
            XCTAssertEqual(pixel(accent.ink, .dark), pixel(was, .dark), accent.rawValue + " as words")
        }
        XCTAssertEqual(pixel(Theme.goodInk, .dark), pixel(Color(red: 0.188, green: 0.820, blue: 0.345), .dark), "good as words")
        XCTAssertEqual(pixel(Theme.dangerInk, .dark), pixel(Color(red: 1.0, green: 0.271, blue: 0.227), .dark), "danger as words")
    }

    /// By day the colours as words take the palette's deeper inks, which
    /// read at 4.5 to 1 or better on the day's grey; a fill keeps the light
    /// shade.
    func testTheDaysWordsInTheColourAreItsInk() {
        let inks: [(Accent, (Int, Int, Int))] = [
            (.amber, (0xA3, 0x5C, 0x09)), (.orange, (0xA1, 0x5E, 0x00)), (.pink, (0xDF, 0x00, 0x2B)),
            (.purple, (0xA3, 0x38, 0xD9)), (.indigo, (0x58, 0x56, 0xD6)), (.blue, (0x00, 0x6B, 0xE0)),
            (.teal, (0x21, 0x79, 0x89)), (.green, (0x21, 0x7E, 0x38)), (.graphite, (0x6E, 0x6E, 0x73)),
        ]
        let page = pixel(Theme.background, .light)
        for (accent, (red, green, blue)) in inks {
            let drawn = pixel(accent.ink, .light)
            XCTAssertEqual(drawn, [UInt8(red), UInt8(green), UInt8(blue), 255], accent.rawValue)
            XCTAssertGreaterThanOrEqual(contrast(drawn, page), 4.5, accent.rawValue)
        }
        XCTAssertEqual(pixel(Theme.goodInk, .light), [0x21, 0x7E, 0x38, 255])
        XCTAssertGreaterThanOrEqual(contrast(pixel(Theme.goodInk, .light), page), 4.5)
        XCTAssertEqual(pixel(Theme.dangerInk, .light), [0xD7, 0x00, 0x15, 255])
        XCTAssertGreaterThanOrEqual(contrast(pixel(Theme.dangerInk, .light), page), 4.5)
    }

    /// The day's quieter words read as words: 4.5 to 1 or better on the
    /// day's grey, on white and on the raised grey, and so do the faint ones,
    /// which by day are the same. At the system's sixty per cent they were
    /// 3.1 to 3.4 to 1.
    func testTheDaysQuieterWordsCanBeRead() {
        for (name, ground) in [("page", Theme.background), ("surface", Theme.surface), ("raised", Theme.surfaceHigh)] {
            let behind = pixel(ground, .light)
            XCTAssertGreaterThanOrEqual(contrast(pixel(Theme.muted, .light, over: ground), behind), 4.5, "muted on " + name)
            XCTAssertGreaterThanOrEqual(contrast(pixel(Theme.faint(0.45), .light, over: ground), behind), 4.5, "faint on " + name)
        }
    }

    /// WCAG's contrast between two drawn pixels.
    private func contrast(_ a: [UInt8], _ b: [UInt8]) -> Double {
        func luminance(_ p: [UInt8]) -> Double {
            let channel = { (byte: UInt8) -> Double in
                let c = Double(byte) / 255
                return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * channel(p[0]) + 0.7152 * channel(p[1]) + 0.0722 * channel(p[2])
        }
        let (x, y) = (luminance(a), luminance(b))
        return (max(x, y) + 0.05) / (min(x, y) + 0.05)
    }

    /// The day, as the shared palette writes it.
    func testTheDayIsThePalette() {
        let palette: [(String, Color, (Int, Int, Int))] = [
            ("background", Theme.background, (0xF2, 0xF2, 0xF7)),
            ("surface", Theme.surface, (0xFF, 0xFF, 0xFF)),
            ("surfaceHigh", Theme.surfaceHigh, (0xE5, 0xE5, 0xEA)),
            ("foreground", Theme.foreground, (0x1C, 0x1C, 0x1E)),
            ("good", Theme.good, (0x34, 0xC7, 0x59)),
            ("danger", Theme.danger, (0xFF, 0x3B, 0x30)),
            ("page", Theme.page, (0xF2, 0xF2, 0xF7)),
            ("amber", Accent.amber.color, (0xF2, 0x90, 0x1E)),
            ("orange", Accent.orange.color, (0xFF, 0x95, 0x00)),
            ("pink", Accent.pink.color, (0xFF, 0x2D, 0x55)),
            ("purple", Accent.purple.color, (0xAF, 0x52, 0xDE)),
            ("indigo", Accent.indigo.color, (0x58, 0x56, 0xD6)),
            ("blue", Accent.blue.color, (0x00, 0x7A, 0xFF)),
            ("teal", Accent.teal.color, (0x30, 0xB0, 0xC7)),
            ("green", Accent.green.color, (0x34, 0xC7, 0x59)),
            ("graphite", Accent.graphite.color, (0x8E, 0x8E, 0x93)),
        ]
        for (name, color, (red, green, blue)) in palette {
            XCTAssertEqual(pixel(color, .light), [UInt8(red), UInt8(green), UInt8(blue), 255], name)
        }
        // The translucent ones as the same shades drawn plainly would be.
        XCTAssertEqual(pixel(Theme.border, .light, over: .white), pixel(Color.black.opacity(0.10), .light, over: .white))
        XCTAssertEqual(pixel(Theme.muted, .light, over: .white),
                       pixel(Color(red: 60 / 255, green: 60 / 255, blue: 67 / 255).opacity(0.75), .light, over: .white))
        XCTAssertEqual(pixel(Theme.sunk(0.22), .light, over: .white), pixel(Color.black.opacity(0.055), .light, over: .white))
        // A lifted surface is white by day, a faint grey on one already lifted.
        XCTAssertEqual(pixel(Theme.lifted(0.05), .light, over: Theme.background), [255, 255, 255, 255])
        XCTAssertEqual(pixel(Theme.lifted(0.05, nested: true), .light, over: .white), [242, 242, 242, 255])
    }

    /// The same colour, told the appearance it is drawn in, follows it: the
    /// room is drawn dark inside an app that is light.
    func testAColourFollowsTheAppearanceItIsDrawnIn() {
        XCTAssertNotEqual(pixel(Theme.background, .dark), pixel(Theme.background, .light))
        XCTAssertEqual(pixel(Theme.ink, .light), [0, 0, 0, 255])
        XCTAssertEqual(pixel(Theme.ink, .dark), [255, 255, 255, 255])
    }
}
