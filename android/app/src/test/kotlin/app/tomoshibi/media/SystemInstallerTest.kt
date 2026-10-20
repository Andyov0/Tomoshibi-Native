package app.tomoshibi.media

import android.app.Activity
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Handler
import android.os.Looper
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.tomoshibi.model.Installed
import java.io.File
import java.util.concurrent.Executor
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/*
 * The app handing its own update to the system's package installer, and
 * giving it up. What these guard is the system's "Do you want to update this
 * app?" coming up after the person said no to it: Cancel pressed while the
 * build was still being copied into the session, or the lobby left for a
 * call in that second, did nothing, and the confirmation came up a moment
 * later over the call, where saying yes replaced the app and ended it.
 *
 * The copy is run by the test, so that a stop can land in the middle of it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35])
class SystemInstallerTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val copies = ArrayDeque<Runnable>()
    private val installer = SystemInstaller(context, Executor { copies.addLast(it) }.asCoroutineDispatcher())
    private val build = File.createTempFile("Tomoshibi-", ".apk").apply {
        writeBytes(ByteArray(200_000) { it.toByte() })
        deleteOnExit()
    }
    private val created = mutableListOf<Int>()
    private val activity: Activity = Robolectric.buildActivity(Activity::class.java).setup().get()

    init {
        context.packageManager.packageInstaller.registerSessionCallback(object : PackageInstaller.SessionCallback() {
            override fun onCreated(sessionId: Int) {
                created += sessionId
            }
            override fun onBadgingChanged(sessionId: Int) {}
            override fun onActiveChanged(sessionId: Int, active: Boolean) {}
            override fun onProgressChanged(sessionId: Int, progress: Float) {}
            override fun onFinished(sessionId: Int, success: Boolean) {}
        }, Handler(Looper.getMainLooper()))
    }

    @After
    fun tidy() {
        Foreground.paused(activity)
        InstallResults.waiting.clear()
    }

    private fun copy() {
        while (copies.isNotEmpty()) copies.removeFirst().run()
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun sessions() = context.packageManager.packageInstaller.allSessions

    /** The receiver the commit hands the system, if it was ever made for [session]. */
    private fun receiver(session: Int): PendingIntent? = PendingIntent.getBroadcast(
        context, session, Intent(context, InstallResults::class.java).setAction(InstallResults.ACTION),
        PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_MUTABLE,
    )

    /** The system asking the person, as it says so to the app. */
    private fun systemAsks(session: Int) = InstallResults().onReceive(
        context,
        Intent(InstallResults.ACTION)
            .putExtra(PackageInstaller.EXTRA_SESSION_ID, session)
            .putExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_PENDING_USER_ACTION)
            .putExtra(Intent.EXTRA_INTENT, Intent("android.content.pm.action.CONFIRM_INSTALL")),
    )

    /** An install handed over and committed, waiting for the person. */
    private fun TestScope.committed(): Pair<kotlinx.coroutines.Deferred<Installed>, Int> {
        val result = backgroundScope.async { installer.install(build) }
        runCurrent()
        copy()
        runCurrent()
        val session = created.single()
        assertNotNull("not committed", receiver(session))
        return result to session
    }

    @Test
    fun stoppedWhileTheBuildIsCopiedItIsNeverCommitted() = runTest {
        val result = backgroundScope.async { installer.install(build) }
        runCurrent()
        assertEquals("the copy was not under way", 1, copies.size)
        // Cancel on the banner, or the lobby left for a call.
        installer.stop()
        copy()
        runCurrent()
        assertTrue("the install went on waiting after it was stopped", result.isCompleted)
        assertEquals(Installed.Declined, result.getCompleted())
        val session = created.single()
        assertNull("committed after it was stopped, so the system asks all the same", receiver(session))
        assertTrue("the session was left behind", sessions().isEmpty())
        assertTrue(InstallResults.waiting.isEmpty())
    }

    @Test
    fun aConfirmationForAnInstallGivenUpIsNotPutUp() = runTest {
        Foreground.resumed(activity)
        val (result, session) = committed()
        installer.stop()
        runCurrent()
        assertEquals(Installed.Declined, result.getCompleted())
        assertTrue(sessions().isEmpty())
        // The system's word arrives after the stop, as it can.
        systemAsks(session)
        assertNull("a confirmation was put up for an install given up", shadowOf(activity).nextStartedActivity)
    }

    @Test
    fun aConfirmationHeldWhileTheAppWasBehindIsDroppedIfTheInstallIsGivenUp() = runTest {
        val (result, session) = committed()
        // Behind another app (or in the small window) when the system asks.
        systemAsks(session)
        assertNull(shadowOf(activity).nextStartedActivity)
        // A call was joined from the notification meanwhile.
        installer.stop()
        runCurrent()
        assertEquals(Installed.Declined, result.getCompleted())
        Foreground.resumed(activity)
        assertNull("a held confirmation was put up over the call", shadowOf(activity).nextStartedActivity)
    }

    @Test
    fun aConfirmationHeldWhileTheAppWasBehindIsPutUpWhenItIsBack() = runTest {
        val (result, session) = committed()
        systemAsks(session)
        assertNull(shadowOf(activity).nextStartedActivity)
        Foreground.resumed(activity)
        assertEquals("android.content.pm.action.CONFIRM_INSTALL", shadowOf(activity).nextStartedActivity?.action)
        assertTrue(!result.isCompleted)
        // The person says no.
        InstallResults().onReceive(
            context,
            Intent(InstallResults.ACTION)
                .putExtra(PackageInstaller.EXTRA_SESSION_ID, session)
                .putExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE_ABORTED),
        )
        runCurrent()
        assertEquals(Installed.Declined, result.getCompleted())
    }
}
