import Foundation

/*
 * How loud everybody else is, and who is not to be heard at all: the record
 * the sound panel writes and the call applies (web/src/live/hearing.ts).
 *
 * The web keeps this record itself rather than on the media library's objects,
 * because those forget: a volume lives on a participant that a reconnect
 * rebuilds, and a block lives on a publication that does not outlive its
 * track, so somebody who stops a share and starts it again comes back audible.
 * The apps forget the same way, more so: every remote sound gets a player of
 * its own when it is subscribed, at full volume. So this is the record, and
 * what the call holds is re-filled from it whenever the room changes.
 *
 * Kept here, apart from the core, so that the rules about what is written down
 * are tested on their own. Where a setting is filed is decided by the caller
 * from the identity's signature, which only the core can read.
 */

/// One thing there is to hear from somebody (hearing.ts `Sound`).
public enum Sound: String, CaseIterable, Sendable {
    case voice, screen, music
}

/// What somebody has decided about one sound.
public struct HearingSetting: Equatable, Sendable {
    /// Between silent and as loud as it was sent.
    public var volume: Double
    /// Stopped at the media server rather than turned down here: a volume of
    /// nought still downloads every packet of a shared film and throws it
    /// away.
    public var blocked: Bool

    public init(volume: Double, blocked: Bool) {
        self.volume = volume
        self.blocked = blocked
    }

    /// Untouched, which is what everybody is until somebody says otherwise.
    public static let asSent = HearingSetting(volume: 1, blocked: false)

    /// Whether this is something somebody has decided not to hear.
    public var silenced: Bool { blocked || volume == 0 }
}

/// Where a setting is filed, and whether it outlives the call.
///
/// A proven signature is derived from a passphrase only its holder can
/// produce, so it is the same string on every visit and worth writing down.
/// Everybody else gets a fresh identity each time, and a setting kept against
/// one of those would be a note about somebody who no longer exists, applied
/// to whoever is issued that string next (hearing.ts `filing`).
public struct HearingFiling: Equatable, Sendable {
    public let key: String
    public let keeps: Bool

    public init(key: String, keeps: Bool) {
        self.key = key
        self.keeps = keeps
    }
}

/// Every setting, filed under `<who>/<sound>`, and what of it is kept.
public struct HearingBook: Equatable, Sendable {
    /// The web's own storage name, old product name and all: renaming it
    /// would abandon what is already written down, and an app that reads the
    /// same name reads like the same product.
    public static let storageKey = "meet-live.hearing"

    private struct Held: Equatable, Sendable {
        var setting: HearingSetting
        var keeps: Bool
    }

    private var entries: [String: Held] = [:]

    public init() {}

    /// The book as it was stored: the web's JSON, `{"<who>/<sound>":
    /// {"volume": 0.5, "blocked": false}}`. Anything unreadable reads as
    /// nothing decided, which is the state of an app that never saw this.
    public init(stored: String?) {
        guard
            let stored,
            let data = stored.data(using: .utf8),
            let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return }
        for (place, value) in parsed {
            guard let value = value as? [String: Any] else { continue }
            entries[place] = Held(
                setting: HearingSetting(volume: Self.level(value["volume"]), blocked: Self.flag(value["blocked"])),
                keeps: true
            )
        }
    }

    /// What has been decided about one sound from one person.
    ///
    /// Clamped on the way out as well as on the way in: storage can be edited
    /// by hand, and this is the one door everything leaves by.
    public func setting(_ filing: HearingFiling, _ sound: Sound) -> HearingSetting {
        guard let held = entries[Self.place(filing, sound)] else { return .asSent }
        return HearingSetting(volume: Self.clamp(held.setting.volume), blocked: held.setting.blocked)
    }

    /// Writes a decision down. Returns whether what is kept changed, which is
    /// the only time storage needs writing: a guest's setting changing leaves
    /// nothing on disk different.
    @discardableResult
    public mutating func set(_ filing: HearingFiling, _ sound: Sound, _ setting: HearingSetting) -> Bool {
        let place = Self.place(filing, sound)
        let clean = HearingSetting(volume: Self.clamp(setting.volume), blocked: setting.blocked)
        let before = entries[place]
        if clean == .asSent {
            // Nothing decided is not a decision to write down, so turning
            // somebody back up removes them rather than leaving a row saying
            // they are ordinary.
            entries[place] = nil
        } else {
            entries[place] = Held(setting: clean, keeps: filing.keeps)
        }
        return filing.keeps && before != entries[place]
    }

    /// What is kept, as the web writes it, or nil where nothing is: the web
    /// removes the key rather than storing an empty object.
    ///
    /// Written out by hand rather than by JSONSerialization, which writes a
    /// volume of 0.35 as 0.34999999999999998; the web's `JSON.stringify`
    /// writes the shortest form, as Swift's own description of a Double does,
    /// and each entry's fields in the order the web gives them.
    public var stored: String? {
        let kept = entries.filter { $0.value.keeps }.sorted { $0.key < $1.key }
        guard !kept.isEmpty else { return nil }
        let fields = kept.map { place, held in
            "\(Self.quoted(place)):{\"volume\":\(Self.number(held.setting.volume)),\"blocked\":\(held.setting.blocked)}"
        }
        return "{" + fields.joined(separator: ",") + "}"
    }

    private static func quoted(_ text: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: text, options: [.fragmentsAllowed, .withoutEscapingSlashes]) else { return "\"\"" }
        return String(decoding: data, as: UTF8.self)
    }

    /// A volume as JavaScript writes a number: 1 rather than 1.0.
    private static func number(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(describing: value)
    }

    private static func place(_ filing: HearingFiling, _ sound: Sound) -> String {
        "\(filing.key)/\(sound.rawValue)"
    }

    /// A volume between nought and one. A slider cannot produce anything
    /// else; a hand-edited entry can, and a player handed 3 would be three
    /// times as loud as the person sent it (hearing.ts `level`).
    static func clamp(_ volume: Double) -> Double {
        volume.isFinite ? min(1, max(0, volume)) : HearingSetting.asSent.volume
    }

    /// A stored volume, read as the web reads one: only a number is a
    /// volume. JSON's `true` arrives here as a number too, and the web, whose
    /// `typeof` tells them apart, reads it as untouched; so does this.
    static func level(_ value: Any?) -> Double {
        guard let number = value as? NSNumber, !isBoolean(number) else { return HearingSetting.asSent.volume }
        return clamp(number.doubleValue)
    }

    /// Only JSON's `true`, as the web's `=== true`: a 1 is not a block.
    static func flag(_ value: Any?) -> Bool {
        guard let number = value as? NSNumber, isBoolean(number) else { return false }
        return number.boolValue
    }

    private static func isBoolean(_ number: NSNumber) -> Bool {
        CFGetTypeID(number) == CFBooleanGetTypeID()
    }
}
