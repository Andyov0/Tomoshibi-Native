#if os(macOS)
import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit
import TomoshibiBasics
import TomoshibiCore

/// Sharing a screen, a window or an app on a Mac, with its sound.
///
/// What a browser cannot do is most of the reason this exists: Chrome on a Mac
/// shares a tab's sound and nothing else, while ScreenCaptureKit gives the
/// sound of whatever is being shown, or of the whole machine, and leaves this
/// app's own windows and sound out of it. The picture goes to a screen-share
/// video input and the sound to its own track, as the web publishes them
/// (room.ts, sound.ts): two tracks and two decisions for whoever listens.
@MainActor
public final class ScreenCapture: NSObject {
    /// The share menu's sizes (room.ts ~39-63): the shorter side, or 0 for
    /// "auto", which is 1440 at 60 frames.
    public enum Quality: Int, CaseIterable, Sendable {
        case auto = 0, p1080 = 1080, p1440 = 1440, p2160 = 2160

        /// The protocol crate's name for the same setting.
        public var share: ShareQuality {
            switch self {
            case .auto: return .auto
            case .p1080: return .p1080
            case .p1440: return .p1440
            case .p2160: return .p4k
            }
        }
    }

    private var stream: SCStream?
    private let output = Output()
    /// The configuration the running stream was started or retuned with, so
    /// a retune changes only what it is asked to.
    private var configuration: SCStreamConfiguration?

    public var isSharing: Bool { stream != nil }

    /// Told, on the main actor, when the system ends a share on its own: the
    /// Stop in the menu bar's video menu, the shared window closing, the
    /// display going away. Without it the tracks stayed published with
    /// nothing feeding them and the button kept saying "Stop sharing".
    public var onStopped: ((Error) -> Void)?

    public override init() {
        super.init()
    }

    /// Shows the system's picker and resolves with what the person chose, or
    /// nil when they cancelled.
    ///
    /// One picker for every capture. The picker is the system's single
    /// shared one and tells every observer it has about every answer, so with
    /// a screen and a sound each watching it, picking a window for one
    /// started both.
    public static func pick() async -> SCContentFilter? {
        await Picker.shared.pick()
    }

    @MainActor
    private final class Picker: NSObject, SCContentSharingPickerObserver {
        static let shared = Picker()
        private var picked: ((SCContentFilter?) -> Void)?

        func pick() async -> SCContentFilter? {
            // A picker the system declined to show never answers at all, so a
            // second press would otherwise leave the first one waiting for ever.
            finish(nil)
            return await withCheckedContinuation { continuation in
                picked = { continuation.resume(returning: $0) }
                let picker = SCContentSharingPicker.shared
                var configuration = SCContentSharingPickerConfiguration()
                configuration.allowedPickerModes = [.singleDisplay, .singleWindow, .singleApplication]
                configuration.excludedBundleIDs = [Bundle.main.bundleIdentifier ?? ""].filter { !$0.isEmpty }
                picker.defaultConfiguration = configuration
                // Control Center allows one stream per app unless told otherwise,
                // and it counts a stream whose app died as still running: after
                // one crash mid-share (2026-10-15, macOS 27) it refused every
                // later picker with "Maximum stream count has been reached",
                // silently, until it was itself restarted. A screen and a sound
                // may now run side by side as well (Sharing), so the system's
                // count is lifted and the app keeps its own.
                picker.maximumStreamCount = nil
                picker.add(self)
                picker.isActive = true
                picker.present()
            }
        }

        nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
            Task { @MainActor in self.finish(nil) }
        }

        nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
            Task { @MainActor in self.finish(filter) }
        }

        nonisolated func contentSharingPickerStartDidFailWithError(_ error: any Error) {
            Task { @MainActor in self.finish(nil) }
        }

        private func finish(_ filter: SCContentFilter?) {
            guard let done = picked else { return }
            picked = nil
            SCContentSharingPicker.shared.remove(self)
            SCContentSharingPicker.shared.isActive = false
            done(filter)
        }
    }

    /// The capture size for a filter at a quality: the content's own pixels,
    /// scaled down so its shorter side is no more than the quality asks.
    public static func size(for filter: SCContentFilter, quality: Quality) -> (width: Int, height: Int) {
        let scale = CGFloat(filter.pointPixelScale)
        var width = filter.contentRect.width * scale
        var height = filter.contentRect.height * scale
        let limit = CGFloat(quality == .auto ? 1440 : quality.rawValue)
        let shorter = min(width, height)
        if shorter > limit {
            width = width * limit / shorter
            height = height * limit / shorter
        }
        // Even sizes: the encoder works in 2x2 blocks of chroma.
        return (Int(width) & ~1, Int(height) & ~1)
    }

    /// Starts sending `filter` at `fps` to `video` (or no picture at all, for
    /// a share of sound alone), and its sound to `sound`. `hearing` captures
    /// the sound even while there is nowhere yet to send it, so that a share
    /// started while sound was being shared on its own can take its sound
    /// back when that stops, without the picture being started again.
    public func start(filter: SCContentFilter, width: Int, height: Int, fps: Int, video: VideoInput?, sound: AudioInput?, hearing: Bool = false) async throws {
        await stop()
        let configuration = SCStreamConfiguration()
        configuration.width = width
        configuration.height = height
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(max(1, fps)))
        configuration.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        configuration.showsCursor = true
        configuration.queueDepth = 6
        let listens = sound != nil || hearing
        configuration.capturesAudio = listens
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 48_000
        configuration.channelCount = 2
        route(video: video, sound: sound)
        if video == nil {
            // ScreenCaptureKit will not capture sound without a picture; the
            // least picture there is, once a second, thrown away (the web's
            // dummy video track for the same reason, sound.ts ~252-263).
            configuration.width = 2
            configuration.height = 2
            configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        }
        let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
        try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: output.queue)
        if listens {
            try stream.addStreamOutput(output, type: .audio, sampleHandlerQueue: output.queue)
        }
        try await stream.startCapture()
        self.stream = stream
        self.configuration = configuration
    }

    /// Sends the running capture's sound to `sound`, or nowhere: the screen's
    /// own sound stood down while sound is shared on its own, and taken back
    /// after. Only for a capture started with sound or `hearing`.
    public func send(sound: AudioInput?) {
        let output = self.output
        output.queue.sync { output.sound = sound }
    }

    /// The running capture at another size and rate, its pictures going to
    /// `video` from the next frame: the share menu's size and rate changed
    /// mid-share, and the uplink card's switch. ScreenCaptureKit changes the
    /// configuration of a running stream in place, so nothing is picked again
    /// and the sound carries on through the change.
    public func retune(width: Int, height: Int, fps: Int, video: VideoInput) async throws {
        guard let stream, let configuration else { throw CocoaError(.featureUnsupported) }
        configuration.width = width
        configuration.height = height
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(max(1, fps)))
        try await stream.updateConfiguration(configuration)
        let output = self.output
        output.queue.sync { output.video = video }
    }

    private func route(video: VideoInput?, sound: AudioInput?) {
        let output = self.output
        output.queue.sync {
            output.video = video
            output.sound = sound
        }
    }

    public func stop() async {
        guard let stream else { return }
        self.stream = nil
        configuration = nil
        try? await stream.stopCapture()
        route(video: nil, sound: nil)
    }

    /// Frames and sound from ScreenCaptureKit, on its own queue.
    final class Output: NSObject, SCStreamOutput, @unchecked Sendable {
        let queue = DispatchQueue(label: "tomoshibi.screen")
        var video: VideoInput?
        var sound: AudioInput?

        func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
            switch type {
            case .screen:
                guard let video, sample.isValid, Self.isComplete(sample), let buffer = sample.imageBuffer else { return }
                let micros = Int64(CMTimeGetSeconds(sample.presentationTimeStamp) * 1_000_000)
                video.push(pixelBuffer: UInt64(UInt(bitPattern: Unmanaged.passRetained(buffer).toOpaque())), timestampUs: micros, rotation: 0)
            case .audio:
                guard let sound, let read = SampleAudio.interleaved(sample) else { return }
                let floats = SampleAudio.stereo(read.samples, channels: read.channels)
                sound.push(samples: SampleAudio.int16(floats))
                // The same sound before it was rounded to 16 bits, for the
                // lossless stream (it does nothing for a sound not offered so).
                let micros = Int64(CMTimeGetSeconds(sample.presentationTimeStamp) * 1_000_000)
                sound.pushLossless(samples: floats, channels: 2, rate: UInt32(read.rate), timestampUs: micros)
            default:
                break
            }
        }

        /// ScreenCaptureKit also delivers buffers that only say nothing
        /// changed; those carry no new picture.
        static func isComplete(_ sample: CMSampleBuffer) -> Bool {
            guard let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
                  let raw = attachments.first?[.status] as? Int,
                  let status = SCFrameStatus(rawValue: raw) else { return false }
            return status == .complete
        }
    }
}

extension ScreenCapture: SCStreamDelegate {
    nonisolated public func stream(_ stream: SCStream, didStopWithError error: any Error) {
        Task { @MainActor in self.ended(stream, error) }
    }

    private func ended(_ ended: SCStream, _ error: Error) {
        // Only the stream still running: one this app stopped itself, or
        // replaced with another, has nothing left to tell.
        guard ended === stream else { return }
        stream = nil
        configuration = nil
        route(video: nil, sound: nil)
        onStopped?(error)
    }
}
#endif
