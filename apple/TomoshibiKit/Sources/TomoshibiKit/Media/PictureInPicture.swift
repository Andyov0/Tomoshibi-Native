#if os(iOS)
import AVKit
import os
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
///
/// It is fed only while it can be seen: from the moment the app is about to
/// leave the front until it is back and the small window has closed. Fed all
/// the time, it took every frame of a shared screen while lying under the
/// room's opaque ground, where the compositor never drew it and so never let
/// a frame go: on the owner's iPhone backboardd, which holds the layers'
/// image queues, grew by about a 2224 by 1440 frame each thirtieth of a
/// second and was killed at 3.45 GB seventeen seconds after a screen share
/// began, taking the screen with it to the home screen (2026-10-17, read off
/// the phone's log: the share's sound arriving at 12:18:28.8, the compositor
/// enhancing the stage's picture alone from 12:18:29, the kill at 12:18:46).
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

    final class HostView: UIView, AVPictureInPictureSampleBufferPlaybackDelegate, AVPictureInPictureControllerDelegate {
        let renderer = VideoRenderer()
        private let call: Call
        /// What the small window would show, and what the renderer is on now:
        /// the same while it is fed, nil while it is not.
        private var sid: String?
        private var fed: String?
        private var controller: AVPictureInPictureController?
        private var away = UIApplication.shared.applicationState != .active
        private var watching: [NSObjectProtocol] = []

        init(call: Call) {
            self.call = call
            super.init(frame: .zero)
            renderer.layer.videoGravity = .resizeAspect
            layer.addSublayer(renderer.layer)
            if AVPictureInPictureController.isPictureInPictureSupported() {
                let source = AVPictureInPictureController.ContentSource(sampleBufferDisplayLayer: renderer.layer, playbackDelegate: self)
                let controller = AVPictureInPictureController(contentSource: source)
                controller.canStartPictureInPictureAutomaticallyFromInline = true
                controller.delegate = self
                self.controller = controller
            }
            let center = NotificationCenter.default
            watching.append(center.addObserver(forName: UIApplication.willResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.away = true; self?.feed() }
            })
            watching.append(center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.away = false; self?.feed() }
            })
        }

        deinit {
            for observer in watching { NotificationCenter.default.removeObserver(observer) }
        }

        required init?(coder: NSCoder) { nil }

        override func layoutSubviews() {
            super.layoutSubviews()
            renderer.layer.frame = bounds
        }

        func follow(_ next: String?) {
            sid = next
            feed()
        }

        /// On the picture it would show while the app is away or the small
        /// window is open, and on nothing otherwise.
        private func feed() {
            let showing = away || controller?.isPictureInPictureActive == true
            let next = showing ? sid : nil
            guard next != fed else { return }
            if let fed { VideoHub.existing(call)?.remove(renderer, from: fed) }
            fed = next
            Logger(subsystem: "app.tomoshibi", category: "video").notice("the small window's picture is \(next ?? "off", privacy: .public)")
            if let next {
                VideoHub.of(call).add(renderer, to: next)
                VideoHub.existing(call)?.want((640, 360), for: renderer, on: next)
            }
            renderer.layer.sampleBufferRenderer.flush()
        }

        func pictureInPictureControllerDidStopPictureInPicture(_ controller: AVPictureInPictureController) {
            feed()
        }

        // A call is live: nothing to pause, nothing to skip, no end.
        func pictureInPictureController(_ controller: AVPictureInPictureController, setPlaying playing: Bool) {}
        func pictureInPictureControllerTimeRangeForPlayback(_ controller: AVPictureInPictureController) -> CMTimeRange {
            CMTimeRange(start: .negativeInfinity, duration: .positiveInfinity)
        }
        func pictureInPictureControllerIsPlaybackPaused(_ controller: AVPictureInPictureController) -> Bool { false }
        func pictureInPictureController(_ controller: AVPictureInPictureController, didTransitionToRenderSize newRenderSize: CMVideoDimensions) {
            guard let sid else { return }
            VideoHub.existing(call)?.want((Int(newRenderSize.width), Int(newRenderSize.height)), for: renderer, on: sid)
        }
        func pictureInPictureController(_ controller: AVPictureInPictureController, skipByInterval skipInterval: CMTime) async {}
    }
}
#endif
