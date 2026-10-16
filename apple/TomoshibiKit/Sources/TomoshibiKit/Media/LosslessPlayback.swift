import AVFoundation
import Foundation
import os
import TomoshibiCore

/// Lossless streams, played through the call's engine on nodes of their own
/// at each stream's own rate (lossless-receiver.ts plays at the stream's rate
/// so that nothing between the packet and the sound card resamples it; the
/// engine's mixer does the one conversion the hardware needs).
///
/// The core keeps the web's playout buffer and paces the samples; this
/// carries them to the hardware at the level the web plays them at
/// (lossless-receiver.ts `levelOf`): the listener's own volume for that
/// person's sound, from the same record the sound panel keeps, and for music
/// the song desk's duck and fade-in on top. Before this a lossless stream
/// played at full level whatever the listener had set, came in at full level
/// in the middle of a sentence, and was never turned down under speech, while
/// the Opus track it stands in for was.
final class LosslessPlayback: LosslessSink, @unchecked Sendable {
    weak var audio: CallAudio?
    private let lock = NSLock()
    private var streams: [String: Stream] = [:]
    /// The listener's level for each person's sound, by stream key.
    private var levels: [String: Float] = [:]
    /// How far each person's music is turned down now, duck and fade-in
    /// together, by identity (duck.ts `musicFactor`).
    private var ducks: [String: Float] = [:]
    /// What has arrived for a stream whose node is still being made on the
    /// main thread, in order, with the format it is in.
    private var pending: [String: (rate: UInt32, channels: UInt32, samples: [Float])] = [:]

    /// What a stream plays at: set from the main thread, followed sample by
    /// sample on the audio thread.
    ///
    /// Eased rather than stepped, as the web's `setTargetAtTime(level, now,
    /// 0.03)` eases it: under speech the duck moves every 40 ms, and a gain
    /// stepped that often is heard as clicks.
    final class Gain: @unchecked Sendable {
        private let target: OSAllocatedUnfairLock<Float>
        private var current: Float
        private let coefficient: Float

        init(level: Float, rate: Double) {
            target = OSAllocatedUnfairLock(initialState: level)
            current = level
            // A time constant of 30 ms at the stream's own rate.
            coefficient = Float(1 - exp(-1 / (0.03 * max(rate, 1))))
        }

        func set(_ level: Float) {
            target.withLock { $0 = max(0, min(1, level)) }
        }

        /// Called on the audio thread: one short lock, no allocation.
        func apply(to list: UnsafeMutableAudioBufferListPointer, frames: Int) {
            let goal = target.withLockUnchecked { $0 }
            if current == goal, goal == 1 { return }
            for frame in 0..<frames {
                current += (goal - current) * coefficient
                for buffer in list {
                    guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
                    data[frame] *= current
                }
            }
            if abs(current - goal) < 1e-5 { current = goal }
        }
    }

    private final class Stream {
        let node: AVAudioSourceNode
        let ring: PCMRing
        let gain: Gain
        let rate: UInt32
        let channels: UInt32

        init(rate: UInt32, channels: UInt32, level: Float) {
            let ring = PCMRing(channels: Int(channels), startFrames: Int(rate) / 50, ceilingFrames: Int(rate) / 5, settleFrames: Int(rate) / 25)
            let gain = Gain(level: level, rate: Double(rate))
            self.ring = ring
            self.gain = gain
            self.rate = rate
            self.channels = channels
            let format = AVAudioFormat(standardFormatWithSampleRate: Double(rate), channels: AVAudioChannelCount(channels))!
            node = AVAudioSourceNode(format: format) { _, _, frames, list in
                let buffers = UnsafeMutableAudioBufferListPointer(list)
                ring.render(into: buffers, frames: Int(frames))
                gain.apply(to: buffers, frames: Int(frames))
                return noErr
            }
        }
    }

    static func key(_ identity: String, music: Bool) -> String {
        identity + (music ? "/music" : "/screen")
    }

    /// How many blocks and ends the core has handed over, for the song
    /// desk's debug log: a stream the core gives up on after a few blocks
    /// looks, from here, exactly like one that never came.
    private(set) var delivered = (blocks: 0, ends: 0, loudest: Float(0))

    func onLossless(identity: String, music: Bool, samples: [Float], rate: UInt32, channels: UInt32) {
        let key = Self.key(identity, music: music)
        let loudest = samples.reduce(Float(0)) { max($0, abs($1)) }
        lock.lock()
        delivered.blocks += 1
        delivered.loudest = max(delivered.loudest, loudest)
        let existing = streams[key]
        if let existing, existing.rate == rate, existing.channels == channels {
            lock.unlock()
            existing.ring.write(floats: samples)
            return
        }
        // A node is made on the main thread, and blocks go on arriving a
        // hundred a second meanwhile: they wait here, in order, for the node
        // rather than each asking for one. Before, every one of them asked,
        // and each new node threw away what the last had been given.
        if let waiting = pending[key], waiting.rate == rate, waiting.channels == channels {
            pending[key]?.samples.append(contentsOf: samples)
            lock.unlock()
            return
        }
        pending[key] = (rate, channels, samples)
        lock.unlock()
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.open(key) }
        }
    }

    func onLosslessEnd(identity: String, music: Bool) {
        let key = Self.key(identity, music: music)
        lock.lock()
        delivered.ends += 1
        lock.unlock()
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.close(key) }
        }
    }

    /// The listener's level for this person's sound.
    @MainActor func setLevel(_ identity: String, music: Bool, _ level: Float) {
        let key = Self.key(identity, music: music)
        lock.lock()
        levels[key] = level
        lock.unlock()
        follow(key, identity: identity, music: music)
    }

    /// How far this person's music is turned down now: the duck and the
    /// fade-in, as the song desk works them out for the Opus track too.
    @MainActor func setDuck(_ identity: String, _ factor: Float) {
        lock.lock()
        ducks[identity] = factor
        lock.unlock()
        follow(Self.key(identity, music: true), identity: identity, music: true)
    }

    /// The level a stream should play at now (lossless-receiver.ts `levelOf`).
    private func level(_ key: String, identity: String, music: Bool) -> Float {
        let volume = levels[key] ?? 1
        return music ? volume * (ducks[identity] ?? 1) : volume
    }

    @MainActor private func follow(_ key: String, identity: String, music: Bool) {
        lock.lock()
        let stream = streams[key]
        let target = level(key, identity: identity, music: music)
        lock.unlock()
        stream?.gain.set(target)
    }

    /// Makes the node for what is waiting under `key`, and hands it all
    /// that has arrived. The waiting blocks are taken and the stream put in
    /// place in one step, so nothing that arrives meanwhile is either lost or
    /// played ahead of what came before it.
    @MainActor private func open(_ key: String) {
        lock.lock()
        let waiting = pending[key]
        lock.unlock()
        guard let waiting else { return }
        removeStream(key)
        guard let audio else {
            lock.lock()
            pending[key] = nil
            lock.unlock()
            return
        }
        try? audio.start()
        let music = key.hasSuffix("/music")
        let identity = String(key.dropLast(music ? "/music".count : "/screen".count))
        lock.lock()
        let stream = Stream(rate: waiting.rate, channels: waiting.channels, level: level(key, identity: identity, music: music))
        let first = pending.removeValue(forKey: key)?.samples ?? []
        stream.ring.write(floats: first)
        streams[key] = stream
        lock.unlock()
        audio.engine.attach(stream.node)
        audio.engine.connect(stream.node, to: audio.engine.mainMixerNode, format: stream.node.outputFormat(forBus: 0))
    }

    @MainActor private func close(_ key: String) {
        lock.lock()
        pending[key] = nil
        lock.unlock()
        removeStream(key)
    }

    @MainActor private func removeStream(_ key: String) {
        lock.lock()
        let stream = streams.removeValue(forKey: key)
        lock.unlock()
        guard let stream, let audio else { return }
        audio.engine.disconnectNodeOutput(stream.node)
        audio.engine.detach(stream.node)
    }

    @MainActor func closeAll() {
        lock.lock()
        let keys = Set(streams.keys).union(pending.keys)
        lock.unlock()
        for key in keys { close(key) }
    }

    /// How loud a stream comes out of its node, after its gain, for the song
    /// desk's debug log: the measurement that says the listener's volume and
    /// the duck reached the sound, not only a number.
    @MainActor func meter(_ identity: String, music: Bool) -> SoundLevel? {
        lock.lock()
        let stream = streams[Self.key(identity, music: music)]
        lock.unlock()
        guard let node = stream?.node else { return nil }
        let level = SoundLevel()
        node.removeTap(onBus: 0)
        node.installTap(onBus: 0, bufferSize: 1024, format: nil) { buffer, _ in
            guard let data = buffer.floatChannelData else { return }
            level.add(data[0], frames: Int(buffer.frameLength), stride: buffer.stride, rate: buffer.format.sampleRate)
        }
        return level
    }

    /// The level this person's sound is meant to play at now, for the log.
    func intended(_ identity: String, music: Bool) -> Float {
        lock.lock()
        defer { lock.unlock() }
        return level(Self.key(identity, music: music), identity: identity, music: music)
    }

    /// Which stream is playing for this person's sound, so a meter can tell
    /// that the one it listens to was replaced.
    func stream(_ identity: String, music: Bool) -> ObjectIdentifier? {
        lock.lock()
        defer { lock.unlock() }
        return streams[Self.key(identity, music: music)].map { ObjectIdentifier($0) }
    }

    /// Whether a stream for this person's sound is playing, for the song
    /// desk's log and the checks that read it.
    func playing(_ identity: String, music: Bool) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return streams[Self.key(identity, music: music)] != nil
    }
}
