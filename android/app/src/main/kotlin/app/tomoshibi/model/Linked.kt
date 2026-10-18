package app.tomoshibi.model

/**
 * Text with its addresses found, so that a message can be drawn with them
 * clickable (components/room/Linked.tsx).
 *
 * Only a complete address, over http or https. Guessing at bare hosts turns
 * "e.g. see chapter 3" into a link to a domain nobody meant, and a false link
 * is worse than a missed one: it is clickable, it goes somewhere, and it looks
 * deliberate. Trailing punctuation is left out, because a sentence that ends
 * in an address is written with a full stop after it.
 */
object Linked {
    sealed class Piece {
        data class Text(val text: String) : Piece()
        data class Link(val href: String) : Piece()
    }

    /**
     * The web's `URLS`, with its `\b` written out as "no ASCII word
     * character before": in JavaScript without the `u` flag a word character
     * is ASCII, so an address straight after Chinese or Japanese text is still
     * found, where a Unicode-aware `\b` would count those letters as word
     * characters and miss it. JavaScript's `\s` is written out too, because
     * Java's is ASCII alone and an address followed by an ideographic space
     * would otherwise run on into the next word.
     */
    private const val SPACE = "\\t\\n\\u000B\\u000C\\r \\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000\\uFEFF"
    private val addresses = Regex(
        "(?<![A-Za-z0-9_])https?://[^$SPACE<>()\\[\\]{}]+[^$SPACE<>()\\[\\]{}.,;:!?'\"]",
        RegexOption.IGNORE_CASE,
    )

    /** [text] cut into its words and its addresses, in order; joined again they are [text] exactly. */
    fun pieces(text: String): List<Piece> {
        val pieces = ArrayList<Piece>()
        var at = 0
        for (match in addresses.findAll(text)) {
            if (match.range.first > at) pieces.add(Piece.Text(text.substring(at, match.range.first)))
            pieces.add(Piece.Link(match.value))
            at = match.range.last + 1
        }
        if (at < text.length) pieces.add(Piece.Text(text.substring(at)))
        return pieces
    }

    /** The addresses alone. */
    fun links(text: String): List<String> = pieces(text).mapNotNull { (it as? Piece.Link)?.href }
}
