import Foundation
import TomoshibiCore

/// Switches for driving watching together from a test, read from the
/// environment once. None of them is set in normal use, and none changes what
/// crosses the call: they stand in for presses a test cannot make (the room
/// view's panel state is not this feature's to open) and report what the
/// player did, so that a run against a browser can be measured.
///
/// - `TOMOSHIBI_DEBUG_WATCH_SIGNIN=name:passphrase` signs the deployment in
///   before asking whether this person may start a show. An app taken straight
///   into a call by `TOMOSHIBI_AUTOJOIN` never signs in, and only somebody
///   signed in can read links and hold a show.
/// - `TOMOSHIBI_DEBUG_WATCH_CONTROL=<file>` is read four times a second for
///   lines appended to it, each one press: `add <link>`, `play`, `pause`,
///   `seek <seconds>`, `skip`, `remove <index in the queue>`, `stop`, `pip`.
/// - `TOMOSHIBI_DEBUG_WATCH_LOG=<file>` gets a line at every keep step: the
///   wall clock in milliseconds, where the player is, whether it plays, its
///   rate, and where the show is. The wall clock because the browser's side
///   is read with `Date.now()` on the same machine.
/// - `TOMOSHIBI_DEBUG_WATCH_PANEL=1` draws the panel over the trailing edge
///   of the stage, so that a picture of the window shows it without a press;
///   `=media` opens it at the media servers.
/// - `TOMOSHIBI_DEBUG_YOUTUBE_API=<url>` loads YouTube's IFrame API from
///   there instead, so a stand-in served locally can exercise the bridge
///   without a single request reaching YouTube.
///
/// - `TOMOSHIBI_DEBUG_WATCH_LINKS=1` stands in for a deployment's watch
///   gateway, for one that has none: this person may start a show, and a
///   link to a file or a playlist on this machine (127.0.0.1 or localhost,
///   nothing else) reads as a video that plays from there; anything else is
///   "not a video". `TOMOSHIBI_DEBUG_WATCH_RELAY=<address>` gives each such
///   video a relay, the address with the link's path after it, standing in
///   for the deployment's own. What crosses the call is unchanged: the show
///   carries the stand-in's ticket as it would the gateway's.
///
/// `TOMOSHIBI_MUTE`, which the interop scripts always set, also silences the
/// shared video: the call's own engine is silenced by it (CallAudio), and a
/// player left out would play a test's video through the person's speakers.
enum WatchDebug {
    private static let environment = ProcessInfo.processInfo.environment

    static let signIn: (name: String, passphrase: String)? = {
        guard let value = environment["TOMOSHIBI_DEBUG_WATCH_SIGNIN"], let colon = value.firstIndex(of: ":") else { return nil }
        return (String(value[..<colon]), String(value[value.index(after: colon)...]))
    }()

    static let panel = environment["TOMOSHIBI_DEBUG_WATCH_PANEL"] != nil
    static let panelAtMedia = environment["TOMOSHIBI_DEBUG_WATCH_PANEL"] == "media"
    static let youtubeAPI = environment["TOMOSHIBI_DEBUG_YOUTUBE_API"]
    static let silenced = environment["TOMOSHIBI_MUTE"] != nil

    static let links = environment["TOMOSHIBI_DEBUG_WATCH_LINKS"] != nil
    private static let relay = environment["TOMOSHIBI_DEBUG_WATCH_RELAY"]
    /// The stand-in's tickets, by the link each was made for. A ticket is a
    /// word of 16 to 64 letters, digits, dashes and underscores (show.rs
    /// `is_ticket`), so the link itself cannot be one.
    @MainActor private static var tickets: [String: String] = [:]

    /// What the stand-in makes of a pasted link.
    @MainActor static func resolve(_ text: String) -> ResolvedLink {
        let link = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: link), ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
              ["127.0.0.1", "localhost"].contains(url.host?.lowercased() ?? ""),
              ["mp4", "m4v", "mov", "m3u8", "webm", "mkv", "ogv", "ogg"].contains(url.pathExtension.lowercased())
        else { return .notPlayable(reason: .notAVideo) }
        return .playable(video: video(link, url))
    }

    /// What a stand-in ticket plays, or nil for a ticket that is not one.
    @MainActor static func info(_ ticket: String) -> WatchInfo? {
        guard links, let link = tickets[ticket], let url = URL(string: link) else { return nil }
        return .known(video(link, url))
    }

    @MainActor private static func video(_ link: String, _ url: URL) -> ResolvedVideo {
        // FNV-1a, so the same link is the same ticket for the whole run.
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in link.utf8 { hash = (hash ^ UInt64(byte)) &* 0x0000_0100_0000_01b3 }
        let ticket = "debugwatch-" + String(hash, radix: 16)
        tickets[ticket] = link
        let hls = url.pathExtension.lowercased() == "m3u8"
        let proxy = relay.map { ResolvedProxy(kind: hls ? .hls : .file, url: $0 + url.path) }
        return ResolvedVideo(ticket: ticket, title: url.deletingPathExtension().lastPathComponent, duration: 0,
                             cover: "", link: link, live: false, relay: false,
                             play: ResolvedPlay(kind: hls ? .hls : .file, url: link, id: nil), proxy: proxy, grantor: nil)
    }

    private static let logHandle: FileHandle? = {
        guard let path = environment["TOMOSHIBI_DEBUG_WATCH_LOG"] else { return nil }
        FileManager.default.createFile(atPath: path, contents: nil)
        return FileHandle(forWritingAtPath: path)
    }()

    static var logging: Bool { logHandle != nil }

    static func log(_ line: String) {
        guard let logHandle else { return }
        let stamp = Int64(Date().timeIntervalSince1970 * 1000)
        logHandle.write(Data("\(stamp) \(line)\n".utf8))
    }

    /// The presses appended to the control file since it was last read.
    @MainActor static func presses() -> [String] {
        guard let path = environment["TOMOSHIBI_DEBUG_WATCH_CONTROL"],
              let data = FileManager.default.contents(atPath: path), data.count > controlRead else { return [] }
        let fresh = data.subdata(in: controlRead..<data.count)
        // Only whole lines: a line half written is read on the next beat.
        guard let end = fresh.lastIndex(of: UInt8(ascii: "\n")) else { return [] }
        controlRead += end - fresh.startIndex + 1
        return String(decoding: fresh[fresh.startIndex...end], as: UTF8.self)
            .split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    @MainActor private static var controlRead = 0
}
