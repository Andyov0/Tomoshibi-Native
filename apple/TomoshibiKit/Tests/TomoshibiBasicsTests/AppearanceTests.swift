import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * Where the look a person chose is written, and that a test driving the app
 * writes none of it (AppModel.rehearsal), over a defaults domain of the tests'
 * own, emptied before and after.
 */
@MainActor
final class AppearanceTests: XCTestCase {
    private let suite = "TomoshibiBasicsTests.Appearance"
    private var defaults: UserDefaults!

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: suite)
    }

    func testAChoiceIsKeptUnderItsKeyAndReadBack() {
        let appearance = Appearance(defaults: defaults)
        appearance.accent = .pink
        appearance.connectionBadge = .hidden
        appearance.controlsPlacement = .side

        XCTAssertEqual(defaults.string(forKey: Appearance.storageKey), "pink")
        XCTAssertEqual(defaults.string(forKey: Appearance.badgeKey), "hidden")
        XCTAssertEqual(defaults.string(forKey: Appearance.controlsKey), "side")
        let again = Appearance(defaults: defaults)
        XCTAssertEqual(again.accent, .pink)
        XCTAssertEqual(again.connectionBadge, .hidden)
        XCTAssertEqual(again.controlsPlacement, .side)

        // controls.ts keeps "always" as nothing.
        again.controlsPlacement = .always
        XCTAssertNil(defaults.string(forKey: Appearance.controlsKey))
    }

    func testARunThatKeepsNothingShowsItsChoicesAndWritesNone() {
        defaults.set("teal", forKey: Appearance.storageKey)
        let appearance = Appearance(defaults: defaults)
        appearance.keeps = false

        appearance.accent = .pink
        appearance.connectionBadge = .detailed
        appearance.controlsPlacement = .idle

        XCTAssertEqual(appearance.accent, .pink)
        XCTAssertEqual(defaults.string(forKey: Appearance.storageKey), "teal")
        XCTAssertNil(defaults.string(forKey: Appearance.badgeKey))
        XCTAssertNil(defaults.string(forKey: Appearance.controlsKey))
        XCTAssertEqual(Appearance(defaults: defaults).accent, .teal)
    }
}
