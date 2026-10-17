import AVFoundation
import Foundation
#if os(macOS)
import CoreAudio
import AudioToolbox
#endif

/// A microphone or a pair of speakers the person can choose.
public struct AudioDevice: Identifiable, Hashable, Sendable {
    public let id: String
    public let name: String
}

/// Choosing which microphone and speakers a call uses.
///
/// On a Mac the engine's input and output units are pointed at a CoreAudio
/// device; on iOS the audio session is asked to prefer an input, and the
/// output is the system's to choose (AVRoutePickerView), as every call app
/// there leaves it.
extension CallAudio {
    #if os(macOS)
    /// The microphones or speakers somebody can pick.
    ///
    /// Not every device CoreAudio lists. While the engine runs, this process
    /// sees the aggregate devices the system builds for it, private to it
    /// ("CADefaultDeviceAggregate-<pid>-0", "VPAUAggregateAudioDevice-0x…"),
    /// and with voice processing on the speakers grow an input of their own,
    /// the echo canceller's reference: the panel listed all of those as
    /// microphones and speakers (2026-10-17, macOS 27). So private aggregates
    /// are left out, and a microphone is only one the system's capture also
    /// calls a microphone, whose identifier is the same UID.
    ///
    /// That alone did not hold. The capture's list catches up with the
    /// hardware a few seconds later, and from then on it calls the speakers'
    /// new input a microphone too (read in a call on macOS 27: not at the
    /// moment the microphone went on, and at three seconds and ten), so the
    /// owner found "MacBook Pro Speakers" under Microphone in a call that had
    /// gone on a while. What does tell them apart is what each input says it
    /// is: the built-in microphone's streams are microphones (terminal type
    /// 0x201), the speakers' reference says nothing (0). A built-in device
    /// is a microphone only if one of its inputs says so; any other is not
    /// one if it was seen here with no input at all and grew one since, as
    /// the speakers did, since it is the call that gave it one.
    public static func devices(input: Bool) -> [AudioDevice] {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDevices, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size) == noErr else { return [] }
        var ids = [AudioDeviceID](repeating: 0, count: Int(size) / MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &ids) == noErr else { return [] }
        let microphones = input
            ? Set(AVCaptureDevice.DiscoverySession(deviceTypes: [.microphone], mediaType: .audio, position: .unspecified).devices.map(\.uniqueID))
            : []
        return ids.compactMap { id in
            let inputs = channels(of: id, input: true)
            if inputs == 0, let uid = string(id, kAudioDevicePropertyDeviceUID) { playsOnly.insert(uid) }
            guard channels(of: id, input: input) > 0, !isPrivateAggregate(id),
                  let uid = string(id, kAudioDevicePropertyDeviceUID), let name = string(id, kAudioObjectPropertyName),
                  !uid.hasPrefix("CADefaultDeviceAggregate"), !uid.hasPrefix("VPAUAggregateAudioDevice") else { return nil }
            if input {
                if !microphones.isEmpty, !microphones.contains(uid) { return nil }
                if transport(of: id) == kAudioDeviceTransportTypeBuiltIn {
                    guard inputTerminals(of: id).contains(where: isMicrophoneTerminal) else { return nil }
                } else if playsOnly.contains(uid) {
                    return nil
                }
            }
            return AudioDevice(id: uid, name: name)
        }
    }

    /// Devices seen in this process with no input: one that has an input
    /// later had it given by the call's voice processing.
    private static var playsOnly: Set<String> = []

    /// A stream that says it is a microphone. A driver gives either Core
    /// Audio's own codes (AudioHardwareBase.h: 'micr', a headset's 'hmic', a
    /// receiver's 'rmic') or a USB input terminal type (0x201 a microphone,
    /// up to 0x206 a processing array), as the built-in one does: 0x201.
    private static func isMicrophoneTerminal(_ terminal: UInt32) -> Bool {
        [UInt32(kAudioStreamTerminalTypeMicrophone), UInt32(kAudioStreamTerminalTypeHeadsetMicrophone),
         UInt32(kAudioStreamTerminalTypeReceiverMicrophone)].contains(terminal) || (0x201...0x2FF).contains(terminal)
    }

    private static func transport(of device: AudioDeviceID) -> UInt32 {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyTransportType, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var transport: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        return AudioObjectGetPropertyData(device, &address, 0, nil, &size, &transport) == noErr ? transport : 0
    }

    private static func inputTerminals(of device: AudioDeviceID) -> [UInt32] {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreams, mScope: kAudioDevicePropertyScopeInput, mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr, size > 0 else { return [] }
        var streams = [AudioStreamID](repeating: 0, count: Int(size) / MemoryLayout<AudioStreamID>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &streams) == noErr else { return [] }
        return streams.map { stream in
            var terminalAddress = AudioObjectPropertyAddress(mSelector: kAudioStreamPropertyTerminalType, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
            var terminal: UInt32 = 0
            var terminalSize = UInt32(MemoryLayout<UInt32>.size)
            return AudioObjectGetPropertyData(stream, &terminalAddress, 0, nil, &terminalSize, &terminal) == noErr ? terminal : 0
        }
    }

    /// An aggregate device made private to one process: the system's own,
    /// built for an engine, rather than one somebody made in Audio MIDI Setup.
    private static func isPrivateAggregate(_ device: AudioDeviceID) -> Bool {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyTransportType, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var transport: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &transport) == noErr,
              transport == kAudioDeviceTransportTypeAggregate else { return false }
        address.mSelector = kAudioAggregateDevicePropertyComposition
        var composition: Unmanaged<CFDictionary>?
        size = UInt32(MemoryLayout<Unmanaged<CFDictionary>?>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &composition) == noErr,
              let dictionary = composition?.takeRetainedValue() as? [String: Any] else { return false }
        return (dictionary[kAudioAggregateDeviceIsPrivateKey] as? Int) == 1
    }

    private static func channels(of device: AudioDeviceID, input: Bool) -> Int {
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreamConfiguration,
                                                 mScope: input ? kAudioDevicePropertyScopeInput : kAudioDevicePropertyScopeOutput,
                                                 mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr, size > 0 else { return 0 }
        let list = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { list.deallocate() }
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, list) == noErr else { return 0 }
        let buffers = UnsafeMutableAudioBufferListPointer(list.assumingMemoryBound(to: AudioBufferList.self))
        return buffers.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    private static func string(_ device: AudioDeviceID, _ selector: AudioObjectPropertySelector) -> String? {
        var address = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var value: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &value) == noErr, let value else { return nil }
        return value.takeRetainedValue() as String
    }

    private static func deviceID(uid: String) -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyTranslateUIDToDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var cf = uid as CFString
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = withUnsafeMutablePointer(to: &cf) { pointer in
            AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, UInt32(MemoryLayout<CFString>.size), pointer, &size, &id)
        }
        return status == noErr && id != 0 ? id : nil
    }

    /// The system's own microphone or speakers, which the engine uses until
    /// somebody picks another.
    static func systemDefault(input: Bool) -> String? {
        var address = AudioObjectPropertyAddress(mSelector: input ? kAudioHardwarePropertyDefaultInputDevice : kAudioHardwarePropertyDefaultOutputDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &id) == noErr, id != 0 else { return nil }
        return string(id, kAudioDevicePropertyDeviceUID)
    }

    /// The I/O unit's device for one direction, as the unit says.
    ///
    /// With voice processing on, the engine's input and output are one
    /// voice-processing unit, which keeps a device for each direction: the
    /// input's on element 1 and the output's on element 0 (read off the unit
    /// on macOS 27, and how Chromium's macOS capture sets them). Setting
    /// element 0 for a microphone would have moved the speakers instead.
    private func unitDevice(input: Bool) -> String? {
        guard let unit = engine.inputNode.audioUnit else { return nil }
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        guard AudioUnitGetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, input ? 1 : 0, &id, &size) == noErr,
              id != 0 else { return nil }
        return Self.string(id, kAudioDevicePropertyDeviceUID)
    }

    /// The device the output unit plays on, while the engine only plays.
    private func outputUnitDevice() -> String? {
        guard let unit = engine.outputNode.audioUnit else { return nil }
        var id = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        guard AudioUnitGetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0, &id, &size) == noErr,
              id != 0 else { return nil }
        return Self.string(id, kAudioDevicePropertyDeviceUID)
    }

    /// Whether a microphone or speakers can be picked here now.
    ///
    /// With voice processing, or with the engine only playing, whose output
    /// unit is pointed at the speakers picked and whose microphone is a
    /// choice kept for when it goes on. Not while recording without voice
    /// processing (original sound, or a Mac where voice processing would not
    /// start): the engine then runs one unit for both directions on the
    /// system's default aggregate device, which refuses a device that only
    /// records or only plays (-10851), and after refusing one reported no
    /// device at all: a call with no sound. So there the system's own choice
    /// stands, and the panel says where to change it. Before the engine has
    /// started a choice is kept for when it does.
    public var choosable: Bool { !started || voiceProcessing || listeningOnly }

    /// The device the engine's microphone or speakers are using: what the
    /// unit says while the engine runs with voice processing, and otherwise
    /// what was picked or the system's default, which is what it will use.
    public func current(input: Bool) -> String? {
        if engine.isRunning, voiceProcessing, let uid = unitDevice(input: input) { return uid }
        if engine.isRunning, listeningOnly, !input, let uid = outputUnitDevice() { return uid }
        if choosable, let chosen = input ? chosenInput : chosenOutput { return chosen }
        return Self.systemDefault(input: input)
    }

    /// Points the voice-processing unit at the devices picked, before the
    /// engine starts: turning voice processing on or off builds a new unit,
    /// which comes up on the system's defaults and would quietly drop a
    /// headset somebody picked.
    func pointAtChosenDevices(listening: Bool = false) {
        if listening {
            // Only playing, on an engine that has never recorded
            // (`freshEngine`): the output unit alone, and the input node not
            // touched, since asking for it is what gives an engine one.
            guard let uid = chosenOutput, let id = Self.deviceID(uid: uid), let unit = engine.outputNode.audioUnit else { return }
            var device = id
            let status = AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                              &device, UInt32(MemoryLayout<AudioDeviceID>.size))
            if status != noErr {
                Self.log.error("could not point the speakers at \(uid, privacy: .public) for playback: \(status, privacy: .public)")
            }
            return
        }
        guard engine.inputNode.isVoiceProcessingEnabled, let unit = engine.inputNode.audioUnit else { return }
        for (uid, input) in [(chosenInput, true), (chosenOutput, false)] {
            guard let uid, let id = Self.deviceID(uid: uid) else { continue }
            var device = id
            let status = AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, input ? 1 : 0,
                                              &device, UInt32(MemoryLayout<AudioDeviceID>.size))
            if status != noErr {
                Self.log.error("could not point the \(input ? "microphone" : "speakers", privacy: .public) at \(uid, privacy: .public): \(status, privacy: .public)")
            }
        }
    }

    /// Points the engine's microphone or speakers at a device, and throws
    /// where it did not take. The engine is stopped for the change and
    /// started again, and the microphone with it: a new device may have
    /// another channel count or rate, and a tap left on the old format went
    /// silent or stopped the engine.
    ///
    /// A device that will not start goes back to the one before. Neither
    /// restart loses the microphone: it went back on with whichever started
    /// (CallAudio `microphone`), where the undo after a failed start used to
    /// come back without it. If the undo will not start either, the engine is
    /// tried again once the hardware is quiet (EngineRecovery).
    public func use(_ device: AudioDevice, input: Bool) throws {
        guard choosable, Self.deviceID(uid: device.id) != nil else { throw CocoaError(.featureUnsupported) }
        let before = input ? chosenInput : chosenOutput
        func undo() {
            if input { chosenInput = before } else { chosenOutput = before }
            try? restart()
        }
        if input { chosenInput = device.id } else { chosenOutput = device.id }
        guard started else { return }
        do {
            try restart()
        } catch {
            undo()
            throw error
        }
        // Read back rather than assumed: a unit that refused the device keeps
        // the one it had, and the tick has to stay with that one. While the
        // engine only plays, a microphone is a choice for later and only the
        // speakers can be read back.
        if listeningOnly {
            guard input || outputUnitDevice() == device.id else {
                undo()
                throw CocoaError(.featureUnsupported)
            }
            return
        }
        guard voiceProcessing, unitDevice(input: input) == device.id else {
            undo()
            throw CocoaError(.featureUnsupported)
        }
    }
    #else
    public static func devices(input: Bool) -> [AudioDevice] {
        guard input else { return [] }
        return (AVAudioSession.sharedInstance().availableInputs ?? []).map { AudioDevice(id: $0.uid, name: $0.portName) }
    }

    public var choosable: Bool { true }

    /// The input the session is routed through, and before the session is
    /// active (nobody heard yet, the microphone off) the one it will take:
    /// the one picked, or the first it offers, which is the built-in one.
    public func current(input: Bool) -> String? {
        guard input else { return nil }
        let session = AVAudioSession.sharedInstance()
        return session.currentRoute.inputs.first?.uid ?? chosenInput ?? session.preferredInput?.uid ?? session.availableInputs?.first?.uid
    }

    func pointAtChosenDevices(listening: Bool = false) {
        guard let chosenInput, let port = AVAudioSession.sharedInstance().availableInputs?.first(where: { $0.uid == chosenInput }) else { return }
        try? AVAudioSession.sharedInstance().setPreferredInput(port)
    }

    public func use(_ device: AudioDevice, input: Bool) throws {
        guard input, let port = AVAudioSession.sharedInstance().availableInputs?.first(where: { $0.uid == device.id }) else {
            throw CocoaError(.featureUnsupported)
        }
        try AVAudioSession.sharedInstance().setPreferredInput(port)
        chosenInput = device.id
    }
    #endif
}

/// Told when microphones, speakers or cameras come and go, or the system's
/// default changes: the devices panel listed what was there when it opened,
/// so a headset plugged in with the panel open was not in it until it was
/// closed and opened again (DeviceMenu.tsx listens to `devicechange` for the
/// same reason).
@MainActor
final class DeviceWatch {
    private var observers: [NSObjectProtocol] = []
    #if os(macOS)
    private var listeners: [(AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []
    #endif

    init(onChange: @escaping @MainActor () -> Void) {
        let center = NotificationCenter.default
        for name in [AVCaptureDevice.wasConnectedNotification, AVCaptureDevice.wasDisconnectedNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated { onChange() }
            })
        }
        #if os(macOS)
        for selector in [kAudioHardwarePropertyDevices, kAudioHardwarePropertyDefaultInputDevice, kAudioHardwarePropertyDefaultOutputDevice] {
            var address = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
            let block: AudioObjectPropertyListenerBlock = { _, _ in
                MainActor.assumeIsolated { onChange() }
            }
            if AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, DispatchQueue.main, block) == noErr {
                listeners.append((address, block))
            }
        }
        #else
        observers.append(center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { onChange() }
        })
        #endif
    }

    func stop() {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        #if os(macOS)
        for (address, block) in listeners {
            var address = address
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, DispatchQueue.main, block)
        }
        listeners = []
        #endif
    }
}
