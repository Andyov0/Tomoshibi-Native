import Foundation

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
