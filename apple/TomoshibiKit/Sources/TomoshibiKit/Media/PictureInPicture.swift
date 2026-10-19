#if os(iOS)
import AVKit
import os
import SwiftUI
import TomoshibiBasics
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
/// While the app is in front it is kept warm rather than fed: two frames a
/// second, so that the small window has a picture the moment it is wanted.
/// Fed nothing in front (the first answer to the screen share that took the
/// phone down, 2026-10-17), the layer was empty when somebody went to the
/// home screen and the system would not open the small window from it: the
/// owner had to leave the app twice. Every frame goes to it from the moment
/// the app is about to leave the front until it is back and the small window
/// has closed, which it is made to as the app comes back, as a FaceTime call
/// does; it stayed open over the call. (The phone itself was taken down by
/// frames left in tokio threads' autorelease pools, core/src/call/remote.rs
/// `drained`, not by this layer.)
///
/// A camera there fills the small window or is shown whole by the two shapes,
/// as it is in its tile (Framing), with the window's size as the system
/// reports it; a shared screen is shown whole. There is no blurred copy
/// behind a camera shown whole in the small window: the system draws that
/// window from this one layer and nothing else.
struct PictureInPictureHost: UIViewRepresentable {
    let call: Call
    let sid: String?
    var camera = false

    func makeUIView(context: Context) -> HostView {
        HostView(call: call)
    }

    func updateUIView(_ view: HostView, context: Context) {
        view.follow(sid, camera: camera)
    }

    static func dismantleUIView(_ view: HostView, coordinator: ()) {
        view.follow(nil, camera: false)
    }

    final class HostView: UIView, AVPictureInPictureSampleBufferPlaybackDelegate, AVPictureInPictureControllerDelegate {
        let renderer = VideoRenderer()
        private let call: Call
        /// What the small window would show, what the renderer is on, and
        /// whether every frame goes to it.
        private var sid: String?
        private var fed: String?
        private var full = false
        /// Whether the picture is a camera, its shape, and the small
        /// window's size, for whether it fills the window.
        private var camera = false
        private var picture = CGSize.zero
        private var smallWindow = CGSize.zero
        private var controller: AVPictureInPictureController?
        private var away = UIApplication.shared.applicationState != .active
        private var watching: [NSObjectProtocol] = []

        init(call: Call) {
            self.call = call
            super.init(frame: .zero)
            renderer.layer.videoGravity = .resizeAspect
            renderer.show(atMostEvery: away ? 0 : 0.5)
            renderer.onPictureSize = { [weak self] size in
                DispatchQueue.main.async {
                    MainActor.assumeIsolated {
                        self?.picture = size
                        self?.reframe()
                    }
                }
            }
            full = away
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
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.away = false
                    if let controller = self.controller, controller.isPictureInPictureActive { controller.stopPictureInPicture() }
                    self.feed()
                }
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

        func follow(_ next: String?, camera: Bool) {
            sid = next
            self.camera = camera
            feed()
            reframe()
        }

        private func reframe() {
            let fill = camera && Framing.fills(picture: picture, in: smallWindow)
            renderer.layer.videoGravity = fill ? .resizeAspectFill : .resizeAspect
        }

        /// On the picture it would show, every frame of it while the app is
        /// away or the small window is open and two a second otherwise.
        private func feed() {
            let showing = away || controller?.isPictureInPictureActive == true
            if sid != fed {
                if let fed { VideoHub.existing(call)?.remove(renderer, from: fed) }
                fed = sid
                if let sid {
                    VideoHub.of(call).add(renderer, to: sid)
                    VideoHub.existing(call)?.want((640, 360), for: renderer, on: sid)
                }
                renderer.layer.sampleBufferRenderer.flush()
            }
            guard showing != full else { return }
            full = showing
            renderer.show(atMostEvery: showing ? 0 : 0.5)
            Logger(subsystem: "app.tomoshibi", category: "video").notice("the small window's picture is \(self.sid ?? "none", privacy: .public), \(showing ? "every frame" : "kept warm", privacy: .public)")
        }

        func pictureInPictureController(_ controller: AVPictureInPictureController,
                                        restoreUserInterfaceForPictureInPictureStopWithCompletionHandler completion: @escaping (Bool) -> Void) {
            completion(true)
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
            smallWindow = CGSize(width: Int(newRenderSize.width), height: Int(newRenderSize.height))
            reframe()
            guard let sid else { return }
            VideoHub.existing(call)?.want((Int(newRenderSize.width), Int(newRenderSize.height)), for: renderer, on: sid)
        }
        func pictureInPictureController(_ controller: AVPictureInPictureController, skipByInterval skipInterval: CMTime) async {}
    }
}
#endif
