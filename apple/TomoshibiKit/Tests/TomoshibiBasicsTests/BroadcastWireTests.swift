import XCTest
@testable import TomoshibiBasics

/// The extension and the app agree on nothing but these bytes; a message cut
/// across two reads, or two in one read, must come out whole and in order.
final class BroadcastWireTests: XCTestCase {
    func testMessagesSurviveBeingCutAnywhere() {
        var video = BroadcastWire.Header(kind: .video)
        video.width = 1920; video.height = 886; video.rotation = 90; video.timestampMicros = 1234
        var audio = BroadcastWire.Header(kind: .audio)
        audio.channels = 2; audio.rate = 48_000
        let stream = BroadcastWire.frame(video, payload: Data(repeating: 7, count: 5000))
            + BroadcastWire.frame(audio, payload: Data(repeating: 9, count: 1920))
            + BroadcastWire.frame(BroadcastWire.Header(kind: .ended), payload: Data())
        for cut in [1, 3, 4, 17, 100, 4999, stream.count - 1] {
            var buffer = Data()
            var got: [(BroadcastWire.Header, Data)] = []
            buffer.append(stream.prefix(cut))
            got += BroadcastWire.take(from: &buffer)
            buffer.append(stream.dropFirst(cut))
            got += BroadcastWire.take(from: &buffer)
            XCTAssertEqual(got.map(\.0.kind), [.video, .audio, .ended], "cut at \(cut)")
            XCTAssertEqual(got[0].1.count, 5000)
            XCTAssertEqual(got[0].0.rotation, 90)
            XCTAssertEqual(got[1].1, Data(repeating: 9, count: 1920))
            XCTAssertTrue(buffer.isEmpty)
        }
    }

    func testAPartialMessageWaits() {
        var buffer = BroadcastWire.frame(BroadcastWire.Header(kind: .video), payload: Data(repeating: 1, count: 64)).dropLast()
        var data = Data(buffer)
        XCTAssertTrue(BroadcastWire.take(from: &data).isEmpty)
        XCTAssertEqual(data.count, buffer.count)
        buffer = Data()
    }
    /*
     * Whether a broadcast may start, as the app leaves it for the extension.
     * What this guards: a "switched off" written once and never taken back,
     * which would refuse every broadcast after the deployment allowed them
     * again; and the one sentence the extension says, which the system shows
     * in whatever language the phone reads.
     */
    func testTheExtensionReadsWhetherSharingIsSwitchedOffAndItIsClearedWhenAllowed() throws {
        let suite = "TomoshibiBasicsTests.BroadcastWire"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defaults.removePersistentDomain(forName: suite)
        defer { defaults.removePersistentDomain(forName: suite) }

        XCTAssertFalse(BroadcastWire.screenShareOff(in: defaults), "nothing left is not switched off")
        XCTAssertFalse(BroadcastWire.screenShareOff(in: nil), "no app group is not switched off")
        BroadcastWire.keep(screenShareOff: true, in: defaults)
        XCTAssertTrue(BroadcastWire.screenShareOff(in: defaults))
        ShareCeiling(fps: 15).keep(in: defaults)
        XCTAssertTrue(BroadcastWire.screenShareOff(in: defaults), "writing the ceiling took the switch with it")

        BroadcastWire.keep(screenShareOff: false, in: defaults)
        XCTAssertFalse(BroadcastWire.screenShareOff(in: defaults), "allowed again, a broadcast was still refused")
        XCTAssertNil(defaults.object(forKey: BroadcastWire.screenShareOffKey), "cleared rather than left as false")
        XCTAssertEqual(ShareCeiling(kept: defaults), ShareCeiling(fps: 15), "clearing the switch took the ceiling with it")
    }

    func testTheRefusalIsSaidInEveryLanguage() {
        let phrase = "Screen sharing is switched off for now."
        // Said by the extension through the shared Localization, in the
        // language it speaks; written nowhere, so the machine running this
        // keeps its own.
        let shared = Localization.shared
        let (was, kept) = (shared.language, shared.keeps)
        shared.keeps = false
        defer {
            shared.language = was
            shared.keeps = kept
        }
        for language in Language.allCases {
            let said = Phrasebook.tables[language]?[phrase]
            XCTAssertNotNil(said, language.rawValue)
            if language != .en { XCTAssertNotEqual(said, phrase, language.rawValue) }
            shared.language = language
            XCTAssertEqual(BroadcastWire.switchedOff, said, "the extension says it in \(language.rawValue)")
        }
    }
}
