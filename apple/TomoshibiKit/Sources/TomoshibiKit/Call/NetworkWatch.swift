import Foundation
import Network
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// Tells the call what the device's default path is, every time the system
/// says (`Call.pathSeen`), and on the Mac when it wakes from sleep
/// (`Call.woke`), for as long as the call lasts.
///
/// Without it a call on a network that went away noticed only when the
/// server's ping went unanswered, a call gone quiet for that long, and a call
/// already reconnecting when the network came back sat out the rest of its
/// wait, up to eight seconds, before it tried; the web resumes or retries the
/// moment the browser says it is online. Every update is passed on as it is,
/// and the core decides whether the route really changed, since each change
/// it acts on is a resume of a call that may have had nothing wrong with it
/// (protocol network.rs, `NetworkRoute`).
final class NetworkWatch: @unchecked Sendable {
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "tomoshibi.network-watch")
    private weak var call: Call?
    #if os(macOS)
    private var wake: NSObjectProtocol?
    #endif

    init(call: Call) {
        self.call = call
    }

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            guard let call = self?.call else { return }
            let route = Self.read(path)
            #if DEBUG
            // The interface and its kind, not the gateways' addresses.
            NSLog("tomoshibi network path: online=%@ via %@", route.online ? "yes" : "no",
                  route.route.split(separator: " ").prefix(2).joined(separator: " "))
            #endif
            call.pathSeen(path: NetworkPath(online: route.online, route: route.route))
        }
        monitor.start(queue: queue)
        #if os(macOS)
        // A Mac asleep keeps no connection, and wakes on the network it slept
        // on, which its path monitor need not report as any change at all.
        wake = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: nil
        ) { [weak self] _ in
            self?.call?.woke()
        }
        #endif
    }

    /// Stops listening; the monitor cannot be started again.
    func stop() {
        monitor.pathUpdateHandler = nil
        monitor.cancel()
        #if os(macOS)
        if let wake { NSWorkspace.shared.notificationCenter.removeObserver(wake) }
        wake = nil
        #endif
    }

    static func read(_ path: NWPath) -> NetworkRoute {
        let preferred = path.availableInterfaces.first
        return NetworkRoute.of(
            satisfied: path.status == .satisfied,
            interface: preferred?.name,
            kind: preferred.map { kind($0.type) },
            gateways: path.gateways.map { "\($0)" })
    }

    private static func kind(_ type: NWInterface.InterfaceType) -> String {
        switch type {
        case .wifi: "wifi"
        case .cellular: "cellular"
        case .wiredEthernet: "wired"
        case .loopback: "loopback"
        case .other: "other"
        @unknown default: "unknown"
        }
    }
}
