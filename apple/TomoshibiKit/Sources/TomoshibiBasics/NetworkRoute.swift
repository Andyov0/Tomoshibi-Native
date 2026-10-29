import Foundation

/// The device's default path as the call is told about it (the core's
/// `NetworkPath`): whether there is one, and what it goes through.
///
/// The core decides from these whether the path has really changed and only
/// then makes the call resume or retry (protocol network.rs), because the
/// system's path monitor speaks far more often than the route changes: on
/// being started, whenever a property of the path changes (cost, constraints,
/// DNS), and several times for one hand-over. What it needs from here is a
/// route that is equal for the same path and different for another one.
public struct NetworkRoute: Equatable, Sendable {
    public var online: Bool
    public var route: String

    public init(online: Bool, route: String) {
        self.online = online
        self.route = route
    }

    /// A path read off the monitor: whether it is satisfied, the interface it
    /// prefers (the first of its available ones) and that interface's kind,
    /// and its gateways. The gateways are what tell one Wi-Fi network from the
    /// next on the same interface, a change of network the interface alone
    /// does not show; sorted, because a path lists them in no promised order
    /// and the same path must read the same. Nothing of a path that is not
    /// satisfied matters.
    public static func of(satisfied: Bool, interface: String?, kind: String?, gateways: [String]) -> NetworkRoute {
        guard satisfied else { return NetworkRoute(online: false, route: "") }
        let parts = [interface ?? "", kind ?? ""] + gateways.sorted()
        return NetworkRoute(online: true, route: parts.joined(separator: " "))
    }
}
