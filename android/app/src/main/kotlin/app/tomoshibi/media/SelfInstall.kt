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
class SystemInstaller(private val context: Context) : UpdateInstaller {
    override val allowed: Boolean get() = context.packageManager.canRequestPackageInstalls()

    override fun askToBeAllowed() {
        // This app's own entry, rather than the list of every app.
        Foreground.start(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")))
    }

    /** The session waiting for the person's answer, for [stop]. */
    private var current: Int? = null

    override fun stop() {
        val id = current ?: return
        current = null
        runCatching { context.packageManager.packageInstaller.abandonSession(id) }
        InstallResults.waiting.remove(id)?.complete(Installed.Declined)
    }

    override suspend fun install(file: File): Installed {
        val installer = context.packageManager.packageInstaller
        val answer = CompletableDeferred<Installed>()
        val session = withContext(Dispatchers.IO) {
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
                setAppPackageName(context.packageName)
                setSize(file.length())
            }
            val id = installer.createSession(params)
            InstallResults.waiting[id] = answer
            try {
                installer.openSession(id).use { open ->
                    open.openWrite("Tomoshibi.apk", 0, file.length()).use { out ->
                        file.inputStream().use { it.copyTo(out, 1 shl 16) }
                        open.fsync(out)
                    }
                    // Mutable, as the system fills in the status and the
                    // confirmation; explicit, which Android 14 asks of a
                    // mutable one.
                    val result = Intent(context, InstallResults::class.java).setAction(InstallResults.ACTION)
                    val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
                    open.commit(PendingIntent.getBroadcast(context, id, result, flags).intentSender)
                }
            } catch (fault: Exception) {
                InstallResults.waiting.remove(id)
                runCatching { installer.abandonSession(id) }
                throw fault
            }
            id
        }
        Log.i(TAG, "update handed to the installer (session $session)")
        current = session
        return answer.await().also { if (current == session) current = null }
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
            if (confirm != null) Foreground.start(confirm) else waiting.remove(session)?.complete(Installed.Failed(t("The update could not be installed.")))
            return
        }
        waiting.remove(session)?.complete(outcome(status))
    }

    companion object {
        const val ACTION = "app.tomoshibi.UPDATE_INSTALLED"

        /** Sessions the app is waiting to hear about. */
        internal val waiting = java.util.concurrent.ConcurrentHashMap<Int, CompletableDeferred<Installed>>()

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
    private val held = ArrayDeque<Intent>()

    fun resumed(activity: Activity) {
        this.activity = WeakReference(activity)
        while (held.isNotEmpty()) open(activity, held.removeFirst())
    }

    fun paused(activity: Activity) {
        if (this.activity?.get() === activity) this.activity = null
    }

    fun start(intent: Intent) {
        val now = activity?.get()
        if (now == null) held.addLast(intent) else open(now, intent)
    }

    private fun open(activity: Activity, intent: Intent) {
        try {
            activity.startActivity(intent)
        } catch (fault: ActivityNotFoundException) {
            Log.w(TAG, "nothing opens ${intent.action}")
        }
    }
}
