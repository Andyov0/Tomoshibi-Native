package app.tomoshibi.basics

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The two palettes (the daylight palette, 2026-10-26). The dark is held to
 * the numbers the app always had, since every dark screen is to stay pixel
 * for pixel what it was; the light to the shared table every platform draws
 * from.
 */
class PaletteTest {
    @Test
    fun theDarkIsWhatTheAppAlwaysWas() {
        val dark = Palette.Dark
        assertEquals(Color(0.043f, 0.043f, 0.051f), dark.background)
        assertEquals(Color(0.110f, 0.110f, 0.118f), dark.surface)
        assertEquals(Color(0.173f, 0.173f, 0.180f), dark.surfaceHigh)
        assertEquals(Color.White.copy(alpha = 0.10f), dark.border)
        assertEquals(Color(0.961f, 0.961f, 0.969f), dark.foreground)
        assertEquals(Color(0.922f, 0.922f, 0.961f).copy(alpha = 0.6f), dark.muted)
        assertEquals(Color(0.188f, 0.820f, 0.345f), dark.good)
        assertEquals(Color(1.0f, 0.271f, 0.227f), dark.danger)
        assertEquals(Color.Black, dark.ground)
        assertEquals(Color.White, dark.wash)
        // Strengths given for the dark come back untouched in it.
        assertEquals(Color.Black.copy(alpha = 0.22f), dark.recess(0.22f))
        assertEquals(0.45f, dark.shade(0.45f))
        assertEquals(Color.White.copy(alpha = 0.07f), dark.plate(0.07f))
        for (accent in Accent.entries) assertEquals(accent.color, dark.tally(accent))
    }

    @Test
    fun theLightIsTheSharedTable() {
        val light = Palette.Light
        assertEquals(Color(0xFFF2F2F7), light.background)
        assertEquals(Color(0xFFFFFFFF), light.surface)
        assertEquals(Color(0xFFE5E5EA), light.surfaceHigh)
        assertEquals(Color.Black.copy(alpha = 0.10f), light.border)
        assertEquals(Color(0xFF1C1C1E), light.foreground)
        assertEquals(Color(60, 60, 67).copy(alpha = 0.6f), light.muted)
        assertEquals(Color(0xFF34C759), light.good)
        assertEquals(Color(0xFFFF3B30), light.danger)
        assertEquals(Color.White, light.dangerForeground)
        val accents = mapOf(
            Accent.Amber to 0xFFF2901E, Accent.Orange to 0xFFFF9500, Accent.Pink to 0xFFFF2D55,
            Accent.Purple to 0xFFAF52DE, Accent.Indigo to 0xFF5856D6, Accent.Blue to 0xFF007AFF,
            Accent.Teal to 0xFF30B0C7, Accent.Green to 0xFF34C759, Accent.Graphite to 0xFF8E8E93,
        )
        for ((accent, value) in accents) assertEquals(accent.name, Color(value), light.tally(accent))
    }

    @Test
    fun eitherEndOfTheTurnIsThePaletteItself() {
        // A blend at nought taken through the blend's colour space came back
        // a shade off the dark, which every dark screen would have shown.
        assertSame(Palette.Dark, Palette.between(0f))
        assertSame(Palette.Light, Palette.between(1f))
        val half = Palette.between(0.5f)
        assertTrue(half.background.red > Palette.Dark.background.red && half.background.red < Palette.Light.background.red)
    }
}
