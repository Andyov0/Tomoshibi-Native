import AVFoundation
import Foundation
import TomoshibiCore

/// Everything a call hears and says, through one AVAudioEngine.
///
/// One engine for both directions is the point: with voice processing on, the
/// system's echo canceller hears exactly what the engine plays and takes it
/// back out of the microphone, which it cannot do for sound played anywhere
/// else. Every remote sound gets a node of its own, so a person's voice, the
/// sound of their screen and the music are turned up, down or off separately,
/// as the web client's sound panel does (web/src/live/hearing.ts).
@MainActor
public final class CallAudio {
    private let engine = AVAudioEngine()
    private var players: [String: Player] = [:]
    private var microphone: AudioInput?
    private var converter: AVAudioConverter?
    private var started = false

    /// What the core takes from a microphone: 48 kHz, 16-bit, mono.
    private let voiceFormat = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48_000, channels: 1, interleaved: true)!

    /// The web's "original sound": no echo cancellation, noise suppression or
    /// gain control, for an instrument or a room that should be heard as it is.
    public private(set) var originalSound = false

    public init() {}

    private final class Player {
        let node: AVAudioSourceNode
        let ring: PCMRing
        let sink: RingSink
        var volume: Float = 1
        var duck: Float = 1

        init(channels: Int) {
            let ring = PCMRing(channels: channels)
            self.ring = ring
            self.sink = RingSink(ring: ring)
            let format = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: AVAudioChannelCount(channels))!
            self.node = AVAudioSourceNode(format: format) { _, _, frames, list in
                ring.render(into: UnsafeMutableAudioBufferListPointer(list), frames: Int(frames))
                return noErr
            }
        }
    }

    /// Starts the engine. On iOS this also claims the audio session for a call.
    public func start() throws {
        guard !started else { return }
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .videoChat, options: [.allowBluetoothHFP, .allowBluetoothA2DP, .defaultToSpeaker])
        try session.setPreferredSampleRate(48_000)
        try session.setActive(true)
        #endif
        try engine.inputNode.setVoiceProcessingEnabled(!originalSound)
        // TOMOSHIBI_MUTE silences everything this engine plays. The interop
        // scripts set it: a test browser's fake microphone beeps once a
        // second, and a native app in the same room plays that through the
        // machine's real speakers, which once sounded exactly like a fault.
        if ProcessInfo.processInfo.environment["TOMOSHIBI_MUTE"] != nil {
            engine.mainMixerNode.outputVolume = 0
        }
        engine.prepare()
        try engine.start()
        started = true
    }

    public func stop() {
        stopMicrophone()
        for sid in Array(players.keys) { stopPlaying(sid: sid) }
        engine.stop()
        started = false
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        #endif
    }

    // MARK: Microphone

    /// Sends the microphone to `input` until `stopMicrophone`.
    public func startMicrophone(into input: AudioInput) throws {
        try start()
        stopMicrophone()
        let node = engine.inputNode
        let format = node.outputFormat(forBus: 0)
        guard let converter = AVAudioConverter(from: format, to: voiceFormat) else {
            throw CocoaError(.featureUnsupported)
        }
        self.converter = converter
        self.microphone = input
        let target = voiceFormat
        node.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            let ratio = target.sampleRate / format.sampleRate
            let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
            guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
            var fed = false
            var error: NSError?
            converter.convert(to: out, error: &error) { _, status in
                if fed {
                    status.pointee = .noDataNow
                    return nil
                }
                fed = true
                status.pointee = .haveData
                return buffer
            }
            guard error == nil, let samples = out.int16ChannelData, out.frameLength > 0 else { return }
            input.push(samples: Array(UnsafeBufferPointer(start: samples[0], count: Int(out.frameLength))))
        }
    }

    public func stopMicrophone() {
        guard microphone != nil else { return }
        engine.inputNode.removeTap(onBus: 0)
        microphone = nil
        converter = nil
    }

    /// Turns the system's voice processing off or on. The engine has to be
    /// restarted for the change to take, and the microphone with it.
    public func setOriginalSound(_ original: Bool) throws {
        guard original != originalSound else { return }
        originalSound = original
        guard started else { return }
        let input = microphone
        stopMicrophone()
        engine.stop()
        started = false
        try start()
        if let input { try startMicrophone(into: input) }
    }

    // MARK: Remote sound

    /// Plays a remote track of `channels` channels on a node of its own.
    public func play(sid: String, channels: Int, from call: Call) throws {
        try start()
        stopPlaying(sid: sid)
        let player = Player(channels: channels)
        engine.attach(player.node)
        engine.connect(player.node, to: engine.mainMixerNode, format: player.node.outputFormat(forBus: 0))
        players[sid] = player
        call.attachAudio(sid: sid, sink: player.sink)
    }

    public func stopPlaying(sid: String, call: Call? = nil) {
        guard let player = players.removeValue(forKey: sid) else { return }
        call?.detach(sid: sid)
        engine.disconnectNodeOutput(player.node)
        engine.detach(player.node)
        player.ring.clear()
    }

    /// The listener's own setting for this sound, 0 to 1.
    public func setVolume(sid: String, _ volume: Float) {
        guard let player = players[sid] else { return }
        player.volume = max(0, min(1, volume))
        player.node.volume = player.volume * player.duck
    }

    /// The music's dip while somebody talks (web/src/live/duck.ts), applied on
    /// top of the listener's own setting.
    public func setDuck(sid: String, _ factor: Float) {
        guard let player = players[sid] else { return }
        player.duck = max(0, min(1, factor))
        player.node.volume = player.volume * player.duck
    }
}

/// The core's audio sink, writing into the ring the hardware reads.
final class RingSink: AudioSink, @unchecked Sendable {
    let ring: PCMRing

    init(ring: PCMRing) {
        self.ring = ring
    }

    func onAudio(samples: [Int16], sampleRate: UInt32, channels: UInt32) {
        ring.write(samples)
    }
}

// MARK: - What the song desk needs (Music/)

extension CallAudio {
    /// Plays sound this app makes itself on a node of its own, filed under
    /// `key` beside the remote tracks, so that `setVolume`, `setDuck` and
    /// `stopPlaying` work on it as on any other sound. The song desk's holder
    /// hears the music this way (sound.ts `monitor`): through the same engine,
    /// so that the echo canceller takes it back out of the microphone, and
    /// with a volume of its own that nothing sent is touched by.
    func playLocally(key: String, channels: Int) throws -> PCMRing {
        try start()
        stopPlaying(sid: key)
        let player = Player(channels: channels)
        engine.attach(player.node)
        engine.connect(player.node, to: engine.mainMixerNode, format: player.node.outputFormat(forBus: 0))
        players[key] = player
        return player.ring
    }

    /// The listener's own setting for this sound, where it is playing.
    public func volume(sid: String) -> Float? {
        players[sid]?.volume
    }

    /// How loud a remote sound playing here is, read off its node as it
    /// plays, before the listener's volume and the duck.
    ///
    /// Read here rather than taken from the server's speaker events because
    /// the music's duck wants a reading every 40 ms (duck.ts reads an
    /// analyser on every microphone), and the server says who is speaking a
    /// few times a second, smoothed. And read off the node rather than off
    /// the core's samples, because attaching a second sink to a subscribed
    /// track is a call the core cannot take from this thread.
    func meter(sid: String) -> SoundLevel? {
        guard let player = players[sid] else { return nil }
        let level = SoundLevel()
        let node = player.node
        node.removeTap(onBus: 0)
        node.installTap(onBus: 0, bufferSize: 1024, format: nil) { buffer, _ in
            guard let data = buffer.floatChannelData else { return }
            level.add(data[0], frames: Int(buffer.frameLength), stride: buffer.stride, rate: buffer.format.sampleRate)
        }
        return level
    }

    /// How loud this participant's own microphone is, after the system's
    /// voice processing, as duck.ts reads the local track.
    ///
    /// The microphone's tap is the sender's and a bus takes one tap, so this
    /// listens from a sink node on the input instead. Only while the
    /// microphone is on: a node on the input holds it open, and a microphone
    /// held open after it was turned off would be a light that lies.
    func meterMicrophone() -> MicrophoneMeter? {
        guard started else { return nil }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0, format.commonFormat == .pcmFormatFloat32 else { return nil }
        let level = SoundLevel()
        let stride = format.isInterleaved ? Int(format.channelCount) : 1
        let rate = format.sampleRate
        let node = AVAudioSinkNode { _, frames, list in
            let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: list))
            if let first = buffers.first, let data = first.mData?.assumingMemoryBound(to: Float.self) {
                level.add(data, frames: Int(frames), stride: stride, rate: rate)
            }
            return noErr
        }
        engine.attach(node)
        engine.connect(input, to: node, format: format)
        return MicrophoneMeter(level: level, node: node)
    }

    func unmeter(_ meter: MicrophoneMeter) {
        engine.disconnectNodeInput(meter.node)
        engine.detach(meter.node)
    }
}
