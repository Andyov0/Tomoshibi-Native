package app.tomoshibi.media

import android.app.Activity
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import app.tomoshibi.basics.t
import app.tomoshibi.model.Installed
import app.tomoshibi.model.UpdateInstaller
import java.io.File
import java.lang.ref.WeakReference
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val TAG = "Tomoshibi"

/**
 * The app installing its own update, through the system's package installer
 * (model/Updates.kt decides when).
 *
 * Android lets an app install another, or a newer copy of itself, only once
 * the person has allowed it in "Install unknown apps", which is asked for on
 * a system page this app opens, never on its own behalf; the app explains why
 * first, in its own words (the lobby's banner), so that the page does not
 * come out of nowhere. The build is handed over in a PackageInstaller
 * session rather than as a file to ACTION_VIEW: the session says how it went
 * (STATUS_PENDING_USER_ACTION, then success, failure or a refusal), where a
 * file handed to the system's installer app says nothing back, and needs no
 * FileProvider to make the file readable to it. Installing the same package
 * signed with the same key replaces the app and ends this process; anything
 * else the system refuses (STATUS_FAILURE_CONFLICT) and the build is not
 * installed.
 */
class SystemInstaller(
    private val context: Context,
    /** Where the build is copied into the session: a test's own, to stop an install in the middle of the copy. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : UpdateInstaller {
    override val allowed: Boolean get() = context.packageManager.canRequestPackageInstalls()

    override fun askToBeAllowed() {
        // This app's own entry, rather than the list of every app.
        Foreground.start(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")))
    }

    /**
     * The install under way, for [stop]: from the moment it is asked for,
     * not from the commit. The build, 56 MB, takes a second or more to copy
     * into the session, and a stop that found nothing to stop in that time
     * did nothing: Cancel on the banner, or leaving the lobby for a call,
     * and the system's confirmation came up all the same a moment later,
     * over whatever the person had gone on to.
     */
    private var current: InstallResults.Handover? = null

    override fun stop() {
        val handover = current ?: return
        current = null
        handover.stopped = true
        // Still being copied: install() gives it up rather than commit it.
        val id = handover.session ?: return
        InstallResults.giveUp(context, id)
    }

    override suspend fun install(file: File): Installed {
        val installer = context.packageManager.packageInstaller
        val handover = InstallResults.Handover()
        current = handover
        val session = try {
            withContext(io) {
                val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
                    setAppPackageName(context.packageName)
                    setSize(file.length())
                }
                val id = installer.createSession(params)
                InstallResults.waiting[id] = handover
                val committed = try {
                    installer.openSession(id).use { open ->
                        open.openWrite("Tomoshibi.apk", 0, file.length()).use { out ->
                            file.inputStream().use { it.copyTo(out, 1 shl 16) }
                            open.fsync(out)
                        }
                        // Stopped while the build was copied: never
                        // committed, so the system never asks.
                        if (handover.stopped) return@use false
                        // Mutable, as the system fills in the status and the
                        // confirmation; explicit, which Android 14 asks of a
                        // mutable one.
                        val result = Intent(context, InstallResults::class.java).setAction(InstallResults.ACTION)
                        val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
                        open.commit(PendingIntent.getBroadcast(context, id, result, flags).intentSender)
                        true
                    }
                } catch (fault: Exception) {
                    InstallResults.waiting.remove(id)
                    runCatching { installer.abandonSession(id) }
                    throw fault
                }
                if (!committed) InstallResults.giveUp(context, id)
                id
            }
        } catch (fault: Exception) {
            if (current === handover) current = null
            throw fault
        }
        handover.session = session
        // Stopped between the look before the commit and here: given up now,
        // and its confirmation, should the system already have sent it, is
        // not put up (InstallResults).
        if (handover.stopped) InstallResults.giveUp(context, session) else Log.i(TAG, "update handed to the installer (session $session)")
        return handover.answer.await().also { if (current === handover) current = null }
    }
}

/** What the installer says about a session, on the main thread. */
class InstallResults : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val session = intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, -1)
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        Log.i(TAG, "installer: session $session status $status")
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
            } else {
                @Suppress("DEPRECATION")
                intent.getParcelableExtra(Intent.EXTRA_INTENT)
            }
            // Stopped (Cancel, or the lobby left), or nobody here waits for
            // it (an earlier run of the app committed it): given up rather
            // than put up over whatever the app is showing now, a call or the
            // door of one, where saying yes would replace the app and end
            // the call. One held for the app's return is asked again then.
            val handover = waiting[session]
            if (handover == null || handover.stopped) {
                giveUp(context, session)
                return
            }
            if (confirm != null) {
                Foreground.start(confirm) { waiting[session]?.stopped == false }
            } else {
                waiting.remove(session)?.answer?.complete(Installed.Failed(t("The update could not be installed.")))
            }
            return
        }
        waiting.remove(session)?.answer?.complete(outcome(status))
    }

    /** One install, from the moment it is asked for until the system's answer. */
    internal class Handover {
        val answer = CompletableDeferred<Installed>()

        /** Set by [SystemInstaller.stop], on the main thread, and read where the build is copied. */
        @Volatile
        var stopped = false

        /** Once committed. */
        var session: Int? = null
    }

    companion object {
        const val ACTION = "app.tomoshibi.UPDATE_INSTALLED"

        /** Sessions the app is waiting to hear about. */
        internal val waiting = java.util.concurrent.ConcurrentHashMap<Int, Handover>()

        /** Abandons [session], so that no confirmation of it can install anything, and answers whoever waits for it. */
        internal fun giveUp(context: Context, session: Int) {
            runCatching { context.packageManager.packageInstaller.abandonSession(session) }
            waiting.remove(session)?.answer?.complete(Installed.Declined)
        }

        internal fun outcome(status: Int): Installed = when (status) {
            PackageInstaller.STATUS_SUCCESS -> Installed.Done
            PackageInstaller.STATUS_FAILURE_ABORTED -> Installed.Declined
            PackageInstaller.STATUS_FAILURE_STORAGE -> Installed.Failed(t("There is not enough room on this device for the update."))
            PackageInstaller.STATUS_FAILURE_INCOMPATIBLE -> Installed.Failed(t("This update does not run on this device."))
            PackageInstaller.STATUS_FAILURE_BLOCKED -> Installed.Failed(t("This device blocked the update."))
            // Signed by somebody other than whoever signed the app installed:
            // a debug build over a release, or the reverse.
            PackageInstaller.STATUS_FAILURE_CONFLICT -> Installed.Failed(t("The update does not match the app that is installed."))
            else -> Installed.Failed(t("The update could not be installed."))
        }
    }
}

/**
 * The activity in front, for the two things that have to be started from
 * one: the system's page for allowing installs, and its confirmation of an
 * install. Started from the application they open in a task of their own,
 * and Back from the settings page went to the home screen rather than to
 * the app; a confirmation that arrives while the app is behind another is
 * held until it is in front again, since Android does not let an app in the
 * background put an activity up.
 */
object Foreground {
    private var activity: WeakReference<Activity>? = null
    private val held = ArrayDeque<Pair<Intent, () -> Boolean>>()

    fun resumed(activity: Activity) {
        this.activity = WeakReference(activity)
        while (held.isNotEmpty()) {
            val (intent, wanted) = held.removeFirst()
            if (wanted()) open(activity, intent)
        }
    }

    fun paused(activity: Activity) {
        if (this.activity?.get() === activity) this.activity = null
    }

    /**
     * Opens [intent] now, or once the app is in front again if [wanted] still
     * says so then: a confirmation held while the app was behind another,
     * for an install given up meanwhile, opened on whatever screen the
     * person came back to.
     */
    fun start(intent: Intent, wanted: () -> Boolean = { true }) {
        val now = activity?.get()
        if (now == null) held.addLast(intent to wanted) else open(now, intent)
    }

    private fun open(activity: Activity, intent: Intent) {
        try {
            activity.startActivity(intent)
        } catch (fault: ActivityNotFoundException) {
            Log.w(TAG, "nothing opens ${intent.action}")
        }
    }
}
