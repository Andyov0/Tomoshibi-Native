import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * What the call is told of the default path. The core resumes the call when
 * the route it is told differs from the one before, so what these guard is a
 * route that changes when the path has not (a resume for nothing, the media
 * stalling, every time the monitor speaks) and one that stays the same when
 * the device has moved to another network.
 */
final class NetworkRouteTests: XCTestCase {
    func testTheSamePathReadsTheSameWhateverOrderItsGatewaysCameIn() {
        let one = NetworkRoute.of(satisfied: true, interface: "en0", kind: "wifi", gateways: ["192.0.2.1:0", "fe80::1%en0:0"])
        let two = NetworkRoute.of(satisfied: true, interface: "en0", kind: "wifi", gateways: ["fe80::1%en0:0", "192.0.2.1:0"])
        XCTAssertEqual(one, two)
        XCTAssertTrue(one.online)
    }

    func testAnotherInterfaceOrAnotherNetworkIsAnotherRoute() {
        let wifi = NetworkRoute.of(satisfied: true, interface: "en0", kind: "wifi", gateways: ["192.0.2.1:0"])
        XCTAssertNotEqual(wifi, NetworkRoute.of(satisfied: true, interface: "pdp_ip0", kind: "cellular", gateways: []))
        XCTAssertNotEqual(wifi, NetworkRoute.of(satisfied: true, interface: "en0", kind: "wifi", gateways: ["198.51.100.1:0"]))
    }

    func testAPathThatIsNotSatisfiedIsOfflineWhateverItSays() {
        let none = NetworkRoute.of(satisfied: false, interface: "en0", kind: "wifi", gateways: ["192.0.2.1:0"])
        XCTAssertEqual(none, NetworkRoute(online: false, route: ""))
    }
}
