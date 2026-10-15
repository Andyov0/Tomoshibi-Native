import AVFoundation
import SwiftUI
import TomoshibiCore

/// One picture: a camera, a shared screen, or a person with their camera off.
///
/// Drawn as the web draws a tile (components/room/SurfaceTile.tsx, Tile.tsx):
/// the picture fitted rather than cropped when it is a screen, the name at the
/// foot, a muted microphone said in the corner, an amber edge while the person
/// speaks, the raised hand in the top corner until it is lowered, and whatever
/// they just reacted with floating up off it.
struct Tile: View {
    let surface: Surface
    let call: Call
    let speaking: Bool
    var compact = false

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            RoundedRectangle(cornerRadius: Theme.tileRadius).fill(Theme.surface)
            if let sid = surface.sid {
                VideoSurface(call: call, sid: sid, fill: surface.kind == .camera)
                    .clipShape(RoundedRectangle(cornerRadius: Theme.tileRadius))
                    .transition(.opacity.animation(Theme.settle))
            } else {
                Avatar(name: surface.person.name, identity: surface.person.id)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .transition(.opacity.animation(Theme.settle))
            }
            Label(surface: surface, compact: compact)
                .padding(compact ? 4 : 8)
            // What they just reacted with, rising off every picture of theirs
            // as the web draws it on each (Room.tsx, Reacting.tsx).
            FloatingReactions(call: call, identity: surface.person.id, compact: compact)
            if surface.person.handUp && surface.kind == .camera {
                Text("\u{270B}")
                    .font(.system(size: compact ? 12 : 15))
                    .frame(width: compact ? 22 : 28, height: compact ? 22 : 28)
                    .background(Circle().fill(Theme.tally))
                    .padding(8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .transition(Theme.arrive)
                    .accessibilityLabel(t("Raise your hand"))
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: Theme.tileRadius)
                .strokeBorder(speaking && surface.kind == .camera ? Theme.tally : .clear, lineWidth: 2)
                .animation(Theme.settle, value: speaking)
        )
        .animation(Theme.settle, value: surface.sid)
        .animation(Theme.settle, value: surface.person.handUp)
    }

    private struct Label: View {
        let surface: Surface
        let compact: Bool

        var body: some View {
            HStack(spacing: 4) {
                if surface.kind == .camera, let mic = surface.person.track(.microphone), mic.muted {
                    Image(systemName: "mic.slash.fill").font(.system(size: compact ? 9 : 11, weight: .semibold))
                        .foregroundStyle(Theme.foreground)
                        .accessibilityLabel(t("microphone off"))
                } else if surface.kind == .camera, surface.person.track(.microphone) == nil {
                    Image(systemName: "mic.slash.fill").font(.system(size: compact ? 9 : 11, weight: .semibold))
                        .foregroundStyle(Theme.muted)
                }
                Text(surface.kind == .screen ? t("{name} (screen)", ["name": surface.person.name]) : surface.person.name)
                    .font(.system(size: compact ? 10.5 : 12.5, weight: .medium))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(1)
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .glassSurface(Capsule())
        }
    }
}

/// A person with no picture: their initials on a colour of their own, steady
/// for the same identity, as the web's marble avatar is steady for its seed.
struct Avatar: View {
    let name: String
    let identity: String

    var body: some View {
        GeometryReader { geometry in
            let side = min(geometry.size.width, geometry.size.height) * 0.36
            ZStack {
                Circle().fill(color)
                Text(initials)
                    .font(.system(size: side * 0.38, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
            }
            .frame(width: side, height: side)
            .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
        }
    }

    private var initials: String {
        let words = name.split(separator: " ")
        let letters = words.prefix(2).compactMap(\.first)
        return letters.isEmpty ? "?" : String(letters).uppercased()
    }

    /// Warm hues only, close to the palette in web/src/live/avatar.ts.
    private var color: Color {
        let palette: [Color] = [
            Color(red: 0.55, green: 0.33, blue: 0.24), Color(red: 0.42, green: 0.36, blue: 0.27),
            Color(red: 0.33, green: 0.38, blue: 0.33), Color(red: 0.48, green: 0.30, blue: 0.33),
            Color(red: 0.36, green: 0.33, blue: 0.40), Color(red: 0.50, green: 0.42, blue: 0.25),
        ]
        var hash: UInt64 = 1469598103934665603
        for byte in identity.utf8 { hash = (hash ^ UInt64(byte)) &* 1099511628211 }
        return palette[Int(hash % UInt64(palette.count))]
    }
}

/// This participant's own camera, drawn straight from the capture session.
struct SelfView: View {
    let session: AVCaptureSession

    var body: some View {
        PreviewRepresentable(session: session)
            .clipShape(RoundedRectangle(cornerRadius: Theme.tileRadius))
    }
}

#if os(macOS)
private struct PreviewRepresentable: NSViewRepresentable {
    let session: AVCaptureSession
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        // Mirrored, as every camera preview is: a person expects to see what a
        // mirror shows them. What is sent is not mirrored.
        layer.connection?.automaticallyAdjustsVideoMirroring = false
        layer.connection?.isVideoMirrored = true
        view.layer = layer
        view.wantsLayer = true
        return view
    }
    func updateNSView(_ view: NSView, context: Context) {}
}
#else
private struct PreviewRepresentable: UIViewRepresentable {
    let session: AVCaptureSession
    func makeUIView(context: Context) -> PreviewView {
        let view = PreviewView()
        view.preview.session = session
        view.preview.videoGravity = .resizeAspectFill
        return view
    }
    func updateUIView(_ view: PreviewView, context: Context) {}

    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var preview: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
    }
}
#endif
