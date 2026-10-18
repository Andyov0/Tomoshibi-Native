package app.tomoshibi.feature.music

import app.tomoshibi.feature.watch.TestCall
import app.tomoshibi.feature.watch.TestDeployment
import app.tomoshibi.feature.watch.callModel
import app.tomoshibi.media.CallAudio
import app.tomoshibi.media.NoAudioRouting
import app.tomoshibi.model.CallModel
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import app.tomoshibi.model.host.standInRules
import org.junit.After
import org.junit.Before
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_core.MediaKind
import uniffi.tomoshibi_core.ParticipantInfo
import uniffi.tomoshibi_core.TrackInfo
import uniffi.tomoshibi_protocol.DeskAction
import uniffi.tomoshibi_protocol.MediaSource
import uniffi.tomoshibi_protocol.MusicLibrary
import uniffi.tomoshibi_protocol.MusicTrack

/*
 * The app's side of the song desk (MusicFeature.swift `SongDesk`): carrying
 * out what the protocol's desk asks -- a song fetched, abandoned, published,
 * stopped -- and the box that stays busy while an ask is looked up. The desk's
 * own rules are the core's and are tested there; this stands in for it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SongDeskTest {
    private val call = TestCall()
    private val deployment = TestDeployment()
    private val machinery = TestDeskMachinery()
    private val desk get() = machinery.desk
    private val audio = CallAudio(NoAudioRouting, muted = true)

    /** The sound panel's record files a setting by the identity's signature, which is the core's to read. */
    @Before
    fun standIn() {
        standInRules()
    }

    @After
    fun stopTheMixer() {
        audio.stop()
    }

    /** A call with its desk, and the call's scope cancelled however the test ends, or a failing test hangs on the desk's clock. */
    private fun desking(test: suspend TestScope.(CallModel, SongDesk) -> Unit) = runTest {
        val model = callModel(call, deployment, StandardTestDispatcher(testScheduler), audio = audio)
        val songs = SongDesk(model, machinery)
        runCurrent()
        try {
            test(model, songs)
        } finally {
            model.scope.cancel()
        }
    }

    /** Has the desk carry out [actions], as it would were the protocol to ask for them. */
    private fun SongDesk.carryOut(vararg actions: DeskAction) {
        desk.next = deskOutput(actions = actions.toList())
        skip()
    }

    @Test
    fun anAskKeepsTheBoxBusyUntilItsSearchIsAnswered() {
        deployment.libraries = listOf(MusicLibrary("netease", "NetEase", true))
        val answer = CompletableDeferred<List<MusicTrack>>()
        deployment.search = { _, _ -> answer.await() }
        desk.onAsk = { deskOutput(actions = listOf(DeskAction.Search(5u, it, true))) }
        desk.onFound = { _, found -> deskOutput(accepted = found.isNotEmpty()) }
        desking { model, songs -> busyUntilAnswered(model, songs, answer) }
    }

    private fun TestScope.busyUntilAnswered(model: CallModel, songs: SongDesk, answer: CompletableDeferred<List<MusicTrack>>) {
        assertNotNull(songs.libraries)

        songs.ask("  clair de lune ")
        // Nobody ran a desk and this person can reach the library: one starts here first.
        assertEquals(listOf("start", "ask clair de lune"), desk.told)
        assertTrue(songs.asking)
        runCurrent()
        assertTrue(songs.asking)
        // A second ask while the first is looked up is refused, as the web's box refuses it.
        songs.ask("another")
        assertEquals(2, desk.told.size)

        answer.complete(listOf(MusicTrack("1", "Clair de lune", listOf("Debussy"), "Suite bergamasque", "", 300.0)))
        runCurrent()
        assertFalse(songs.asking)
        assertEquals("found 5 [Clair de lune]", desk.told.last())
        assertEquals(emptyList<String>(), model.toasts.map { it.title })
    }

    @Test
    fun anAskTheLibraryDidNotAnswerIsSaidAndTheBoxIsFreed() {
        deployment.libraries = listOf(MusicLibrary("netease", "NetEase", true))
        deployment.search = { _, _ -> throw IllegalStateException("the library is down") }
        desk.onAsk = { deskOutput(actions = listOf(DeskAction.Search(6u, it, true))) }
        desk.onFound = { _, _ -> deskOutput(accepted = false) }
        desking { model, songs ->
            songs.ask("clair de lune")
            runCurrent()
            assertFalse(songs.asking)
            assertEquals(listOf("The library did not answer. Try again."), model.toasts.map { it.title })
        }
    }

    @Test
    fun aSongTakenOffWhileItLoadsIsNeverToldToTheDesk() = desking { model, songs ->
        val waiting = CompletableDeferred<MusicLoader.Outcome>()
        machinery.loads["a"] = waiting
        songs.carryOut(DeskAction.Prepare(1u, entry("a")))
        runCurrent()
        assertEquals(listOf("a"), machinery.loaded)
        songs.carryOut(DeskAction.Abandon(1u))
        waiting.complete(MusicLoader.Outcome.Ready(silentSong()))
        runCurrent()
        assertTrue(desk.told.none { it.startsWith("prepared") })
    }

    @Test
    fun whatCameOfEachSongIsToldToTheDesk() = desking { model, songs ->
        machinery.loads["a"] = CompletableDeferred(MusicLoader.Outcome.Ready(silentSong("FLAC · 24-bit · 96 kHz")))
        machinery.loads["b"] = CompletableDeferred(MusicLoader.Outcome.Unavailable)
        songs.carryOut(DeskAction.Prepare(1u, entry("a")), DeskAction.Prepare(2u, entry("b")), DeskAction.Prepare(3u, entry("c")))
        runCurrent()
        assertEquals(
            listOf("prepared 1 Ready(quality=FLAC · 24-bit · 96 kHz)", "prepared 2 Unavailable", "prepared 3 Failed"),
            desk.told.filter { it.startsWith("prepared") },
        )
    }

    /**
     * One track for all the songs that follow, as the web keeps one, so that
     * everybody hears the next song without the stream starting again; and
     * taken down when the music stops.
     */
    @Test
    fun theMusicsTrackIsPublishedOnceAndTakenDownWhenTheMusicStops() = desking { model, songs ->
        machinery.loads["a"] = CompletableDeferred(MusicLoader.Outcome.Ready(silentSong()))
        machinery.loads["b"] = CompletableDeferred(MusicLoader.Outcome.Ready(silentSong()))
        songs.carryOut(DeskAction.Prepare(1u, entry("a")))
        runCurrent()
        songs.carryOut(DeskAction.Play(1u, entry("a")))
        runCurrent()
        assertTrue(songs.playingHere)
        songs.carryOut(DeskAction.Prepare(2u, entry("b")))
        runCurrent()
        songs.carryOut(DeskAction.Play(2u, entry("b")))
        runCurrent()
        assertEquals(1, machinery.published)
        assertEquals(listOf("played 1 true", "played 2 true"), desk.told.filter { it.startsWith("played") })
        songs.carryOut(DeskAction.Stop)
        runCurrent()
        assertEquals(1, machinery.unpublished)
        assertFalse(songs.playingHere)
    }

    /** sound.ts: "a stop that arrives in between used to be outrun", and the room was left a track of silence. */
    @Test
    fun aStopThatOvertakesThePublishingTakesTheTrackDownAgain() = desking { model, songs ->
        machinery.publishing = CompletableDeferred()
        machinery.loads["a"] = CompletableDeferred(MusicLoader.Outcome.Ready(silentSong()))
        songs.carryOut(DeskAction.Prepare(1u, entry("a")))
        runCurrent()
        songs.carryOut(DeskAction.Play(1u, entry("a")))
        runCurrent()
        songs.carryOut(DeskAction.Stop)
        machinery.publishing!!.complete(Unit)
        runCurrent()
        assertEquals(1, machinery.published)
        assertEquals(1, machinery.unpublished)
        assertEquals(listOf("played 1 false"), desk.told.filter { it.startsWith("played") })
        assertFalse(songs.playingHere)
    }

    @Test
    fun aSongTheLibraryCannotPlayAndADeskNobodyRunsAreSaidInTheCorner() = desking { model, songs ->
        desk.next = deskOutput(skipped = listOf(entry("x", title = "Song X")))
        songs.skip()
        desk.onAdd = { deskOutput(accepted = false) }
        songs.add(listOf(SongDesk.song(MusicTrack("1", "Song Y", emptyList(), "", "", 1.0), "netease")))
        assertEquals(
            listOf("Skipped Song X: the library's account cannot play it.", "Nobody is running the song desk."),
            model.toasts.map { it.title },
        )
    }

    /**
     * The listener's volume for somebody's music reaches the core, which
     * blocks a silenced song at the server: a volume of nought counts as
     * silenced (hearing.ts `silenced`), or a stream asked for and played at
     * nothing is upload spent on nobody.
     */
    @Test
    fun theListenersVolumeForSomebodysMusicReachesTheCall() = desking { model, songs ->
        val music = TrackInfo("TR_music", MediaKind.AUDIO, MediaSource.UNKNOWN, "music", false, 0u, 0u, false, true)
        model.apply(CallEvent.ParticipantJoined(ParticipantInfo("g-desk-bea", "Bea", emptyMap(), listOf(music))))
        songs.setVolume("g-desk-bea", 0.5)
        runCurrent()
        songs.setVolume("g-desk-bea", 0.0)
        runCurrent()
        assertEquals(
            listOf("g-desk-bea music=true level=0.5 blocked=false", "g-desk-bea music=true level=0.0 blocked=true"),
            call.hearing,
        )
        // Somebody with no music is nobody to tell.
        songs.setVolume("g-desk-nobody", 0.3)
        runCurrent()
        assertEquals(2, call.hearing.size)
    }
}
