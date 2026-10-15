import AVFAudio
import CoreMedia
import XCTest
@testable import TomoshibiBasics

/// Reading sound out of a system sample buffer in each layout the apps are
/// given it. The first case is the one that crashed: ScreenCaptureKit's
/// stereo, one buffer per channel, which overflowed a buffer list sized for
/// one and aborted the Mac app whenever a screen was shared with its sound.
final class SampleAudioTests: XCTestCase {
    /// A sample buffer carrying `buffer`'s sound, as a capture hands one over.
    private func sample(_ buffer: AVAudioPCMBuffer) throws -> CMSampleBuffer {
        var description: CMAudioFormatDescription?
        XCTAssertEqual(CMAudioFormatDescriptionCreate(
            allocator: nil, asbd: buffer.format.streamDescription, layoutSize: 0, layout: nil,
            magicCookieSize: 0, magicCookie: nil, extensions: nil, formatDescriptionOut: &description), noErr)
        var timing = CMSampleTimingInfo(
            duration: CMTime(value: 1, timescale: CMTimeScale(buffer.format.sampleRate)),
            presentationTimeStamp: .zero, decodeTimeStamp: .invalid)
        var made: CMSampleBuffer?
        XCTAssertEqual(CMSampleBufferCreate(
            allocator: nil, dataBuffer: nil, dataReady: false, makeDataReadyCallback: nil, refcon: nil,
            formatDescription: description, sampleCount: CMItemCount(buffer.frameLength),
            sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 0,
            sampleSizeArray: nil, sampleBufferOut: &made), noErr)
        let sample = try XCTUnwrap(made)
        XCTAssertEqual(CMSampleBufferSetDataBufferFromAudioBufferList(
            sample, blockBufferAllocator: nil, blockBufferMemoryAllocator: nil, flags: 0,
            bufferList: buffer.audioBufferList), noErr)
        return sample
    }

    func testStereoInOneBufferPerChannelIsReadWithoutOverrunningTheList() throws {
        let format = try XCTUnwrap(AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48_000, channels: 2, interleaved: false))
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 480))
        buffer.frameLength = 480
        let planes = try XCTUnwrap(buffer.floatChannelData)
        for frame in 0..<480 {
            planes[0][frame] = Float(frame) / 1000
            planes[1][frame] = -Float(frame) / 1000
        }
        let read = try XCTUnwrap(SampleAudio.interleaved(try sample(buffer)))
        XCTAssertEqual(read.channels, 2)
        XCTAssertEqual(read.rate, 48_000)
        XCTAssertEqual(read.samples.count, 960)
        XCTAssertEqual(read.samples[0], 0)
        XCTAssertEqual(read.samples[2 * 300], 0.3, accuracy: 1e-6)
        XCTAssertEqual(read.samples[2 * 300 + 1], -0.3, accuracy: 1e-6)
        XCTAssertEqual(read.samples[2 * 479 + 1], -0.479, accuracy: 1e-6)
    }

    func testInterleavedSixteenBitIsReadAsItIs() throws {
        let format = try XCTUnwrap(AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 44_100, channels: 2, interleaved: true))
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4))
        buffer.frameLength = 4
        let data = try XCTUnwrap(buffer.int16ChannelData)[0]
        for (index, value) in [16384, -16384, 8192, -8192, 0, 32767, -32768, 1].enumerated() {
            data[index] = Int16(value)
        }
        let read = try XCTUnwrap(SampleAudio.interleaved(try sample(buffer)))
        XCTAssertEqual(read.channels, 2)
        XCTAssertEqual(read.rate, 44_100)
        XCTAssertEqual(read.samples, [0.5, -0.5, 0.25, -0.25, 0, Float(32767) / 32768, -1, Float(1) / 32768])
    }

    /// ReplayKit has handed an app's sound over in network byte order.
    func testBigEndianSixteenBitIsTurnedRoundBeforeItIsRead() throws {
        var format = AudioStreamBasicDescription(
            mSampleRate: 44_100, mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsBigEndian | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4, mFramesPerPacket: 1, mBytesPerFrame: 4, mChannelsPerFrame: 2,
            mBitsPerChannel: 16, mReserved: 0)
        var description: CMAudioFormatDescription?
        XCTAssertEqual(CMAudioFormatDescriptionCreate(
            allocator: nil, asbd: &format, layoutSize: 0, layout: nil, magicCookieSize: 0,
            magicCookie: nil, extensions: nil, formatDescriptionOut: &description), noErr)
        let values: [Int16] = [16384, -16384, 8192, -8192]
        let bytes = values.flatMap { value -> [UInt8] in
            let big = UInt16(bitPattern: value).bigEndian
            return [UInt8(big & 0xff), UInt8(big >> 8)]
        }
        var block: CMBlockBuffer?
        XCTAssertEqual(CMBlockBufferCreateWithMemoryBlock(
            allocator: nil, memoryBlock: nil, blockLength: bytes.count, blockAllocator: nil,
            customBlockSource: nil, offsetToData: 0, dataLength: bytes.count,
            flags: kCMBlockBufferAssureMemoryNowFlag, blockBufferOut: &block), noErr)
        let filled = try XCTUnwrap(block)
        XCTAssertEqual(bytes.withUnsafeBytes {
            CMBlockBufferReplaceDataBytes(with: $0.baseAddress!, blockBuffer: filled, offsetIntoDestination: 0, dataLength: bytes.count)
        }, noErr)
        var made: CMSampleBuffer?
        XCTAssertEqual(CMAudioSampleBufferCreateReadyWithPacketDescriptions(
            allocator: nil, dataBuffer: filled, formatDescription: try XCTUnwrap(description),
            sampleCount: 2, presentationTimeStamp: .zero, packetDescriptions: nil, sampleBufferOut: &made), noErr)
        let read = try XCTUnwrap(SampleAudio.interleaved(try XCTUnwrap(made)))
        XCTAssertEqual(read.samples, [0.5, -0.5, 0.25, -0.25])
    }

    func testMonoBecomesTheSameOnBothSides() {
        XCTAssertEqual(SampleAudio.stereo([0.1, 0.2], channels: 1), [0.1, 0.1, 0.2, 0.2])
        XCTAssertEqual(SampleAudio.stereo([0.1, 0.2, 0.3, 0.4], channels: 2), [0.1, 0.2, 0.3, 0.4])
    }

    func testSixteenBitIsClippedRatherThanWrapped() {
        XCTAssertEqual(SampleAudio.int16([1.5, -1.5, 0.5]), [32767, -32767, 16383])
    }
}
