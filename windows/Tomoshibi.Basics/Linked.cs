using System.Text.RegularExpressions;

namespace Tomoshibi.Basics;

/// <summary>
/// Text with its addresses found, so that a message can be drawn with them
/// clickable (components/room/Linked.tsx; Android's basics/Linked.kt).
///
/// Only a complete address, over http or https. Guessing at bare hosts turns
/// "e.g. see chapter 3" into a link to a domain nobody meant, and a false
/// link is worse than a missed one: it is clickable, it goes somewhere, and
/// it looks deliberate. Trailing punctuation is left out, because a sentence
/// that ends in an address is written with a full stop after it.
/// </summary>
public static partial class Linked
{
    public abstract record Piece
    {
        public sealed record Text(string Words) : Piece;
        public sealed record Link(string Href) : Piece;
    }

    /// <summary>
    /// The web's <c>URLS</c>, with its <c>\b</c> written out as "no ASCII word
    /// character before": in JavaScript without the <c>u</c> flag a word
    /// character is ASCII, so an address straight after Chinese or Japanese
    /// text is still found. JavaScript's <c>\s</c> is written out too: .NET's
    /// is wider (it counts NEL, U+0085, as a space, which a browser does not),
    /// and an address must end where it ends in a browser.
    /// </summary>
    private const string Space = @"\t\n\u000B\u000C\r    -     　﻿";

    private static readonly Regex Addresses = new(
        @"(?<![A-Za-z0-9_])https?://[^" + Space + @"<>()\[\]{}]+[^" + Space + @"<>()\[\]{}.,;:!?'""]",
        RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);

    /// <summary>The text cut into its words and its addresses, in order; joined again they are the text exactly.</summary>
    public static IReadOnlyList<Piece> Pieces(string text)
    {
        var pieces = new List<Piece>();
        var at = 0;
        foreach (Match match in Addresses.Matches(text))
        {
            if (match.Index > at) pieces.Add(new Piece.Text(text[at..match.Index]));
            pieces.Add(new Piece.Link(match.Value));
            at = match.Index + match.Length;
        }
        if (at < text.Length) pieces.Add(new Piece.Text(text[at..]));
        return pieces;
    }

    /// <summary>The addresses alone.</summary>
    public static IReadOnlyList<string> Links(string text) =>
        Pieces(text).OfType<Piece.Link>().Select(link => link.Href).ToList();
}
