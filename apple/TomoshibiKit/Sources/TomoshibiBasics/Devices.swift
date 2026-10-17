import Foundation

/// The choices about one's own devices that a call starts from, kept under
/// the web's own names (live/preferences.ts, live/sound.ts) and read the way
/// the web reads them.
///
/// `meet-live.devices` is whether the microphone and the camera start on;
/// `meet-live.device-choice` is which microphone, camera and speakers were
/// last picked, by kind; `meet-live.original-sound` is whether the voice goes
/// as it is. Until these were kept, the switches on the join screen started
/// from the same defaults every time and a chosen headset was picked again at
/// the start of every call.
///
/// Each is a string, as the browser's storage holds it, so that what the app
/// keeps reads the same as what the web keeps. Every reader treats a value it
/// does not recognise as never having been told anything (preferences.ts):
/// a value half written, or written by a later build, may not become a call
/// that starts with a silent microphone.
public struct DeviceMemory: @unchecked Sendable {
    public static let devicesKey = "meet-live.devices"
    public static let choiceKey = "meet-live.device-choice"
    public static let originalKey = "meet-live.original-sound"
    /// Whether the background is blurred (preferences.ts `BLUR_KEY`), in the
    /// web's own words, "on" or nothing.
    public static let blurKey = "meet-live.blur"

    /// The kinds of device somebody picks from a list (preferences.ts
    /// `CHOSEN_KINDS`), named as the web names them.
    public enum Kind: String, CaseIterable, Sendable {
        case microphone = "audioinput"
        case camera = "videoinput"
        case speakers = "audiooutput"
    }

    /// Whether the camera and the microphone start on.
    public struct Joining: Equatable, Sendable {
        public var camera: Bool
        public var microphone: Bool

        public init(camera: Bool, microphone: Bool) {
            self.camera = camera
            self.microphone = microphone
        }

        /// The microphone on and the camera off: how most people walk into a
        /// call, and the cheaper of the two mistakes (preferences.ts).
        public static let first = Joining(camera: false, microphone: true)
    }

    /// Where it is all kept; nil keeps nothing and remembers nothing, which
    /// is what a test driving the app gets (AppModel.rehearsal). The other
    /// choices a call remembers (the signal's figures, the share's size and
    /// rate) are kept here too, so that a rehearsal keeps none of them
    /// either.
    public let defaults: UserDefaults?

    public init(defaults: UserDefaults?) {
        self.defaults = defaults
    }

    /// Whether anything is kept at all.
    public var keeps: Bool { defaults != nil }

    public static var standard: DeviceMemory { DeviceMemory(defaults: .standard) }
    public static let nothing = DeviceMemory(defaults: nil)

    /// Longer than any device's identifier, and short enough to refuse what
    /// is not one (preferences.ts `MOST_ID`).
    static let mostID = 256

    // MARK: Joining

    public var joining: Joining {
        guard let stored = defaults?.string(forKey: Self.devicesKey),
              let parsed = try? JSONSerialization.jsonObject(with: Data(stored.utf8)) as? [String: Any] else {
            return .first
        }
        // Each on its own: a record that lost one half keeps the other.
        // JSONSerialization reads `true` as a number that is also a Bool;
        // only a real Bool counts, as only `typeof === "boolean"` does.
        func flag(_ key: String, _ fallback: Bool) -> Bool {
            guard let value = parsed[key] as? NSNumber, CFGetTypeID(value) == CFBooleanGetTypeID() else { return fallback }
            return value.boolValue
        }
        return Joining(camera: flag("camera", Joining.first.camera), microphone: flag("microphone", Joining.first.microphone))
    }

    public func remember(_ joining: Joining) {
        defaults?.set("{\"camera\":\(joining.camera),\"microphone\":\(joining.microphone)}", forKey: Self.devicesKey)
    }

    // MARK: Which device

    /// The device last picked of this kind, or nil.
    public func choice(_ kind: Kind) -> String? {
        choices[kind]
    }

    private var choices: [Kind: String] {
        guard let stored = defaults?.string(forKey: Self.choiceKey),
              let parsed = try? JSONSerialization.jsonObject(with: Data(stored.utf8)) as? [String: Any] else {
            return [:]
        }
        var out: [Kind: String] = [:]
        for kind in Kind.allCases {
            if let id = parsed[kind.rawValue] as? String, !id.isEmpty, id.count <= Self.mostID { out[kind] = id }
        }
        return out
    }

    /// Kept once the device took, and not before (DeviceMenu.tsx `select`):
    /// a choice remembered before the device came up would bring back the one
    /// that failed.
    public func remember(_ kind: Kind, id: String) {
        guard let defaults, !id.isEmpty, id.count <= Self.mostID else { return }
        var all = choices
        all[kind] = id
        let object = Dictionary(uniqueKeysWithValues: all.map { ($0.key.rawValue, $0.value) })
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]) else { return }
        defaults.set(String(decoding: data, as: UTF8.self), forKey: Self.choiceKey)
    }

    /// Which of the remembered devices can be asked for, given what is
    /// listed now (preferences.ts `restorable`): one that is listed. One that
    /// is not is unplugged or gone, so the system's default stands and the
    /// choice stays written down for the day it comes back. A list that is
    /// empty says nothing either way and asks for nothing.
    public func restorable(_ kind: Kind, present: [String]) -> String? {
        guard let id = choice(kind), present.contains(id) else { return nil }
        return id
    }

    // MARK: The voice

    /// Whether the voice was last sent as it is (sound.ts `rememberedOriginal`).
    public var originalSound: Bool {
        defaults?.string(forKey: Self.originalKey) == "on"
    }

    public func remember(originalSound on: Bool) {
        defaults?.set(on ? "on" : nil, forKey: Self.originalKey)
    }

    /// The background blurred behind somebody's camera, remembered as the
    /// web remembers it (preferences.ts `blurWanted`).
    public var blur: Bool {
        defaults?.string(forKey: Self.blurKey) == "on"
    }

    public func remember(blur on: Bool) {
        defaults?.set(on ? "on" : nil, forKey: Self.blurKey)
    }
}

/// The frame rates each size of screen share may be asked for (room.ts
/// `RATES_FOR`), by the shorter side the size names, 0 being automatic.
///
/// Ceilings rather than preferences: 4K at 120 frames is a billion pixels a
/// second, which the encoder does not refuse but falls behind on, so it is
/// not offered at all.
public enum ShareRates {
    public static let all = [15, 30, 60, 120, 240]

    /// What automatic sends: a ceiling of 1440p at sixty (room.ts
    /// `AUTOMATIC`), with no rate to choose.
    public static let automatic = 60

    public static func allowed(shorterSide: Int) -> [Int] {
        switch shorterSide {
        case 1080: return [15, 30, 60, 120, 240]
        case 1440: return [15, 30, 60, 120]
        case 2160: return [15, 30, 60]
        default: return [automatic]
        }
    }

    /// The rate a size can carry (ShareButton.tsx `clamp`): the one wanted
    /// where it is allowed, and otherwise the size's highest. A sixty
    /// remembered from 1080p, taken to 4K, is sixty; a 240 is 4K's sixty
    /// rather than an ask the encoder silently falls behind on.
    public static func clamp(_ wanted: Int, shorterSide: Int) -> Int {
        let rates = allowed(shorterSide: shorterSide)
        return rates.contains(wanted) ? wanted : (rates.last ?? automatic)
    }
}

/// How the microphone meter reads a level (MicLevel.tsx).
public enum MicrophoneLevel {
    /// Anything above this has been heard, and the words under the meter
    /// change from "Say something" to "Hearing you" (MicLevel.tsx's 0.06).
    public static let heard = 0.06

    /// How full the meter is, 0 to 1, for a root-mean-square level of the
    /// samples, 0 to 1.
    ///
    /// The web reads an analyser's byte frequency data, which is already in
    /// decibels (-100 to -30 dB across 0 to 255), and then bends it upward
    /// so ordinary speech is not in the bottom fifth of the bar. A sample's
    /// RMS is linear: speech at -26 dBFS is 0.05, and drawn as it is, the
    /// meter barely moved for a voice. So it is drawn in decibels, from -60
    /// (nothing) to -10 (as loud as anybody speaks into a microphone), which
    /// puts a voice in the upper half of the bar and the hiss of a quiet room
    /// under the "heard" line.
    public static func shown(rms: Double) -> Double {
        guard rms > 0 else { return 0 }
        let decibels = 20 * log10(rms)
        return min(1, max(0, (decibels + 60) / 50))
    }
}
