import AVFoundation
import Foundation
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// Switches for driving the song desk without a pointer, read from the
/// environment, which is how the interop checks run the app. None of them is
/// set in an ordinary launch, and then none of this does anything.
///
/// - `TOMOSHIBI_DEBUG_MUSIC_ACCOUNT=name:passphrase` signs the deployment in
///   before the libraries are asked for: the library is account-gated, and an
///   app taken straight into a call by `TOMOSHIBI_AUTOJOIN` never passed the
///   lobby where one signs in.
/// - `TOMOSHIBI_DEBUG_MUSIC_ASK=first|second` asks for each, in turn, as if
///   typed into the desk's box.
/// - `TOMOSHIBI_DEBUG_MUSIC_WINDOW=1280x800` sizes the window for pictures.
/// - `TOMOSHIBI_DEBUG_MUSIC_TAB=desk|search|playlist` chooses the panel's tab,
///   and `TOMOSHIBI_DEBUG_MUSIC_CLICK=x,y` clicks that point of the window
///   (points from its top left; macOS), which is how the checks open the
///   panel from the control bar.
/// - `TOMOSHIBI_DEBUG_MUSIC_SEARCH=words` and `TOMOSHIBI_DEBUG_MUSIC_LINK=text`
///   fill in the search and playlist tabs.
/// - `TOMOSHIBI_DEBUG_MUSIC_SCRIPT=6:pause;9:play;12:skip` presses the
///   holder's controls, or a listener's vote, at so many seconds after the
///   asks (pause, play, skip, vote, close, top, drop, dropnext, monitor50,
///   noduck, duck, original, processed; and a listener's volume30,
///   volume100, mute, unmute for the holder's music, and soundpanel50, the
///   same setting written as the sound panel writes it).
/// - `TOMOSHIBI_DEBUG_MUSIC_NO_VOICE_PROCESSING=1` and
///   `TOMOSHIBI_DEBUG_MUSIC_METER_MONITOR=1`: see `prepareEngine` and
///   `meterMonitor` below.
/// - `TOMOSHIBI_DEBUG_MUSIC_LOG=1` writes what the desk does, and how far the
///   music is turned down, to standard error.
/// - `TOMOSHIBI_DEBUG_MUSIC_METER_MIC=1` listens to the microphone for the
///   duck whenever there is music, on or off, to prove the input can be
///   listened to and let go of while the engine runs.
/// - `TOMOSHIBI_DEBUG_MUSIC_FAKE_LOSSLESS=1` feeds the lossless player, for
///   whoever's music arrives, a tone of its own (1 kHz, half scale, 44.1 kHz)
///   as if the core had handed it over, and logs how loud it comes out ten
///   times a second for its first three seconds and once a second after: the
///   listener's volume, the duck and the fade-in measured on the sound itself,
///   whatever becomes of the real stream on the way.
/// - `TOMOSHIBI_DEBUG_MUSIC_FILES=<directory>` stands a library in for the
///   deployment's, for a deployment that has none: one library called
///   "Files", whose search finds the files there by name (a `*` finds them
///   all) and whose songs are those files, decoded as a fetched track is,
///   first bytes and all. Nothing about what the desk says to the room or
///   what is published changes; only where the song comes from.
@MainActor
final class MusicDebug {
    private let environment = ProcessInfo.processInfo.environment

    nonisolated static let filesSource = "files"

    /// The directory a test's library is read from, if one is.
    var files: URL? {
        environment["TOMOSHIBI_DEBUG_MUSIC_FILES"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    /// The stand-in library, where there is one.
    var fileLibraries: [MusicLibrary]? {
        files.map { _ in [MusicLibrary(id: Self.filesSource, name: "Files", signedIn: true)] }
    }

    /// The files whose names hold the words, as a library's hits.
    nonisolated static func search(_ query: String, in directory: URL) -> [TomoshibiCore.MusicTrack] {
        let names = ((try? FileManager.default.contentsOfDirectory(atPath: directory.path)) ?? []).sorted()
        let words = query.trimmingCharacters(in: .whitespaces).lowercased()
        return names.filter { !$0.hasPrefix(".") && (words == "*" || $0.lowercased().contains(words)) }.map { name in
            let length = (try? AVAudioFile(forReading: directory.appendingPathComponent(name)))
                .map { Double($0.length) / $0.processingFormat.sampleRate } ?? 0
            return TomoshibiCore.MusicTrack(id: name, title: (name as NSString).deletingPathExtension, artists: ["Test files"],
                              album: (name as NSString).pathExtension.uppercased(), cover: "", duration: length)
        }
    }

    /// One of them, as a fetched song is decoded: copied to a name that says
    /// nothing of what it is, as a download arrives, and named by its bytes.
    nonisolated static func load(_ name: String, in directory: URL, keepOriginal: Bool) async throws -> MusicLoader.Outcome {
        let source = directory.appendingPathComponent(name)
        let fetched = FileManager.default.temporaryDirectory.appendingPathComponent("tomoshibi-fetched-\(UUID().uuidString).tmp")
        try FileManager.default.copyItem(at: source, to: fetched)
        defer { try? FileManager.default.removeItem(at: fetched) }
        let decoded = try MusicLoader.decodeFile(at: fetched, said: nil, mime: nil, keepOriginal: keepOriginal)
        return .ready(DecodedSong(samples: decoded.samples, original: decoded.original, rate: decoded.rate,
                                  quality: MusicLoader.describe(decoded)))
    }

    var fakeLossless: Bool { environment["TOMOSHIBI_DEBUG_MUSIC_FAKE_LOSSLESS"] != nil }
    private var feeding: Set<String> = []

    func feedLossless(_ call: CallModel, from identity: String) {
        guard fakeLossless, !feeding.contains(identity) else { return }
        feeding.insert(identity)
        let sink = call.lossless
        let thread = Thread {
            let rate = 44_100.0
            var phase = 0.0
            var next = DispatchTime.now().uptimeNanoseconds
            while true {
                var block = [Float](repeating: 0, count: 441 * 2)
                for frame in 0..<441 {
                    let sample = Float(0.5 * sin(phase))
                    block[frame * 2] = sample
                    block[frame * 2 + 1] = sample
                    phase += 2 * .pi * 1000 / rate
                }
                sink.onLossless(identity: identity, music: true, samples: block, rate: 44_100, channels: 2)
                next += 10_000_000
                let wait = Int64(next) - Int64(DispatchTime.now().uptimeNanoseconds)
                if wait > 0 { Thread.sleep(forTimeInterval: Double(wait) / 1e9) }
            }
        }
        thread.name = "Tomoshibi fake lossless"
        thread.start()
        Task { @MainActor [weak self] in
            var meter: SoundLevel?
            var metered: ObjectIdentifier?
            for step in 0..<400 {
                try? await Task.sleep(for: .milliseconds(step < 30 ? 100 : 1000))
                let now = call.lossless.stream(identity, music: true)
                if now != metered {
                    metered = now
                    meter = call.lossless.meter(identity, music: true)
                }
                self?.log(String(format: "fake lossless from %@ at %.1f s: rms %.4f", identity, step < 30 ? Double(step) / 10 : 3 + Double(step - 30),
                                 meter?.rms ?? -1))
            }
        }
    }

    /// Every lossless music stream playing here, and how loud it comes out of
    /// its node, after the listener's volume, the duck and the fade-in.
    private var losslessMeters: [String: SoundLevel] = [:]

    func logLossless(_ call: CallModel) {
        guard logging else { return }
        for person in call.people where person.musicTrack != nil {
            let playing = call.lossless.playing(person.id, music: true)
            if playing, !fakeLossless, losslessMeters[person.id] == nil { losslessMeters[person.id] = call.lossless.meter(person.id, music: true) }
            if !playing { losslessMeters[person.id] = nil }
            let level = losslessMeters[person.id].map { String(format: "%.4f", $0.rms) } ?? "-"
            log("lossless from \(person.id): core handed over \(call.lossless.delivered), playing \(playing), level \(call.lossless.intended(person.id, music: true)), rms \(level), opus node \(person.musicTrack.flatMap { call.audio.volume(sid: $0.sid) }.map { String($0) } ?? "-")")
        }
    }

    var logging: Bool { environment["TOMOSHIBI_DEBUG_MUSIC_LOG"] != nil }
    var meterMicrophone: Bool { environment["TOMOSHIBI_DEBUG_MUSIC_METER_MIC"] != nil }

    func log(_ text: @autoclosure () -> String) {
        guard logging else { return }
        FileHandle.standardError.write(Data("[music \(String(format: "%.3f", Date().timeIntervalSince1970))] \(text())\n".utf8))
    }

    /// `TOMOSHIBI_DEBUG_MUSIC_NO_VOICE_PROCESSING=1`: the call's engine without
    /// the system's voice processing, which on a machine that refuses it
    /// (-10875 from the output unit) is the only way the holder's own monitor
    /// plays at all; set before anything else starts the engine.
    func prepareEngine(_ audio: CallAudio) {
        guard environment["TOMOSHIBI_DEBUG_MUSIC_NO_VOICE_PROCESSING"] != nil else { return }
        do {
            try audio.setOriginalSound(true)
            try audio.start()
            log("engine started without voice processing")
        } catch {
            log("engine: \(error)")
        }
    }

    /// `TOMOSHIBI_DEBUG_MUSIC_METER_MONITOR=1` counts the holder's own music
    /// as a microphone, so the duck can be watched moving on a machine with
    /// no microphone to speak into: down while a song plays, back up a hold
    /// and a release after it pauses.
    var meterMonitor: Bool { environment["TOMOSHIBI_DEBUG_MUSIC_METER_MONITOR"] != nil }

    private var counted = 0

    /// True once in `n` calls, for a line now and then from something that
    /// runs many times a second.
    func every(_ n: Int) -> Bool {
        guard logging else { return false }
        counted += 1
        return counted % n == 0
    }

    func signIn(_ deployment: Deployment) async {
        guard let account = environment["TOMOSHIBI_DEBUG_MUSIC_ACCOUNT"], let colon = account.firstIndex(of: ":") else { return }
        do {
            let signed = try await deployment.signIn(name: String(account[..<colon]), passphrase: String(account[account.index(after: colon)...]))
            log("signed in as \(signed.name)")
        } catch {
            log("sign-in failed: \(error)")
        }
    }

    func drive(_ desk: SongDesk) async {
        #if os(macOS)
        if let size = environment["TOMOSHIBI_DEBUG_MUSIC_WINDOW"]?.split(separator: "x").compactMap({ Double($0) }), size.count == 2,
           let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) {
            window.setContentSize(NSSize(width: size[0], height: size[1]))
        }
        #endif
        if let tab = environment["TOMOSHIBI_DEBUG_MUSIC_TAB"] {
            desk.tab = tab == "search" ? .search : tab == "playlist" ? .playlist : .desk
        }
        if let spot = environment["TOMOSHIBI_DEBUG_MUSIC_CLICK"] {
            try? await Task.sleep(for: .seconds(1))
            log("clicked \(spot): \(click(spot))")
        }
        if let query = environment["TOMOSHIBI_DEBUG_MUSIC_SEARCH"] {
            desk.searchQuery = query
            await desk.searchLibrary()
        }
        if let link = environment["TOMOSHIBI_DEBUG_MUSIC_LINK"] {
            desk.pasted = link
            await desk.openLink()
        }
        // Somebody who cannot reach a library asks whoever holds the desk,
        // whose word may not have arrived yet.
        if desk.libraries == nil {
            for _ in 0..<50 where desk.state == nil { try? await Task.sleep(for: .milliseconds(100)) }
        }
        for asked in (environment["TOMOSHIBI_DEBUG_MUSIC_ASK"] ?? "").split(separator: "|") {
            while desk.asking { try? await Task.sleep(for: .milliseconds(100)) }
            log("asking for \(asked)")
            desk.ask(String(asked))
            try? await Task.sleep(for: .milliseconds(1500))
        }
        // The holder's own controls, at so many seconds from here.
        var elapsed = 0.0
        for step in (environment["TOMOSHIBI_DEBUG_MUSIC_SCRIPT"] ?? "").split(separator: ";") {
            let parts = step.split(separator: ":")
            guard parts.count == 2, let at = Double(parts[0]) else { continue }
            if at > elapsed { try? await Task.sleep(for: .milliseconds(Int((at - elapsed) * 1000))) }
            elapsed = max(elapsed, at)
            log("script: \(parts[1])")
            switch parts[1] {
            case "pause": desk.pause(true)
            case "play": desk.pause(false)
            case "skip": desk.skip()
            case "vote": desk.vote()
            case "close": desk.close()
            case "top": if let last = desk.state?.queue.last { desk.toTop(last.key) }
            case "drop": if let first = desk.state?.queue.first { desk.remove(first.key) }
            case "dropnext": if let next = desk.state?.next { desk.remove(next.key) }
            case "monitor50": desk.monitorVolume = 0.5
            // A listener's own setting for the holder's music, as the panel's
            // row sets it.
            case "volume30": if let holder = desk.state?.holder ?? desk.player?.id { desk.setVolume(of: holder, 0.3) }
            case "volume100": if let holder = desk.state?.holder ?? desk.player?.id { desk.setVolume(of: holder, 1) }
            // Written straight into the book, as the sound panel's music row
            // writes it, to show the song desk follows a change made there.
            case "soundpanel50":
                if let holder = desk.state?.holder ?? desk.player?.id {
                    Hearing.shared.set(holder, .music, HearingSetting(volume: 0.5, blocked: false))
                }
            case "mute": if let holder = desk.state?.holder ?? desk.player?.id { desk.setBlocked(of: holder, true) }
            case "unmute": if let holder = desk.state?.holder ?? desk.player?.id { desk.setBlocked(of: holder, false) }
            case "noduck": desk.ducks = false
            case "duck": desk.ducks = true
            // The engine restarted under the microphone's meter, as the
            // original-sound switch restarts it.
            case "original", "processed":
                do {
                    try desk.call.audio.setOriginalSound(parts[1] == "original")
                    log("original sound \(parts[1] == "original"): engine running")
                } catch {
                    log("original sound \(parts[1] == "original"): \(error)")
                }
            default: break
            }
        }
    }

    /// Clicks at a point of this app's window, given in points from its top
    /// left, by handing the window the events a click makes. In this process
    /// only: the pointer does not move and no other app is touched.
    private func click(_ spot: String) -> Bool {
        #if os(macOS)
        let parts = spot.split(separator: ",").compactMap { Double($0) }
        guard parts.count == 2, let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }),
              let content = window.contentView else { return false }
        let point = NSPoint(x: parts[0], y: content.bounds.height - parts[1])
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                 windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)
            else { return false }
            window.sendEvent(event)
        }
        return true
        #else
        return false
        #endif
    }
}
