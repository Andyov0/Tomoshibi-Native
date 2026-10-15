import AVFoundation
import Foundation
import os

/// Sound handed over from the core's threads to the audio hardware's.
///
/// The core delivers each remote track in 10 ms pieces on a clock of its own;
/// the hardware asks for its buffer on another. The two drift, so the ring
/// keeps a little in hand, starts playing only once it has some, plays
/// silence rather than stale sound when it runs dry, and throws away the
/// oldest sound when it has gathered more than it should hold — a voice that
/// falls further and further behind is worse than one that skips a few
/// milliseconds once. WebRTC has already absorbed the network's jitter before
/// any of this sees the sound, so the cushion here is only for the two clocks.
final class PCMRing: @unchecked Sendable {
    let channels: Int
    private let capacity: Int
    private let start: Int
    private let ceiling: Int
    private let settle: Int
    private var samples: [Float]
    private var read = 0
    private var count = 0
    private var playing = false
    private let lock = OSAllocatedUnfairLock()

    /// Frames are per channel; at 48 kHz, 480 frames are 10 ms.
    init(channels: Int, startFrames: Int = 1440, ceilingFrames: Int = 9600, settleFrames: Int = 2880) {
        self.channels = channels
        self.start = startFrames
        self.ceiling = ceilingFrames
        self.settle = settleFrames
        self.capacity = ceilingFrames * 2
        self.samples = [Float](repeating: 0, count: ceilingFrames * 2 * channels)
    }

    /// Interleaved 16-bit samples from the core.
    func write(_ input: [Int16]) {
        let frames = input.count / channels
        lock.withLockUnchecked {
            for frame in 0..<frames {
                if count == capacity {
                    read = (read + 1) % capacity
                    count -= 1
                }
                let at = ((read + count) % capacity) * channels
                for c in 0..<channels {
                    samples[at + c] = Float(input[frame * channels + c]) / 32768
                }
                count += 1
            }
            if count > ceiling {
                let drop = count - settle
                read = (read + drop) % capacity
                count -= drop
            }
        }
    }

    /// Interleaved 32-bit float samples, as a lossless stream arrives.
    func write(floats input: [Float]) {
        let frames = input.count / channels
        lock.withLockUnchecked {
            for frame in 0..<frames {
                if count == capacity {
                    read = (read + 1) % capacity
                    count -= 1
                }
                let at = ((read + count) % capacity) * channels
                for c in 0..<channels {
                    samples[at + c] = input[frame * channels + c]
                }
                count += 1
            }
            if count > ceiling {
                let drop = count - settle
                read = (read + drop) % capacity
                count -= drop
            }
        }
    }

    /// Fills `frames` frames of each non-interleaved channel buffer the
    /// hardware gave, from the ring or with silence. Called on the render
    /// thread: no allocation, one short lock.
    func render(into buffers: UnsafeMutableAudioBufferListPointer, frames: Int) {
        lock.withLockUnchecked {
            if !playing && count >= start { playing = true }
            if playing && count == 0 { playing = false }
            let available = playing ? min(frames, count) : 0
            for (index, buffer) in buffers.enumerated() {
                guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
                let channel = min(index, channels - 1)
                for frame in 0..<frames {
                    if frame < available {
                        data[frame] = samples[((read + frame) % capacity) * channels + channel]
                    } else {
                        data[frame] = 0
                    }
                }
            }
            read = (read + available) % capacity
            count -= available
        }
    }

    func clear() {
        lock.withLockUnchecked {
            read = 0
            count = 0
            playing = false
        }
    }
}
