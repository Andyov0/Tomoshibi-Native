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
    /// A share being started, changed or stopped: its control shows it, and
    /// a second start is not taken.
    ///
    /// Never the picker itself. The busy mark used to be held across it, and
    /// the controls took no press while it was out; but a picker the system
    /// declined to show never answers, and a second press is the only thing
    /// that releases it (ScreenCapture.pick), so the share controls stayed
    /// busy for the rest of the call.
    public var screenBusy: Bool { screenWork > 0 }
    public var soundBusy: Bool { soundWork > 0 }
    private var screenWork = 0
    private var soundWork = 0

    /// Moved on by every stop of the screen (the person's, the system's,
    /// leaving), and of the sound: whatever was under way before it is no
    /// longer wanted. A start part way through its publishing, a change of
    /// size waiting on the network or a pick answered late sees the move
    /// when it next wakes and takes back what it made, rather than finishing
    /// into a share that was stopped or a call that was left. A stop never
    /// waits for any of them, so leaving never hangs behind the network.
    @ObservationIgnored private var screenEpoch = 0
    @ObservationIgnored private var soundEpoch = 0
    /// Changes of a running share's size, one at a time (Turns): the menu's
    /// and the uplink card's at once each took down the track they found,
    /// and the one published in between stayed up with nothing feeding it.
    @ObservationIgnored private let changes = Turns()

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

    /// Where the choices below are kept (CallModel.memory): under the web's
    /// names (room.ts `QUALITY_KEY`, `RATE_KEY`, sound.ts `LOSSLESS_KEY`) for
    /// a real call, nowhere for a rehearsal, whose share settings are no
    /// business of the owner's next call.
    @ObservationIgnored private var store: UserDefaults?
    @ObservationIgnored private var loading = false
    static let qualityKey = "meet.share.quality"
    static let rateKey = "meet-live.share-frame-rate"
    static let losslessKey = "meet-live.lossless"

    /// The size chosen in the menu, kept between calls.
    public var quality: ScreenCapture.Quality = .auto {
        didSet { if !loading { store?.set(quality.rawValue, forKey: Self.qualityKey) } }
    }
    /// The rate chosen in the menu, kept between calls. It may be one the
    /// size cannot do (sixty chosen at 1080p, then 4K); `rate(for:)` is what
    /// is sent.
    public var fps: Int = 60 {
        didSet { if !loading { store?.set(fps, forKey: Self.rateKey) } }
    }
    /// Whether the shared thing's sound goes too. On by default: a share of a
    /// video with no sound is the complaint that started all of this.
    public var withSound = true
    /// Whether a share of sound alone goes losslessly as well, under the web's
    /// own storage name: on unless turned off (sound.ts `rememberedLossless`).
    public var lossless = true {
        didSet { if !loading { store?.set(lossless ? nil : "off", forKey: Self.losslessKey) } }
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
    private var shared: ScreenCapture.Picked?

    static let log = Logger(subsystem: "app.tomoshibi", category: "share")

    public init() {
        screen.onStopped = { [weak self] error in
            Self.log.notice("the system ended the screen share: \(error.localizedDescription, privacy: .public)")
            Task { await self?.stopScreen() }
        }
        soundCapture.onStopped = { [weak self] error in
            Self.log.notice("the system ended the sound share: \(error.localizedDescription, privacy: .public)")
            Task { await self?.stopSound() }
        }
    }

    /// Reads the menu's choices from where this call keeps them, and keeps
    /// them there from now on; nil starts from the web's defaults and keeps
    /// nothing.
    func keep(in store: UserDefaults?) {
        self.store = store
        loading = true
        defer { loading = false }
        quality = store.flatMap { ScreenCapture.Quality(rawValue: $0.integer(forKey: Self.qualityKey)) } ?? .auto
        let remembered = store?.integer(forKey: Self.rateKey) ?? 0
        fps = remembered > 0 ? remembered : 60
        lossless = store?.string(forKey: Self.losslessKey) != "off"
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

    /// The share is not there any more: stopped while a change to it was
    /// waiting its turn or on the network.
    public struct Gone: Error {}

    // MARK: The screen

    /// Asks the person what to share and starts sending it.
    ///
    /// A press while the picker is out asks again, which releases a picker
    /// the system never showed; a press while a share is being published or
    /// runs is not taken (the web's `guard`, ControlBar.tsx).
    public func start(on call: Call) async throws {
        guard !active, !screenBusy else { return }
        let epoch = screenEpoch
        guard let picked = await ScreenCapture.pick() else {
            Self.log.notice("nothing picked")
            return
        }
        guard epoch == screenEpoch, !active, !screenBusy else {
            Self.log.notice("a pick answered after the share was called off; nothing is shared")
            return
        }
        screenWork += 1
        defer { screenWork -= 1 }
        let setting = Setting(quality: quality, fps: rate(for: quality))
        let size = ScreenCapture.size(for: picked, quality: setting.quality)
        let video = try await call.publishVideo(kind: .screen(fps: UInt32(setting.fps), quality: setting.quality.share),
                                                width: UInt32(size.width), height: UInt32(size.height))
        guard epoch == screenEpoch else {
            Self.log.notice("the share was called off while it was published; taking it back")
            try? await video.unpublish()
            return
        }
        // Its own sound unless sound is already being shared on its own,
        // which carries on and is not doubled (room.ts `alongside`).
        let sound: AudioInput?
        do {
            sound = withSound && !listening ? try await call.publishAudio(kind: .screenSound) : nil
        } catch {
            try? await video.unpublish()
            throw error
        }
        guard epoch == screenEpoch else {
            Self.log.notice("the share was called off while its sound was published; taking it back")
            try? await video.unpublish()
            if let sound { try? await sound.unpublish() }
            return
        }
        do {
            try await capture(picked, size: size, fps: setting.fps, video: video, sound: sound)
        } catch {
            Self.log.error("the share would not start: \(String(describing: error), privacy: .public)")
            try? await video.unpublish()
            if let sound { try? await sound.unpublish() }
            throw error
        }
        guard epoch == screenEpoch else {
            // The capture had not started when the stop came, so the stop
            // had nothing to end; it is ended here, or Control Center goes on
            // counting it and refuses the next picker.
            Self.log.notice("the share was called off while its capture started; stopping it")
            await screen.stop()
            try? await video.unpublish()
            if let sound { try? await sound.unpublish() }
            return
        }
        self.call = call
        self.video = video
        self.screenSound = sound
        shared = picked
        running = setting
        active = true
        Self.log.notice("sharing the screen at \(setting.quality.rawValue, privacy: .public) and \(setting.fps, privacy: .public) fps, with its sound \(sound != nil, privacy: .public)")
        // Sound shared on its own may have started, or stopped, while this
        // was being published.
        await soundStandsDown()
        await soundBack()
    }

    /// The running share at another setting, from the menu: remembered, as a
    /// choice made there is, and applied at once where a share is running
    /// (ShareButton.tsx: a setting that takes effect on the next share is one
    /// nobody can judge).
    public func choose(quality: ScreenCapture.Quality, fps: Int? = nil) async throws {
        self.quality = quality
        if let fps { self.fps = fps }
        guard active else { return }
        do {
            try await retune(Setting(quality: quality, fps: rate(for: quality, wanted: fps ?? shown.fps)))
        } catch is Gone {
            // Stopped under the change: the choice is remembered, and there
            // is nothing left to apply it to.
        }
    }

    /// The uplink card's switch (StrainCard): applied to the running share
    /// and not remembered (strain.ts: the next share starts from what was
    /// chosen in the menu). Throws `Gone` where the share stopped first, so
    /// the card does not tell the core a switch took that never happened.
    public func restart(quality: ScreenCapture.Quality, fps: Int, on call: Call) async throws {
        guard active else { throw Gone() }
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
    ///
    /// One at a time, and given up where the share stops meanwhile (the
    /// system's Stop, the person's): a change that finished into a stopped
    /// share set the new track as the running one on a share that was gone,
    /// and one that overlapped another left a track published that nothing
    /// fed, for everybody, until the call ended.
    private func retune(_ setting: Setting) async throws {
        try await changes.take {
            let epoch = screenEpoch
            guard active, let call, let shared, let old = video else { throw Gone() }
            guard setting != running else { return }
            screenWork += 1
            defer { screenWork -= 1 }
            let size = ScreenCapture.size(for: shared, quality: setting.quality)
            let fresh = try await call.publishVideo(kind: .screen(fps: UInt32(setting.fps), quality: setting.quality.share),
                                                    width: UInt32(size.width), height: UInt32(size.height))
            guard epoch == screenEpoch else {
                // The stop took the old track down; this one is nobody's.
                try? await fresh.unpublish()
                throw Gone()
            }
            do {
                try await recapture(shared, size: size, fps: setting.fps, video: fresh)
            } catch {
                Self.log.error("the share would not change: \(String(describing: error), privacy: .public)")
                try? await fresh.unpublish()
                throw error
            }
            guard epoch == screenEpoch else {
                try? await fresh.unpublish()
                throw Gone()
            }
            video = fresh
            running = setting
            try? await old.unpublish()
            Self.log.notice("share now at \(setting.quality.rawValue, privacy: .public) and \(setting.fps, privacy: .public) fps")
        }
    }

    /// Stops the screen at once, whatever is under way: what it stops is
    /// taken off the share before anything is awaited, so a change or a start
    /// that wakes after finds nothing of its own to carry on with.
    public func stopScreen() async {
        screenEpoch += 1
        let video = self.video
        let screenSound = self.screenSound
        self.video = nil
        self.screenSound = nil
        shared = nil
        running = nil
        active = false
        screenWork += 1
        defer { screenWork -= 1 }
        await screen.stop()
        if let video { try? await video.unpublish() }
        if let screenSound { try? await screenSound.unpublish() }
    }

    private func capture(_ picked: ScreenCapture.Picked, size: (width: Int, height: Int), fps: Int,
                         video: VideoInput, sound: AudioInput?) async throws {
        switch picked {
        case let .content(filter):
            try await screen.start(filter: filter, width: size.width, height: size.height, fps: fps,
                                   video: video, sound: sound, hearing: withSound)
        #if DEBUG
        case .pretend:
            try await ControlDebug.pretendCapture()
        #endif
        }
    }

    private func recapture(_ picked: ScreenCapture.Picked, size: (width: Int, height: Int), fps: Int, video: VideoInput) async throws {
        switch picked {
        case .content:
            try await screen.retune(width: size.width, height: size.height, fps: fps, video: video)
        #if DEBUG
        case .pretend:
            try await ControlDebug.pretendCapture()
        #endif
        }
    }

    // MARK: Sound on its own

    /// A share of sound alone (the web's "Share only sound", sound.ts
    /// `startListening`): 510 kb/s stereo on the share's sound channel, named
    /// "listening", with no picture. Taken and called off as the screen's
    /// start is.
    public func startSoundOnly(on call: Call, sealed: Bool) async throws {
        guard !listening, !soundBusy else { return }
        let epoch = soundEpoch
        guard let picked = await ScreenCapture.pick() else {
            Self.log.notice("nothing picked")
            return
        }
        guard epoch == soundEpoch, !listening, !soundBusy else {
            Self.log.notice("a pick answered after the sound share was called off; nothing is shared")
            return
        }
        soundWork += 1
        defer { soundWork -= 1 }
        // Never in a sealed call: the stream is data, which crosses the relay
        // in the clear, in a call whose promise is that the relay hears nothing.
        let offered = soundLosslessOffered(wanted: lossless, encrypted: sealed, able: true)
        let sound = try await call.publishAudio(kind: .listening(losslessOffered: offered))
        guard epoch == soundEpoch else {
            Self.log.notice("the sound share was called off while it was published; taking it back")
            try? await sound.unpublish()
            return
        }
        do {
            switch picked {
            case let .content(filter):
                try await soundCapture.start(filter: filter, width: 2, height: 2, fps: 1, video: nil, sound: sound)
            #if DEBUG
            case .pretend:
                try await ControlDebug.pretendCapture()
            #endif
            }
        } catch {
            Self.log.error("the sound share would not start: \(String(describing: error), privacy: .public)")
            try? await sound.unpublish()
            throw error
        }
        guard epoch == soundEpoch else {
            Self.log.notice("the sound share was called off while its capture started; stopping it")
            await soundCapture.stop()
            try? await sound.unpublish()
            return
        }
        self.sound = sound
        listening = true
        sendingLossless = offered
        Self.log.notice("sharing sound, lossless offered \(offered, privacy: .public)")
        await soundStandsDown()
    }

    public func stopSound() async {
        soundEpoch += 1
        let sound = self.sound
        self.sound = nil
        listening = false
        sendingLossless = false
        soundWork += 1
        defer { soundWork -= 1 }
        await soundCapture.stop()
        if let sound { try? await sound.unpublish() }
        await soundBack()
    }

    /// The screen's own sound stands down while sound shared on its own
    /// carries the sound: the two would be one song twice a beat apart.
    private func soundStandsDown() async {
        guard listening, let screenSound else { return }
        screen.send(sound: nil)
        self.screenSound = nil
        try? await screenSound.unpublish()
    }

    /// And the screen, still shared, has its sound back once sound is no
    /// longer shared on its own. Whatever changed while it was published
    /// (the screen stopped, sound shared again, another give-back first)
    /// takes it straight back down.
    private func soundBack() async {
        guard active, withSound, !listening, screenSound == nil, let call else { return }
        let epoch = screenEpoch
        guard let back = try? await call.publishAudio(kind: .screenSound) else { return }
        guard epoch == screenEpoch, active, !listening, screenSound == nil else {
            try? await back.unpublish()
            return
        }
        screenSound = back
        screen.send(sound: back)
    }

    /// Everything stopped: leaving, or the call ending under it. A picker
    /// still out is taken down with it, so nothing picked afterwards is
    /// shared into a call that has gone.
    public func stop() async {
        ScreenCapture.dismissPicker()
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

    /// TOMOSHIBI_DEBUG_PRESS's `systemstop`: the Stop in the menu bar's video
    /// menu, through the same path the system's own takes.
    func rehearseSystemStop() {
        screen.onStopped?(CocoaError(.userCancelled))
    }

    /// What the share is doing, for a rehearsal's log.
    var rehearsalState: String {
        "screen \(active ? "on" : "off") busy \(screenBusy) running \(running.map { "\($0.quality.rawValue)@\($0.fps)" } ?? "-"), sound \(listening ? "on" : "off") busy \(soundBusy)"
    }
    #endif
}
#endif
