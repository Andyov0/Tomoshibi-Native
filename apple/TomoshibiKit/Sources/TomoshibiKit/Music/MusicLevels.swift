import AVFoundation
import Foundation
import os

/// How loud a microphone is: the RMS of 1024 samples, which is the frame
/// duck.ts reads off its analyser (fftSize 1024) every 40 ms.
///
/// A tap on a node hands its sound over a tenth of a second at a time (4410
/// frames, measured), not as it plays. Read as "the latest 1024 samples", a
/// tap's reading changed only every 92 ms, so two readings in a row, which is
/// what tells a voice from a click, were the same 23 ms read twice: every
/// click that landed at the end of a delivery counted as speech. So the
/// delivery is played out against the clock instead: a reading is of the
/// window that was sounding as long after the delivery's start as it is now
/// after its arrival. That is the analyser's reading, one delivery late, and
/// a click is read once. The microphone's own node delivers 10 ms at a time,
/// where the two readings are the same.
final class SoundLevel: @unchecked Sendable {
    private static let window = 1024
    private var ring = [Float](repeating: 0, count: 16_384)
    private var written = 0
    private var chunk = 0
    private var arrived: UInt64 = 0
    private var rate = 48_000.0
    private let lock = OSAllocatedUnfairLock()

    /// Called on the audio hardware's thread: no allocation, one short lock.
    func add(_ samples: UnsafePointer<Float>, frames: Int, stride: Int, rate: Double) {
        let now = DispatchTime.now().uptimeNanoseconds
        lock.withLockUnchecked {
            let frames = min(frames, ring.count)
            for frame in 0..<frames {
                ring[(written + frame) % ring.count] = samples[frame * stride]
            }
            written += frames
            chunk = frames
            arrived = now
            if rate > 0 { self.rate = rate }
        }
    }

    var rms: Double {
        let now = DispatchTime.now().uptimeNanoseconds
        return lock.withLockUnchecked {
            guard written > 0 else { return 0 }
            let since = Double(now &- arrived) / 1e9
            let into = min(chunk, Int(since * rate))
            let end = written - chunk + into
            let start = max(0, end - Self.window, written - ring.count)
            var sum: Float = 0
            for index in start..<max(start, end) {
                let sample = ring[index % ring.count]
                sum += sample * sample
            }
            return Double((sum / Float(Self.window)).squareRoot())
        }
    }

    /// How many frames the latest delivery held, for the debug log.
    var lastChunk: Int {
        lock.withLockUnchecked { chunk }
    }
}

/// The node listening to this participant's microphone, and what it heard.
final class MicrophoneMeter {
    let level: SoundLevel
    let node: AVAudioSinkNode

    init(level: SoundLevel, node: AVAudioSinkNode) {
        self.level = level
        self.node = node
    }
}
