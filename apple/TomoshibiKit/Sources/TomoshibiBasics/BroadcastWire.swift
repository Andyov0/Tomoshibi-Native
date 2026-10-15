import Foundation

/// How the iPhone's screen gets from the broadcast extension to the app.
///
/// A ReplayKit broadcast runs in an extension of its own with a fifty
/// megabyte ceiling, far too little for WebRTC and an encoder; the app, which
/// is already in the call, publishes instead. The extension compresses each
/// frame to a JPEG (LiveKit's own SDK does the same, for the same reason) and
/// sends it with the screen's sound over a Unix socket in the app group's
/// container; the app decodes and publishes them as the share and its sound,
/// two tracks as the web sends them.
///
/// The wire is as plain as it can be: a 4-byte big-endian length of a small
/// JSON header, the header, then the payload the header says is coming.
public enum BroadcastWire {
    /// The app group both ends share; the socket lives in its container.
    public static let group = "group.app.tomoshibi"
    /// The broadcast extension's bundle identifier, which the app's picker
    /// asks the system to offer first.
    public static let extensionBundle = "app.tomoshibi.ios.broadcast"
    public static let socketName = "broadcast.sock"
    /// The longer side a shared iPhone screen is scaled to before it is sent:
    /// enough for text to stay sharp in a 1080p tile, small enough that a
    /// frame fits the extension's memory and the socket comfortably.
    public static let longest = 1920

    public struct Header: Codable, Sendable {
        public enum Kind: String, Codable, Sendable { case video, audio, ended }
        public var kind: Kind
        public var width = 0
        public var height = 0
        /// Clockwise degrees the picture is to be turned to stand upright.
        public var rotation = 0
        public var timestampMicros: Int64 = 0
        public var channels = 0
        public var rate = 0
        public var length = 0

        public init(kind: Kind) {
            self.kind = kind
        }
    }

    public static var socketURL: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?.appendingPathComponent(socketName)
    }

    public static func frame(_ header: Header, payload: Data) -> Data {
        var header = header
        header.length = payload.count
        let encoded = (try? JSONEncoder().encode(header)) ?? Data()
        var out = Data(capacity: 4 + encoded.count + payload.count)
        var size = UInt32(encoded.count).bigEndian
        withUnsafeBytes(of: &size) { out.append(contentsOf: $0) }
        out.append(encoded)
        out.append(payload)
        return out
    }

    /// Takes whole messages off the front of `buffer`, leaving any partial
    /// one for the next read.
    public static func take(from buffer: inout Data) -> [(Header, Data)] {
        var out: [(Header, Data)] = []
        while buffer.count >= 4 {
            let size = buffer.prefix(4).reduce(0) { $0 << 8 | Int($1) }
            guard buffer.count >= 4 + size,
                  let header = try? JSONDecoder().decode(Header.self, from: buffer.subdata(in: buffer.startIndex + 4 ..< buffer.startIndex + 4 + size)) else { break }
            let start = buffer.startIndex + 4 + size
            guard buffer.count >= 4 + size + header.length else { break }
            out.append((header, buffer.subdata(in: start ..< start + header.length)))
            buffer.removeSubrange(buffer.startIndex ..< start + header.length)
        }
        return out
    }
}
