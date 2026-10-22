import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The deployment's ceiling over a shared screen (ShareCeiling). What these
 * guard would go wrong out of sight: a ceiling handed to the core as a rate
 * its size does not offer, which the core reads as the size's highest and so
 * publishes a 1080p share at 240 under a ceiling of 24; a ceiling of nought,
 * which the deployment writes for "do not lower", sending no frames at all;
 * a height cut without the width, a squashed picture; and a ceiling the
 * iPhone's broadcast extension never sees, because it reads it from where
 * the app did not write it.
 */
final class ShareCeilingTests: XCTestCase {
    func testWithoutACeilingTheMenusRateIsSentAsBefore() {
        let none = ShareCeiling.none
        XCTAssertEqual(none.published(60, shorterSide: 1080), 60)
        XCTAssertEqual(none.published(240, shorterSide: 2160), 60, "the size's own clamp, as ever")
        XCTAssertEqual(none.published(30, shorterSide: 0), ShareRates.automatic)
        XCTAssertEqual(none.captured(60, hot: true), 60, "hot, with no hot ceiling, is not slowed")
        XCTAssertNil(none.limit(hot: true))
        XCTAssertTrue(none.size(width: 2560, height: 1440) == (2560, 1440))
    }

    func testTheRatePublishedIsOneTheSizeOffers() {
        let ceiling = ShareCeiling(fps: 24)
        XCTAssertEqual(ceiling.published(60, shorterSide: 1080), 15, "24 is not one of 1080p's rates; 15 is the highest under it")
        XCTAssertEqual(ShareCeiling(fps: 30).published(60, shorterSide: 1440), 30)
        XCTAssertEqual(ShareCeiling(fps: 30).published(15, shorterSide: 1440), 15, "a ceiling over what was chosen changes nothing")
        XCTAssertEqual(ShareCeiling(fps: 10).published(60, shorterSide: 1080), 15, "under every rate, the lowest")
        XCTAssertEqual(ShareCeiling(fps: 30).published(60, shorterSide: 0), ShareRates.automatic,
                       "automatic has one rate, and keeps it; its frames are held at the capture")
        for side in [1080, 1440, 2160, 0] {
            for cap in [1, 7, 15, 24, 30, 59, 60, 61, 120, 500] {
                let published = ShareCeiling(fps: cap).published(240, shorterSide: side)
                XCTAssertTrue(ShareRates.allowed(shorterSide: side).contains(published), "\(cap) at \(side) published \(published)")
            }
        }
    }

    func testTheCaptureIsHeldToTheCeilingAndLowerWhileHot() {
        let ceiling = ShareCeiling(fps: 30, hotFps: 15)
        XCTAssertEqual(ceiling.captured(60, hot: false), 30, "automatic's sixty, captured at the ceiling")
        XCTAssertEqual(ceiling.captured(60, hot: true), 15)
        XCTAssertEqual(ceiling.captured(15, hot: false), 15)
        XCTAssertEqual(ShareCeiling(hotFps: 20).captured(60, hot: false), 60, "a hot ceiling is only for when hot")
        XCTAssertEqual(ShareCeiling(hotFps: 20).captured(60, hot: true), 20)
        XCTAssertEqual(ShareCeiling(fps: 10, hotFps: 20).limit(hot: true), 10, "the lower of the two")
    }

    func testTheHeightComesDownWithTheWidthAndBothStayEven() {
        let ceiling = ShareCeiling(height: 720)
        XCTAssertTrue(ceiling.size(width: 2560, height: 1440) == (1280, 720))
        XCTAssertTrue(ceiling.size(width: 1280, height: 720) == (1280, 720), "at the ceiling, untouched")
        XCTAssertTrue(ceiling.size(width: 800, height: 600) == (800, 600), "under it, untouched")
        let odd = ShareCeiling(height: 719).size(width: 1919, height: 1081)
        XCTAssertEqual(odd.width % 2, 0)
        XCTAssertEqual(odd.height % 2, 0)
        XCTAssertLessThanOrEqual(odd.height, 719)
        let tall = ceiling.size(width: 1170, height: 2532)
        XCTAssertEqual(tall.height, 720)
        XCTAssertEqual(Double(tall.width) / Double(tall.height), 1170.0 / 2532.0, accuracy: 0.01, "the picture's shape")
    }

    func testOnlyAPositiveCountIsACeiling() {
        let read = ShareCeiling(params: ["screen_share_max_fps": 0, "screen_share_max_height": -720,
                                         "screen_share_hot_fps": 0.5, "update_check_hours": 3])
        XCTAssertEqual(read, .none, "nought, a negative or a fraction under one would ask for no frames at all")
        let whole = ShareCeiling(params: ["screen_share_max_fps": 29.97, "screen_share_max_height": 1080, "screen_share_hot_fps": 15])
        XCTAssertEqual(whole, ShareCeiling(fps: 29, height: 1080, hotFps: 15))
        XCTAssertEqual(ShareCeiling(params: ["screen_share_max_fps": .infinity]), .none)
    }

    func testTheBroadcastExtensionReadsWhatTheAppLeft() throws {
        let suite = "TomoshibiBasicsTests.ShareCeiling"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defaults.removePersistentDomain(forName: suite)
        defer { defaults.removePersistentDomain(forName: suite) }

        XCTAssertEqual(ShareCeiling(kept: defaults), .none, "nothing left is no ceiling")
        let ceiling = ShareCeiling(fps: 30, height: 1080, hotFps: 15)
        ceiling.keep(in: defaults)
        XCTAssertEqual(ShareCeiling(kept: defaults), ceiling)

        // Taken away by the deployment: gone from what the extension reads.
        ShareCeiling(fps: 30).keep(in: defaults)
        XCTAssertEqual(ShareCeiling(kept: defaults), ShareCeiling(fps: 30))
        ShareCeiling.none.keep(in: defaults)
        XCTAssertEqual(ShareCeiling(kept: defaults), .none)
    }

    func testHotIsSeriousOrWorse() {
        XCTAssertFalse(ShareCeiling.hot(.nominal))
        XCTAssertFalse(ShareCeiling.hot(.fair))
        XCTAssertTrue(ShareCeiling.hot(.serious))
        XCTAssertTrue(ShareCeiling.hot(.critical))
    }
}
