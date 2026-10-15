import AVFoundation
import Foundation
import TomoshibiCore

/// Lossless streams, played through the call's engine on nodes of their own
/// at each stream's own rate (lossless-receiver.ts plays at the stream's rate
/// so that nothing between the packet and the sound card resamples it; the
/// engine's mixer does the one conversion the hardware needs).
///
/// The core keeps the web's playout buffer and paces the samples; this only
/// carries them to the hardware. A stream's level follows whatever the
/// listener set for the Opus track it stands in for.
final class LosslessPlayback: LosslessSink, @unchecked Sendable {
    weak var audio: CallAudio?
    private let lock = NSLock()
    private var streams: [String: Stream] = [:]
    private var levels: [String: Float] = [:]

    private final class Stream {
        let node: AVAudioSourceNode
        let ring: PCMRing
        let rate: UInt32
        let channels: UInt32

        init(rate: UInt32, channels: UInt32) {
            let ring = PCMRing(channels: Int(channels), startFrames: Int(rate) / 50, ceilingFrames: Int(rate) / 5, settleFrames: Int(rate) / 25)
            self.ring = ring
            self.rate = rate
            self.channels = channels
            let format = AVAudioFormat(standardFormatWithSampleRate: Double(rate), channels: AVAudioChannelCount(channels))!
            node = AVAudioSourceNode(format: format) { _, _, frames, list in
                ring.render(into: UnsafeMutableAudioBufferListPointer(list), frames: Int(frames))
                return noErr
            }
        }
    }

    static func key(_ identity: String, music: Bool) -> String {
        identity + (music ? "/music" : "/screen")
    }

    func onLossless(identity: String, music: Bool, samples: [Float], rate: UInt32, channels: UInt32) {
        let key = Self.key(identity, music: music)
        lock.lock()
        let existing = streams[key]
        lock.unlock()
        if let existing, existing.rate == rate, existing.channels == channels {
            existing.ring.write(floats: samples)
            return
        }
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.open(key, rate: rate, channels: channels, first: samples) }
        }
    }

    func onLosslessEnd(identity: String, music: Bool) {
        let key = Self.key(identity, music: music)
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.close(key) }
        }
    }

    /// The listener's level for this person's sound.
    @MainActor func setLevel(_ identity: String, music: Bool, _ level: Float) {
        let key = Self.key(identity, music: music)
        lock.lock()
        levels[key] = level
        let stream = streams[key]
        lock.unlock()
        stream?.node.volume = level
    }

    @MainActor private func open(_ key: String, rate: UInt32, channels: UInt32, first: [Float]) {
        close(key)
        guard let audio else { return }
        try? audio.start()
        let stream = Stream(rate: rate, channels: channels)
        audio.engine.attach(stream.node)
        audio.engine.connect(stream.node, to: audio.engine.mainMixerNode, format: stream.node.outputFormat(forBus: 0))
        lock.lock()
        stream.node.volume = levels[key] ?? 1
        streams[key] = stream
        lock.unlock()
        stream.ring.write(floats: first)
    }

    @MainActor private func close(_ key: String) {
        lock.lock()
        let stream = streams.removeValue(forKey: key)
        lock.unlock()
        guard let stream, let audio else { return }
        audio.engine.disconnectNodeOutput(stream.node)
        audio.engine.detach(stream.node)
    }

    @MainActor func closeAll() {
        lock.lock()
        let keys = Array(streams.keys)
        lock.unlock()
        for key in keys { close(key) }
    }
}
