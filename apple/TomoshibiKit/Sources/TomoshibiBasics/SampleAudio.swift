import CoreMedia
import Foundation

/// Sound out of a system's sample buffer, whatever its layout.
///
/// ScreenCaptureKit hands a shared screen's sound over as 32-bit float with
/// one buffer per channel; ReplayKit hands an app's sound over as 16-bit
/// integers, interleaved, in either byte order. Both arrive in a
/// `CMSampleBuffer`, and the buffer list that describes them is as long as
/// there are buffers.
///
/// The screen share read it into an `AudioBufferList` on the stack, which
/// holds one buffer, while telling CoreMedia there was room for two. Stereo
/// sound is two buffers, so the second was written past the end of the
/// variable and the process aborted with a stack buffer overflow the moment a
/// screen was shared with its sound. The list is now asked how big it needs to
/// be and given exactly that, on the heap. The broadcast extension's reader
/// said honestly that it had room for one, and so read nothing at all from
/// sound that came in two.
public enum SampleAudio {
    /// The sound interleaved as 32-bit float, with its channel count and rate,
    /// or nothing for a buffer that is not linear PCM in a format read here.
    public static func interleaved(_ sample: CMSampleBuffer) -> (samples: [Float], channels: Int, rate: Double)? {
        guard let description = CMSampleBufferGetFormatDescription(sample),
              let format = CMAudioFormatDescriptionGetStreamBasicDescription(description)?.pointee,
              format.mFormatID == kAudioFormatLinearPCM else { return nil }
        let frames = CMSampleBufferGetNumSamples(sample)
        let channels = Int(format.mChannelsPerFrame)
        guard frames > 0, channels > 0 else { return nil }

        var needed = 0
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sample, bufferListSizeNeededOut: &needed, bufferListOut: nil, bufferListSize: 0,
            blockBufferAllocator: nil, blockBufferMemoryAllocator: nil, flags: 0, blockBufferOut: nil) == noErr,
            needed > 0 else { return nil }
        let storage = UnsafeMutableRawPointer.allocate(byteCount: needed, alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { storage.deallocate() }
        let list = storage.bindMemory(to: AudioBufferList.self, capacity: 1)
        // Held until the samples are copied out: the buffers point into it.
        var block: CMBlockBuffer?
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sample, bufferListSizeNeededOut: nil, bufferListOut: list, bufferListSize: needed,
            blockBufferAllocator: nil, blockBufferMemoryAllocator: nil,
            flags: kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, blockBufferOut: &block) == noErr
        else { return nil }
        let buffers = UnsafeMutableAudioBufferListPointer(list)

        let flags = format.mFormatFlags
        let float = flags & kAudioFormatFlagIsFloat != 0
        let bigEndian = flags & kAudioFormatFlagIsBigEndian != 0
        let planar = flags & kAudioFormatFlagIsNonInterleaved != 0
        let bits = Int(format.mBitsPerChannel)
        guard (float && bits == 32) || (!float && (bits == 16 || bits == 32)) else { return nil }

        func value(_ data: UnsafeMutableRawPointer, _ index: Int) -> Float {
            if float {
                let raw = data.load(fromByteOffset: index * 4, as: UInt32.self)
                return Float(bitPattern: bigEndian ? UInt32(bigEndian: raw) : UInt32(littleEndian: raw))
            }
            if bits == 16 {
                let raw = data.load(fromByteOffset: index * 2, as: Int16.self)
                return Float(bigEndian ? Int16(bigEndian: raw) : Int16(littleEndian: raw)) / 32768
            }
            let raw = data.load(fromByteOffset: index * 4, as: Int32.self)
            return Float(bigEndian ? Int32(bigEndian: raw) : Int32(littleEndian: raw)) / 2_147_483_648
        }

        var out = [Float](repeating: 0, count: frames * channels)
        if planar {
            // One buffer per channel; a buffer missing or short is silence
            // rather than a read past its end.
            for channel in 0..<channels {
                guard channel < buffers.count, let data = buffers[channel].mData else { continue }
                let available = Int(buffers[channel].mDataByteSize) / (bits / 8)
                for frame in 0..<min(frames, available) {
                    out[frame * channels + channel] = value(data, frame)
                }
            }
        } else {
            guard let data = buffers.first?.mData else { return nil }
            let available = Int(buffers[0].mDataByteSize) / (bits / 8)
            for index in 0..<min(out.count, available) {
                out[index] = value(data, index)
            }
        }
        return (out, channels, format.mSampleRate)
    }

    /// Interleaved float as 16-bit, clipped rather than wrapped.
    public static func int16(_ samples: [Float]) -> [Int16] {
        samples.map { Int16(max(-1, min(1, $0)) * 32767) }
    }

    /// Any channel count as stereo: mono is copied to both sides, and beyond
    /// two only the first two are kept.
    public static func stereo(_ samples: [Float], channels: Int) -> [Float] {
        if channels == 2 { return samples }
        let frames = samples.count / max(channels, 1)
        var out = [Float](repeating: 0, count: frames * 2)
        for frame in 0..<frames {
            let left = samples[frame * channels]
            out[frame * 2] = left
            out[frame * 2 + 1] = channels > 1 ? samples[frame * channels + 1] : left
        }
        return out
    }
}
