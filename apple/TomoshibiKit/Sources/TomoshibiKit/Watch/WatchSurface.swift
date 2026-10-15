import AVFoundation
import SwiftUI
import WebKit

/// Where the player's picture is drawn: the AVPlayer's layer, or YouTube's
/// web view, both owned by the player rather than this view. The view only
/// lends them a place, so the stage and the strip trading places, or the room
/// redrawing, moves the picture without reloading the video.
struct WatchSurface: View {
    let player: WatchPlayer

    var body: some View {
        Representable(backend: player.backend)
    }
}

#if os(macOS)
private struct Representable: NSViewRepresentable {
    let backend: WatchBackend?

    func makeNSView(context: Context) -> SurfaceHost { SurfaceHost() }

    func updateNSView(_ view: SurfaceHost, context: Context) {
        view.show(backend)
    }
}

/// Layer-backed, not layer-hosting: YouTube's web view is a subview, and a
/// view that hosts a layer of its own draws no subviews -- the first build
/// showed a grey stage with the video playing unseen inside it. AVPlayer's
/// layer goes in a child that does host a layer, as the call's own pictures do
/// (VideoSurface).
final class SurfaceHost: NSView {
    private var current: WatchBackend?
    private let pictures = LayerHost()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pictures.autoresizingMask = [.width, .height]
        addSubview(pictures)
    }

    required init?(coder: NSCoder) { nil }

    @MainActor func show(_ backend: WatchBackend?) {
        guard backend !== current else { return }
        if let av = current as? AVBackend { av.layer.removeFromSuperlayer() }
        if let youtube = current as? YouTubeBackend { youtube.web.removeFromSuperview() }
        current = backend
        pictures.frame = bounds
        if let av = backend as? AVBackend {
            pictures.layer?.addSublayer(av.layer)
        } else if let youtube = backend as? YouTubeBackend {
            youtube.web.frame = bounds
            youtube.web.autoresizingMask = [.width, .height]
            addSubview(youtube.web)
        }
        needsLayout = true
        shown()
    }

    override func layout() {
        super.layout()
        pictures.frame = bounds
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        (current as? AVBackend)?.layer.frame = bounds
        CATransaction.commit()
        MainActor.assumeIsolated { shown() }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        MainActor.assumeIsolated { shown() }
    }

    @MainActor private func shown() {
        guard let av = current as? AVBackend, window != nil else { return }
        let scale = window?.backingScaleFactor ?? 2
        av.shown(pixels: CGSize(width: bounds.width * scale, height: bounds.height * scale))
        av.layerShown()
    }
}

/// Hosts a plain layer, into which AVPlayer's layer is put.
private final class LayerHost: NSView {
    override init(frame: NSRect) {
        super.init(frame: frame)
        layer = CALayer()
        wantsLayer = true
    }

    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#else
private struct Representable: UIViewRepresentable {
    let backend: WatchBackend?

    func makeUIView(context: Context) -> SurfaceHost { SurfaceHost() }

    func updateUIView(_ view: SurfaceHost, context: Context) {
        view.show(backend)
    }
}

final class SurfaceHost: UIView {
    private var current: WatchBackend?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
    }

    required init?(coder: NSCoder) { nil }

    @MainActor func show(_ backend: WatchBackend?) {
        guard backend !== current else { return }
        if let av = current as? AVBackend { av.layer.removeFromSuperlayer() }
        if let youtube = current as? YouTubeBackend { youtube.web.removeFromSuperview() }
        current = backend
        if let av = backend as? AVBackend {
            layer.addSublayer(av.layer)
        } else if let youtube = backend as? YouTubeBackend {
            youtube.web.frame = bounds
            youtube.web.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            addSubview(youtube.web)
        }
        setNeedsLayout()
        shown()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        (current as? AVBackend)?.layer.frame = bounds
        CATransaction.commit()
        MainActor.assumeIsolated { shown() }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        MainActor.assumeIsolated { shown() }
    }

    @MainActor private func shown() {
        guard let av = current as? AVBackend, window != nil else { return }
        let scale = window?.screen.scale ?? 2
        av.shown(pixels: CGSize(width: bounds.width * scale, height: bounds.height * scale))
        av.layerShown()
    }
}
#endif
