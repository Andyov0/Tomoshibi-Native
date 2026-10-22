import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * What the app leaves for its Messages extension. What these guard: a test's
 * local deployment and the real one sharing a record (a test that signed in
 * would leave the extension offering the test's meetings), a signed-out app
 * leaving its session where the extension can still use it, and one address
 * written two ways ("http://127.0.0.1:8080" by a test, with its slash by the
 * core) being two records that never meet.
 */
final class HandoverTests: XCTestCase {
    private final class Memory: Secrets {
        var kept: [String: String] = [:]
        func read(_ account: String) -> String? { kept[account] }
        func write(_ account: String, _ value: String?) { kept[account] = value }
    }

    private let suite = "TomoshibiBasicsTests.Handover"

    private func handover() throws -> (Handover, Memory) {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defaults.removePersistentDomain(forName: suite)
        let memory = Memory()
        return (Handover(defaults: defaults, secrets: memory), memory)
    }

    override func tearDown() {
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
        super.tearDown()
    }

    private func record(_ base: String, account: String? = "tester") -> Handover.Record {
        Handover.Record(base: base, account: account, arranges: true,
                        meetings: [Meeting(id: "m1", room: "standup", at: "2026-10-23T10:00:00Z", token: "t", mine: true)],
                        language: "ja", accent: "teal")
    }

    func testTheExtensionReadsWhatTheAppLeft() throws {
        let (handover, _) = try handover()
        handover.keep(record("https://meet.example.com/"), cookie: "s3ssion")
        XCTAssertEqual(handover.record(for: "https://meet.example.com/"), record("https://meet.example.com/"))
        XCTAssertEqual(handover.cookie(for: "https://meet.example.com/"), "s3ssion")
    }

    func testOneAddressWrittenTwoWaysIsOneRecord() throws {
        let (handover, _) = try handover()
        handover.keep(record("http://127.0.0.1:8080/"), cookie: "local")
        XCTAssertEqual(handover.record(for: "http://127.0.0.1:8080")?.account, "tester")
        XCTAssertEqual(handover.cookie(for: "HTTP://127.0.0.1:8080/#/room"), "local")
        XCTAssertEqual(Handover.origin("https://Meet.Example.com:443/path?x=1"), "https://meet.example.com/")
        XCTAssertEqual(Handover.origin("http://127.0.0.1:8080"), "http://127.0.0.1:8080/")
        XCTAssertNil(Handover.origin("tomoshibi://meet.example.com/"))
    }

    func testATestsDeploymentAndTheRealOneAreApart() throws {
        let (handover, _) = try handover()
        handover.keep(record("https://meet.example.com/"), cookie: "real")
        handover.keep(record("http://127.0.0.1:8080/", account: "localtester"), cookie: "local")
        XCTAssertEqual(handover.record(for: "https://meet.example.com/")?.account, "tester")
        XCTAssertEqual(handover.cookie(for: "https://meet.example.com/"), "real")
        XCTAssertEqual(handover.cookie(for: "http://127.0.0.1:8080/"), "local")
    }

    func testSigningOutTakesTheSessionAway() throws {
        let (handover, memory) = try handover()
        handover.keep(record("https://meet.example.com/"), cookie: "s3ssion")
        // Signed out, whatever cookie the core still held is not left behind.
        handover.keep(record("https://meet.example.com/", account: nil), cookie: "s3ssion")
        XCTAssertNil(handover.cookie(for: "https://meet.example.com/"))
        XCTAssertTrue(memory.kept.isEmpty)
        XCTAssertNil(handover.record(for: "https://meet.example.com/")?.account)
    }
}
