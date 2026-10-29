import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The guest identity and its proof, kept together. What these guard: a proof
 * sent beside an identity it was not issued with (the server would issue a
 * fresh identity, and a guest would lose its place and any pin), a proof left
 * behind when a passphrase join replaced the identity, and the identity no
 * longer being found where earlier versions of the app kept it.
 */
final class KeptIdentityTests: XCTestCase {
    private let suite = "TomoshibiBasicsTests.KeptIdentity"
    private let base = "http://127.0.0.1:8080/"

    private func defaults() throws -> UserDefaults {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defaults.removePersistentDomain(forName: suite)
        return defaults
    }

    override func tearDown() {
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
    }

    func testAGuestsIdentityComesBackWithItsProof() throws {
        let defaults = try defaults()
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity())
        KeptIdentity.keep(KeptIdentity(identity: "g1", proof: "p1"), in: defaults, base: base)
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity(identity: "g1", proof: "p1"))
    }

    func testAnAnswerWithoutAProofForgetsTheOldOne() throws {
        let defaults = try defaults()
        KeptIdentity.keep(KeptIdentity(identity: "g1", proof: "p1"), in: defaults, base: base)
        // A passphrase join: its identity is kept by the passphrase.
        KeptIdentity.keep(KeptIdentity(identity: "s1", proof: nil), in: defaults, base: base)
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity(identity: "s1", proof: nil))
        KeptIdentity.keep(KeptIdentity(identity: "g2", proof: ""), in: defaults, base: base)
        XCTAssertEqual(KeptIdentity.read(defaults, base: base).proof, nil)
    }

    func testTheIdentityIsWhereEarlierVersionsKeptItAndEachDeploymentHasItsOwn() throws {
        let defaults = try defaults()
        defaults.set("old", forKey: "tomoshibi.identity." + base)
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity(identity: "old", proof: nil))
        KeptIdentity.keep(KeptIdentity(identity: "g1", proof: "p1"), in: defaults, base: "https://meet.example.com/")
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity(identity: "old", proof: nil))
    }

    func testAProofWithoutItsIdentityIsNeverRead() throws {
        let defaults = try defaults()
        defaults.set("stray", forKey: KeptIdentity.keys(base: base).proof)
        XCTAssertEqual(KeptIdentity.read(defaults, base: base), KeptIdentity())
    }
}
