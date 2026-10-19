package app.tomoshibi.media

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import app.tomoshibi.R
import app.tomoshibi.basics.t
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeoutOrNull

private const val TAG = "Tomoshibi"

/**
 * The call kept alive while the app is out of sight: a foreground service
 * for as long as there is a call, with its ongoing notification.
 *
 * Without one, a call that went behind the home screen or a locked screen
 * went on in the room with its microphone cut off by the system and its
 * process soon frozen, a participant who is there and silent; the iPhone app
 * keeps its call with the audio background mode, and this is Android's way
 * to the same. Everything it holds is in [CallKeeper]; the service itself
 * only puts the notification up and says which devices it holds.
 */
class CallService : Service() {
    override fun onCreate() {
        super.onCreate()
        CallKeeper.attached(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        CallKeeper.show(this)
        // Never brought back by the system after its process was killed:
        // the call died with the process, and a notification for it would
        // be a call that is not there.
        return START_NOT_STICKY
    }

    /** The app swiped away from the recent apps: the call goes with it, as it does when the activity is finished. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        Log.i(TAG, "the app was taken out of recent apps; leaving the call")
        CallKeeper.control(CallKeeper.Control.Leave)
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        CallKeeper.detached(this)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}

/** The buttons on the call's notification and in the small window, which work without the app in front. */
class CallActions : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val control = CallKeeper.Control.entries.firstOrNull { it.action == intent.action } ?: return
        CallKeeper.control(control)
    }
}

/**
 * What the call service holds and shows, from the call (CallMedia.follow
 * writes [held]); main thread only.
 *
 * Android 14 lets a foreground service hold a device only while the app may
 * use it, and only under a type naming it, so the types follow the devices:
 * `phoneCall` for as long as there is a call, which rests on MANAGE_OWN_CALLS
 * and holds the process and its network; `microphone` while the microphone
 * is on, `camera` while the camera is, and `mediaProjection` while the screen
 * or its sound is shared, and each is dropped again when its device goes
 * off. A type that the system refuses to add (the app gone to the
 * background, say) is left out and said in the log, and the rest stand.
 */
object CallKeeper {
    /** What there is to hold: a call in [room], and which devices it uses. */
    data class Held(
        val room: String,
        val microphone: Boolean,
        val camera: Boolean,
        val screen: Boolean,
        val sound: Boolean,
        /** Moved on when the language changes, so the notification is said again in the new one. */
        val language: Any? = null,
    )

    /** What the buttons do. */
    enum class Control(val action: String) {
        Mute("app.tomoshibi.call.MUTE"),
        Unmute("app.tomoshibi.call.UNMUTE"),
        Leave("app.tomoshibi.call.LEAVE"),
        StopSharing("app.tomoshibi.call.STOP_SHARING"),
    }

    private const val NOTIFICATION = 7
    private const val CHANNEL = "call"

    private var context: Context? = null
    private var service: CallService? = null
    private var running: CompletableDeferred<CallService>? = null
    private var held: Held? = null
    /** The last call shown, for a service that has to show something on its way out. */
    private var last: Held? = null
    /** Whether a screen capture's projection is open, which only the projection says (ScreenProjection). */
    private var projecting = false
    /** The types last given to the system, to know what changed. */
    private var types = 0
    private var since = 0L

    /** What a button pressed does; set by CallMedia.follow, which knows the app. */
    var onControl: ((Control) -> Unit)? = null

    /** The activity's own intent that turns the microphone on with the app in front (see [notification]). */
    var openApp: ((Control?) -> Intent?)? = null

    fun install(context: Context) {
        this.context = context.applicationContext
    }

    /**
     * The call there is now, or none. The service starts with the first
     * call, from the app in front as Android requires, and stops when there
     * is no call left to hold.
     */
    fun hold(next: Held?) {
        val context = context ?: return
        val was = held
        held = next
        if (next == null) {
            if (was != null) stop(context)
            return
        }
        if (was == null || service == null && running == null) {
            since = System.currentTimeMillis()
            start(context)
        } else {
            service?.let(::show)
        }
    }

    private fun start(context: Context) {
        running = CompletableDeferred()
        try {
            ContextCompat.startForegroundService(context, Intent(context, CallService::class.java))
            Log.i(TAG, "call service starting")
        } catch (fault: Exception) {
            // Started from the background (a call joined again after a move
            // while the screen was off): the call goes on without it, as it
            // did before there was one, and the next start in front takes it.
            Log.w(TAG, "the call service would not start: ${fault.message}")
            running = null
        }
    }

    private fun stop(context: Context) {
        projecting = false
        types = 0
        running?.cancel()
        running = null
        val service = service ?: run {
            context.stopService(Intent(context, CallService::class.java))
            return
        }
        ServiceCompat.stopForeground(service, ServiceCompat.STOP_FOREGROUND_REMOVE)
        service.stopSelf()
        Log.i(TAG, "call service stopped")
    }

    /**
     * The screen's projection is about to open. It needs the service to be
     * holding `mediaProjection` before the projection is taken from the
     * consent (Android 14 throws otherwise), so this waits for the service
     * and says whether the type took.
     */
    suspend fun project(): Boolean {
        projecting = true
        if (held == null) return false
        val service = service ?: withTimeoutOrNull(5_000) { running?.await() } ?: return false
        show(service)
        return types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION != 0
    }

    /** The notification put up again as it is, where the service runs. */
    fun refresh() {
        service?.let(::show)
    }

    /** The projection has closed: the type goes. */
    fun unproject() {
        if (!projecting) return
        projecting = false
        service?.let(::show)
    }

    internal fun attached(service: CallService) {
        this.service = service
        running?.complete(service)
        running = null
    }

    internal fun detached(service: CallService) {
        if (this.service === service) this.service = null
        types = 0
    }

    internal fun control(control: Control) {
        Log.i(TAG, "call control: ${control.name.lowercase()}")
        onControl?.invoke(control)
    }

    /** Puts the notification up or brings it up to date, with the types the devices in use need. */
    internal fun show(service: CallService) {
        val held = held
        if (held == null) {
            // Asked to start and then told there is no call before it had: it
            // still goes to the front once, because a service started that
            // way and stopped without doing so takes the process down with
            // it (ForegroundServiceDidNotStartInTimeException).
            // CallStyle refuses a person without a name, so the app's stands in.
            val notification = notification(service, last ?: Held("Tomoshibi", false, false, false, false))
            try {
                ServiceCompat.startForeground(service, NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL)
            } catch (fault: Exception) {
                Log.w(TAG, "the call service could not go to the front to stop: ${fault.message}")
            }
            ServiceCompat.stopForeground(service, ServiceCompat.STOP_FOREGROUND_REMOVE)
            service.stopSelf()
            return
        }
        last = held
        val notification = notification(service, held)
        val wanted = typesFor(held)
        try {
            ServiceCompat.startForeground(service, NOTIFICATION, notification, wanted)
            if (wanted != types) Log.i(TAG, "call service holds ${describe(wanted)}")
            types = wanted
        } catch (fault: Exception) {
            // A type the system will not give now: the app is not in front
            // for a device that needs it to be, or a permission is missing.
            // What it already held is kept, and the new one goes without.
            Log.w(TAG, "the call service could not hold ${describe(wanted)}: ${fault.message}")
            val fallback = if (types != 0) types else ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL
            try {
                ServiceCompat.startForeground(service, NOTIFICATION, notification, fallback)
                types = fallback
            } catch (again: Exception) {
                Log.e(TAG, "the call service could not stay in front: ${again.message}")
            }
        }
    }

    private fun typesFor(held: Held): Int {
        var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (held.microphone) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
            if (held.camera) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA
        }
        if (projecting) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
        return types
    }

    private fun describe(types: Int): String = buildList {
        if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL != 0) add("call")
        if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE != 0) add("microphone")
        if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA != 0) add("camera")
        if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION != 0) add("projection")
    }.joinToString("+")

    /**
     * The ongoing call: the room's name, what it is doing, the microphone's
     * button, Stop sharing while something is shared, and the system's own
     * Hang up (CallStyle), which also puts the call's chip in the status bar
     * on Android 12 and later, from which the call is one press away.
     *
     * Turning the microphone on opens the app rather than working from the
     * shade: Android lets a service take the microphone only while the app is
     * in front, and a microphone turned on behind it records silence while
     * its button says it is on, which is the fault DeviceAccess exists for.
     * Turning it off, hanging up and stopping a share work from anywhere.
     */
    private fun notification(context: Context, held: Held): Notification {
        channel(context)
        val open = openApp?.invoke(null)?.let {
            PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val leave = broadcast(context, Control.Leave)
        val person = Person.Builder().setName(held.room).setImportant(true).build()
        val status = when {
            held.screen -> t("You are sharing your screen")
            held.sound -> t("Sharing sound")
            else -> t("In progress")
        }
        val builder = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.call_status)
            .setContentTitle(held.room)
            .setContentText(status)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setShowWhen(true)
            .setUsesChronometer(true)
            .setWhen(since)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            // The app's own colour, said rather than left to the system. A
            // call's buttons other than Hang up are drawn in the notification's
            // colour, and with none given One UI drew Mute microphone as white
            // on white, unreadable (the owner's Fold6, 2026-10-19).
            .setColor(CALL_COLOUR)
            .setStyle(NotificationCompat.CallStyle.forOngoingCall(person, leave).setIsVideo(held.camera))
        if (open != null) builder.setContentIntent(open)
        if (held.microphone) {
            builder.addAction(NotificationCompat.Action(R.drawable.call_microphone_off, t("Mute microphone"), broadcast(context, Control.Mute)))
        } else {
            openApp?.invoke(Control.Unmute)?.let { intent ->
                val unmute = PendingIntent.getActivity(context, 1, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                builder.addAction(NotificationCompat.Action(R.drawable.call_microphone, t("Unmute microphone"), unmute))
            }
        }
        if (held.screen || held.sound) {
            builder.addAction(NotificationCompat.Action(R.drawable.call_stop_sharing, t("Stop sharing"), broadcast(context, Control.StopSharing)))
        }
        return builder.build()
    }

    /** The lamp's amber (Theme's default accent), for the notification's buttons. */
    private const val CALL_COLOUR = 0xFFFFA23B.toInt()

    /** A button's intent to [CallActions], which works with the app anywhere. */
    fun broadcast(context: Context, control: Control): PendingIntent = PendingIntent.getBroadcast(
        context, control.ordinal + 10,
        Intent(control.action).setClass(context, CallActions::class.java),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** Quiet, and named in the reader's language; made again under the same id when the language changes. */
    private fun channel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(CHANNEL, t("Call"), NotificationManager.IMPORTANCE_LOW)
        channel.setShowBadge(false)
        manager.createNotificationChannel(channel)
    }
}
