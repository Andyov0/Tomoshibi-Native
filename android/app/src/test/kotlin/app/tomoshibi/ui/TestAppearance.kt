package app.tomoshibi.ui

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalConfiguration
import app.tomoshibi.basics.Palette
import app.tomoshibi.basics.Theme
import app.tomoshibi.ui.kit.KitPalette

/**
 * The appearance the pictures are drawn in: the dark, as they always were,
 * unless the run asks for the light (`-Pappearance=light`, which the build
 * hands to the tests as `tomoshibi.appearance`). Either run records into the
 * same places, so that a light picture lies where its dark one would be
 * compared; keep a copy of each run.
 *
 * Robolectric's own configuration is not in night mode, which would draw
 * the app in the light in every run: the system's mode is said here too, as
 * TomoshibiApp reads it.
 */
object TestAppearance {
    val light: Boolean get() = System.getProperty("tomoshibi.appearance") == "light"
    val palette: Palette get() = if (light) Palette.Light else Palette.Dark
}

/** [content] drawn as a screen outside a call is, in the run's appearance. */
@Composable
fun InAppearance(content: @Composable () -> Unit) = Appearing(TestAppearance.palette, content)

/**
 * [content] drawn as everything inside a call is: in the meeting's palette
 * (Theme.MEETING_STAYS_DARK), whatever the run's appearance.
 */
@Composable
fun InMeeting(content: @Composable () -> Unit) = Appearing(Theme.meeting(TestAppearance.palette), content)

@Composable
private fun Appearing(palette: Palette, content: @Composable () -> Unit) {
    val base = LocalConfiguration.current
    val night = if (TestAppearance.light) Configuration.UI_MODE_NIGHT_NO else Configuration.UI_MODE_NIGHT_YES
    val configuration = remember(base, night) {
        Configuration(base).apply { uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or night }
    }
    CompositionLocalProvider(LocalConfiguration provides configuration) { KitPalette(palette, content) }
}
