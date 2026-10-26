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
        // The dark writes in its one colour.
        for (accent in Accent.entries) assertEquals(accent.color, dark.tallyInk(accent))
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

    /**
     * The palette's addendum: the person's colour as words or a thin symbol
     * in the light is a darker ink, at least 4.5 to one on the light page
     * (#F2F2F7), where several of the light fills were under three and
     * amber about 2.3. Worked out as WCAG works out contrast.
     */
    @Test
    fun theLightInksReadOnThePage() {
        val inks = mapOf(
            Accent.Amber to 0xFFA35C09, Accent.Orange to 0xFFA15E00, Accent.Pink to 0xFFDF002B,
            Accent.Purple to 0xFFA338D9, Accent.Indigo to 0xFF5856D6, Accent.Blue to 0xFF006BE0,
            Accent.Teal to 0xFF217989, Accent.Green to 0xFF217E38, Accent.Graphite to 0xFF6E6E73,
        )
        val page = Palette.Light.background
        assertEquals(Color(0xFFF2F2F7), page)
        for ((accent, value) in inks) {
            val ink = Palette.Light.tallyInk(accent)
            assertEquals(accent.name, Color(value), ink)
            val ratio = contrast(ink, page)
            assertTrue("${accent.name}'s ink is ${"%.2f".format(ratio)} to one on the page", ratio >= 4.5)
        }
        // The fill is what failed: the guard is not passing for want of anything to catch.
        assertTrue(contrast(Palette.Light.tally(Accent.Amber), page) < 3.0)
    }

    private fun contrast(a: Color, b: Color): Double {
        fun channel(c: Float): Double = if (c <= 0.04045f) c / 12.92 else Math.pow((c + 0.055) / 1.055, 2.4)
        fun luminance(c: Color) = 0.2126 * channel(c.red) + 0.7152 * channel(c.green) + 0.0722 * channel(c.blue)
        val (light, dark) = listOf(luminance(a), luminance(b)).sortedDescending()
        return (light + 0.05) / (dark + 0.05)
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
