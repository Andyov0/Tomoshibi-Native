package app.tomoshibi.model

import app.tomoshibi.model.Updates.Said
import app.tomoshibi.model.Updates.Step
import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/*
 * When the app looks for an update, what it says, and above all where it
 * says it and does anything about it. What these guard would go wrong in
 * front of somebody without any test of the core noticing: an offer, a
 * download or an install in the middle of a call; a version put away with
 * "Later" coming back an hour later; a check that failed shouting about it,
 * or a check somebody asked for saying nothing; a refusal of the
 * install permission mistaken for consent, or the act of coming back to the
 * app mistaken for a refusal; a build downloaded twice where it is already
 * here, or kept after the system refused it.
 *
 * Time is the test scheduler's, so six hours pass in an instant.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class UpdatesTest {
    private class FakeSource : UpdateSource {
        var answer: () -> Offer? = { null }
        var asked = 0
        var cleared = 0
        val transfers = mutableListOf<FakeTransfer>()

        override suspend fun latest(running: String): Offer? {
            asked++
            return answer()
        }

        override fun download(offer: Offer): Transfer = FakeTransfer(offer).also { transfers += it }

        override fun clear() {
            cleared++
            transfers.forEach { it.file.delete() }
        }
    }

    private class FakeTransfer(offer: Offer) : Transfer {
        /** A real file, since a build kept for a second try is used only while it is still there. */
        override val file: File = File.createTempFile("Tomoshibi-${offer.version}-", ".apk").apply { deleteOnExit() }
        val outcome = CompletableDeferred<Exception?>()
        var progress: ((Long, Long) -> Unit)? = null
        var cancelled = false
        /** A cancel that arrives after the download has already finished, as one can. */
        var deaf = false

        override suspend fun run(progress: (received: Long, total: Long) -> Unit) {
            this.progress = progress
            outcome.await()?.let { throw it }
        }

        override fun cancel() {
            cancelled = true
            if (!deaf) outcome.complete(Failure.Cancelled())
        }
    }

    private class FakeInstaller : UpdateInstaller {
        override var allowed = true
        var asked = 0
        var stopped = 0
        val installs = mutableListOf<File>()
        var answer = CompletableDeferred<Installed>()

        override fun askToBeAllowed() {
            asked++
        }

        override suspend fun install(file: File): Installed {
            installs += file
            return answer.await()
        }

        override fun stop() {
            stopped++
            answer.complete(Installed.Declined)
        }
    }

    private val source = FakeSource()
    private val installer = FakeInstaller()

    private fun offer(version: String) = Offer(version, 56_093_610, { language -> if (language == "en") "Fixed." else null })

    private fun TestScope.updates(): Updates = Updates(source, installer, "0.1.0.7", backgroundScope)

    /** Started in the lobby with [version] on offer. */
    private fun TestScope.offered(version: String = "0.1.0.8"): Updates {
        source.answer = { offer(version) }
        val updates = updates()
        updates.lobby(true)
        updates.start()
        runCurrent()
        assertEquals(Step.Offered(offer(version)), updates.step)
        return updates
    }

    /** Offered, accepted, and the download finished. */
    private fun TestScope.downloaded(): Pair<Updates, FakeTransfer> {
        val updates = offered()
        updates.accept()
        runCurrent()
        val transfer = source.transfers.last()
        transfer.outcome.complete(null)
        runCurrent()
        return updates to transfer
    }

    @Test
    fun asksAtStartAndEverySixHoursAndRetriesAFailureQuietly() = runTest {
        var fail = true
        source.answer = { if (fail) error("no network") else null }
        val updates = updates()
        updates.lobby(true)
        updates.start()
        runCurrent()
        assertEquals(1, source.asked)
        assertEquals(1, source.cleared)
        // Nothing said about a check nobody asked for.
        assertEquals(Step.None, updates.step)
        assertNull(updates.said)
        assertFalse(updates.shown)

        advanceTimeBy(Updates.RETRY - 1)
        assertEquals(1, source.asked)
        advanceTimeBy(2)
        assertEquals("a failed check was not tried again in ten minutes", 2, source.asked)

        fail = false
        advanceTimeBy(Updates.RETRY)
        assertEquals(3, source.asked)
        advanceTimeBy(Updates.EVERY - 1)
        assertEquals("an answered check was asked again before six hours", 3, source.asked)
        advanceTimeBy(2)
        assertEquals(4, source.asked)
        updates.start()
        runCurrent()
        assertEquals("started twice", 4, source.asked)
    }

    @Test
    fun anOfferIsShownInTheLobbyAndNowhereElse() = runTest {
        source.answer = { offer("0.1.0.8") }
        val updates = updates()
        updates.start()
        runCurrent()
        // Found during a call: kept until the lobby.
        assertTrue(updates.step is Step.Offered)
        assertFalse(updates.shown)
        updates.lobby(true)
        assertTrue(updates.shown)
        updates.lobby(false)
        assertFalse(updates.shown)
        assertEquals("Fixed.", updates.offer?.notes("en"))
    }

    @Test
    fun laterPutsThatVersionAwayAndANewerOneIsOfferedAllTheSame() = runTest {
        val updates = offered("0.1.0.8")
        updates.later()
        assertFalse(updates.shown)
        advanceTimeBy(Updates.EVERY + 1)
        assertEquals(2, source.asked)
        assertFalse("a version put away came back at the next check", updates.shown)

        source.answer = { offer("0.1.0.9") }
        advanceTimeBy(Updates.EVERY + 1)
        assertEquals(Step.Offered(offer("0.1.0.9")), updates.step)
        assertTrue(updates.shown)
    }

    @Test
    fun aCheckSomebodyAsksForAlwaysAnswers() = runTest {
        val updates = updates()
        updates.lobby(true)
        source.answer = { null }
        updates.checkNow()
        assertEquals(Said.Checking, updates.said)
        runCurrent()
        assertEquals(Said.Current, updates.said)

        source.answer = { error("no network") }
        updates.checkNow()
        runCurrent()
        assertEquals(Said.Failed, updates.said)

        // A version put away is brought back by asking.
        source.answer = { offer("0.1.0.8") }
        updates.checkNow()
        runCurrent()
        updates.later()
        assertFalse(updates.shown)
        updates.checkNow()
        runCurrent()
        assertNull(updates.said)
        assertTrue(updates.shown)
    }

    @Test
    fun aDownloadIsHeldOutsideTheLobbyAndCarriesOnWhenBack() = runTest {
        val updates = offered()
        updates.accept()
        runCurrent()
        val first = source.transfers.single()
        first.progress!!(1_000, 56_093_610)
        runCurrent()
        assertEquals(Step.Downloading(offer("0.1.0.8"), 1_000, 56_093_610), updates.step)
        assertTrue(updates.shown)

        updates.lobby(false)
        runCurrent()
        assertTrue("a download went on into a call", first.cancelled)
        assertTrue(updates.step is Step.Held)
        assertFalse(updates.shown)

        updates.lobby(true)
        runCurrent()
        assertEquals("the download did not carry on in the lobby", 2, source.transfers.size)
        assertTrue(updates.step is Step.Downloading)
        assertTrue(updates.shown)
    }

    @Test
    fun cancelStopsTheDownloadAndOffersItAgain() = runTest {
        val updates = offered()
        updates.accept()
        runCurrent()
        updates.cancel()
        runCurrent()
        assertTrue(source.transfers.single().cancelled)
        assertEquals(Step.Offered(offer("0.1.0.8")), updates.step)
        assertTrue(updates.shown)
        assertTrue(installer.installs.isEmpty())
    }

    @Test
    fun nothingIsInstalledWithoutTheInstallPermissionAndARefusalIsSaid() = runTest {
        installer.allowed = false
        val (updates, transfer) = downloaded()
        assertEquals(Step.Permission(offer("0.1.0.8"), transfer.file), updates.step)
        assertTrue(installer.installs.isEmpty())
        assertEquals("the system's page opened before the app said why", 0, installer.asked)

        // Coming back to the app before the page was ever opened is not a
        // refusal: the app was only behind the notification shade.
        updates.resumed()
        assertEquals(Step.Permission(offer("0.1.0.8"), transfer.file), updates.step)

        updates.allow()
        assertEquals(1, installer.asked)
        updates.resumed()
        assertEquals(Step.Permission(offer("0.1.0.8"), transfer.file, refused = true), updates.step)
        assertTrue("installed without the permission", installer.installs.isEmpty())
        assertTrue(updates.shown)

        updates.allow()
        installer.allowed = true
        updates.resumed()
        runCurrent()
        assertEquals(Step.Installing(offer("0.1.0.8")), updates.step)
        assertEquals(listOf(transfer.file), installer.installs)
    }

    @Test
    fun permissionGivenAfterTheLobbyWasLeftInstallsNothingUntilBack() = runTest {
        installer.allowed = false
        val (updates, transfer) = downloaded()
        updates.allow()
        // A link opened from the settings page took the app to a meeting's
        // door before it was in front again.
        updates.lobby(false)
        installer.allowed = true
        updates.resumed()
        runCurrent()
        assertTrue("installed outside the lobby", installer.installs.isEmpty())
        updates.lobby(true)
        assertTrue(updates.shown)
        updates.accept()
        runCurrent()
        assertEquals(listOf(transfer.file), installer.installs)
    }

    @Test
    fun aDeclinedInstallKeepsTheBuildForASecondTry() = runTest {
        val (updates, transfer) = downloaded()
        assertEquals(listOf(transfer.file), installer.installs)
        installer.answer.complete(Installed.Declined)
        runCurrent()
        assertEquals(Step.Offered(offer("0.1.0.8"), "The update was not installed."), updates.step)

        installer.answer = CompletableDeferred()
        updates.accept()
        runCurrent()
        assertEquals("downloaded again though it was here", 1, source.transfers.size)
        assertEquals(listOf(transfer.file, transfer.file), installer.installs)
    }

    @Test
    fun aFailedInstallLetsGoOfTheBuild() = runTest {
        val (updates, _) = downloaded()
        val cleared = source.cleared
        installer.answer.complete(Installed.Failed("The update does not match the app that is installed."))
        runCurrent()
        assertEquals(Step.Offered(offer("0.1.0.8"), "The update does not match the app that is installed."), updates.step)
        assertEquals(cleared + 1, source.cleared)
        updates.accept()
        runCurrent()
        assertEquals(2, source.transfers.size)
    }

    @Test
    fun aConfirmationLeftOpenCanBeGivenUp() = runTest {
        val (updates, _) = downloaded()
        assertTrue(updates.step is Step.Installing)
        updates.stopInstalling()
        runCurrent()
        assertEquals(1, installer.stopped)
        assertTrue(updates.step is Step.Offered)
    }

    @Test
    fun aDamagedDownloadSaysSoAndASupersededOneAsksAgain() = runTest {
        val updates = offered()
        updates.accept()
        runCurrent()
        source.transfers.last().outcome.complete(Failure.Damaged("its SHA-256 is …"))
        runCurrent()
        assertEquals(Step.Offered(offer("0.1.0.8"), "The download was damaged and has been deleted. Try again."), updates.step)
        assertTrue(installer.installs.isEmpty())

        updates.accept()
        runCurrent()
        val asked = source.asked
        source.answer = { offer("0.1.0.9") }
        source.transfers.last().outcome.complete(Failure.Superseded())
        runCurrent()
        assertEquals("a superseded build was not asked about again", asked + 1, source.asked)
        assertEquals(Step.Offered(offer("0.1.0.9")), updates.step)
    }

    @Test
    fun aBuildThatArrivesAsTheLobbyIsLeftIsNotInstalledUntilAskedThere() = runTest {
        val updates = offered()
        updates.accept()
        runCurrent()
        val transfer = source.transfers.single()
        transfer.deaf = true
        updates.lobby(false)
        transfer.outcome.complete(null)
        runCurrent()
        assertTrue("installed outside the lobby", installer.installs.isEmpty())
        assertFalse(updates.shown)

        updates.lobby(true)
        runCurrent()
        assertTrue(updates.shown)
        assertTrue("installed without being asked again", installer.installs.isEmpty())
        updates.accept()
        runCurrent()
        assertEquals(1, source.transfers.size)
        assertEquals(listOf(transfer.file), installer.installs)
    }

    @Test
    fun aBuildTakenBackIsNotOfferedAnyMore() = runTest {
        val updates = offered()
        source.answer = { null }
        advanceTimeBy(Updates.EVERY + 1)
        assertEquals(Step.None, updates.step)
        assertFalse(updates.shown)
    }
}
