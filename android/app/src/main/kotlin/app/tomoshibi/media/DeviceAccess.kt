package app.tomoshibi.media

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.CompletableDeferred

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

/**
 * Asking the system for the camera and the microphone (the Apple apps'
 * DeviceAccess).
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

    /** In the activity's onCreate, before it is started: the only time a result launcher may be registered. */
    fun install(activity: ComponentActivity) {
        context = activity.applicationContext
        val registered = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            val permission = asking ?: return@registerForActivityResult
            asking = null
            pending.remove(permission)?.complete(granted)
            askNext()
        }
        launcher = registered
        activity.lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onDestroy(owner: LifecycleOwner) {
                // A launcher outlives nothing of its activity's but the
                // activity itself, which it would keep.
                if (launcher === registered) launcher = null
            }
        })
        askNext()
    }

    fun granted(context: Context, kind: DeviceKind): Boolean =
        ContextCompat.checkSelfPermission(context, kind.permission) == PackageManager.PERMISSION_GRANTED

    /** Asks where it has not been granted, and throws [DeviceRefused] where the answer was no. */
    suspend fun ensure(kind: DeviceKind) {
        val context = context ?: throw DeviceRefused(kind)
        if (granted(context, kind)) return
        if (launcher == null) throw DeviceRefused(kind)
        val answer = pending.getOrPut(kind.permission) { CompletableDeferred() }
        askNext()
        if (!answer.await()) throw DeviceRefused(kind)
    }

    private fun askNext() {
        if (asking != null) return
        val launcher = launcher ?: return
        val next = pending.keys.firstOrNull() ?: return
        asking = next
        launcher.launch(next)
    }

    /** Opens this app's page of Settings, where both switches are. */
    fun openSettings(context: Context? = this.context) {
        val context = context ?: return
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(intent)
    }
}
