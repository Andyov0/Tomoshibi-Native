// Draws the disk image's window background: the app on the left, an arrow,
// Applications on the right, and one line saying what to do, in the app's
// own colours. Run once, by make-dmg.sh; the picture is not kept in the repo.
//
//   swift background.swift <out.png>
import AppKit

let width = 640.0, height = 400.0
let scale = 2.0
let out = CommandLine.arguments[1]
let image = NSImage(size: NSSize(width: width, height: height))
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
rep.size = NSSize(width: width, height: height)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

// Light, warm paper: Finder writes the icons' names in dark text over a
// window's picture whatever the system's appearance, and over the evening
// blue of the app the names could not be read.
let ground = NSGradient(colors: [NSColor(red: 0.98, green: 0.97, blue: 0.95, alpha: 1), NSColor(red: 0.92, green: 0.90, blue: 0.87, alpha: 1)])!
ground.draw(in: NSRect(x: 0, y: 0, width: width, height: height), angle: -90)
let glow = NSGradient(colors: [NSColor(red: 1.0, green: 0.62, blue: 0.24, alpha: 0.16), NSColor(red: 1.0, green: 0.62, blue: 0.24, alpha: 0)])!
glow.draw(fromCenter: NSPoint(x: 170, y: 210), radius: 0, toCenter: NSPoint(x: 170, y: 210), radius: 190, options: [])

// The arrow, between where the two icons are placed (make-dmg.sh: 170 and 470).
let arrow = NSBezierPath()
arrow.move(to: NSPoint(x: 262, y: 210))
arrow.line(to: NSPoint(x: 370, y: 210))
arrow.lineWidth = 5
arrow.lineCapStyle = .round
NSColor(white: 0.25, alpha: 0.45).setStroke()
arrow.stroke()
let head = NSBezierPath()
head.move(to: NSPoint(x: 352, y: 228))
head.line(to: NSPoint(x: 374, y: 210))
head.line(to: NSPoint(x: 352, y: 192))
head.lineWidth = 5
head.lineCapStyle = .round
head.lineJoinStyle = .round
head.stroke()

func line(_ text: String, y: Double, size: Double, alpha: Double, weight: NSFont.Weight) {
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: weight),
        .foregroundColor: NSColor(white: 0.1, alpha: alpha),
        .paragraphStyle: style,
    ]
    NSAttributedString(string: text, attributes: attributes).draw(in: NSRect(x: 0, y: y, width: width, height: size * 1.6))
}
line("把 Tomoshibi 拖到「应用程序」", y: 70, size: 17, alpha: 0.8, weight: .semibold)
line("Drag Tomoshibi to Applications", y: 46, size: 12.5, alpha: 0.5, weight: .regular)

NSGraphicsContext.restoreGraphicsState()
image.addRepresentation(rep)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
