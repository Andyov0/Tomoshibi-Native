package app.tomoshibi.ui.music

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import app.tomoshibi.basics.Appearance
import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import app.tomoshibi.feature.music.MusicTab
import app.tomoshibi.feature.music.SongDesk
import app.tomoshibi.feature.music.TestDeskMachinery
import app.tomoshibi.feature.music.deskState
import app.tomoshibi.feature.music.entry
import app.tomoshibi.feature.watch.TestDeployment
import app.tomoshibi.feature.watch.callModel
import app.tomoshibi.model.CallModel
import app.tomoshibi.ui.kit.Backdrop
import app.tomoshibi.ui.kit.GlassBackdrop
import app.tomoshibi.ui.kit.glassSource
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.Dispatchers
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.MusicLibrary
import uniffi.tomoshibi_protocol.MusicLink
import uniffi.tomoshibi_protocol.MusicLinkKind
import uniffi.tomoshibi_protocol.MusicQuality
import uniffi.tomoshibi_protocol.MusicTrack

/**
 * Pictures of the music panel in its states, drawn from a desk stood in for,
 * at a phone's width and at a tablet's (the panel as wide as the room makes
 * it beside the call), in English and in simplified Chinese. Recorded with
 * `./gradlew :app:recordRoborazziDebug` into src/test/screenshots/music.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h900dp-hdpi")
@OptIn(ExperimentalRoborazziApi::class)
class MusicScreenshotTest(private val language: Language, private val width: Int) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}-{1}")
        fun cases() = listOf(Language.En, Language.ZhHans).flatMap { language -> listOf(412, 840).map { arrayOf<Any>(language, it) } }

        const val DIRECTORY = "src/test/screenshots/music"
    }

    @get:Rule
    val compose = createComposeRule()

    private val machinery = TestDeskMachinery()

    @Before
    fun setUp() {
        RuntimeEnvironment.setQualifiers("w${width}dp-h900dp-hdpi")
        val context = ApplicationProvider.getApplicationContext<Context>()
        Localization.install(context).choose(language)
        Appearance.install(context).keeps = false
    }

    private val panelWidth: Dp get() = if (width < 360 + 24 + 172 + 22) (width - 24).dp else 360.dp

    private lateinit var model: CallModel

    private fun desk(libraries: Boolean = true): SongDesk {
        model = callModel(deployment = TestDeployment(), context = Dispatchers.Main.immediate)
        val desk = SongDesk(model, machinery, live = false)
        if (libraries) {
            desk.libraries = listOf(MusicLibrary("netease", "NetEase", true), MusicLibrary("qq", "QQ Music", false))
            desk.searchSource = "netease"
        }
        return desk
    }

    private fun shoot(desk: SongDesk, name: String) {
        compose.setContent {
            GlassBackdrop {
                Box(Modifier.fillMaxWidth().testTag("page")) {
                    Box(Modifier.matchParentSize().background(Theme.background))
                    Backdrop(Modifier.matchParentSize().glassSource())
                    Box(Modifier.padding(12.dp)) { MusicPanelContent(desk, close = {}, modifier = Modifier.width(panelWidth)) }
                }
            }
        }
        compose.mainClock.advanceTimeBy(1500)
        compose.waitForIdle()
        compose.onNodeWithTag("page").captureRoboImage("$DIRECTORY/$name-${language.code}-$width.png")
    }

    private val queue = listOf(
        entry("2", title = "Gymnopédie No. 1", artists = listOf("Erik Satie"), by = "Bea"),
        entry("3", title = "Clair de lune", artists = listOf("Claude Debussy"), by = "Me"),
        entry("4", title = "Arabesque No. 1", artists = listOf("Claude Debussy"), by = "Cy"),
    )

    @Test
    fun idleForSomebodyWithNoLibrary() = shoot(desk(libraries = false), "idle-guest")

    @Test
    fun idleForSomebodyWhoCanStartTheDesk() = shoot(desk(), "idle-library")

    @Test
    fun aDeskSomebodyElseRunsHeardByAListener() {
        val desk = desk()
        desk.state = deskState(
            now = entry("1", title = "Moonlight Sonata, I. Adagio sostenuto", artists = listOf("Ludwig van Beethoven", "Daniel Barenboim")),
            queue = queue, votes = listOf("g-cy"), needed = 2u, quality = "FLAC · 16-bit · 44.1 kHz",
        )
        shoot(desk, "listening")
    }

    @Test
    fun theHoldersOwnDeskPausedWithTheNextLoading() {
        val desk = desk()
        desk.mine = true
        desk.state = deskState(
            now = entry("1", title = "Moonlight Sonata, I. Adagio sostenuto", artists = listOf("Ludwig van Beethoven")),
            queue = queue, next = entry("n", title = "Für Elise"), paused = true, quality = "FLAC · 24-bit · 96 kHz",
        )
        shoot(desk, "holding")
    }

    @Test
    fun musicFromSomebodyWhoseDeskIsNotHeard() {
        val desk = desk(libraries = false)
        val music = TrackInfo("TR_music", MediaKind.AUDIO, MediaSource.UNKNOWN, "music", false, 0u, 0u, false, true)
        model.apply(CallEvent.ParticipantJoined(ParticipantInfo("g-shot-bea", "Bea", emptyMap(), listOf(music))))
        shoot(desk, "someones-music")
    }

    @Test
    fun searchResultsWithOneAdded() {
        val desk = desk()
        desk.tab = MusicTab.Search
        desk.searchQuery = "debussy"
        desk.searchResults = listOf(
            MusicTrack("a", "Clair de lune", listOf("Claude Debussy"), "Suite bergamasque", "", 301.0),
            MusicTrack("b", "Rêverie", listOf("Claude Debussy", "Zoltán Kocsis"), "Piano Works", "", 262.0),
            MusicTrack("c", "La fille aux cheveux de lin", listOf("Claude Debussy"), "Préludes", "", 151.0),
        )
        desk.searchAdded = setOf("b")
        shoot(desk, "search-results")
    }

    @Test
    fun searchingASignedOutLibraryAtTheHighestQuality() {
        val desk = desk()
        desk.tab = MusicTab.Search
        desk.searchSource = "qq"
        desk.quality = MusicQuality.BEST
        shoot(desk, "search-notes")
    }

    @Test
    fun aSearchTheLibraryDidNotAnswer() {
        val desk = desk()
        desk.tab = MusicTab.Search
        desk.searchQuery = "satie"
        desk.searchProblem = t("The library did not answer. Try again.")
        desk.searchResults = emptyList()
        shoot(desk, "search-problem")
    }

    @Test
    fun aPlaylistOpened() {
        val desk = desk()
        desk.tab = MusicTab.Playlist
        desk.pasted = "https://music.example/playlist?id=42"
        desk.linked = MusicLink(
            "netease", MusicLinkKind.PLAYLIST, "Rainy afternoon piano",
            listOf(
                MusicTrack("p1", "Nocturne Op. 9 No. 2", listOf("Frédéric Chopin"), "Nocturnes", "", 270.0),
                MusicTrack("p2", "Clair de lune", listOf("Claude Debussy"), "Suite bergamasque", "", 301.0),
                MusicTrack("p3", "Gymnopédie No. 1", listOf("Erik Satie"), "Gymnopédies", "", 185.0),
            ),
        )
        desk.linkAdded = setOf("p1")
        shoot(desk, "playlist")
    }

    @Test
    fun aLinkThatNamesNothing() {
        val desk = desk()
        desk.tab = MusicTab.Playlist
        desk.pasted = "https://example.org/not-music"
        desk.linkProblem = t("That link names no playlist or song.")
        shoot(desk, "playlist-problem")
    }
}
