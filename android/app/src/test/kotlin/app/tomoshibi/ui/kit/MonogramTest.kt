package app.tomoshibi.ui.kit

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * A person's monogram is the same two colours and the same letters on
 * Android as on the iPhone and the Mac. The expected values are what
 * Monogram.pair and Monogram.initials in Kit+Notices.swift give for the same
 * identities and names, run in Swift (2026-10-18), so a port that drifts from
 * the Swift arithmetic (signed overflow, another hash, a letter cut through a
 * character) fails here rather than in somebody's call.
 */
class MonogramTest {
    @Test
    fun theColoursAreTheSwiftOnesForTheSameIdentity() {
        val swift = mapOf(
            "" to (1 to 2),
            "a" to (0 to 4),
            "andy" to (5 to 1),
            "Andy" to (1 to 2),
            "guest-7f3a9c" to (1 to 4),
            "sig:Zq3n0Vf8" to (2 to 5),
            "张伟" to (3 to 1),
            "田中さん" to (4 to 5),
            "😀x" to (0 to 2),
            "0f9e2d4c-8b1a-4f6e-9c3d-2a7b5e1f0c84" to (2 to 3),
            "participant-1" to (0 to 5),
            "participant-2" to (5 to 4),
            "participant-3" to (4 to 5),
        )
        for ((identity, pair) in swift) assertEquals("for \"$identity\"", pair, Monograms.indices(identity))
    }

    @Test
    fun theLettersAreTheSwiftOnes() {
        val swift = mapOf(
            "Andy Wang" to "AW",
            "andy" to "A",
            "  mei   lin  " to "ML",
            "张 伟" to "张伟",
            "田中 さん" to "田さ",
            "é clair" to "ÉC",
            "😀 smile" to "😀S",
            "" to "?",
            "   " to "?",
            "óscar wilde" to "ÓW",
            "a b c" to "AB",
        )
        for ((name, letters) in swift) assertEquals("for \"$name\"", letters, Monograms.initials(name))
    }
}
