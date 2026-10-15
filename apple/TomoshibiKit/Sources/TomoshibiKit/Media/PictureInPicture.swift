#if os(iOS)
import AVKit
import SwiftUI
import TomoshibiCore

/// The call in a small window when the app goes to the background: whatever
/// is on the stage, or the person being talked to.
///
/// A browser tab that is not in front shows nothing; a telephone call shows
/// the other person in a corner over whatever else is open, and so does this.
/// The picture comes from the hub like any tile's, through a renderer of its
/// own whose layer sits under the stage, because the system will only take a
/// picture into a small window from a layer that is in a window already.
struct PictureInPictureHost: UIViewRepresentable {
    let call: Call
    let sid: String?

    func makeUIView(context: Context) -> HostView {
        HostView(call: call)
    }

    func updateUIView(_ view: HostView, context: Context) {
        view.follow(sid)
    }

    static func dismantleUIView(_ view: HostView, coordinator: ()) {
        view.follow(nil)
    }

    final class HostView: UIView, AVPictureInPictureSampleBufferPlaybackDelegate {
        let renderer = VideoRenderer()
        private let call: Call
        private var sid: String?
        private var controller: AVPictureInPictureController?

        init(call: Call) {
            self.call = call
            super.init(frame: .zero)
            renderer.layer.videoGravity = .resizeAspect
            layer.addSublayer(renderer.layer)
            if AVPictureInPictureController.isPictureInPictureSupported() {
                let source = AVPictureInPictureController.ContentSource(sampleBufferDisplayLayer: renderer.layer, playbackDelegate: self)
                let controller = AVPictureInPictureController(contentSource: source)
                controller.canStartPictureInPictureAutomaticallyFromInline = true
                self.controller = controller
            }
        }

        required init?(coder: NSCoder) { nil }

        override func layoutSubviews() {
            super.layoutSubviews()
            renderer.layer.frame = bounds
        }

        func follow(_ next: String?) {
            guard next != sid else { return }
            if let sid { VideoHub.of(call).remove(renderer, from: sid) }
            sid = next
            if let next {
                VideoHub.of(call).add(renderer, to: next)
                VideoHub.of(call).want((640, 360), for: renderer, on: next)
            }
            renderer.layer.sampleBufferRenderer.flush()
        }

        // A call is live: nothing to pause, nothing to skip, no end.
        func pictureInPictureController(_ controller: AVPictureInPictureController, setPlaying playing: Bool) {}
        func pictureInPictureControllerTimeRangeForPlayback(_ controller: AVPictureInPictureController) -> CMTimeRange {
            CMTimeRange(start: .negativeInfinity, duration: .positiveInfinity)
        }
        func pictureInPictureControllerIsPlaybackPaused(_ controller: AVPictureInPictureController) -> Bool { false }
        func pictureInPictureController(_ controller: AVPictureInPictureController, didTransitionToRenderSize newRenderSize: CMVideoDimensions) {
            guard let sid else { return }
            VideoHub.of(call).want((Int(newRenderSize.width), Int(newRenderSize.height)), for: renderer, on: sid)
        }
        func pictureInPictureController(_ controller: AVPictureInPictureController, skipByInterval skipInterval: CMTime) async {}
    }
}
#endif
