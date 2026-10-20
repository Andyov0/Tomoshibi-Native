import Foundation

/// Which colour a person's monogram wears (TomoshibiKit's `Monogram`), kept
/// here, apart from the view, so that a test can hold it: TomoshibiKit's own
/// test bundle would link the core and does not run.
///
/// FNV-1a over the identity's UTF-8, as the tile's avatar hashes it, modulo
/// the number of colours. It is the first of the two colours the monogram's
/// blend was drawn from until 2026-10-20, so a person kept their colour when
/// the blend went flat; Android's `Monograms.index` (ui/kit/Notices.kt) is the
/// same arithmetic and is held to the same answers.
public enum Monograms {
    public static func index(_ identity: String, of count: Int) -> Int {
        var hash: UInt64 = 1469598103934665603
        for byte in identity.utf8 { hash = (hash ^ UInt64(byte)) &* 1099511628211 }
        return Int(hash % UInt64(count))
    }
}
