import Foundation

/// What the deployment allows a shared screen at most (Remote, the core's
/// api/remote.rs): a rate, a height, and a lower rate while the device is
/// hot. None of it is the web's, and none of it is a setting: each is a
/// ceiling over what the share menu chose, so that the deployment can take
/// a share down a step for a while without a release.
///
/// Ceilings only. The tracks a share publishes keep their names, their
/// simulcast ladder and their encodings' shape, which are the web's
/// (CLAUDE.md): a lower rate is one of the rates the size already offers,
/// as if it had been chosen in the menu, and a lower height is a smaller
/// capture, which the core's ladder is worked out from as it is for a
/// window smaller than its screen.
public struct ShareCeiling: Equatable, Sendable {
    /// Frames a second at most (screen_share_max_fps).
    public var fps: Int?
    /// The captured picture's height at most, in pixels, its width brought
    /// down with it (screen_share_max_height).
    public var height: Int?
    /// Frames a second at most while the device is hot (screen_share_hot_fps).
    public var hotFps: Int?

    public init(fps: Int? = nil, height: Int? = nil, hotFps: Int? = nil) {
        self.fps = fps
        self.height = height
        self.hotFps = hotFps
    }

    /// From the deployment's numbers. A number that is not a positive count
    /// (nought, which the deployment writes for "do not lower", a negative,
    /// a fraction under one) is no ceiling: a share is never asked for no
    /// frames at all because a field was left at zero.
    init(params: [String: Double]) {
        fps = Self.count(params["screen_share_max_fps"])
        height = Self.count(params["screen_share_max_height"])
        hotFps = Self.count(params["screen_share_hot_fps"])
    }

    private static func count(_ number: Double?) -> Int? {
        guard let number, number.isFinite, number >= 1 else { return nil }
        return Int(min(number, 1_000_000).rounded(.down))
    }

    public static let none = ShareCeiling()

    /// The rate a share is published at, for a rate chosen at a size (the
    /// share menu's shorter side, 0 for automatic): the size's own clamp
    /// first (ShareRates), then the highest of its rates under the ceiling,
    /// or its lowest where every one is over it.
    ///
    /// One of the size's rates and never another number: the core takes a
    /// rate a size does not offer for its highest (share.rs `share_profile`),
    /// so a ceiling of 24 handed on as it was would have published 1080p at
    /// 240. Automatic has the one rate, and keeps it; its frames are held to
    /// the ceiling at the capture (`captured`).
    public func published(_ wanted: Int, shorterSide: Int) -> Int {
        let chosen = ShareRates.clamp(wanted, shorterSide: shorterSide)
        guard let fps, fps < chosen else { return chosen }
        let rates = ShareRates.allowed(shorterSide: shorterSide)
        return rates.filter { $0 <= fps }.max() ?? rates.min() ?? chosen
    }

    /// The frames a second the capture is asked for, a share published at
    /// `published`: no more than that, the ceiling, and while the device is
    /// hot the hot ceiling.
    public func captured(_ published: Int, hot: Bool) -> Int {
        max(1, min(published, limit(hot: hot) ?? published))
    }

    /// The lowest of the rates that apply, or none where nothing does: for
    /// a capture that has no rate of its own to start from (the iPhone's
    /// broadcast, which sends what ReplayKit gives).
    public func limit(hot: Bool) -> Int? {
        [fps, hot ? hotFps : nil].compactMap { $0 }.min()
    }

    /// A capture's size under the height, its width scaled with it and both
    /// even, as the encoder needs them (ScreenCapture `size`).
    public func size(width: Int, height: Int) -> (width: Int, height: Int) {
        guard let ceiling = self.height, height > ceiling else { return (width, height) }
        let scale = Double(ceiling) / Double(height)
        return (max(2, Int(Double(width) * scale) & ~1), max(2, ceiling & ~1))
    }

    /// Whether the device is hot enough for the hot ceiling: serious or
    /// critical, where the system itself starts to slow things down (the
    /// same line the room's pictures are drawn at a lower density from,
    /// VideoSurface).
    public static func hot(_ state: ProcessInfo.ThermalState) -> Bool {
        state.rawValue >= ProcessInfo.ThermalState.serious.rawValue
    }

    // MARK: For the iPhone's broadcast

    /// Where the app leaves the ceiling for the broadcast extension, which is
    /// a process of its own and cannot read the app's (BroadcastWire's app
    /// group). Written whenever what the deployment says changes, and on
    /// every start, so a ceiling taken away is gone from here too.
    static let keys = (fps: "tomoshibi.share-ceiling.fps", height: "tomoshibi.share-ceiling.height",
                       hotFps: "tomoshibi.share-ceiling.hot-fps")

    public func keep(in defaults: UserDefaults?) {
        guard let defaults else { return }
        for (key, value) in [(Self.keys.fps, fps), (Self.keys.height, height), (Self.keys.hotFps, hotFps)] {
            if let value { defaults.set(value, forKey: key) } else { defaults.removeObject(forKey: key) }
        }
    }

    public init(kept defaults: UserDefaults?) {
        let read = { (key: String) -> Int? in
            guard let number = defaults?.object(forKey: key) as? NSNumber else { return nil }
            return Self.count(number.doubleValue)
        }
        self.init(fps: read(Self.keys.fps), height: read(Self.keys.height), hotFps: read(Self.keys.hotFps))
    }
}
