import Foundation

/// How sharp a video watched together may get on this screen
/// (web/src/live/preferences.ts, `watchQuality`, `levelCap`, `capping`).
///
/// A ceiling and not a choice of quality: what plays is still decided by the
/// line, so somebody on a slow connection who said 1080p gets whatever their
/// connection carries, and somebody who said 720p never gets more however
/// fast it is. It exists for the second person, on a metered phone or a line
/// shared with a household.
///
/// Kept under the web's own key and in the web's own words, so the account
/// page's setting and the apps' are one habit. The settings screen writes it;
/// the players read it when a video is opened, as the web's does, so a change
/// applies from the next video.
public enum WatchCeiling: String, CaseIterable, Sendable {
    case auto
    case p1080 = "1080p"
    case p720 = "720p"

    public static let storageKey = "meet-live.watch-quality"

    /// The ceiling chosen, or none: anything that is not one of the stored
    /// words (a hand edit, "1080P", "4k") reads as none, as the web's does.
    public static func stored(in defaults: UserDefaults = .standard) -> WatchCeiling {
        guard let word = defaults.string(forKey: storageKey), let chosen = WatchCeiling(rawValue: word) else { return .auto }
        return chosen
    }

    /// Writes it down; no ceiling removes the key rather than writing "auto".
    public func remember(in defaults: UserDefaults = .standard) {
        if self == .auto {
            defaults.removeObject(forKey: Self.storageKey)
        } else {
            defaults.set(rawValue, forKey: Self.storageKey)
        }
    }

    /// The picture height each ceiling allows.
    var lines: Int? {
        switch self {
        case .auto: nil
        case .p1080: 1080
        case .p720: 720
        }
    }

    /// One rung of a playlist, as much of it as the rule needs: its size, if
    /// the playlist says it.
    public struct Level: Equatable, Sendable {
        public var width: Double?
        public var height: Double

        public init(width: Double?, height: Double) {
            self.width = width
            self.height = height
        }
    }

    /// The highest rung this ceiling allows, as an index into the rungs in
    /// the order the player ranks them (by bitrate), or nil for no ceiling at
    /// all (`levelCap`, whose -1 this is).
    ///
    /// Measured on the shorter side, so that 1080p means what people mean by
    /// it whatever the shape: a film cropped to 1920x800 is a 1080p film and a
    /// 4K one cropped to 3840x1600 is not. Eight lines of slack, because
    /// encoders round a picture up to whole sixteen-pixel blocks and a 1080p
    /// stream that says 1088 is still the one asked for. A rung that does not
    /// say its size is allowed, since refusing it could leave nothing to play;
    /// the ceiling stops below the first rung that is too tall, wherever the
    /// list puts it; and where even the first is too tall, that one is
    /// allowed, because somebody who asked for less is better served by the
    /// least there is than by a player with nothing it may play.
    public func cap(_ levels: [Level]) -> Int? {
        guard let lines else { return nil }
        let most = Double(lines + 8)
        guard let tooTall = levels.firstIndex(where: { level in
            let side = level.width.map { $0 > 0 ? min($0, level.height) : level.height } ?? level.height
            return side > most
        }) else { return nil }
        return max(0, tooTall - 1)
    }

    /// What a player's own ceiling should become (`capping`): lowered to the
    /// viewer's, never raised. The player may already hold the picture below
    /// it for its own reasons -- sized to the tile -- and that ceiling is the
    /// deployment's bandwidth, which this choice may only save more of.
    public func capping(_ current: Int?, _ levels: [Level]) -> Int? {
        guard let most = cap(levels) else { return current }
        guard let current else { return most }
        return min(current, most)
    }
}
