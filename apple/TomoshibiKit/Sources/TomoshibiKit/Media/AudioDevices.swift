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
    public static func devices(input: Bool) -> [AudioDevice] {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDevices, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size) == noErr else { return [] }
        var ids = [AudioDeviceID](repeating: 0, count: Int(size) / MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &ids) == noErr else { return [] }
        return ids.compactMap { id in
            guard channels(of: id, input: input) > 0, let uid = string(id, kAudioDevicePropertyDeviceUID), let name = string(id, kAudioObjectPropertyName) else { return nil }
            return AudioDevice(id: uid, name: name)
        }
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

    /// Points the engine's microphone or speakers at a device. The engine is
    /// stopped for the change and started again; the microphone tap goes on.
    public func use(_ device: AudioDevice, input: Bool) {
        guard let id = Self.deviceID(uid: device.id) else { return }
        let node: AVAudioIONode = input ? engine.inputNode : engine.outputNode
        guard let unit = node.audioUnit else { return }
        let running = engine.isRunning
        if running { engine.stop() }
        var device = id
        AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0, &device, UInt32(MemoryLayout<AudioDeviceID>.size))
        if running { try? engine.start() }
    }
    #else
    public static func devices(input: Bool) -> [AudioDevice] {
        guard input else { return [] }
        return (AVAudioSession.sharedInstance().availableInputs ?? []).map { AudioDevice(id: $0.uid, name: $0.portName) }
    }

    public func use(_ device: AudioDevice, input: Bool) {
        guard input, let port = AVAudioSession.sharedInstance().availableInputs?.first(where: { $0.uid == device.id }) else { return }
        try? AVAudioSession.sharedInstance().setPreferredInput(port)
    }
    #endif
}
