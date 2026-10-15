import AVFoundation
import SwiftUI
import TomoshibiCore

/// A remote picture in SwiftUI.
///
/// While it is on screen it is attached to the track and asks the server for
/// the layer that matches its size in device pixels, not points: the web asks
/// for `pixelDensity: "screen"` for the same reason (room.ts ~624), a share
/// read on a Retina screen at half its pixels is a share that cannot be read.
/// When it leaves the screen it detaches, and the server stops sending.
public struct VideoSurface: View {
    let call: Call
    let sid: String
    var fill: Bool

    public init(call: Call, sid: String, fill: Bool = false) {
        self.call = call
        self.sid = sid
        self.fill = fill
    }

    public var body: some View {
        GeometryReader { geometry in
            Representable(call: call, sid: sid, fill: fill, size: geometry.size)
        }
    }
}

#if os(macOS)
private struct Representable: NSViewRepresentable {
    let call: Call
    let sid: String
    let fill: Bool
    let size: CGSize

    func makeNSView(context: Context) -> SurfaceView {
        let view = SurfaceView()
        view.connect(call: call, sid: sid)
        return view
    }

    func updateNSView(_ view: SurfaceView, context: Context) {
        view.renderer.layer.videoGravity = fill ? .resizeAspectFill : .resizeAspect
        view.ask(points: size, scale: view.window?.backingScaleFactor ?? 2)
    }

    static func dismantleNSView(_ view: SurfaceView, coordinator: ()) {
        view.disconnect()
    }
}

final class SurfaceView: NSView {
    let renderer = VideoRenderer()
    private var call: Call?
    private var sid = ""
    private var asked = (0, 0)

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer = CALayer()
        layer?.addSublayer(renderer.layer)
    }

    required init?(coder: NSCoder) { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        renderer.layer.frame = bounds
        CATransaction.commit()
    }

    func connect(call: Call, sid: String) {
        self.call = call
        self.sid = sid
        VideoHub.of(call).add(renderer, to: sid)
    }

    func disconnect() {
        if let call { VideoHub.of(call).remove(renderer, from: sid) }
        call = nil
    }

    func ask(points: CGSize, scale: CGFloat) {
        let wanted = (Int(points.width * scale), Int(points.height * scale))
        guard wanted.0 > 0, wanted.1 > 0, wanted != asked, let call else { return }
        asked = wanted
        VideoHub.of(call).want(wanted, for: renderer, on: sid)
    }
}
#else
private struct Representable: UIViewRepresentable {
    let call: Call
    let sid: String
    let fill: Bool
    let size: CGSize

    func makeUIView(context: Context) -> SurfaceView {
        let view = SurfaceView()
        view.connect(call: call, sid: sid)
        return view
    }

    func updateUIView(_ view: SurfaceView, context: Context) {
        view.renderer.layer.videoGravity = fill ? .resizeAspectFill : .resizeAspect
        view.ask(points: size, scale: view.window?.screen.scale ?? 3)
    }

    static func dismantleUIView(_ view: SurfaceView, coordinator: ()) {
        view.disconnect()
    }
}

final class SurfaceView: UIView {
    let renderer = VideoRenderer()
    private var call: Call?
    private var sid = ""
    private var asked = (0, 0)

    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.addSublayer(renderer.layer)
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        renderer.layer.frame = bounds
        CATransaction.commit()
    }

    func connect(call: Call, sid: String) {
        self.call = call
        self.sid = sid
        VideoHub.of(call).add(renderer, to: sid)
    }

    func disconnect() {
        if let call { VideoHub.of(call).remove(renderer, from: sid) }
        call = nil
    }

    func ask(points: CGSize, scale: CGFloat) {
        let wanted = (Int(points.width * scale), Int(points.height * scale))
        guard wanted.0 > 0, wanted.1 > 0, wanted != asked, let call else { return }
        asked = wanted
        VideoHub.of(call).want(wanted, for: renderer, on: sid)
    }
}
#endif
