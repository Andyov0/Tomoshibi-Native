package app.tomoshibi.model

import app.tomoshibi.basics.Language
import app.tomoshibi.basics.Localization
import app.tomoshibi.basics.Pace
import app.tomoshibi.media.ShareCeiling
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * What the deployment says to the apps (Remote): when it is asked, what is
 * kept, and what applies when nothing can be asked. What these guard would go
 * wrong where nobody is looking: a feature switched off coming back on a
 * train because the answer was not kept, the deployment asked three times
 * for one return to the app, a failed question undoing what applied, a
 * notice over a call, or one closed coming back at the next start.
 *
 * Time is the test scheduler's, so half hours pass in an instant.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RemoteTest {
    /** A deployment that answers what [answer] says, or fails where it throws. */
    private class FakeSource : RemoteSource {
        var answer: () -> RemoteAnswer = { error("not reachable") }
        var asked = 0
        val read = mutableListOf<String>()

        override suspend fun fetch(): RemoteAnswer {
            asked++
            return answer()
        }

        /** What was kept is read back as the core would: here, kept as the revision. */
        override fun read(kept: String): RemoteAnswer? {
            read += kept
            return answers[kept]
        }

        val answers = HashMap<String, RemoteAnswer>()
    }

    private val source = FakeSource()
    private val store = MemoryDefaults()
    private val words = Localization.forTesting(mapOf(Language.ZhHans to mapOf("Leave" to "离开")), Language.ZhHans)

    private fun answer(
        revision: String,
        off: Set<String> = emptySet(),
        params: Map<String, Double> = emptyMap(),
        notice: LobbyNotice? = null,
        phrases: Map<Language, Map<String, String>> = emptyMap(),
    ) = RemoteAnswer(revision, phrases, off, params, notice, kept = revision).also { source.answers[revision] = it }

    private fun TestScope.remote(store: Defaults? = this@RemoteTest.store) =
        Remote(source, store, backgroundScope, words, now = { testScheduler.currentTime })

    @Test
    fun whatWasKeptAppliesBeforeAnythingIsAskedAndStaysWhenNothingAnswers() = runTest {
        answer("r1", off = setOf("screen-share"), phrases = mapOf(Language.ZhHans to mapOf("Leave" to "退出会议")))
        store.set(Remote.KEPT_KEY, "r1")
        val remote = remote()
        remote.restore()
        assertFalse("a switch kept off is on before the first question", remote.allows(RemoteFeature.ScreenShare))
        assertEquals("退出会议", words.t("Leave"))
        assertEquals(0, source.asked)
        // Offline: the question fails, and what was kept goes on applying.
        remote.start()
        runCurrent()
        assertEquals(1, source.asked)
        assertFalse(remote.allows(RemoteFeature.ScreenShare))
        assertEquals("退出会议", words.t("Leave"))
    }

    @Test
    fun withNothingKeptAndNothingHeardTheAppIsAsItWasBuilt() = runTest {
        val remote = remote()
        remote.restore()
        remote.start()
        runCurrent()
        for (feature in RemoteFeature.entries) assertTrue(remote.allows(feature))
        assertEquals(ShareCeiling.NONE, remote.ceiling)
        assertNull(remote.answer.updateInterval)
        assertEquals("离开", words.t("Leave"))
    }

    @Test
    fun anAnswerAppliesAndIsKeptAndAFailureAfterItChangesNothing() = runTest {
        var heard = answer("r2", off = setOf("music"))
        source.answer = { heard }
        val remote = remote()
        var told = 0
        remote.onApply = { told++ }
        remote.start()
        runCurrent()
        assertFalse(remote.allows(RemoteFeature.Music))
        assertEquals("r2", store.string(Remote.KEPT_KEY))
        assertEquals(1, told)
        // Half an hour on, the deployment cannot be reached.
        source.answer = { error("down") }
        advanceTimeBy(Remote.EVERY)
        runCurrent()
        assertEquals(2, source.asked)
        assertFalse("a failed question took back what applied", remote.allows(RemoteFeature.Music))
        assertEquals("r2", store.string(Remote.KEPT_KEY))
        assertEquals(1, told)
        // And once it answers again, what it says now.
        heard = answer("r3")
        source.answer = { heard }
        advanceTimeBy(Remote.EVERY)
        runCurrent()
        assertTrue(remote.allows(RemoteFeature.Music))
        assertEquals("r3", store.string(Remote.KEPT_KEY))
    }

    @Test
    fun asksAtStartEveryHalfHourAndOnComingBackButNotTwiceInAMinute() = runTest {
        source.answer = { answer("r") }
        val remote = remote()
        remote.refresh()
        runCurrent()
        assertEquals("asked before it was started", 0, source.asked)
        remote.start()
        runCurrent()
        assertEquals(1, source.asked)
        // The app comes to the front and lands in the lobby in the same moment.
        remote.refresh()
        remote.lobby(true)
        runCurrent()
        assertEquals("asked again for a return a moment after the last question", 1, source.asked)
        advanceTimeBy(Remote.SOONEST + 1)
        remote.lobby(false)
        remote.lobby(true)
        runCurrent()
        assertEquals("not asked on coming back to the lobby", 2, source.asked)
        advanceTimeBy(Remote.EVERY - Remote.SOONEST - 2)
        runCurrent()
        assertEquals(2, source.asked)
        advanceTimeBy(Remote.SOONEST + 2)
        runCurrent()
        assertEquals("not asked on the half hour", 3, source.asked)
    }

    @Test
    fun theNoticeIsShownInTheLobbyAndNowhereElse() = runTest {
        source.answer = { answer("r", notice = LobbyNotice("n1", text = mapOf(Language.En to "Down tonight"))) }
        val remote = remote()
        remote.start()
        runCurrent()
        assertNull("a notice before the lobby", remote.notice)
        remote.lobby(true)
        assertEquals("n1", remote.notice?.id)
        // In a call.
        remote.lobby(false)
        assertNull("a notice over a call", remote.notice)
    }

    @Test
    fun aNoticeClosedStaysClosedAfterAStartAndANewOneIsShown() = runTest {
        answer("r1", notice = LobbyNotice("n1", text = mapOf(Language.En to "Down tonight")))
        store.set(Remote.KEPT_KEY, "r1")
        val first = remote()
        first.restore()
        first.lobby(true)
        val shown = first.notice
        assertNotNull(shown)
        first.close(shown!!)
        assertNull("still shown once closed", first.notice)

        // The app started again, with the same notice kept.
        val next = remote()
        next.restore()
        next.lobby(true)
        assertNull("a closed notice came back at the next start", next.notice)

        // The deployment puts up another.
        source.answer = { answer("r2", notice = LobbyNotice("n2", warning = true, text = mapOf(Language.En to "Moved"))) }
        next.start()
        runCurrent()
        assertEquals("n2", next.notice?.id)
    }

    @Test
    fun aNoticeIsTakenDownWhenItRunsOut() = runTest {
        val until = testScheduler.currentTime + 60_000
        source.answer = { answer("r", notice = LobbyNotice("n1", text = mapOf(Language.En to "Down until nine"), until = until)) }
        val remote = remote()
        remote.lobby(true)
        remote.start()
        runCurrent()
        assertEquals("n1", remote.notice?.id)
        advanceTimeBy(59_000)
        runCurrent()
        assertEquals("n1", remote.notice?.id)
        advanceTimeBy(2_000)
        runCurrent()
        assertNull("still up after it ran out", remote.notice)
        assertNull("still in the answer after it ran out", remote.answer.notice)
    }

    @Test
    fun theUpdateIntervalIsHoursAndNeverUnderTenMinutes() {
        assertEquals(2 * 3_600_000L, RemoteAnswer(params = mapOf("update_check_hours" to 2.0)).updateInterval)
        assertEquals(Updates.RETRY, RemoteAnswer(params = mapOf("update_check_hours" to 0.01)).updateInterval)
        assertNull(RemoteAnswer(params = mapOf("update_check_hours" to 0.0)).updateInterval)
        assertNull(RemoteAnswer(params = mapOf("update_check_hours" to -3.0)).updateInterval)
        assertNull(RemoteAnswer().updateInterval)
    }

    @Test
    fun anUnknownFeatureSwitchedOffIsNothingToThisApp() {
        val answer = RemoteAnswer(off = setOf("teleport"))
        for (feature in RemoteFeature.entries) assertTrue(answer.allows(feature))
    }

    /** The app's model follows the deployment's word on its updates: whether it looks, and how often. */
    @Test
    fun theAppsUpdatesFollowWhatTheDeploymentSays() = runTest {
        val app = AppModel(MemoryDefaults(), MemorySecrets(), media = null, home = "https://example.test/", context = kotlinx.coroutines.Dispatchers.Unconfined)
        val updates = Updates(NoUpdates, NoUpdates, "0.1.4", backgroundScope, Pace(null, whole = 0))
        app.updates = updates
        source.answer = { answer("r", off = setOf("app-updates"), params = mapOf("update_check_hours" to 1.0)) }
        app.remote = remote(store = null)
        app.remote.start()
        runCurrent()
        assertFalse(updates.allowed)
        source.answer = { answer("r2") }
        advanceTimeBy(Remote.EVERY)
        runCurrent()
        assertTrue(updates.allowed)
    }

    private object NoUpdates : UpdateSource, UpdateInstaller {
        override suspend fun latest(running: String): Offer? = null
        override fun download(offer: Offer): Transfer = error("nothing to download")
        override fun clear(keepingKept: Boolean) {}
        override fun keep(file: java.io.File?) {}
        override suspend fun kept(offer: Offer): java.io.File? = null
        override val allowed = true
        override fun askToBeAllowed() {}
        override suspend fun install(file: java.io.File): Installed = Installed.Declined
        override fun stop() {}
    }
}
