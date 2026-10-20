package app.tomoshibi.model

import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.media.CallMedia
import app.tomoshibi.ui.lobby.BASE
import app.tomoshibi.ui.lobby.ada
import app.tomoshibi.ui.lobby.stage
import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.JoinGrant
import uniffi.tomoshibi_protocol.NoHandle
import uniffi.tomoshibi_protocol.ProtocolException

/*
 * The app's model and its updates together, over a join from the lobby's
 * start and join sheet, which leaves the screen at the lobby until the call
 * has landed. What this guards is an install begun in that second or so: the
 * system's confirmation then came up over the call that had just begun, and
 * saying yes to it ended the call.
 *
 * Under Robolectric only for the context a call's media is made with; a model
 * without media fails every join before it asks anything.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class UpdatesAroundAJoinTest {
    /** A deployment that answers a join only when the test says. */
    private class SlowDeployment(private val answer: CompletableDeferred<JoinGrant>) : Deployment(NoHandle) {
        override fun baseUrl(): String = BASE

        override suspend fun join(room: String, name: String, passphrase: String, identity: String?, relay: String, invite: String?): JoinGrant =
            answer.await()
    }

    private class Source(private val offer: Offer) : UpdateSource {
        val transfers = mutableListOf<Download>()
        override suspend fun latest(running: String): Offer = offer
        override fun download(offer: Offer): Transfer = Download().also { transfers += it }
        override fun clear() {}
    }

    private class Download : Transfer {
        override val file: File = File.createTempFile("Tomoshibi-", ".apk").apply { deleteOnExit() }
        val outcome = CompletableDeferred<Exception?>()
        var cancelled = false

        override suspend fun run(progress: (received: Long, total: Long) -> Unit) {
            outcome.await()?.let { throw it }
        }

        override fun cancel() {
            cancelled = true
            outcome.complete(Failure.Cancelled())
        }
    }

    private class Installer : UpdateInstaller {
        override val allowed = true
        val installs = mutableListOf<File>()
        override fun askToBeAllowed() {}
        override suspend fun install(file: File): Installed {
            installs += file
            return CompletableDeferred<Installed>().await()
        }
        override fun stop() {}
    }

    @Test
    fun aBuildThatArrivesWhileAJoinFromTheLobbyIsOutIsNotInstalled() = runTest {
        val source = Source(Offer("0.1.0.8", 56_093_610))
        val installer = Installer()
        val door = CompletableDeferred<JoinGrant>()
        val app = AppModel(
            MemoryDefaults(), MemorySecrets(), CallMedia(RuntimeEnvironment.getApplication(), muted = true),
            home = BASE, context = Dispatchers.Unconfined,
        )
        app.stage("deployment", SlowDeployment(door))
        app.stage("account", ada)
        app.stage("screenState", AppModel.Screen.Lobby)
        val updates = Updates(source, installer, "0.1.0.7", backgroundScope)
        app.updates = updates
        updates.start()
        runCurrent()
        updates.accept()
        runCurrent()
        val first = source.transfers.single()
        assertTrue(updates.shown)

        // Join pressed in the sheet: the deployment is being asked, and the
        // screen is still the lobby's.
        backgroundScope.launch { app.enter("design-review") }
        runCurrent()
        assertTrue(app.joining)
        assertEquals(AppModel.Screen.Lobby, app.screen)
        assertTrue("the download went on while a join was out", first.cancelled)
        assertFalse(updates.shown)

        // The build arrives (or would have) before the call lands.
        first.outcome.complete(null)
        runCurrent()
        assertTrue("an install began while a join was out", installer.installs.isEmpty())

        // The join is refused: back in the lobby, the download carries on
        // where it was, and the build is installed there.
        door.completeExceptionally(ProtocolException.Network("no route"))
        runCurrent()
        assertFalse(app.joining)
        assertTrue(updates.shown)
        assertEquals(2, source.transfers.size)
        source.transfers.last().outcome.complete(null)
        runCurrent()
        assertEquals(listOf(source.transfers.last().file), installer.installs)
    }
}
