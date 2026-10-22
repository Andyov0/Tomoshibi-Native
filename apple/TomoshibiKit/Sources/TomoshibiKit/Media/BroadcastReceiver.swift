#if os(iOS)
import AVFoundation
import CoreImage
import Foundation
import Network
import TomoshibiCore

/// The app's end of an iPhone screen share: listens on the app group's socket
/// while a call is on, and publishes what the broadcast extension sends as the
/// share and its sound (see `BroadcastWire`).
///
/// The picture is published on its first frame, at that frame's size, so the
/// share's ladder is the one the web would choose for it; the sound as the
/// share's own track, resampled to the call's 48 kHz stereo. The extension
/// has already held the frames to the deployment's ceiling (ShareCeiling,
/// SampleHandler); the rate published at is held to it here, so that what
/// the encoder is told to expect is what arrives.
@MainActor
final class BroadcastReceiver {
    private let call: Call
    private var listener: NWListener?
    private var video: VideoInput?
    private var sound: AudioInput?
    private var publishing = false
    private let decode = Decoder()
    var onChange: ((Bool) -> Void)?

    init(call: Call) {
        self.call = call
    }

    func start() {
        guard listener == nil, let url = BroadcastWire.socketURL else { return }
        try? FileManager.default.removeItem(at: url)
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .unix(path: url.path)
        guard let listener = try? NWListener(using: parameters) else { return }
        listener.newConnectionHandler = { [weak self] connection in
            Task { @MainActor in self?.accept(connection) }
        }
        listener.start(queue: decode.queue)
        self.listener = listener
    }

    func stop() {
        listener?.cancel()
        listener = nil
        Task { await end() }
    }

    private func accept(_ connection: NWConnection) {
        var buffer = Data()
        func read() {
            connection.receive(minimumIncompleteLength: 1, maximumLength: 1 << 20) { [weak self] data, _, done, error in
                if let data { buffer.append(data) }
                let messages = BroadcastWire.take(from: &buffer)
                if !messages.isEmpty {
                    Task { @MainActor in
                        for (header, payload) in messages { await self?.handle(header, payload) }
                    }
                }
                if done || error != nil {
                    Task { @MainActor in await self?.end() }
                    connection.cancel()
                } else {
                    read()
                }
            }
        }
        connection.start(queue: decode.queue)
        read()
    }

    private func handle(_ header: BroadcastWire.Header, _ payload: Data) async {
        switch header.kind {
        case .video:
            if video == nil && !publishing {
                publishing = true
                let turned = header.rotation == 90 || header.rotation == 270
                let (width, height) = turned ? (header.height, header.width) : (header.width, header.height)
                let ceiling = ShareCeiling(kept: UserDefaults(suiteName: BroadcastWire.group))
                let fps = min(30, ceiling.fps ?? 30)
                video = try? await call.publishVideo(kind: .screen(fps: UInt32(fps), quality: .auto), width: UInt32(width), height: UInt32(height))
                sound = try? await call.publishAudio(kind: .screenSound)
                publishing = false
                onChange?(true)
            }
            guard let video else { return }
            decode.picture(payload, rotation: header.rotation, timestamp: header.timestampMicros, into: video)
        case .audio:
            guard let sound else { return }
            decode.sound(payload, channels: header.channels, rate: header.rate, into: sound)
        case .ended:
            await end()
        }
    }

    private func end() async {
        if let video { try? await video.unpublish() }
        if let sound { try? await sound.unpublish() }
        let was = video != nil
        video = nil
        sound = nil
        if was { onChange?(false) }
    }

    /// JPEG to a CVPixelBuffer the encoder takes, and sound to 48 kHz stereo,
    /// off the main thread.
    final class Decoder: @unchecked Sendable {
        let queue = DispatchQueue(label: "tomoshibi.broadcast.receive")
        private let context = CIContext()
        private var pool: CVPixelBufferPool?
        private var poolSize = (0, 0)
        private var converter: AVAudioConverter?
        private var converterFrom: (Int, Int) = (0, 0)
        private let target = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48_000, channels: 2, interleaved: true)!

        func picture(_ jpeg: Data, rotation: Int, timestamp: Int64, into video: VideoInput) {
            queue.async { [self] in
                guard let image = CIImage(data: jpeg) else { return }
                let size = (Int(image.extent.width), Int(image.extent.height))
                if pool == nil || poolSize != size {
                    let attributes: [String: Any] = [
                        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                        kCVPixelBufferWidthKey as String: size.0,
                        kCVPixelBufferHeightKey as String: size.1,
                        kCVPixelBufferIOSurfacePropertiesKey as String: [:],
                    ]
                    pool = nil
                    CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool)
                    poolSize = size
                }
                guard let pool else { return }
                var out: CVPixelBuffer?
                CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out)
                guard let out else { return }
                context.render(image, to: out)
                video.push(pixelBuffer: UInt64(UInt(bitPattern: Unmanaged.passRetained(out).toOpaque())), timestampUs: timestamp, rotation: UInt32(rotation))
            }
        }

        func sound(_ samples: Data, channels: Int, rate: Int, into input: AudioInput) {
            queue.async { [self] in
                guard channels > 0, rate > 0,
                      let source = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: Double(rate), channels: AVAudioChannelCount(channels), interleaved: true) else { return }
                if converter == nil || converterFrom != (channels, rate) {
                    converter = AVAudioConverter(from: source, to: target)
                    converterFrom = (channels, rate)
                }
                let frames = samples.count / (2 * channels)
                guard let converter, frames > 0, let incoming = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: AVAudioFrameCount(frames)) else { return }
                incoming.frameLength = AVAudioFrameCount(frames)
                samples.withUnsafeBytes { raw in
                    memcpy(incoming.int16ChannelData![0], raw.baseAddress!, samples.count)
                }
                let capacity = AVAudioFrameCount(Double(frames) * 48_000 / Double(rate)) + 64
                guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
                var fed = false
                converter.convert(to: out, error: nil) { _, status in
                    if fed { status.pointee = .noDataNow; return nil }
                    fed = true
                    status.pointee = .haveData
                    return incoming
                }
                guard out.frameLength > 0, let data = out.int16ChannelData else { return }
                input.push(samples: Array(UnsafeBufferPointer(start: data[0], count: Int(out.frameLength) * 2)))
            }
        }
    }
}
#endif
