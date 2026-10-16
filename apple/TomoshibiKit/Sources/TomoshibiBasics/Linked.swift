import Foundation

/// Text with its addresses found, so that a message can be drawn with them
/// clickable (components/room/Linked.tsx).
///
/// The commonest thing anybody types during a call is an address, and drawn
/// as plain characters it has to be copied out by hand. The rules are the
/// web's, so that one message is linked the same way in a browser and here.
///
/// Only a complete address, over http or https. Guessing at bare hosts turns
/// "e.g. see chapter 3" into a link to a domain nobody meant, and a false link
/// is worse than a missed one: it is clickable, it goes somewhere, and it
/// looks deliberate.
///
/// Trailing punctuation is left out, because a sentence that ends in an
/// address is written with a full stop after it and the stop is not part of
/// the address.
public enum Linked {
    public enum Piece: Equatable, Sendable {
        case text(String)
        case link(String)
    }

    /// The web's `URLS`, `/\bhttps?:\/\/[^\s<>()[\]{}]+[^\s<>()[\]{}.,;:!?'"]/gi`,
    /// with its `\b` written out. In JavaScript without the `u` flag a word
    /// character is ASCII, so an address straight after Chinese or Japanese
    /// text, with no space, is still found; ICU's `\b` counts those letters
    /// as word characters, so "见https://example.com" would be linked in a
    /// browser and not here.
    private static let addresses = try! NSRegularExpression(
        pattern: #"(?<![A-Za-z0-9_])https?://[^\s<>()\[\]{}]+[^\s<>()\[\]{}.,;:!?'"]"#,
        options: [.caseInsensitive]
    )

    /// `text` cut into its words and its addresses, in order; joined again
    /// they are `text` exactly.
    public static func pieces(_ text: String) -> [Piece] {
        let whole = text as NSString
        var pieces: [Piece] = []
        var at = 0
        for match in addresses.matches(in: text, range: NSRange(location: 0, length: whole.length)) {
            if match.range.location > at {
                pieces.append(.text(whole.substring(with: NSRange(location: at, length: match.range.location - at))))
            }
            pieces.append(.link(whole.substring(with: match.range)))
            at = match.range.location + match.range.length
        }
        if at < whole.length { pieces.append(.text(whole.substring(from: at))) }
        return pieces
    }

    /// The addresses alone.
    public static func links(_ text: String) -> [String] {
        pieces(text).compactMap { if case let .link(href) = $0 { href } else { nil } }
    }
}
