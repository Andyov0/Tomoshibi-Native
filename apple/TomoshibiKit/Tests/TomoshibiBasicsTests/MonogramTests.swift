import XCTest
@testable import TomoshibiBasics

/// A person's monogram is one colour now, and it has to be the colour they
/// already had: the first of the two the blend was drawn from before
/// 2026-10-20. The answers are the first of each pair the Swift gave on
/// 2026-10-18 (MonogramTest.kt records the pairs' history), and Android's
/// test holds its port to the same list, so a hash that drifts on either side
/// puts somebody in another colour on one of them and fails here first.
final class MonogramTests: XCTestCase {
    func testTheColourIsTheFirstOfTheOldPairAndAgreesWithAndroid() {
        let expected: [(String, Int)] = [
            ("", 1), ("a", 0), ("andy", 5), ("Andy", 1), ("guest-7f3a9c", 1),
            ("sig:Zq3n0Vf8", 2), ("张伟", 3), ("田中さん", 4), ("😀x", 0),
            ("0f9e2d4c-8b1a-4f6e-9c3d-2a7b5e1f0c84", 2),
            ("participant-1", 0), ("participant-2", 5), ("participant-3", 4),
        ]
        for (identity, index) in expected {
            XCTAssertEqual(Monograms.index(identity, of: 6), index, "for \"\(identity)\"")
        }
    }
}
