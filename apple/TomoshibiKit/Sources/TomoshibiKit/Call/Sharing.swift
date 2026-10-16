#if os(macOS)
import Foundation
import Observation
import os
import ScreenCaptureKit
import TomoshibiBasics
import TomoshibiCore

/// Sharing on a Mac: a screen, and sound on its own, each with the system's
/// picker, its own capture and its own tracks.
///
/// They were one control and one capture, so sharing sound to listen to
/// something together meant not sharing a screen, and the owner asked why
/// they were the same thing at all. They are two now and can run at once, as
/// the web allows a screen share on top of sound shared on its own (room.ts
/// `share`). What the web does about the screen's own sound then is kept:
/// while sound is being shared on its own, the screen goes without its own,
/// because the two would be one song twice a beat apart. A browser cannot add
/// sound to a screen capture it already has, so on the web a screen started
/// during a sound share stays silent after the sound share ends; here the
/// capture keeps hearing, and the screen takes its sound back when the sound
/// share stops, and gives it up when one starts.
///
/// The settings are the web's share menu (room.ts ~39-63, ShareButton.tsx): a
/// size and a frame rate, remembered between calls under the web's own
/// storage names, and the lossless switch (sound.ts `rememberedLossless`).
@MainActor
@Observable
public final class Sharing {
    /// Whether the screen is being shared.
    public private(set) var active = false
    /// Whether sound is being shared on its own (the web's "listening").
    public private(set) var listening = false
    /// Whether the sound being shared on its own was offered losslessly,
    /// which the sound control says while it runs (ShareButton.tsx: the
    /// switch is not shown mid-share, so what is going out is said instead).
    public private(set) var sendingLossless = false
    /// A share being started or stopped: its control shows it and does not
    /// take a second press.
    public private(set) var screenBusy = false
    public private(set) var soundBusy = false

    /// One setting of a screen share: a size and a rate.
    public struct Setting: Equatable, Sendable {
        public var quality: ScreenCapture.Quality
        public var fps: Int
    }

    /// What the running share is sending (room.ts `runningShare`), which the
    /// menu shows while the share runs: the uplink card can change it without
    /// the menu, and a menu ticking the old one would describe a share nobody
    /// is sending. Not the remembered choice either; what one afternoon's
    /// line could carry is not a thing to keep.
    public private(set) var running: Setting?

    /// Whether the choices below are kept: not in a rehearsal, whose share
    /// settings are no business of the owner's next call (CallModel.memory).
    @ObservationIgnored var remembers = true

    /// The size chosen in the menu, kept between calls.
    public var quality: ScreenCapture.Quality {
        didSet { if remembers { UserDefaults.standard.set(quality.rawValue, forKey: "meet.share.quality") } }
    }
    /// The rate chosen in the menu, kept between calls. It may be one the
    /// size cannot do (sixty chosen at 1080p, then 4K); `rate(for:)` is what
    /// is sent.
    public var fps: Int {
        didSet { if remembers { UserDefaults.standard.set(fps, forKey: "meet-live.share-frame-rate") } }
    }
    /// Whether the shared thing's sound goes too. On by default: a share of a
    /// video with no sound is the complaint that started all of this.
    public var withSound = true
    /// Whether a share of sound alone goes losslessly as well, under the web's
    /// own storage name: on unless turned off (sound.ts `rememberedLossless`).
    public var lossless: Bool = UserDefaults.standard.string(forKey: "meet-live.lossless") != "off" {
        didSet { if remembers { UserDefaults.standard.set(lossless ? nil : "off", forKey: "meet-live.lossless") } }
    }

    private let screen = ScreenCapture()
    private let soundCapture = ScreenCapture()
    /// The call the screen was shared into, for giving it its sound back.
    private var call: Call?
    private var video: VideoInput?
    /// The screen's own sound, while it has one.
    private var screenSound: AudioInput?
    /// Sound shared on its own.
    private var sound: AudioInput?
    private var shared: SCContentFilter?

    static let log = Logger(subsystem: "app.tomoshibi", category: "share")

    public init() {
        quality = ScreenCapture.Quality(rawValue: UserDefaults.standard.integer(forKey: "meet.share.quality")) ?? .auto
        let remembered = UserDefaults.standard.integer(forKey: "meet-live.share-frame-rate")
        fps = remembered > 0 ? remembered : 60
        screen.onStopped = { [weak self] error in
            Self.log.notice("the system ended the screen share: \(error.localizedDescription, privacy: .public)")
            Task { await self?.stopScreen() }
        }
        soundCapture.onStopped = { [weak self] error in
            Self.log.notice("the system ended the sound share: \(error.localizedDescription, privacy: .public)")
            Task { await self?.stopSound() }
        }
    }

    /// The frame rates each size allows (room.ts `RATES_FOR`).
    public static func rates(for quality: ScreenCapture.Quality) -> [Int] {
        ShareRates.allowed(shorterSide: quality.rawValue)
    }

    /// The rate that goes out for `quality` (ShareButton.tsx `clamp`): the
    /// one chosen where the size allows it, and the size's highest where it
    /// does not.
    public func rate(for quality: ScreenCapture.Quality, wanted: Int? = nil) -> Int {
        ShareRates.clamp(wanted ?? fps, shorterSide: quality.rawValue)
    }

    /// What the menu shows: the running share's setting while there is one,
    /// otherwise the remembered choice at a rate its size can do.
    public var shown: Setting {
        running ?? Setting(quality: quality, fps: rate(for: quality))
    }

    // MARK: The screen

    /// Asks the person what to share and starts sending it.
    public func start(on call: Call) async throws {
        guard !screenBusy, !active else { return }
        screenBusy = true
        defer { screenBusy = false }
        guard let filter = await ScreenCapture.pick() else {
            Self.log.notice("nothing picked")
            return
        }
        let setting = Setting(quality: quality, fps: rate(for: quality))
        let size = ScreenCapture.size(for: filter, quality: setting.quality)
        let video = try await call.publishVideo(kind: .screen(fps: UInt32(setting.fps), quality: setting.quality.share),
                                                width: UInt32(size.width), height: UInt32(size.height))
        // Its own sound unless sound is already being shared on its own,
        // which carries on and is not doubled (room.ts `alongside`).
        let sound: AudioInput?
        do {
            sound = withSound && !listening ? try await call.publishAudio(kind: .screenSound) : nil
        } catch {
            try? await video.unpublish()
            throw error
        }
        do {
            try await screen.start(filter: filter, width: size.width, height: size.height, fps: setting.fps,
                                   video: video, sound: sound, hearing: withSound)
        } catch {
            Self.log.error("the share would not start: \(String(describing: error), privacy: .public)")
            try? await video.unpublish()
            if let sound { try? await sound.unpublish() }
            throw error
        }
        self.call = call
        self.video = video
        self.screenSound = sound
        shared = filter
        running = setting
        active = true
        Self.log.notice("sharing the screen at \(setting.quality.rawValue, privacy: .public) and \(setting.fps, privacy: .public) fps, with its sound \(sound != nil, privacy: .public)")
    }

    /// The running share at another setting, from the menu: remembered, as a
    /// choice made there is, and applied at once where a share is running
    /// (ShareButton.tsx: a setting that takes effect on the next share is one
    /// nobody can judge).
    public func choose(quality: ScreenCapture.Quality, fps: Int? = nil) async throws {
        self.quality = quality
        if let fps { self.fps = fps }
        guard active else { return }
        try await retune(Setting(quality: quality, fps: rate(for: quality, wanted: fps ?? shown.fps)))
    }

    /// The uplink card's switch (StrainCard): applied to the running share
    /// and not remembered (strain.ts: the next share starts from what was
    /// chosen in the menu).
    public func restart(quality: ScreenCapture.Quality, fps: Int, on call: Call) async throws {
        guard active else { return }
        try await retune(Setting(quality: quality, fps: rate(for: quality, wanted: fps)))
    }

    /// The same capture at another size and rate, with a picture track of its
    /// own.
    ///
    /// The web retunes the track it has (room.ts `retune`): the capture's
    /// constraints and the sender's encodings are changed in place. The core
    /// has no such call on a video input: its publish fixes the encodings and
    /// the simulcast ladder for the size and rate it was given. So the picture
    /// is published again at the new setting and the old one is taken down
    /// once the capture feeds the new one, in that order so the share never
    /// leaves the stage for those who watch it. The capture itself is changed
    /// in place, nothing is picked again, and the sound track is untouched:
    /// only the picture restarts.
    private func retune(_ setting: Setting) async throws {
        guard let call, let shared, let old = video, setting != running else { return }
        screenBusy = true
        defer { screenBusy = false }
        let size = ScreenCapture.size(for: shared, quality: setting.quality)
        let fresh = try await call.publishVideo(kind: .screen(fps: UInt32(setting.fps), quality: setting.quality.share),
                                                width: UInt32(size.width), height: UInt32(size.height))
        do {
            try await screen.retune(width: size.width, height: size.height, fps: setting.fps, video: fresh)
        } catch {
            Self.log.error("the share would not change: \(String(describing: error), privacy: .public)")
            try? await fresh.unpublish()
            throw error
        }
        video = fresh
        running = setting
        try? await old.unpublish()
        Self.log.notice("share now at \(setting.quality.rawValue, privacy: .public) and \(setting.fps, privacy: .public) fps")
    }

    public func stopScreen() async {
        screenBusy = true
        defer { screenBusy = false }
        await screen.stop()
        if let video { try? await video.unpublish() }
        if let screenSound { try? await screenSound.unpublish() }
        video = nil
        screenSound = nil
        shared = nil
        running = nil
        active = false
    }

    // MARK: Sound on its own

    /// A share of sound alone (the web's "Share only sound", sound.ts
    /// `startListening`): 510 kb/s stereo on the share's sound channel, named
    /// "listening", with no picture.
    public func startSoundOnly(on call: Call, sealed: Bool) async throws {
        guard !soundBusy, !listening else { return }
        soundBusy = true
        defer { soundBusy = false }
        guard let filter = await ScreenCapture.pick() else {
            Self.log.notice("nothing picked")
            return
        }
        // Never in a sealed call: the stream is data, which crosses the relay
        // in the clear, in a call whose promise is that the relay hears nothing.
        let offered = soundLosslessOffered(wanted: lossless, encrypted: sealed, able: true)
        let sound = try await call.publishAudio(kind: .listening(losslessOffered: offered))
        do {
            try await soundCapture.start(filter: filter, width: 2, height: 2, fps: 1, video: nil, sound: sound)
        } catch {
            Self.log.error("the sound share would not start: \(String(describing: error), privacy: .public)")
            try? await sound.unpublish()
            throw error
        }
        self.sound = sound
        listening = true
        sendingLossless = offered
        // The screen's own sound stands down while this carries the sound.
        if let screenSound {
            screen.send(sound: nil)
            self.screenSound = nil
            try? await screenSound.unpublish()
        }
        Self.log.notice("sharing sound, lossless offered \(offered, privacy: .public)")
    }

    public func stopSound() async {
        soundBusy = true
        defer { soundBusy = false }
        await soundCapture.stop()
        if let sound { try? await sound.unpublish() }
        sound = nil
        listening = false
        sendingLossless = false
        // And the screen, still shared, has its sound back.
        if active, withSound, screenSound == nil, let call {
            if let back = try? await call.publishAudio(kind: .screenSound) {
                screenSound = back
                screen.send(sound: back)
            }
        }
    }

    /// Everything stopped: leaving, or the call ending under it.
    public func stop() async {
        // The screen first, so that stopping the sound does not hand the
        // screen its sound back on the way out.
        await stopScreen()
        await stopSound()
        call = nil
    }

    #if DEBUG
    /// TOMOSHIBI_DEBUG_SHARING, in a rehearsal: the controls as they are
    /// while sharing, with nothing captured. The system's picker cannot be
    /// answered by a script, and a capture without it needs a permission a
    /// test should not ask for.
    func pretend(screen: Bool, sound: Bool, lossless: Bool) {
        active = screen
        listening = sound
        sendingLossless = sound && lossless
        running = screen ? Setting(quality: quality, fps: rate(for: quality)) : nil
    }
    #endif
}
#endif
