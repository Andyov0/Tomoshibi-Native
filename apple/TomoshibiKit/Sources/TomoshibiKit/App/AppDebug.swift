#if DEBUG
import AVFoundation
import Foundation
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// TOMOSHIBI_DEBUG_APP=<file>: drives the app around its calls from a test,
/// in a rehearsal only.
///
/// Every line appended to <file> is done as the matching control would do it,
/// and what came of it is appended to <file>.out; the host panel has its own
/// (Host/Debug.swift), and this is the same thing for what lies outside a
/// call: joining, calling a join off, a link, the menu's switches, a move,
/// the door. Lines run in order, and the ones that start something that takes
/// a while (`join`, `open`, `move`, ...) start it and go on, as a press does,
/// so the next line can be the press that races it; `wait <ms>` paces them.
///
///     state                      what the screens draw from, and the call
///     start                      the model's start again, as a new window's
///                                root asks for it
///     join | enter <room> | prepare <room>
///     back                       the join screen's Back
///     cancel                     the start and join sheet's Cancel
///     open <url>                 a link, straight to the model
///     url <url>                  a link handed over by the system, as an
///                                Apple event (Mac)
///     forget-server              Change server
///     mic                        the menu's ⌘D
///     leave                      the menu's Leave
///     relay <name>|auto          the server picker
///     move <relay>               the server moving the call, as the call
///                                reports it (CallModel `onMoved`)
///     knock | stop-knocking
///     remember <kind> <id>       a device taken in the call, as the devices
///                                panel remembers it (audioinput, videoinput,
///                                audiooutput)
///     choose <kind> <id>|none    a device picked in the settings
///     devices                    the record both of those write
///     preview-camera             the camera the join screen's preview opens
///     window <width> <height>    the main window's size (Mac)
///     close-window | reopen      ⌘W on the main window, and the Dock's
///                                reopen (Mac)
///     menu                       the menus' items and their keys (Mac)
///     key <character>            that key with ⌘, as the keyboard sends it
///                                (Mac)
///     choose-menu <menu> <item>  that menu item chosen (Mac)
///     click <x> <y> [settings]   a plain click at a point in the main or the
///                                Settings window, from its top left (Mac)
///     language <code> | accent <name>
///                                a choice in the settings
///     close-problem              the cross on the line saying why a press
///                                did not take
///     wait <ms>
@MainActor
enum AppDebug {
    private static var offset = 0
    private static var running = false

    static func start(_ app: AppModel) {
        guard !running, let path = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_APP"] else { return }
        running = true
        Task { [weak app] in
            while let app {
                if let data = FileManager.default.contents(atPath: path), data.count > offset {
                    let fresh = String(decoding: data[offset...], as: UTF8.self)
                    // Only whole lines; a line still being written waits.
                    if let end = fresh.lastIndex(of: "\n") {
                        let lines = fresh[..<end].split(separator: "\n", omittingEmptySubsequences: true)
                        offset += fresh[...end].utf8.count
                        for line in lines {
                            let said = await perform(String(line), app)
                            write("> \(line)\n\(said)\n", to: path + ".out")
                        }
                    }
                }
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }

    private static func write(_ text: String, to path: String) {
        let data = Data(text.utf8)
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }

    private static func perform(_ line: String, _ app: AppModel) async -> String {
        let words = line.split(separator: " ").map(String.init)
        guard let verb = words.first else { return "" }
        let rest = Array(words.dropFirst())
        switch verb {
        case "state":
            return state(app)
        case "start":
            Task { await app.start() }
            return "started"
        case "join":
            Task { await app.join() }
            return "joining"
        case "enter":
            guard let room = rest.first else { return "enter <room>" }
            Task { await app.enter(room: room) }
            return "entering"
        case "prepare":
            guard let room = rest.first else { return "prepare <room>" }
            app.prepare(room: room)
            return "prepared"
        case "back":
            app.backToLobby()
            return "back"
        case "cancel":
            app.cancelJoin()
            return "cancelled"
        case "open":
            guard let text = rest.first, let url = URL(string: text) else { return "open <url>" }
            Task { await app.open(url) }
            return "opening"
        case "forget-server":
            app.forgetServer()
            return "forgot: \(app.screen)"
        case "mic":
            Task { await app.toggleMicrophone() }
            return "pressed"
        case "leave":
            Task { await app.leave() }
            return "leaving"
        case "relay":
            app.relayChoice = rest.first == "auto" ? nil : rest.first
            return "relay \(app.relayChoice ?? "auto")"
        case "move":
            guard let call = app.call, let relay = rest.first else { return "move <relay>, in a call" }
            call.onMoved?(relay)
            return "moving"
        case "knock":
            Task { await app.knock() }
            return "knocking"
        case "stop-knocking":
            app.stopKnocking()
            return "stopped"
        case "remember":
            guard rest.count == 2, let kind = DeviceMemory.Kind(rawValue: rest[0]) else { return "remember <kind> <id>" }
            app.deviceMemory.remember(kind, id: rest[1])
            return "remembered"
        case "choose":
            guard rest.count == 2 else { return "choose <kind> <id>|none" }
            app.preferences.choose(rest[0], rest[1] == "none" ? nil : rest[1])
            return "chosen"
        case "devices":
            return (app.deviceDefaults?.string(forKey: DeviceMemory.choiceKey) ?? "nothing")
                + " | shown " + app.preferences.devices.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: ",")
        case "preview-camera":
            // Which camera the join screen's preview opens, worked out
            // without opening any.
            let all = CameraCapture.cameras().map { "\($0.localizedName)=\($0.uniqueID)" }.joined(separator: "; ")
            let picked = CameraStage.joiningCamera(app.deviceMemory)
            return "cameras: \(all)\nsystem default: \(AVCaptureDevice.default(for: .video)?.localizedName ?? "none")\n"
                + "remembered: \(app.deviceMemory.choice(.camera) ?? "none")\npreview opens: \(picked?.localizedName ?? "none")"
        case "language":
            // A tile in the settings or a row of the language popover.
            guard let code = rest.first, let language = Language(rawValue: code) else { return "language <code>" }
            Localization.shared.language = language
            return "speaking \(Localization.shared.language.rawValue)"
        case "accent":
            guard let name = rest.first, let accent = Accent(rawValue: name) else { return "accent <name>" }
            Appearance.shared.accent = accent
            return "accent \(Appearance.shared.accent.rawValue)"
        case "close-problem":
            // The cross on the line under the form.
            app.problem = nil
            return "closed"
        case "wait":
            try? await Task.sleep(for: .milliseconds(Int(rest.first ?? "") ?? 0))
            return "waited"
        #if os(macOS)
        case "window":
            guard rest.count == 2, let width = Double(rest[0]), let height = Double(rest[1]),
                  let window = mainWindows().first else { return "window <width> <height>" }
            window.setContentSize(NSSize(width: width, height: height))
            return "sized \(window.frame.size)"
        case "close-window":
            let windows = mainWindows()
            for window in windows { window.performClose(nil) }
            return "closed \(windows.count)"
        case "reopen":
            // The event a press on the Dock icon sends, sent by the app to
            // itself, so that the system's own handling of it runs.
            let event = NSAppleEventDescriptor(eventClass: AEEventClass(kCoreEventClass), eventID: AEEventID(kAEReopenApplication),
                                               targetDescriptor: .currentProcess(), returnID: AEReturnID(kAutoGenerateReturnID),
                                               transactionID: AETransactionID(kAnyTransactionID))
            do {
                _ = try event.sendEvent(options: [.noReply], timeout: 5)
                return "reopen sent"
            } catch {
                return "reopen failed: \(error)"
            }
        case "url":
            // A link handed over by the system, as the system hands it: the
            // event a browser or another app sends, sent by the app to itself.
            guard let text = rest.first else { return "url <url>" }
            let event = NSAppleEventDescriptor(eventClass: AEEventClass(kInternetEventClass), eventID: AEEventID(kAEGetURL),
                                               targetDescriptor: .currentProcess(), returnID: AEReturnID(kAutoGenerateReturnID),
                                               transactionID: AETransactionID(kAnyTransactionID))
            event.setParam(NSAppleEventDescriptor(string: text), forKeyword: AEKeyword(keyDirectObject))
            do {
                _ = try event.sendEvent(options: [.noReply], timeout: 5)
                return "url sent"
            } catch {
                return "url failed: \(error)"
            }
        case "menu":
            var menus: [String] = []
            for menu in NSApp.mainMenu?.items ?? [] {
                var items: [String] = []
                if let submenu = menu.submenu {
                    // As when the menu is opened.
                    submenu.delegate?.menuNeedsUpdate?(submenu)
                    submenu.update()
                }
                for item in menu.submenu?.items ?? [] where !item.isSeparatorItem {
                    let command = item.keyEquivalentModifierMask.contains(.command) ? "cmd-" : ""
                    let off = item.isEnabled ? "" : " (off)"
                    items.append((item.keyEquivalent.isEmpty ? item.title : "\(item.title)[\(command)\(item.keyEquivalent)]") + off)
                }
                menus.append("\(menu.title): " + items.joined(separator: ", "))
            }
            return menus.joined(separator: "\n")
        case "key":
            // A key pressed with ⌘, as the keyboard sends it: the menus'
            // key equivalents answer it as they answer a person.
            guard let key = rest.first, key.count == 1 else { return "key <character>" }
            let window = NSApp.keyWindow ?? NSApp.orderedWindows.first { $0.isVisible }
            // The keys' codes on a US keyboard, which the menus also read.
            let codes: [String: UInt16] = ["d": 2, "e": 14, "n": 45, "w": 13, "q": 12]
            for type in [NSEvent.EventType.keyDown, .keyUp] {
                guard let event = NSEvent.keyEvent(with: type, location: .zero, modifierFlags: .command,
                                                   timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window?.windowNumber ?? 0,
                                                   context: nil, characters: key, charactersIgnoringModifiers: key,
                                                   isARepeat: false, keyCode: codes[key] ?? 0) else { return "no event" }
                NSApp.postEvent(event, atStart: false)
            }
            return "pressed cmd-\(key)"
        case "choose-menu":
            // A menu item chosen, by its menu's title and its own.
            guard rest.count >= 2 else { return "choose-menu <menu> <item...>" }
            let title = rest.dropFirst().joined(separator: " ")
            guard let menu = NSApp.mainMenu?.items.first(where: { $0.title == rest[0] })?.submenu else { return "no such menu" }
            // As when the menu is opened: SwiftUI brings its items up to date
            // then, and not before.
            menu.delegate?.menuNeedsUpdate?(menu)
            menu.update()
            guard let index = menu.items.firstIndex(where: { $0.title == title }) else { return "no such item" }
            guard menu.items[index].isEnabled else { return "disabled" }
            menu.performActionForItem(at: index)
            return "chose \(title)"
        case "click":
            guard rest.count >= 2, let x = Double(rest[0]), let y = Double(rest[1]) else { return "click <x> <y> [settings]" }
            return await click(at: x, y, settings: rest.count > 2 && rest[2] == "settings")
        #endif
        default:
            return "unknown: \(verb)"
        }
    }

    private static func state(_ app: AppModel) -> String {
        var parts = [
            "screen=\(app.screen)",
            "busy=\(app.busy)",
            "joining=\(app.joining)",
            "room=\(app.room)",
            "relayChoice=\(app.relayChoice ?? "auto")",
            "lastJoinRelay=\(app.lastJoinRelay.map { $0.isEmpty ? "\"\"" : $0 } ?? "none")",
            "joinsAnswered=\(app.joinsAnswered)",
            "moving=\(app.movingTo ?? "no")",
            "problem=\(app.problem ?? "none")",
            "switchingMicrophone=\(app.switchingMicrophone)",
            "microphoneAccess=\(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)",
        ]
        if let call = app.call {
            let microphones = call.localTracks.values.filter { $0.source == .microphone }
            parts.append("call=\(call.room) as \(call.localIdentity), model \(UInt(bitPattern: ObjectIdentifier(call).hashValue) % 100_000)")
            parts.append("microphoneOn=\(call.microphoneOn)")
            parts.append("microphoneTracks=\(microphones.count) unmuted=\(microphones.filter { !$0.muted }.count)")
            parts.append("toasts=\(call.toasts.map(\.title).joined(separator: " / "))")
            parts.append("people=\(call.people.map { "\($0.name)(\($0.id.suffix(6)))" }.joined(separator: ", "))")
        } else {
            parts.append("call=none")
        }
        #if os(macOS)
        parts.append("windows=\(mainWindows().count)")
        #endif
        return parts.joined(separator: "\n")
    }

    #if os(macOS)
    /// The app's own windows that show its screens, not the Settings window
    /// or the system's panels.
    private static func mainWindows() -> [NSWindow] {
        NSApp.windows.filter { $0.isVisible && $0.contentView != nil && $0.canBecomeMain && !isSettings($0) }
    }

    private static func isSettings(_ window: NSWindow) -> Bool {
        window.identifier?.rawValue.contains("Settings") == true || window.identifier?.rawValue.contains("settings") == true
    }

    private static func click(at x: Double, _ y: Double, settings: Bool) async -> String {
        let window = settings
            ? NSApp.windows.first { $0.isVisible && isSettings($0) }
            : mainWindows().first
        guard let window else { return "no window" }
        let point = NSPoint(x: x, y: window.frame.height - y)
        // Down, a moment, and up, as a finger does: a tap gesture took the
        // two arriving together for no tap at all.
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: [],
                                                 timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                                 context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseDown ? 1 : 0) else { return "no event" }
            NSApp.postEvent(event, atStart: false)
            try? await Task.sleep(for: .milliseconds(60))
        }
        return "posted in \(window.identifier?.rawValue ?? window.title)"
    }
    #endif
}
#endif
