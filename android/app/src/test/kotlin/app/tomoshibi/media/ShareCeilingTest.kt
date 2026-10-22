package app.tomoshibi.media

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.tomoshibi_protocol.ShareQuality

/**
 * The deployment's ceiling over a shared screen (ShareCeiling) and the share
 * that is made under it (Sharing `plan`): the lower of what was chosen and
 * what is allowed, always one of the size's own rates, its frames slower
 * while the device is hot, and its picture no taller than allowed. A
 * ceiling of nothing, or of nought, changes nothing.
 */
class ShareCeilingTest {
    /** A screen 2400 high in its own pixels, held to the shorter side a rate asks for as the core would at 1080p. */
    private val fit: (Int) -> Pair<Int, Int> = { _ -> 1080 to 2400 }

    @Test
    fun theRateIsTheLowerOfTheChosenAndTheCeilingAndOneTheSizeOffers() {
        val ceiling = ShareCeiling(fps = 24)
        // 1080p offers 15, 30, 60, 120, 240: under 24 is 15, never 24.
        assertEquals(15, ceiling.published(60, 1080))
        assertEquals(30, ShareCeiling(fps = 30).published(240, 1080))
        assertEquals("chosen under the ceiling", 15, ShareCeiling(fps = 30).published(15, 1080))
        assertEquals("every rate over it: the size's lowest", 15, ShareCeiling(fps = 5).published(60, 2160))
        // Automatic keeps its one rate; its frames are held at the capture.
        assertEquals(60, ceiling.published(60, 0))
        assertEquals(24, ceiling.captured(60, hot = false))
        assertEquals(60, ShareCeiling.NONE.published(60, 0))
    }

    @Test
    fun aHotDeviceTakesFramesSlowerAndOnlyWhileHot() {
        val ceiling = ShareCeiling(fps = 30, hotFps = 10)
        assertEquals(30, ceiling.captured(30, hot = false))
        assertEquals(10, ceiling.captured(30, hot = true))
        assertEquals("no hot ceiling: as before", 30, ShareCeiling(fps = 30).captured(30, hot = true))
        assertEquals("never more than published", 15, ShareCeiling(hotFps = 20).captured(15, hot = true))
    }

    @Test
    fun aPictureTallerThanAllowedIsBroughtDownWithItsWidth() {
        val ceiling = ShareCeiling(height = 1280)
        assertEquals(576 to 1280, ceiling.size(1080, 2400))
        assertEquals("shorter than the ceiling", 1080 to 720, ceiling.size(1080, 720))
        // Even, as the encoder needs both.
        assertEquals(484 to 720, ShareCeiling(height = 721).size(1080, 1606))
    }

    @Test
    fun noughtOrANumberThatIsNotACountIsNoCeiling() {
        assertEquals(ShareCeiling.NONE, ShareCeiling.of(mapOf("screen_share_max_fps" to 0.0, "screen_share_max_height" to -5.0, "screen_share_hot_fps" to 0.5)))
        assertEquals(ShareCeiling(30, 720, 15), ShareCeiling.of(mapOf("screen_share_max_fps" to 30.0, "screen_share_max_height" to 720.9, "screen_share_hot_fps" to 15.0)))
        assertEquals(ShareCeiling.NONE, ShareCeiling.of(emptyMap()))
    }

    /** What a share is made at is the menu's choice under the ceiling the call points it at. */
    @Test
    fun aShareIsMadeUnderTheCeilingThatApplies() {
        val sharing = Sharing()
        val free = sharing.plan(ShareQuality.P1080, 120, hot = false, fit)
        assertEquals(Sharing.Setting(ShareQuality.P1080, 120), free.setting)
        assertEquals(1080 to 2400, free.size)
        assertEquals(120, free.captured)

        sharing.ceiling = { ShareCeiling(fps = 30, height = 1280, hotFps = 10) }
        val held = sharing.plan(ShareQuality.P1080, 120, hot = false, fit)
        assertEquals(Sharing.Setting(ShareQuality.P1080, 30), held.setting)
        assertEquals(576 to 1280, held.size)
        assertEquals(30, held.captured)
        assertEquals(10, sharing.plan(ShareQuality.P1080, 120, hot = true, fit).captured)
        // Chosen under it, the choice stands.
        assertEquals(15, sharing.plan(ShareQuality.P1080, 15, hot = false, fit).setting.fps)
    }
}
