#if DEBUG
import Foundation
import Observation
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// TOMOSHIBI_DEBUG_HOST=<file>: drives this feature from a test.
///
/// Every line appended to <file> is done as the matching control would do it,
/// and what came of it is appended to <file>.out. It exists because the
/// controls cannot be pressed from outside on the machine these were tested
/// on: its accessibility answers with the application for every element, and
/// a click posted to a process that is not in front goes nowhere. Nothing here
/// runs unless the variable is set.
///
///     panel host|sound|messages|none     open a panel, or close it
///     tray open|close                    the reactions above the hand
///     react <n>                          send the nth reaction
///     hand up|down
///     invite | revoke | close-room
///     ending on|off | elsewhere on|off   the two questions in the host panel
///     mute|remove|handover <name>        as the buttons on that person's row
///                                        (a name may have spaces; so may an
///                                        identity be given instead)
///     move <relay>
///     letin|notnow <name>                answer a knock
///     block|unblock <name> <sound>       voice, screen or music
///     volume <name> <sound> <0..1>
///     enabled <sid> on|off               the core's remote switch, directly
///     state                              everything the panels draw from
///     rightclick|ctrlclick|leftclick <x> <y>
///                                        the other button, a Control-click or a
///                                        plain one,
///                                        at a point in the window (points from
///                                        its top left, title bar included),
///                                        put on the application's queue as the
///                                        system would put it (Mac only)
///     window <width> <height>            resize the window, to see a phone's
///                                        width on the Mac (Mac only)
///
/// The `move` and the relays need the host panel to have been opened once,
/// which is when the relays are asked for, as on the web. With
/// TOMOSHIBI_DEBUG_PASTEBOARD=<name> as well, "Invite somebody" copies to a
/// private pasteboard of that name instead of the person's own, and `state`
/// reads it back.
///
/// In a debug build driven by a test (`Rehearsal.running`) only, as AppDebug
/// is. A release build once compiled all of this and read the variable: any
/// process that could start the released Mac app could have it invite a
/// stranger, close the room, let people in and click its own share and camera
/// buttons, with the permissions the person had given the app.
@MainActor
@Observable
final class HostDebug {
    struct PanelRequest: Equatable {
        let id = UUID()
        let panel: RoomView.Panel?
    }

    /// The last panel asked for, which the hand control opens.
    private(set) var panelRequest: PanelRequest?

    private static let path = Rehearsal.running ? ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_HOST"] : nil
    /// How far the file has been read, for the whole process: a call that is
    /// moved is joined again with a new driver, and one that started from the
    /// top replayed the move that ended the last call, and moved it again.
    private static var offset = 0
    private static var drivers: [ObjectIdentifier: (call: WeakCall, driver: HostDebug)] = [:]

    struct WeakCall { weak var call: Call? }

    /// The driver for this call, made on first sight when the switch is on.
    private static func of(_ call: Call) -> HostDebug? {
        guard path != nil else { return nil }
        let key = ObjectIdentifier(call)
        if let entry = drivers[key], entry.call.call === call { return entry.driver }
        let driver = HostDebug()
        drivers[key] = (WeakCall(call: call), driver)
        return driver
    }

    static func panelWanted(for call: Call) -> PanelRequest? {
        of(call)?.panelRequest
    }

    static func start(_ feature: HostFeature) {
        guard let path, let driver = of(feature.call.call) else { return }
        Task { [weak feature] in
            while let feature, !Task.isCancelled, feature.call.endedReason == nil {
                if let data = FileManager.default.contents(atPath: path), data.count > offset {
                    let fresh = String(decoding: data[offset...], as: UTF8.self)
                    // Only whole lines; a line still being written waits.
                    if let end = fresh.lastIndex(of: "\n") {
                        let lines = fresh[..<end].split(separator: "\n", omittingEmptySubsequences: true)
                        offset += fresh[...end].utf8.count
                        for line in lines {
                            let said = await driver.perform(String(line), feature)
                            driver.write("> \(line)\n\(said)\n", to: path + ".out")
                        }
                    }
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    private func write(_ text: String, to path: String) {
        let data = Data(text.utf8)
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }

    private func perform(_ line: String, _ feature: HostFeature) async -> String {
        let words = line.split(separator: " ").map(String.init)
        guard let verb = words.first else { return "" }
        let rest = Array(words.dropFirst())
        let call = feature.call
        func person(_ name: String?) -> Person? {
            guard let name else { return nil }
            return call.people.first { $0.name == name || $0.id == name }
        }
        func sound(_ word: String?) -> Sound? { word.flatMap(Sound.init(rawValue:)) }

        switch verb {
        case "panel":
            let wanted: RoomView.Panel? = switch rest.first {
            case "host": .host
            case "sound": .sound
            case "messages": .messages
            default: nil
            }
            panelRequest = PanelRequest(panel: wanted)
        case "tray":
            ReactionBoard.of(call.call).trayOpen = rest.first == "open"
        case "react":
            let choices = reactionChoices()
            guard let n = rest.first.flatMap(Int.init), choices.indices.contains(n) else { return "no such reaction" }
            ReactionBoard.of(call.call).send(choices[n], in: call)
            ReactionBoard.of(call.call).trayOpen = false
        case "hand":
            try? await call.setHand(rest.first == "up")
        case "invite": await feature.invite()
        case "revoke": await feature.revoke()
        case "close-room": await feature.dissolve()
        case "ending": feature.ending = rest.first == "on"
        case "elsewhere": feature.choosingRelay = rest.first == "on"
        case "mute", "remove", "handover":
            let name = rest.joined(separator: " ")
            guard let one = person(name) else { return "nobody called \(name)" }
            switch verb {
            case "mute": await feature.silence(one)
            case "remove": await feature.turnOut(one)
            default:
                if feature.isGuest(one) { feature.refuseHandOver() } else { await feature.handOver(to: one) }
            }
        case "move":
            guard let relay = feature.usableRelays.first(where: { $0.name == rest.first }) else { return "no such relay" }
            await feature.move(to: relay)
        case "letin", "notnow":
            let name = rest.joined(separator: " ")
            guard let one = feature.waiting.first(where: { $0.name == name }) else { return "nobody waiting called \(name)" }
            await feature.answer(one, admit: verb == "letin")
        case "block", "unblock":
            guard let one = person(rest.dropLast().joined(separator: " ")), let what = sound(rest.last) else { return "block <name> <sound>" }
            feature.setBlocked(one, what, verb == "block")
        case "volume":
            guard rest.count >= 3, let one = person(rest.dropLast(2).joined(separator: " ")),
                  let what = sound(rest[rest.count - 2]), let level = Double(rest[rest.count - 1])
            else { return "volume <name> <sound> <level>" }
            feature.setVolume(one, what, level)
        case "enabled":
            // The core's own answer, error and all, which the feature swallows.
            guard rest.count == 2 else { return "enabled <sid> on|off" }
            do {
                try call.call.setRemoteEnabled(sid: rest[0], enabled: rest[1] == "on")
            } catch {
                return "error: \(error)"
            }
        case "state":
            return state(feature)
        #if os(macOS)
        case "window":
            guard rest.count == 2, let width = Double(rest[0]), let height = Double(rest[1]),
                  let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) else { return "window <width> <height>" }
            var frame = window.frame
            frame.origin.y += frame.height - height
            frame.size = NSSize(width: width, height: height)
            window.setFrame(frame, display: true)
            return "now \(Int(window.frame.width))x\(Int(window.frame.height))"
        case "rightclick", "ctrlclick", "leftclick":
            guard rest.count == 2, let x = Double(rest[0]), let y = Double(rest[1]) else { return "\(verb) <x> <y>" }
            return click(verb, at: x, y)
        #endif
        default:
            return "unknown: \(verb)"
        }
        return "done"
    }

    #if os(macOS)
    /// A click made in this process and put on the application's own queue,
    /// so that it is dispatched as a real one is: a click posted from outside
    /// goes nowhere on the machine these were checked on while the app is not
    /// in front.
    private func click(_ kind: String, at x: Double, _ y: Double) -> String {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.contentView != nil }) else { return "no window" }
        let point = NSPoint(x: x, y: window.frame.height - y)
        let right = kind == "rightclick"
        let kinds: [NSEvent.EventType] = right ? [.rightMouseDown, .rightMouseUp] : [.leftMouseDown, .leftMouseUp]
        for type in kinds {
            guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: kind == "ctrlclick" ? .control : [],
                                                 timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                                 context: nil, eventNumber: 0, clickCount: 1, pressure: 1) else { return "no event" }
            // Through the queue rather than straight to `sendEvent`, so it
            // meets the application's event monitors on the way, as a real
            // click does.
            NSApp.postEvent(event, atStart: false)
        }
        return "posted"
    }
    #endif

    private func state(_ feature: HostFeature) -> String {
        let call = feature.call
        var out: [String] = []
        out.append("standing yours=\(feature.standing.yours) admin=\(feature.standing.admin) scoped=\(feature.scoped)")
        out.append("busy=\(feature.busy ?? "-") link=\(feature.link ?? "-") copied=\(feature.copied)")
        #if os(macOS)
        if let name = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_PASTEBOARD"] {
            out.append("pasteboard=\(NSPasteboard(name: NSPasteboard.Name(name)).string(forType: .string) ?? "-")")
        }
        #endif
        out.append("waiting=\(feature.waiting.map { "\($0.name)@\($0.address)" })")
        out.append("relays=\((feature.relays ?? []).map(\.name)) usable=\(feature.usableRelays.map(\.name))")
        out.append("moving=\(feature.movingTo ?? "-") handUp=\(call.handUp) tray=\(ReactionBoard.of(call.call).trayOpen)")
        for person in call.people {
            let tracks = person.tracks.values.sorted { $0.sid < $1.sid }
                .map { "\($0.source)/\($0.name)/\($0.sid)\($0.muted ? "/muted" : "")\($0.subscribed ? "" : "/unsubscribed")" }
            let hearing = Sound.allCases.map { sound -> String in
                let setting = feature.setting(person, sound)
                return "\(sound.rawValue)=\(setting.volume)\(setting.blocked ? "/blocked" : "")"
            }
            out.append("person \(person.name) \(person.id) hand=\(person.handUp) tracks=\(tracks) hearing=\(hearing)")
        }
        out.append("reactions=\(ReactionBoard.of(call.call).shown.map { "\($0.from):\($0.what.unicodeScalars.map { String($0.value, radix: 16) }.joined(separator: "+"))" })")
        // What the corner is saying, the host's notices among the rest.
        out.append("toasts=\(call.toasts.map { ($0.kind == .failure ? "!" : "") + $0.title })")
        out.append("stored=\(UserDefaults.standard.string(forKey: HearingBook.storageKey) ?? "-")")
        return out.joined(separator: "\n")
    }
}
#endif
