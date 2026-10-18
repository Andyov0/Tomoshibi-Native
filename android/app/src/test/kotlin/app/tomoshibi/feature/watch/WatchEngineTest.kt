package app.tomoshibi.feature.watch

import app.tomoshibi.model.CallModel
import app.tomoshibi.model.MemoryDefaults
import app.tomoshibi.model.Rehearsal
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.tomoshibi_core.CallEvent
import uniffi.tomoshibi_protocol.ResolvedLink
import uniffi.tomoshibi_protocol.ResolvedPlayKind
import uniffi.tomoshibi_protocol.TicketVideo
import uniffi.tomoshibi_protocol.UnresolvedReason
import uniffi.tomoshibi_protocol.WatchAnswer
import uniffi.tomoshibi_protocol.WatchAsk
import uniffi.tomoshibi_protocol.WatchSnapshot

/*
 * The app's side of watching together (WatchFeature.swift): who reads a link
 * and what the show is handed, what is said when it is refused, the order the
 * show's words leave in, and a show that stays gone once the call is. The
 * protocol itself is the core's and is tested there; these stand in for it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class WatchEngineTest {
    private val call = TestCall()
    private val deployment = TestDeployment()
    private val machinery = TestMachinery()
    private val theatre get() = machinery.theatre

    /**
     * A call with its show, and the call's scope cancelled however the test
     * ends: left running, the show's beat keeps the test's scheduler busy for
     * ever, and a failing test hangs instead of failing.
     */
    private fun watching(sealed: Boolean = false, test: suspend TestScope.(CallModel, WatchEngine) -> Unit) = runTest {
        val model = callModel(call, deployment, StandardTestDispatcher(testScheduler), sealed = sealed)
        val engine = WatchEngine(model, machinery)
        runCurrent()
        try {
            test(model, engine)
        } finally {
            model.scope.cancel()
        }
    }

    private fun theirs(now: String? = "a") {
        theatre.snapshot = WatchSnapshot(show(now?.let { video(it) }), 0u, mine = false, offered = true, expected = 0.0)
    }

    @Test
    fun somebodyWhoMayNotReadLinksHandsTheLinkToWhoeverRunsTheShow() {
        deployment.ready = false
        watching { _, engine -> handsTheLinkOver(engine) }
    }

    private suspend fun TestScope.handsTheLinkOver(engine: WatchEngine) {
        assertFalse(engine.ready)
        val added = async { engine.add("  https://archive.example/general  ") }
        runCurrent()
        assertTrue(added.await())
        assertEquals(listOf("addLink https://archive.example/general"), theatre.asked)
        assertEquals(emptyList<String>(), deployment.resolved)
    }

    @Test
    fun somebodyWhoMayReadLinksStartsTheShowAndReadsTheLinkThemselves() = watching { model, engine ->
        assertTrue(engine.ready)
        theatre.onAddLink = { text -> output(asks = listOf(WatchAsk.Resolve(1u, text, false))) }
        deployment.resolve = { _, _ -> ResolvedLink.Playable(resolved("t-general")) }
        val added = async { engine.add("https://archive.example/general") }
        runCurrent()
        assertTrue(added.await())
        assertEquals("start", theatre.asked[0])
        assertEquals("addLink https://archive.example/general", theatre.asked[1])
        assertTrue(theatre.asked[2].startsWith("answer 1 Found(ticket=t-general"))
        // What this person read is known, and not asked of the server again.
        assertNotNull(engine.infos["t-general"]?.video)
    }

    @Test
    fun aLinkTheDeploymentRefusesIsSaidAndTheFieldKeepsIt() = watching { model, engine ->
        theatre.onAddLink = { text -> output(asks = listOf(WatchAsk.Resolve(2u, text, false))) }
        theatre.onAnswer = { _, answer -> output(refusals = listOf((answer as WatchAnswer.Refused).why)) }
        deployment.resolve = { _, _ -> ResolvedLink.NotPlayable(UnresolvedReason.MEMBERS_ONLY) }
        val added = async { engine.add("https://bilibili.example/bangumi/1") }
        runCurrent()
        assertFalse(added.await())
        assertTrue(theatre.asked.last().contains("Refused(why=members_only)"))
        assertEquals(listOf(WatchWords.refusal("members_only")), model.toasts.map { it.title })
    }

    /**
     * Read here under this person's grant, and only the ticket handed over:
     * with the link beside it for an ordinary video, never for a media
     * server's film, which a holder would read as its own choice.
     */
    @Test
    fun inSomebodyElsesShowTheLinkIsReadHereAndOnlyTheTicketGoes() = watching { model, engine ->
        theirs()
        engine.refresh(watchClock())
        deployment.resolve = { _, _ -> ResolvedLink.Playable(resolved("t-file")) }
        val file = async { engine.add("https://archive.example/file") }
        runCurrent()
        assertTrue(file.await())
        deployment.resolve = { _, _ -> ResolvedLink.Playable(resolved("t-film", kind = ResolvedPlayKind.RELAY, url = null)) }
        val film = async { engine.add("tomoshibi-media:emby/1") }
        runCurrent()
        assertTrue(film.await())
        assertEquals(listOf("addTicket t-file https://archive.example/file", "addTicket t-film null"), theatre.asked)
        assertEquals(listOf("https://archive.example/file forOther=false", "tomoshibi-media:emby/1 forOther=false"), deployment.resolved)
    }

    @Test
    fun aLinkThatCouldNotBeReadIsSaidAndNothingIsAdded() = watching { model, engine ->
        theirs()
        engine.refresh(watchClock())
        deployment.resolve = { _, _ -> throw IllegalStateException("the gateway is down") }
        val added = async { engine.add("https://archive.example/general") }
        runCurrent()
        assertFalse(added.await())
        assertEquals(emptyList<String>(), theatre.asked)
        assertEquals(listOf(WatchWords.refusal("failed")), model.toasts.map { it.title })
    }

    /** One at a time and in order: a state overtaken by the word that the show closed would have everybody believe in it again. */
    @Test
    fun theShowsWordsLeaveInTheOrderTheyWereSaid() = watching { model, engine ->
        theirs()
        engine.refresh(watchClock())
        theatre.onPause = { output(packets = listOf(packet("pause"), packet("state"))) }
        theatre.onClose = { output(packets = listOf(packet("closed"))) }
        engine.toggle()
        engine.stop()
        runCurrent()
        assertEquals(listOf("watch" to "pause", "watch" to "state", "watch" to "closed"), call.sent)
    }

    @Test
    fun theHolderSaysTheShowClosedBeforeTheCallGoes() = watching { model, engine ->
        theatre.snapshot = WatchSnapshot(show(video("a")), 0u, mine = true, offered = true, expected = 0.0)
        engine.refresh(watchClock())
        theatre.onClose = { output(packets = listOf(packet("closed"))) }
        val leaving = model.beforeLeaving[WatchEngine.PART]!!
        val done = async { leaving() }
        runCurrent()
        done.await()
        assertEquals(listOf("watch" to "closed"), call.sent)
    }

    @Test
    fun aVideoGetsAPlayerOfItsOwnAndOneThatStopsBeingPlayableTakesItWith() = watching { model, engine ->
        deployment.tickets = { TicketVideo.Known(resolved(it)) }
        theirs("a")
        engine.refresh(watchClock())
        runCurrent()
        val player = engine.player
        assertNotNull(player)
        assertEquals("a", player!!.video.key)
        // The relay stopped and the server now says it will not relay it.
        deployment.tickets = { TicketVideo.Unavailable }
        assertEquals(WatchInfo.Unavailable, engine.askAgain("t-a"))
        assertNull(engine.player)
        assertTrue(machinery.made.single().stopped)
    }

    /**
     * The fault on Apple: a video's description that arrived after the call
     * ended read the show again, found the last one still there, and started
     * the film on a call already left.
     */
    @Test
    fun aDescriptionThatArrivesAfterTheCallEndedStartsNothing() = watching { model, engine ->
        val answer = CompletableDeferred<TicketVideo>()
        deployment.tickets = { answer.await() }
        theirs("a")
        engine.refresh(watchClock())
        runCurrent()
        assertNull(engine.player)
        model.apply(CallEvent.Ended("RoomDeleted"))
        assertFalse(engine.onStage)
        answer.complete(TicketVideo.Known(resolved("t-a")))
        runCurrent()
        assertNull(engine.player)
        assertNull(engine.show)
        assertTrue(machinery.made.isEmpty())
    }

    @Test
    fun aSealedCallIsNotOfferedTheShowAtAll() = watching(sealed = true) { _, engine ->
        assertFalse(engine.ready)
        assertFalse(engine.add("https://archive.example/general"))
        assertEquals(emptyList<String>(), theatre.asked)
    }
}

/*
 * Where the video plays from (WatchPlayer.swift): the place it is, then the
 * relay, and what is said when neither plays; the stall that gives a source
 * up; and a relay-only video that stops part way.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class WatchPlayerTest {
    private val deployment = TestDeployment()
    private val machinery = TestMachinery()

    /** A player for [info], in a call whose scope is cancelled however the test ends. */
    private fun playing(
        info: uniffi.tomoshibi_protocol.ResolvedVideo,
        mainland: Boolean = false,
        test: suspend TestScope.(WatchPlayer) -> Unit,
    ) = runTest {
        val model = callModel(TestCall(), deployment, StandardTestDispatcher(testScheduler))
        val engine = WatchEngine(model, machinery, live = false)
        try {
            test(WatchPlayer(video("a", info.ticket), info, engine, machinery, "https://meet.example", mainland))
        } finally {
            model.scope.cancel()
        }
    }

    private fun WatchPlayer.made() = backend as TestBackend

    @Test
    fun aDirectSourceThatFailsBeforePlayingIsGivenTheRelayAndThenSaysSo() = playing(resolved("t-a")) { player ->
        assertFalse(player.relayed)
        assertEquals("https://cdn.example/t-a.mp4", player.made().url)
        player.backendFailed(player.backend!!, "error", format = false)
        assertTrue(player.relayed)
        assertEquals("https://meet.example/api/watch/media?t=t-a", player.made().url)
        assertNull(player.unreachable)
        player.backendFailed(player.backend!!, "error", format = false)
        assertEquals(WatchPlayer.Lost.Network, player.unreachable)
    }

    @Test
    fun aKindOfVideoThePlayerCannotOpenIsSaidAsThatAndNotAsTheConnection() = playing(resolved("t-a")) { player ->
        player.backendFailed(player.backend!!, "error", format = true)
        player.backendFailed(player.backend!!, "error", format = false)
        assertEquals(WatchPlayer.Lost.Format, player.unreachable)
    }

    @Test
    fun aRelayChosenFirstFallsBackOnceToTheCopyWhereTheVideoIs() = playing(resolved("t-a", relay = true)) { player ->
        assertTrue(player.relayed)
        player.backendFailed(player.backend!!, "error", format = false)
        assertFalse(player.relayed)
        assertEquals("https://cdn.example/t-a.mp4", player.made().url)
        player.backendFailed(player.backend!!, "error", format = false)
        assertEquals(WatchPlayer.Lost.Network, player.unreachable)
        assertEquals(2, machinery.made.size)
    }

    @Test
    fun aSourceThatNeverBeginsIsGivenTheRelayOnceTheWaitIsOver() = playing(resolved("t-a")) { player ->
        val playing = show(video("a", "t-a"), playing = true)
        player.keep(watchClock() + 1_000, playing, 0)
        assertFalse(player.relayed)
        player.keep(watchClock() + WatchPlayer.STALL_MS + 2_000, playing, 0)
        assertTrue(player.relayed)
    }

    @Test
    fun aSourceThatHasPlayedIsNeverMovedForBuffering() = playing(resolved("t-a")) { player ->
        val backend = player.made()
        player.backendBegan(backend)
        player.keep(watchClock() + WatchPlayer.STALL_MS + 2_000, show(video("a", "t-a"), playing = true), 0)
        player.backendFailed(backend, "error", format = false)
        assertFalse(player.relayed)
        assertNull(player.unreachable)
        assertEquals(1, machinery.made.size)
    }

    @Test
    fun aPausedShowIsNoStall() = playing(resolved("t-a")) { player ->
        player.keep(watchClock() + WatchPlayer.STALL_MS + 2_000, show(video("a", "t-a"), playing = false), 0)
        assertFalse(player.relayed)
    }

    @Test
    fun youtubeThatWillNotBeEmbeddedAndCannotBeRelayedSaysWhereItPlays() = playing(resolved("t-y", kind = ResolvedPlayKind.YOUTUBE, url = null, id = "dQw4", proxy = null)) { player ->
        assertTrue(player.playsAsYoutube)
        player.youtubeRefused(player.backend!!, 150)
        assertEquals(WatchPlayer.Lost.Refused, player.unreachable)
        // The site's answer stands: a later stall is that answer, not the connection.
        player.youtubeUnreachable(player.backend!!)
        assertEquals(WatchPlayer.Lost.Refused, player.unreachable)
    }

    @Test
    fun youtubeThatCannotBeReachedIsRelayedWhereTheServerRelaysIt() = playing(resolved("t-y", kind = ResolvedPlayKind.YOUTUBE, url = null, id = "dQw4")) { player ->
        assertEquals("youtube:dQw4", player.made().url)
        player.youtubeUnreachable(player.backend!!)
        assertTrue(player.relayed)
        assertEquals("https://meet.example/api/watch/media?t=t-y", player.made().url)
    }

    @Test
    fun aViewerInMainlandChinaGoesStraightToTheRelayForYoutube() = playing(resolved("t-y", kind = ResolvedPlayKind.YOUTUBE, url = null, id = "dQw4"), mainland = true) { player ->
        assertTrue(player.relayed)
        assertFalse(player.playsAsYoutube)
        assertEquals(1, machinery.made.size)
    }

    @Test
    fun aRelayOnlyVideoThatStopsIsAskedAboutAndStartedAgainTwiceThenTheViewerIsAsked() {
        deployment.tickets = { TicketVideo.Known(resolved(it, kind = ResolvedPlayKind.RELAY, url = null)) }
        playing(resolved("t-r", kind = ResolvedPlayKind.RELAY, url = null)) { player ->
            val backend = player.made()
            player.backendBegan(backend)
            repeat(2) {
                player.backendFailed(backend, "stopped", format = false)
                runCurrent()
            }
            assertEquals(2, backend.reloads)
            player.backendFailed(backend, "stopped", format = false)
            runCurrent()
            assertEquals(2, backend.reloads)
            assertEquals(WatchPlayer.Halted.Stopped, player.halted)
            player.tryAgain()
            assertEquals(3, backend.reloads)
            assertEquals(WatchPlayer.Halted.Resuming, player.halted)
        }
    }

    @Test
    fun aRelayTheServerNoLongerRunsIsNotRetried() {
        deployment.tickets = { TicketVideo.Unavailable }
        playing(resolved("t-r", kind = ResolvedPlayKind.RELAY, url = null)) { player ->
            val backend = player.made()
            player.backendBegan(backend)
            player.backendFailed(backend, "stopped", format = false)
            runCurrent()
            assertEquals(0, backend.reloads)
        }
    }

    @Test
    fun theVolumeIsKeptOnlyWhenItIsNotFullAndNeverInARehearsal() {
        val defaults = MemoryDefaults()
        try {
            Rehearsal.install(running = false, defaults)
            playing(resolved("t-a")) { player ->
                player.volume = 0.4
                assertEquals("0.4", defaults.string(WatchPlayer.VOLUME_KEY))
                player.volume = 1.0
                assertFalse(defaults.has(WatchPlayer.VOLUME_KEY))
                Rehearsal.install(running = true, defaults)
                player.volume = 0.2
                assertFalse(defaults.has(WatchPlayer.VOLUME_KEY))
            }
        } finally {
            Rehearsal.install(running = true, MemoryDefaults())
        }
    }
}
