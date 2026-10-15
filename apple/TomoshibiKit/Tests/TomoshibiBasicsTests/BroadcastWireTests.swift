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
}
