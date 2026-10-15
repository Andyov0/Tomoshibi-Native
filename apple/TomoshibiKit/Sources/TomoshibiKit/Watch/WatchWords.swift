import Foundation
import TomoshibiCore

/// What a ticket plays, as this app has learned it (watch-api.ts `infos`):
/// what the server said, or one of the two ways it can say nothing. Told
/// apart because they are different things to say: "could not be read, try
/// again" sent somebody retrying a video that the switch on the management
/// pages had turned off, and that no retry would play.
enum WatchInfo: Equatable {
    case known(ResolvedVideo)
    case unavailable
    case failed

    var video: ResolvedVideo? {
        if case let .known(video) = self { return video }
        return nil
    }
}

enum WatchWords {
    /// The code a refusal crosses the call as (watch.ts `REFUSALS`), from
    /// the reason the deployment gave.
    static func code(_ reason: UnresolvedReason) -> String {
        switch reason {
        case .notAVideo: "not_a_video"
        case .unavailable: "unavailable"
        case .membersOnly: "members_only"
        case .copyProtected: "copy_protected"
        case .regionLocked: "region_locked"
        case .notFound: "not_found"
        case .notAllowed: "not_allowed"
        case .embyNotAllowed: "emby_not_allowed"
        case .failed: "failed"
        }
    }

    /// What to say about a video that was not added (WatchTab.tsx `refusal`).
    ///
    /// The two refusals of a person are said as what the person seeing them
    /// was not given, because by the time one reaches somebody it is about
    /// them: a link is read by whoever adds it wherever they can, a link read
    /// on somebody else's behalf is refused a media server's film whoever they
    /// are, and the holder's own refusal is not passed on as one.
    static func refusal(_ why: String) -> String {
        switch why {
        case "not_a_video": t("That link is not a video that can be played here.")
        case "unavailable": t("That video cannot be played here.")
        // Bilibili's shows, today. "Its own site" because that is where it
        // does play; the deployment's own premium account is given only a
        // preview for a page elsewhere.
        case "members_only": t("That video is for paying members only, and plays only on its own site.")
        case "copy_protected": t("That video is copy-protected, and plays only on its own site.")
        case "region_locked": t("That video is not available in this server's region.")
        // Both, because which it is cannot be told from here.
        case "not_found": t("That episode could not be found: it may have been taken down, or not be offered in this server's region.")
        case "not_allowed": t("Watching together has not been opened to you.")
        case "emby_not_allowed": t("The media servers have not been opened to you.")
        default: t("The video could not be read. Try again.")
        }
    }

    /// The site a link is on, as people say it: "youtube.com", not
    /// "www.youtube.com" (WatchScreen.tsx `siteOf`).
    static func site(_ link: String) -> String {
        guard let url = URL(string: link), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = url.host(percentEncoded: false), !host.isEmpty else { return "" }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    /// At most `count` UTF-16 units, as a JavaScript `slice(0, count)` cuts:
    /// the web's field takes 2000 and the protocol counts the same way.
    static func cut(_ text: String, _ count: Int) -> String {
        let units = Array(text.utf16.prefix(count))
        return String(decoding: units, as: UTF16.self)
    }
}

/// The one clock every call into the theatre and the keeper is given: a
/// monotonic count of milliseconds. The protocol takes any such clock as long
/// as it is always the same one; the wall clock is not one, since it jumps
/// when the system sets it.
func watchClock() -> UInt64 {
    UInt64(ProcessInfo.processInfo.systemUptime * 1000)
}
