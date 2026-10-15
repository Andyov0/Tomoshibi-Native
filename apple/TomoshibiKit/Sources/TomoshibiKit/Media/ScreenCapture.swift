#if os(macOS)
import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit
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
    private var picked: ((SCContentFilter?) -> Void)?

    public var isSharing: Bool { stream != nil }

    public override init() {
        super.init()
    }

    /// Shows the system's picker and resolves with what the person chose, or
    /// nil when they cancelled.
    public func pick() async -> SCContentFilter? {
        await withCheckedContinuation { continuation in
            picked = { continuation.resume(returning: $0) }
            let picker = SCContentSharingPicker.shared
            var configuration = SCContentSharingPickerConfiguration()
            configuration.allowedPickerModes = [.singleDisplay, .singleWindow, .singleApplication]
            configuration.excludedBundleIDs = [Bundle.main.bundleIdentifier ?? ""].filter { !$0.isEmpty }
            picker.defaultConfiguration = configuration
            picker.add(self)
            picker.isActive = true
            picker.present()
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
    /// a share of sound alone), and its sound to `sound` when there is one.
    public func start(filter: SCContentFilter, width: Int, height: Int, fps: Int, video: VideoInput?, sound: AudioInput?) async throws {
        await stop()
        let configuration = SCStreamConfiguration()
        configuration.width = width
        configuration.height = height
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(max(1, fps)))
        configuration.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        configuration.showsCursor = true
        configuration.queueDepth = 6
        configuration.capturesAudio = sound != nil
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 48_000
        configuration.channelCount = 2
        output.video = video
        output.sound = sound
        if video == nil {
            // ScreenCaptureKit will not capture sound without a picture; the
            // least picture there is, once a second, thrown away (the web's
            // dummy video track for the same reason, sound.ts ~252-263).
            configuration.width = 2
            configuration.height = 2
            configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        }
        let stream = SCStream(filter: filter, configuration: configuration, delegate: nil)
        try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: output.queue)
        if sound != nil {
            try stream.addStreamOutput(output, type: .audio, sampleHandlerQueue: output.queue)
        }
        try await stream.startCapture()
        self.stream = stream
    }

    public func stop() async {
        guard let stream else { return }
        self.stream = nil
        try? await stream.stopCapture()
        output.video = nil
        output.sound = nil
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
                guard let sound, let samples = Self.interleaved(sample) else { return }
                sound.push(samples: samples)
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

        /// The sound as 16-bit interleaved stereo, which is what the input
        /// takes; ScreenCaptureKit gives 32-bit float, one buffer per channel.
        static func interleaved(_ sample: CMSampleBuffer) -> [Int16]? {
            var list = AudioBufferList()
            var block: CMBlockBuffer?
            let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sample, bufferListSizeNeededOut: nil, bufferListOut: &list,
                bufferListSize: MemoryLayout<AudioBufferList>.size + MemoryLayout<AudioBuffer>.size,
                blockBufferAllocator: nil, blockBufferMemoryAllocator: nil,
                flags: kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, blockBufferOut: &block)
            guard status == noErr else { return nil }
            let buffers = UnsafeMutableAudioBufferListPointer(&list)
            let frames = CMSampleBufferGetNumSamples(sample)
            guard frames > 0, buffers.count > 0 else { return nil }
            var out = [Int16](repeating: 0, count: frames * 2)
            let left = buffers[0].mData?.assumingMemoryBound(to: Float.self)
            let right = (buffers.count > 1 ? buffers[1] : buffers[0]).mData?.assumingMemoryBound(to: Float.self)
            guard let left, let right else { return nil }
            for i in 0..<frames {
                out[i * 2] = Int16(max(-1, min(1, left[i])) * 32767)
                out[i * 2 + 1] = Int16(max(-1, min(1, right[i])) * 32767)
            }
            return out
        }
    }
}

extension ScreenCapture: SCContentSharingPickerObserver {
    nonisolated public func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        Task { @MainActor in self.finish(nil) }
    }

    nonisolated public func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
        Task { @MainActor in self.finish(filter) }
    }

    nonisolated public func contentSharingPickerStartDidFailWithError(_ error: any Error) {
        Task { @MainActor in self.finish(nil) }
    }

    private func finish(_ filter: SCContentFilter?) {
        let done = picked
        picked = nil
        SCContentSharingPicker.shared.remove(self)
        SCContentSharingPicker.shared.isActive = false
        done?(filter)
    }
}
#endif
