import AVFoundation
import os
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
    let engine = AVAudioEngine()
    private var players: [String: Player] = [:]
    private var microphone: AudioInput?
    private var converter: AVAudioConverter?
    private(set) var started = false

    /// What the core takes from a microphone: 48 kHz, 16-bit, mono.
    private let voiceFormat = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48_000, channels: 1, interleaved: true)!

    /// The web's "original sound": no echo cancellation, noise suppression or
    /// gain control, for an instrument or a room that should be heard as it is.
    public private(set) var originalSound = false

    /// The microphone and speakers picked in the devices panel, or restored
    /// from the last call (`meet-live.device-choice`), by CoreAudio UID on a
    /// Mac and port UID on a phone; nil is the system's own choice. Kept here
    /// as well as on the unit because a unit built again (voice processing
    /// turned on or off, the hardware changing under the engine) starts on
    /// the system's defaults.
    var chosenInput: String?
    var chosenOutput: String?

    /// How loud the microphone is as it is sent, while it is on: the devices
    /// panel's meter. Written by the microphone's own tap, from the samples it
    /// is about to send, so the meter shows what everybody else hears and
    /// holds nothing open that the call does not already hold.
    private(set) var microphoneLevel: SoundLevel?

    nonisolated(unsafe) private var configurationWatch: NSObjectProtocol?

    public init() {
        // The engine stops itself when the hardware under it changes: a
        // headset plugged in or taken out, a default device that changes its
        // rate. Nothing started it again, so a call went silent both ways the
        // moment somebody put their headphones on.
        configurationWatch = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.hardwareChanged() }
        }
    }

    deinit {
        if let configurationWatch { NotificationCenter.default.removeObserver(configurationWatch) }
    }

    /// When the engine was last started again for a change of hardware.
    private var restarts: [Date] = []

    private func hardwareChanged() {
        guard started, !engine.isRunning else { return }
        // A start that itself changes the hardware (voice processing builds
        // its own aggregate device) must not become a loop: three in ten
        // seconds is a machine that will not settle, and the engine is left
        // as the system left it.
        restarts = restarts.filter { Date().timeIntervalSince($0) < 10 } + [Date()]
        guard restarts.count <= 3 else {
            Self.log.error("the audio hardware keeps changing; not starting the engine again")
            return
        }
        Self.log.notice("the audio hardware changed under the engine; starting it again")
        do {
            try restart()
        } catch {
            Self.log.error("the engine would not start again: \(String(describing: error), privacy: .public)")
        }
    }

    /// The devices and processing a call starts with. The engine may already
    /// be running by then, for the sound of whoever was in the room first, so
    /// it is started again where anything changed.
    func prefer(input: String?, output: String?, original: Bool) {
        let changed = input != chosenInput || output != chosenOutput || original != originalSound
        chosenInput = input
        chosenOutput = output
        originalSound = original
        guard changed, started else { return }
        do {
            try restart()
        } catch {
            Self.log.error("the engine would not start on the remembered devices: \(String(describing: error), privacy: .public)")
        }
    }

    /// Stops the engine and starts it again, with the microphone if it was
    /// on: for a change of device or of processing, which only takes on a
    /// unit that is not running.
    func restart() throws {
        let input = microphone
        stopMicrophone()
        engine.stop()
        started = false
        try start()
        if let input { try startMicrophone(into: input) }
    }

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
        do {
            // Voice processing is chosen before anything touches the output:
            // asking for the mixer first builds and initialises a plain I/O
            // unit that turning voice processing on then has to tear down,
            // and that first initialisation is where the simulator's audio
            // server once timed out and aborted the app.
            try engine.inputNode.setVoiceProcessingEnabled(!originalSound)
            pointAtChosenDevices()
            engine.prepare()
            try engine.start()
            muteForTests()
            voiceProcessing = !originalSound
            Self.log.notice("engine started, voice processing \(self.voiceProcessing ? "on" : "off", privacy: .public)")
        } catch where !originalSound {
            Self.log.error("voice processing would not start (\(String(describing: error), privacy: .public)); starting without it")
            // The system's voice processing would not start: on the macOS 27
            // Mac this was written on, the output unit failed to initialise
            // with it on (-10875) and the call had no sound at all, either
            // way. A call without echo cancellation is still a call; one
            // without sound is not. So the engine starts again without it.
            engine.stop()
            try engine.inputNode.setVoiceProcessingEnabled(false)
            pointAtChosenDevices()
            engine.prepare()
            try engine.start()
            muteForTests()
            voiceProcessing = false
            Self.log.notice("engine started without voice processing")
        }
        started = true
    }

    /// Whether the system's voice processing (echo cancellation, noise
    /// suppression) is really on, which is not the same as asked for: it is
    /// left off when it would not start.
    public private(set) var voiceProcessing = false

    /// What happened to the engine, in the system log under the app's own
    /// subsystem: a call without sound otherwise left nothing to go on.
    static let log = Logger(subsystem: "app.tomoshibi", category: "audio")

    /// TOMOSHIBI_MUTE silences everything this engine plays. The interop
    /// scripts set it: a test browser's fake microphone beeps once a second,
    /// and a native app in the same room plays that through the machine's
    /// real speakers, which once sounded exactly like a fault.
    ///
    /// Only once the engine runs: asking for the mixer before the start
    /// builds the output side early, and on this Mac voice processing then
    /// failed to initialise (-10875) on every muted run and none of the
    /// others, so every test saw a call without echo cancellation that no
    /// real call ever had (2026-10-17: three muted runs in three failed
    /// before the move, three in three started with it after).
    private func muteForTests() {
        if ProcessInfo.processInfo.environment["TOMOSHIBI_MUTE"] != nil {
            engine.mainMixerNode.outputVolume = 0
        }
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
    ///
    /// One channel is taken out of the input before anything is converted.
    /// The input is never mono on a Mac: with voice processing on, macOS 27
    /// hands over nine channels that all carry the same processed voice, and
    /// with it off, one channel for each of the three built-in microphones.
    /// Given more channels than it was asked for and no channel map,
    /// AVAudioConverter wrote zeros, so every call from a Mac sent digital
    /// silence while the microphone's own light was on (2026-10-16). The first
    /// channel is the processed voice in the one case and the microphone the
    /// system records mono from in the other.
    public func startMicrophone(into input: AudioInput) throws {
        try start()
        stopMicrophone()
        let node = engine.inputNode
        let format = node.outputFormat(forBus: 0)
        guard format.sampleRate > 0,
              let mono = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: format.sampleRate, channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: mono, to: voiceFormat) else {
            throw CocoaError(.featureUnsupported)
        }
        self.converter = converter
        self.microphone = input
        let target = voiceFormat
        let permission = AVCaptureDevice.authorizationStatus(for: .audio)
        Self.log.notice("microphone on: \(format.sampleRate, privacy: .public) Hz \(format.channelCount, privacy: .public) ch, permission \(permission.rawValue, privacy: .public), voice processing \(self.voiceProcessing ? "on" : "off", privacy: .public)")
        // How loud the microphone was in its first two seconds, as it came and
        // as it was sent: a microphone the system refuses gives silence rather
        // than an error, and so did the conversion above, with nothing to say
        // which.
        let first = FirstReading()
        let level = SoundLevel()
        microphoneLevel = level
        node.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            guard let data = buffer.floatChannelData, buffer.frameLength > 0,
                  let one = AVAudioPCMBuffer(pcmFormat: mono, frameCapacity: buffer.frameLength),
                  let into = one.floatChannelData?[0] else { return }
            let frames = Int(buffer.frameLength)
            let stride = buffer.stride
            one.frameLength = buffer.frameLength
            for i in 0..<frames { into[i] = data[0][i * stride] }
            level.add(into, frames: frames, stride: 1, rate: mono.sampleRate)
            if !first.done {
                for i in 0..<frames { first.peak = max(first.peak, abs(into[i])) }
            }
            let ratio = target.sampleRate / mono.sampleRate
            let capacity = AVAudioFrameCount(Double(frames) * ratio) + 32
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
                return one
            }
            guard error == nil, let samples = out.int16ChannelData, out.frameLength > 0 else { return }
            if !first.done {
                for i in 0..<Int(out.frameLength) { first.sent = max(first.sent, Int(abs(Int32(samples[0][i])))) }
                if Date().timeIntervalSince(first.started) > 2 {
                    first.done = true
                    Self.log.notice("microphone peak in its first 2 s: \(first.peak, privacy: .public), as sent \(first.sent, privacy: .public) of 32767")
                }
            }
            input.push(samples: Array(UnsafeBufferPointer(start: samples[0], count: Int(out.frameLength))))
        }
    }

    public func stopMicrophone() {
        guard microphone != nil else { return }
        engine.inputNode.removeTap(onBus: 0)
        microphone = nil
        converter = nil
        microphoneLevel = nil
    }

    /// Turns the system's voice processing off or on. The engine has to be
    /// restarted for the change to take, and the microphone with it.
    public func setOriginalSound(_ original: Bool) throws {
        guard original != originalSound else { return }
        originalSound = original
        guard started else { return }
        try restart()
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
        Self.log.notice("playing \(sid, privacy: .public) in \(channels) channels")
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

/// The microphone's loudest sample in its first seconds, written only from
/// the tap's own thread.
private final class FirstReading: @unchecked Sendable {
    var peak: Float = 0
    var sent = 0
    var done = false
    let started = Date()
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
