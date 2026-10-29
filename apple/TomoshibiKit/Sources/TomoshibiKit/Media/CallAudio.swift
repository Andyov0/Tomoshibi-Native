import AVFoundation
import os
import Foundation
import TomoshibiBasics
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
    /// Replaced on a Mac when the microphone goes off (`freshEngine`), which
    /// is why nothing outside holds on to it: a node played into the call
    /// goes through `attachPlayback`.
    private(set) var engine = AVAudioEngine()
    private var players: [String: Player] = [:]
    /// Nodes others play into the call's mixer (the lossless streams), with
    /// the format each was connected in, so that a new engine takes them over.
    private var others: [ObjectIdentifier: (node: AVAudioNode, format: AVAudioFormat)] = [:]
    #if os(macOS)
    /// Whether this engine has recorded. Once it has, it records for good:
    /// see `freshEngine`.
    private var engineRecorded = false
    #endif
    /// The track the microphone feeds while it is on, kept apart from whether
    /// the tap that feeds it is on the engine right now (`tapped`). A restart
    /// takes the tap off, and it used to forget the track with it: a restart
    /// that failed, or the undo after it, came back with no microphone, and
    /// the call went on sending nothing with the microphone's button on. Now
    /// whatever starts the engine puts the tap back for this track.
    private var microphone: AudioInput?
    private var tapped = false
    /// Whether the engine is set up to record, from a press of the
    /// microphone until it is pressed off. While it is not, the engine plays
    /// and nothing else: no voice processing, and on an iPhone a session for
    /// playback (`start`).
    ///
    /// On an iPhone the call's session recorded from the first sound to the
    /// last, microphone on or off: voice processing took the whole of the
    /// output, so music in a call sounded like a telephone call however
    /// still the microphone was, the orange light stayed lit, and Bluetooth
    /// headphones stayed on the narrow headset profile. The owner chose a
    /// short gap at each press of the microphone over that (2026-10-17).
    ///
    /// A Mac did the same, and there it cost every other app its sound: the
    /// system's voice processing ducks whatever else is playing while it
    /// runs, and it ran from the moment somebody walked into a room, so Apple
    /// Music fell silent on entering one with the microphone off, and a
    /// sound shared from it was shared from an app the sharer could no
    /// longer hear (2026-10-17).
    private var recording = false
    /// How the engine was last started: for playback only, or to record.
    private(set) var listeningOnly = false
    private var converter: AVAudioConverter?
    private(set) var started = false

    /// What the call is told about the engine that it did not ask for.
    enum News {
        /// It stopped and could not be brought back (EngineRecovery): nothing
        /// is heard and nothing is sent until something starts it again.
        case gaveUp(Error)
        /// It runs again after having been given up.
        case back
        /// It runs again, but the microphone would not go back on it.
        case microphoneLost(Error)
    }

    /// Told on the main actor; the call says these to the person.
    var onNews: ((News) -> Void)?

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
        watchConfiguration()
    }

    private func watchConfiguration() {
        if let configurationWatch { NotificationCenter.default.removeObserver(configurationWatch) }
        configurationWatch = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.hardwareChanged() }
        }
    }

    /// A node somebody else plays into the call, on the call's mixer.
    func attachPlayback(_ node: AVAudioNode, format: AVAudioFormat) {
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        others[ObjectIdentifier(node)] = (node, format)
    }

    func detachPlayback(_ node: AVAudioNode) {
        guard others.removeValue(forKey: ObjectIdentifier(node)) != nil else { return }
        engine.disconnectNodeOutput(node)
        engine.detach(node)
    }

    #if os(macOS)
    /// A new engine for playing only, with every sound the old one played.
    ///
    /// An engine whose microphone has been on keeps recording on a Mac. With
    /// voice processing turned off again it ran on the system's default
    /// aggregate of microphone and speakers, the microphone stayed in use
    /// with nothing taking from it (DeviceIsRunningSomewhere 1, the orange
    /// light lit), and the speakers somebody picked could not be pointed at
    /// alone (the aggregate refuses a device that only plays, -10851).
    /// Turning the unit's input off instead left an engine that would not
    /// start at all, since it still had an input node. AVAudioEngine has no
    /// way to give an input node back, so the engine is replaced: the nodes
    /// are moved across in the formats they played in, and their volumes go
    /// with them, being the nodes' own.
    private func freshEngine() {
        let old = engine
        let moving = players.values.map { ($0.node as AVAudioNode, $0.format) } + others.values.map { ($0.node, $0.format) }
        for (node, _) in moving {
            old.disconnectNodeOutput(node)
            old.detach(node)
        }
        engine = AVAudioEngine()
        watchConfiguration()
        for (node, format) in moving {
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
        }
        engineRecorded = false
        voiceProcessing = false
        Self.log.notice("a new engine for playback only, with \(moving.count, privacy: .public) sounds moved across")
    }
    #endif

    deinit {
        if let configurationWatch { NotificationCenter.default.removeObserver(configurationWatch) }
    }

    /// When the engine is started again after stopping under the call, and
    /// when that is given up (EngineRecovery).
    private var recovery = EngineRecovery()
    /// The one more try once the hardware has been quiet.
    private var quietTry: Task<Void, Never>?
    private static func clock() -> Double { ProcessInfo.processInfo.systemUptime }

    private func hardwareChanged() {
        #if os(iOS)
        // The system stopped it for a telephone call or Siri, and hands it
        // back when that is over (`resume`); starting it in between fails,
        // and would have been given up and said in the middle of the call.
        if interrupted { return }
        #endif
        // A change while waiting for quiet pushes the try back.
        guard (started && !engine.isRunning) || recovery.waiting else { return }
        Self.log.notice("the audio hardware changed under the engine")
        follow(recovery.changed(at: Self.clock()))
    }

    private func follow(_ step: EngineRecovery.Step, _ error: Error? = nil) {
        switch step {
        case .restart:
            Self.log.notice("starting the engine again")
            do {
                try restartNow()
            } catch {
                Self.log.error("the engine would not start again: \(String(describing: error), privacy: .public)")
                follow(recovery.failed(at: Self.clock()), error)
            }
        case let .wait(seconds):
            // Stopped, and said to be: `started` false is what lets anything
            // that wants sound in the meantime (a voice arriving, the
            // microphone pressed) start it again, where `started` left true
            // over a stopped engine made every start return at once.
            pause()
            Self.log.notice("waiting \(seconds, privacy: .public) s for the audio hardware to settle")
            quietTry?.cancel()
            quietTry = Task { [weak self] in
                try? await Task.sleep(for: .seconds(seconds))
                guard !Task.isCancelled, let self else { return }
                self.quietTry = nil
                self.follow(self.recovery.quiet(at: Self.clock()))
            }
        case .giveUp:
            pause()
            Self.log.error("the audio hardware will not settle; the engine stays stopped until it is asked for again")
            onNews?(.gaveUp(error ?? CocoaError(.featureUnsupported)))
        case .none:
            break
        }
    }

    /// The engine stopped and marked stopped, the microphone's track kept for
    /// when it starts again.
    private func pause() {
        untap()
        engine.stop()
        started = false
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
    /// unit that is not running. One that does not start again is not left
    /// at that: it is tried once more when the hardware is quiet, and given
    /// up and said only if that fails as well (EngineRecovery), while the
    /// caller hears the error now and may try something else first (`use`
    /// goes back to the device it had).
    func restart() throws {
        do {
            try restartNow()
        } catch {
            follow(recovery.failed(at: Self.clock()), error)
            throw error
        }
    }

    private func restartNow() throws {
        pause()
        try start()
    }

    private final class Player {
        let node: AVAudioSourceNode
        let ring: PCMRing
        let sink: RingSink
        let format: AVAudioFormat
        var volume: Float = 1
        var duck: Float = 1

        init(channels: Int) {
            let ring = PCMRing(channels: channels)
            self.ring = ring
            self.sink = RingSink(ring: ring)
            let format = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: AVAudioChannelCount(channels))!
            self.format = format
            self.node = AVAudioSourceNode(format: format) { _, _, frames, list in
                ring.render(into: UnsafeMutableAudioBufferListPointer(list), frames: Int(frames))
                return noErr
            }
        }
    }

    /// Starts the engine. On iOS this also claims the audio session for a call.
    public func start() throws {
        guard !started else { return }
        #if DEBUG
        if failingStarts > 0 {
            failingStarts -= 1
            Self.log.notice("rehearsal: this start of the engine fails")
            throw CocoaError(.featureUnsupported)
        }
        #endif
        let listening = !recording
        #if os(macOS)
        if listening && engineRecorded { freshEngine() }
        #endif
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        if listening {
            try session.setCategory(.playback, mode: .default, options: [])
        } else {
            try session.setCategory(.playAndRecord, mode: .videoChat, options: [.allowBluetoothHFP, .allowBluetoothA2DP, .defaultToSpeaker])
        }
        try session.setPreferredSampleRate(48_000)
        try session.setActive(true)
        #endif
        do {
            // Voice processing is chosen before anything touches the output:
            // asking for the mixer first builds and initialises a plain I/O
            // unit that turning voice processing on then has to tear down,
            // and that first initialisation is where the simulator's audio
            // server once timed out and aborted the app.
            if !listening {
                try engine.inputNode.setVoiceProcessingEnabled(!originalSound)
            } else {
                if voiceProcessing {
                    // Off again after a microphone that had it; the input is
                    // not touched otherwise, since one made under a session
                    // that records nothing has no channels.
                    try engine.inputNode.setVoiceProcessingEnabled(false)
                }
                // The output, made before the engine is prepared: with the
                // input left alone the graph had neither, and the first voice
                // to arrive while the microphone was off aborted the app
                // ("inputNode != nullptr || outputNode != nullptr"). The
                // warning above is about voice processing, which is off here.
                _ = engine.mainMixerNode
            }
            pointAtChosenDevices(listening: listening)
            engine.prepare()
            try engine.start()
            muteForTests()
            voiceProcessing = !listening && !originalSound
            listeningOnly = listening
            if voiceProcessing { duckOthersLeast() }
            #if os(macOS)
            if !listening { engineRecorded = true }
            #endif
            Self.log.notice("engine started, \(listening ? "for playback only" : "to record", privacy: .public), voice processing \(self.voiceProcessing ? "on" : "off", privacy: .public)")
        } catch where !originalSound && !listening {
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
            #if os(macOS)
            engineRecorded = true
            #endif
            Self.log.notice("engine started without voice processing")
        }
        started = true
        let wasGivenUp = recovery.gaveUp
        recovery.started()
        quietTry?.cancel()
        quietTry = nil
        // Whatever started it, the microphone goes back on: the restart for a
        // change of device, the try after the hardware settled, a voice
        // arriving while the engine waited for quiet, the end of a telephone
        // call on a phone.
        if let microphone, !tapped {
            do {
                try tap(microphone)
            } catch {
                Self.log.error("the microphone would not go back on the engine: \(String(describing: error), privacy: .public)")
                self.microphone = nil
                onNews?(.microphoneLost(error))
            }
        }
        if wasGivenUp { onNews?(.back) }
    }

    /// Voice processing ducks every other app's sound while it runs, and by
    /// default nearly to nothing: on a Mac, Apple Music was silent through a
    /// call with the microphone on, and an app's sound shared into the call
    /// was one its sharer could not hear. The least the system offers is
    /// asked for, and none of its own judging of when somebody speaks, which
    /// is the call's business (the web's echo canceller ducks nothing).
    ///
    /// Asked of a running engine: set before the start, it brought the
    /// voice-processing unit up early, the engine's own preparing of it was
    /// then refused (-10849, already initialised), and the microphone would
    /// not go on with voice processing or without it.
    private func duckOthersLeast() {
        engine.inputNode.voiceProcessingOtherAudioDuckingConfiguration =
            AVAudioVoiceProcessingOtherAudioDuckingConfiguration(enableAdvancedDucking: false, duckingLevel: .min)
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
        if Rehearsal.silenced {
            engine.mainMixerNode.outputVolume = 0
        }
    }

    public func stop() {
        microphone = nil
        untap()
        recording = false
        for sid in Array(players.keys) { stopPlaying(sid: sid) }
        engine.stop()
        started = false
        quietTry?.cancel()
        quietTry = nil
        recovery = EngineRecovery()
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
        // Forgotten first, so that a start of the engine below does not put
        // the last one back only to have it replaced.
        microphone = nil
        untap()
        // Set up to record before anything is tapped: an engine started for
        // playback only is started again for it, which is the gap a press of
        // the microphone costs.
        recording = true
        do {
            if started && listeningOnly {
                try restart()
            } else {
                try start()
            }
            try tap(input)
        } catch {
            recording = false
            throw error
        }
        microphone = input
    }

    /// The tap that feeds `input`, on the engine as it is now: a restart may
    /// bring the input back with another rate or channel count, so it is
    /// installed again rather than kept.
    private func tap(_ input: AudioInput) throws {
        untap()
        let node = engine.inputNode
        let format = node.outputFormat(forBus: 0)
        guard format.sampleRate > 0,
              let mono = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: format.sampleRate, channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: mono, to: voiceFormat) else {
            throw CocoaError(.featureUnsupported)
        }
        self.converter = converter
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
        tapped = true
    }

    public func stopMicrophone() {
        microphone = nil
        untap()
        // Back to playback only, so that what is heard is heard whole. Only
        // over an engine that is running: one stopped or given up starts
        // set up for playback when it is next asked for sound.
        guard recording else { return }
        recording = false
        if started && engine.isRunning && !listeningOnly {
            do {
                try restart()
            } catch {
                Self.log.error("the engine would not start again for playback only: \(String(describing: error), privacy: .public)")
            }
        }
    }

    private func untap() {
        guard tapped else { return }
        engine.inputNode.removeTap(onBus: 0)
        tapped = false
        converter = nil
        microphoneLevel = nil
    }

    /// Turns the system's voice processing off or on. The engine has to be
    /// restarted for the change to take, and the microphone with it. One that
    /// does not take is put back as it was, so that the voice track the call
    /// keeps (published for one processing or the other) and the engine
    /// agree.
    public func setOriginalSound(_ original: Bool) throws {
        guard original != originalSound else { return }
        originalSound = original
        guard started else { return }
        do {
            try restart()
        } catch {
            originalSound = !original
            try? restart()
            throw error
        }
    }

    #if os(iOS)
    /// A telephone call, an alarm or Siri has the audio session; the system
    /// has stopped the engine.
    private var interrupted = false

    /// The session was taken away. The engine is marked stopped, the
    /// microphone's track kept, and nothing tries to start it until `resume`:
    /// `started` left true across the interruption made the start at its end
    /// return at once, and the call came back silent both ways.
    func suspend() {
        interrupted = true
        quietTry?.cancel()
        quietTry = nil
        recovery = EngineRecovery()
        pause()
        Self.log.notice("the audio session was interrupted")
    }

    /// The session is handed back: the engine starts again, and with it the
    /// microphone if it was on. A start that fails is tried again once the
    /// hardware is quiet, and said if that fails too.
    func resume() {
        guard interrupted else { return }
        interrupted = false
        Self.log.notice("the audio session is back; starting the engine again")
        do {
            try start()
        } catch {
            Self.log.error("the engine would not start after the interruption: \(String(describing: error), privacy: .public)")
            follow(recovery.failed(at: Self.clock()), error)
        }
    }
    #endif

    #if DEBUG
    /// TOMOSHIBI_DEBUG_PRESS's `audiofail:<n>`, in a rehearsal: the next n
    /// starts of the engine throw, as a device that will not start does.
    var failingStarts = 0

    /// TOMOSHIBI_DEBUG_PRESS's `audiochange`: the engine stopped under the
    /// call as the system stops it when the hardware changes, and the
    /// system's notice of it.
    func rehearseHardwareChange() {
        engine.stop()
        NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: engine)
    }

    /// What the engine is doing, for a rehearsal's log.
    var rehearsalState: String {
        "engine \(engine.isRunning ? "running" : "stopped"), started \(started), microphone \(microphone != nil ? "wanted" : "off"), tap \(tapped ? "on" : "off"), waiting \(recovery.waiting), given up \(recovery.gaveUp)"
    }
    #endif

    // MARK: Remote sound

    /// Plays a remote track of `channels` channels on a node of its own.
    public func play(sid: String, channels: Int, from call: Call) throws {
        try start()
        stopPlaying(sid: sid)
        let player = Player(channels: channels)
        engine.attach(player.node)
        engine.connect(player.node, to: engine.mainMixerNode, format: player.format)
        players[sid] = player
        call.attachAudio(sid: sid, sink: player.sink)
        Self.log.notice("playing \(sid, privacy: .public) in \(channels) channels")
        #if DEBUG
        // TOMOSHIBI_DEBUG_PLAYER_LEVEL: how loud each remote sound comes out
        // of its node, once a second. A node is only pulled while the engine
        // plays it, so a level is the proof that a sound is still heard
        // across a restart of the engine (the switch between playback only
        // and recording on iOS).
        if ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PLAYER_LEVEL"] != nil {
            let ring = player.ring
            Task { @MainActor [weak self] in
                while let self, self.players[sid] === player {
                    try? await Task.sleep(for: .seconds(1))
                    let taken = ring.debugTake()
                    Self.log.notice("PLAYER \(sid, privacy: .public) in \(taken.written, privacy: .public) out \(taken.rendered, privacy: .public) peak \(taken.peak, privacy: .public)")
                }
            }
        }
        #endif
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
        engine.connect(player.node, to: engine.mainMixerNode, format: player.format)
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
