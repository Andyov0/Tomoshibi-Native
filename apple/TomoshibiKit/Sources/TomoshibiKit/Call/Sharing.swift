#if os(macOS)
import Foundation
import Observation
import ScreenCaptureKit
import TomoshibiCore

/// Sharing on a Mac: the picker, the capture, and the two tracks it feeds.
///
/// The settings are the web's share menu (room.ts ~39-63, ~1139-1191): a size
/// (auto, 1080p, 1440p, 4K) and a frame rate, remembered between calls under
/// the web's own storage names so a person who chose sixty once keeps sixty.
@MainActor
@Observable
public final class Sharing {
    public private(set) var active = false
    public var quality: ScreenCapture.Quality {
        didSet { UserDefaults.standard.set(quality.rawValue, forKey: "meet.share.quality") }
    }
    public var fps: Int {
        didSet { UserDefaults.standard.set(fps, forKey: "meet-live.share-frame-rate") }
    }
    /// Whether the shared thing's sound goes too. On by default: a share of a
    /// video with no sound is the complaint that started all of this.
    public var withSound = true
    /// Whether a share of sound alone goes losslessly as well, under the web's
    /// own storage name: on unless turned off (sound.ts `rememberedLossless`).
    public var lossless: Bool = UserDefaults.standard.string(forKey: "meet-live.lossless") != "off" {
        didSet { UserDefaults.standard.set(lossless ? nil : "off", forKey: "meet-live.lossless") }
    }

    private let capture = ScreenCapture()
    /// What was picked, kept so a change of setting does not ask again.
    private var filter: SCContentFilter?
    private var video: VideoInput?
    private var sound: AudioInput?

    public init() {
        quality = ScreenCapture.Quality(rawValue: UserDefaults.standard.integer(forKey: "meet.share.quality")) ?? .auto
        let remembered = UserDefaults.standard.integer(forKey: "meet-live.share-frame-rate")
        fps = remembered > 0 ? remembered : 60
    }

    /// The frame rates each size allows (room.ts ~59-63).
    public static func rates(for quality: ScreenCapture.Quality) -> [Int] {
        switch quality {
        case .auto: return [60]
        case .p1080: return [15, 30, 60, 120, 240]
        case .p1440: return [15, 30, 60, 120]
        case .p2160: return [15, 30, 60]
        }
    }

    /// Asks the person what to share and starts sending it.
    public func start(on call: Call) async throws {
        guard let filter = await capture.pick() else { return }
        try await begin(filter, on: call)
    }

    /// The same thing shared again at another setting: the uplink card's
    /// switch, and the menu while a share is running.
    public func restart(quality: ScreenCapture.Quality, fps: Int, on call: Call) async throws {
        guard let filter else { return }
        self.quality = quality
        self.fps = fps
        await capture.stop()
        if let video { try? await video.unpublish() }
        if let sound { try? await sound.unpublish() }
        video = nil
        sound = nil
        try await begin(filter, on: call)
    }

    private func begin(_ filter: SCContentFilter, on call: Call) async throws {
        self.filter = filter
        let rate = quality == .auto ? 60 : (Self.rates(for: quality).contains(fps) ? fps : 60)
        let size = ScreenCapture.size(for: filter, quality: quality)
        let video = try await call.publishVideo(kind: .screen(fps: UInt32(rate), quality: quality.share), width: UInt32(size.width), height: UInt32(size.height))
        let sound = withSound ? try await call.publishAudio(kind: .screenSound) : nil
        do {
            try await capture.start(filter: filter, width: size.width, height: size.height, fps: rate, video: video, sound: sound)
        } catch {
            try? await video.unpublish()
            if let sound { try? await sound.unpublish() }
            throw error
        }
        self.video = video
        self.sound = sound
        active = true
    }

    /// A share of sound alone (the web's "Share only sound", sound.ts
    /// `startListening`): 510 kb/s stereo on the share's sound channel, named
    /// "listening", with no picture.
    public private(set) var listening = false

    public func startSoundOnly(on call: Call, sealed: Bool) async throws {
        guard let filter = await capture.pick() else { return }
        // Never in a sealed call: the stream is data, which crosses the relay
        // in the clear, in a call whose promise is that the relay hears nothing.
        let offered = soundLosslessOffered(wanted: lossless, encrypted: sealed, able: true)
        let sound = try await call.publishAudio(kind: .listening(losslessOffered: offered))
        do {
            try await capture.start(filter: filter, width: 2, height: 2, fps: 1, video: nil, sound: sound)
        } catch {
            try? await sound.unpublish()
            throw error
        }
        self.sound = sound
        listening = true
        active = true
    }

    public func stop() async {
        await capture.stop()
        if let video { try? await video.unpublish() }
        if let sound { try? await sound.unpublish() }
        video = nil
        sound = nil
        active = false
        listening = false
    }
}
#endif
