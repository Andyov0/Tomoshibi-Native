package app.tomoshibi.ui.kit

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * A person's monogram is the same colour and the same letters on Android as
 * on the iPhone and the Mac. The expected values are what Monogram's hash
 * and Monogram.initials in Kit+Notices.swift give for the same identities and
 * names, run in Swift (2026-10-18), so a port that drifts from the Swift
 * arithmetic (signed overflow, another hash, a letter cut through a
 * character) fails here rather than in somebody's call.
 *
 * The monogram was a blend of two colours until 2026-10-20 and is now the
 * first of them alone; the expected indices are the first of each pair that
 * was recorded then, so a person kept the colour they had. Monograms.index in
 * TomoshibiBasics is held to the same answers (MonogramTests.swift).
 */
class MonogramTest {
    @Test
    fun theColourIsTheSwiftOneForTheSameIdentity() {
        val swift = mapOf(
            "" to 1,
            "a" to 0,
            "andy" to 5,
            "Andy" to 1,
            "guest-7f3a9c" to 1,
            "sig:Zq3n0Vf8" to 2,
            "张伟" to 3,
            "田中さん" to 4,
            "😀x" to 0,
            "0f9e2d4c-8b1a-4f6e-9c3d-2a7b5e1f0c84" to 2,
            "participant-1" to 0,
            "participant-2" to 5,
            "participant-3" to 4,
        )
        for ((identity, index) in swift) assertEquals("for \"$identity\"", index, Monograms.index(identity))
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
