import CoreVideo
import Foundation
import TomoshibiCore

/// Every place one remote picture is drawn at once.
///
/// The core sends a track's frames to one sink. A picture is often wanted in
/// two places — its tile and the picture-in-picture window — so each track
/// gets one fan-out sink here, attached once, which hands every frame to every
/// renderer that wants it; and the size the server is asked for is the
/// largest any of them wants, because the smaller one can always scale down.
final class VideoHub: @unchecked Sendable {
    private static let lock = NSLock()
    private static var hubs: [ObjectIdentifier: VideoHub] = [:]

    /// The hub for a call, made on first use.
    static func of(_ call: Call) -> VideoHub {
        lock.lock()
        defer { lock.unlock() }
        let key = ObjectIdentifier(call)
        if let hub = hubs[key] { return hub }
        let hub = VideoHub(call: call)
        hubs[key] = hub
        return hub
    }

    /// The hub for a call if there is one, without making one: what a
    /// renderer going away or resizing asks for. Asking `of` there made a new
    /// hub for a call already left, and a hub holds its call.
    static func existing(_ call: Call) -> VideoHub? {
        lock.lock()
        defer { lock.unlock() }
        return hubs[ObjectIdentifier(call)]
    }

    static func forget(_ call: Call) {
        lock.lock()
        hubs[ObjectIdentifier(call)] = nil
        lock.unlock()
    }

    private let call: Call
    private let lock = NSLock()
    private var fans: [String: Fanout] = [:]

    private init(call: Call) {
        self.call = call
    }

    func add(_ renderer: VideoRenderer, to sid: String) {
        lock.lock()
        let fan = fans[sid] ?? Fanout()
        let fresh = fans[sid] == nil
        fans[sid] = fan
        fan.add(renderer)
        lock.unlock()
        if fresh { call.attachVideo(sid: sid, sink: fan) }
    }

    func remove(_ renderer: VideoRenderer, from sid: String) {
        lock.lock()
        guard let fan = fans[sid] else { lock.unlock(); return }
        let empty = fan.remove(renderer)
        if empty { fans[sid] = nil }
        let nothingLeft = fans.isEmpty
        lock.unlock()
        if empty { call.detach(sid: sid) } else { ask(sid) }
        // The last picture of the call gone, the hub goes too. Nothing ever
        // forgot a hub, and the table of hubs kept every call that had shown
        // a remote camera alive after it was left, its per-second loops in
        // the core with it, one more for every call and every move
        // (2026-10-17).
        if nothingLeft { Self.forget(call) }
    }

    /// A renderer's wish, in device pixels.
    func want(_ size: (Int, Int), for renderer: VideoRenderer, on sid: String) {
        lock.lock()
        fans[sid]?.wants[ObjectIdentifier(renderer)] = size
        lock.unlock()
        ask(sid)
    }

    private func ask(_ sid: String) {
        lock.lock()
        let largest = fans[sid]?.wants.values.max { $0.0 * $0.1 < $1.0 * $1.1 }
        lock.unlock()
        guard let largest, largest.0 > 0, largest.1 > 0 else { return }
        try? call.requestVideoSize(sid: sid, width: UInt32(largest.0), height: UInt32(largest.1))
    }

    /// The one sink the core knows for a track.
    final class Fanout: VideoSink, @unchecked Sendable {
        private let lock = NSLock()
        private var renderers: [ObjectIdentifier: VideoRenderer] = [:]
        var wants: [ObjectIdentifier: (Int, Int)] = [:]

        func add(_ renderer: VideoRenderer) {
            lock.lock()
            renderers[ObjectIdentifier(renderer)] = renderer
            lock.unlock()
        }

        /// Whether nobody is left.
        func remove(_ renderer: VideoRenderer) -> Bool {
            lock.lock()
            renderers[ObjectIdentifier(renderer)] = nil
            wants[ObjectIdentifier(renderer)] = nil
            let empty = renderers.isEmpty
            lock.unlock()
            return empty
        }

        func onFrame(frame: RemoteFrame) {
            let buffer: CVPixelBuffer?
            if frame.pixelBuffer != 0, let pointer = UnsafeRawPointer(bitPattern: UInt(frame.pixelBuffer)) {
                buffer = Unmanaged<CVPixelBuffer>.fromOpaque(pointer).takeRetainedValue()
            } else if let planes = frame.i420 {
                buffer = VideoRenderer.pixelBuffer(i420: planes, width: Int(frame.width), height: Int(frame.height))
            } else {
                buffer = nil
            }
            guard let buffer else { return }
            lock.lock()
            let all = Array(renderers.values)
            lock.unlock()
            for renderer in all { renderer.show(buffer, rotation: Int(frame.rotation)) }
        }
    }
}
