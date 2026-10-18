package app.tomoshibi.media

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.CompletableDeferred

private const val TAG = "Tomoshibi"

/**
 * A camera or a microphone, for the two things said about either: whether
 * the system lets this app use it, and what to say when it would not start.
 */
enum class DeviceKind(val permission: String) {
    Camera(Manifest.permission.CAMERA),
    Microphone(Manifest.permission.RECORD_AUDIO);

    /** What the call's notice about this device is filed under, so that the next start that works takes it down. */
    val noticeKey: String get() = if (this == Camera) "device.camera" else "device.microphone"
}

/** The system said no: the person refused the camera or the microphone. The one failure with a place to go and fix it. */
class DeviceRefused(val kind: DeviceKind) : Exception("${kind.name} refused")

/** The person's consent to capture the screen, as the system's dialog answered it: one projection's worth. */
class ScreenConsent(val resultCode: Int, val data: Intent)

/**
 * Asking the system for the camera, the microphone and the screen (the Apple
 * apps' DeviceAccess), and for the call's notification.
 *
 * Asked before the device is opened rather than learned from it, because the
 * microphone does not fail when it is refused: it records silence, and a call
 * that sends silence while its button says "on" was the fault this exists for
 * on the Mac.
 *
 * The activity on screen [install]s itself so that a prompt can be shown;
 * with none, the permission is only checked. One prompt at a time: a second
 * ask while one is up waits its turn, and two asks for the same device wait
 * for the same answer.
 *
 * Main thread only.
 */
object DeviceAccess {
    private var context: Context? = null
    private var launcher: ActivityResultLauncher<String>? = null
    private var asking: String? = null
    private val pending = LinkedHashMap<String, CompletableDeferred<Boolean>>()
    private var screenLauncher: ActivityResultLauncher<Intent>? = null
    private var screenAnswer: CompletableDeferred<ScreenConsent?>? = null
    private var notificationsAsked = false

    /**
     * In the activity's onCreate, before it is started: the only time a
     * result launcher may be registered. Everything else in the media layer
     * that only an activity can do is installed from here too (CallWindow:
     * the small window, and the notification's way back into the app), so
     * that the app's one activity has one line to say about media.
     */
    fun install(activity: ComponentActivity) {
        context = activity.applicationContext
        val registered = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            val permission = asking ?: return@registerForActivityResult
            asking = null
            pending.remove(permission)?.complete(granted)
            askNext()
        }
        launcher = registered
        val screen = activity.registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val data = result.data
            val consent = if (result.resultCode == Activity.RESULT_OK && data != null) ScreenConsent(result.resultCode, data) else null
            Log.i(TAG, if (consent != null) "the screen may be captured" else "capturing the screen was declined")
            screenAnswer?.complete(consent)
            screenAnswer = null
        }
        screenLauncher = screen
        activity.lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onDestroy(owner: LifecycleOwner) {
                // A launcher outlives nothing of its activity's but the
                // activity itself, which it would keep.
                if (launcher === registered) launcher = null
                if (screenLauncher === screen) screenLauncher = null
            }
        })
        CallKeeper.install(activity)
        Thermal.install(activity)
        CallWindow.install(activity)
        askNext()
    }

    fun granted(context: Context, kind: DeviceKind): Boolean =
        ContextCompat.checkSelfPermission(context, kind.permission) == PackageManager.PERMISSION_GRANTED

    /** Asks where it has not been granted, and throws [DeviceRefused] where the answer was no. */
    suspend fun ensure(kind: DeviceKind) {
        if (!ask(kind.permission)) throw DeviceRefused(kind)
    }

    /** Whether [permission] is granted, asking where it is not and there is an activity to ask from. */
    private suspend fun ask(permission: String): Boolean {
        val context = context ?: return false
        if (ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED) return true
        if (launcher == null) return false
        val answer = pending.getOrPut(permission) { CompletableDeferred() }
        askNext()
        return answer.await()
    }

    /**
     * The call's notification, on Android 13 and later where it is a
     * permission: asked once a run of the app, when the first call starts,
     * which is the moment the notification first means something. Refused,
     * the call is kept all the same; only its notification is not shown.
     */
    suspend fun notifications() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU || notificationsAsked) return
        notificationsAsked = true
        if (!ask(Manifest.permission.POST_NOTIFICATIONS)) Log.i(TAG, "notifications refused; the call goes on without one")
    }

    private fun askNext() {
        if (asking != null) return
        val launcher = launcher ?: return
        val next = pending.keys.firstOrNull() ?: return
        asking = next
        launcher.launch(next)
    }

    /**
     * The system's dialog asking whether the screen may be captured; null
     * where it was declined or there is no activity to show it from.
     *
     * Each answer is good for one projection (Android 14 refuses a second
     * from the same consent), so this is asked once per share, and a second
     * ask while one is out answers the first with nothing rather than
     * leaving it waiting.
     */
    suspend fun screen(): ScreenConsent? {
        val context = context ?: return null
        val launcher = screenLauncher ?: return null
        val manager = context.getSystemService(MediaProjectionManager::class.java) ?: return null
        screenAnswer?.complete(null)
        val answer = CompletableDeferred<ScreenConsent?>()
        screenAnswer = answer
        // The person's own choice of a single app or the whole screen, where
        // the system offers it (Android 14 and later).
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForUserChoice())
        } else {
            manager.createScreenCaptureIntent()
        }
        try {
            launcher.launch(intent)
        } catch (fault: Exception) {
            Log.w(TAG, "the screen capture dialog would not open: ${fault.message}")
            screenAnswer = null
            return null
        }
        return answer.await()
    }

    /**
     * Answers a screen dialog still out with nothing: leaving the call with
     * it open. Whatever the person presses in it afterwards reaches nobody.
     */
    fun cancelScreen() {
        screenAnswer?.complete(null)
        screenAnswer = null
    }

    /** Opens this app's page of Settings, where both switches are. */
    fun openSettings(context: Context? = this.context) {
        val context = context ?: return
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    }
}
