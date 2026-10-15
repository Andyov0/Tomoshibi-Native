// Draws the app icon from the web client's own mark (web/public/favicon.svg):
// a room of pictures with one of them live, in the ground, a neutral from the
// warm ramp, and the one hue the interface spends.
//
// The art is full-bleed: the systems cut the corners themselves, iOS always
// and macOS from 26 on, and a mark that drew its own rounded corner was cut
// again inside them and sat in a frame of the wrong shape. The four pictures
// keep the favicon's proportions on its 32-unit grid, a little smaller in the
// field, since an icon is seen at its full size and a favicon at sixteen
// pixels.
//
//   swift apple/icon/make-icon.swift apple/App/Assets.xcassets/AppIcon.appiconset

import AppKit
import CoreGraphics
import Foundation

let out = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

func colour(_ hex: UInt32) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

func draw(_ pixels: Int) -> Data {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let side = CGFloat(pixels)
    context.setFillColor(colour(0x0F0E0D))
    context.fill(CGRect(x: 0, y: 0, width: side, height: side))
    // The favicon's 32 units, the pictures drawn at 0.86 of their size about
    // the middle so the corners the system cuts leave them room.
    let unit = side / 32 * 0.86
    let offset = (side - 32 * unit) / 2
    let tiles: [(CGFloat, CGFloat, UInt32)] = [(6, 8, 0x55524E), (17, 8, 0x55524E), (6, 17, 0x55524E), (17, 17, 0xFFA23B)]
    for (x, y, hex) in tiles {
        // CoreGraphics counts up from the bottom; the SVG counts down from the top.
        let rect = CGRect(x: offset + x * unit, y: side - (offset + (y + 7) * unit), width: 9 * unit, height: 7 * unit)
        context.addPath(CGPath(roundedRect: rect, cornerWidth: 2 * unit, cornerHeight: 2 * unit, transform: nil))
        context.setFillColor(colour(hex))
        context.fillPath()
    }
    let image = context.makeImage()!
    return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
}

var images: [[String: String]] = []
for (points, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
    let name = "mac-\(points)@\(scale)x.png"
    try draw(points * scale).write(to: out.appendingPathComponent(name))
    images.append(["idiom": "mac", "size": "\(points)x\(points)", "scale": "\(scale)x", "filename": name])
}
try draw(1024).write(to: out.appendingPathComponent("ios-1024.png"))
images.append(["idiom": "universal", "platform": "ios", "size": "1024x1024", "filename": "ios-1024.png"])

let contents: [String: Any] = ["images": images, "info": ["author": "xcode", "version": 1]]
let json = try JSONSerialization.data(withJSONObject: contents, options: [.prettyPrinted, .sortedKeys])
try json.write(to: out.appendingPathComponent("Contents.json"))
let catalog = out.deletingLastPathComponent().appendingPathComponent("Contents.json")
if !FileManager.default.fileExists(atPath: catalog.path) {
    try #"{"info":{"author":"xcode","version":1}}"#.data(using: .utf8)!.write(to: catalog)
}
print("wrote \(images.count) images to \(out.path)")
